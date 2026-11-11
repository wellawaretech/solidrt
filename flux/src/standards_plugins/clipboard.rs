//! `navigator.clipboard` and `ClipboardItem`: the web clipboard, a thin
//! marshalling layer over `ore::clipboard`'s `Session`, the session this
//! engine keeps. The standard shape (`readText`/`writeText`, `read`/`write`
//! over `ClipboardItem`, all promises) with simplified semantics: no
//! permissions model and no clipboard events (a single known app does not
//! ask itself for permission), a `ClipboardItem` over `Uint8Array`s since
//! flux has no `Blob`, and an image as the one item with the one type
//! `image/png`. `read` reports only that image, never `text/plain`: text
//! stays with `readText`, which resolves "" for an empty or non-text
//! clipboard. Every failure rejects rather than throwing (an async binding
//! never throws synchronously); only the `ClipboardItem` constructor
//! throws. A bitmap a Windows clipboard holds in place of a PNG (ore hands
//! it over as a BMP) is converted to PNG here, through forge's codec.
//!
//! Each call holds the engine as work in flight until it is answered, so
//! `settle` waits on it. Installed from `gui::install` (the clipboard
//! needs the windowing platform's video subsystem), so a headless flux has
//! no `navigator.clipboard` and no `ClipboardItem`: absence is the
//! availability check for text, and "clipboard-image" in
//! `Flux.capabilities` the one for images.

use std::future::Future;
use std::sync::Arc;

use ore::clipboard::{ClipboardError, Image, ImageFormat, Responder, Session};
use rquickjs::class::Trace;
use rquickjs::promise::Promised;
use rquickjs::{
  Array, CatchResultExt, CaughtError, Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Promise, TypedArray,
  Value,
};
use tokio::sync::oneshot;

use crate::plugins::js_error::named_error;
use crate::plugins::marshal::{bytes_of, OptArg};

/// The capability an image write and read are listed under: where the
/// platform carries an image on its clipboard
/// (`ore::clipboard::image_available`), or where the session records only.
pub(crate) const IMAGE_CAPABILITY: &str = "clipboard-image";

/// The one item type `write` takes and `read` reports.
const PNG_TYPE: &str = "image/png";

const READ_TEXT_API: &str = "navigator.clipboard.readText";
const WRITE_TEXT_API: &str = "navigator.clipboard.writeText";
const READ_API: &str = "navigator.clipboard.read";
const WRITE_API: &str = "navigator.clipboard.write";
const ITEM_API: &str = "ClipboardItem";

#[derive(Clone, JsLifetime)]
struct ClipboardState(#[qjs(skip_trace)] Arc<Session>);

/// Store the clipboard session in userdata. Runs at the gui seam, before
/// the clipboard is defined.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(ClipboardState(Session::new())).expect("store clipboard state");
}

/// This engine's clipboard session, for the embedder's test verbs and
/// control surface (what the app last wrote); None without the gui.
pub fn session(ctx: &Ctx<'_>) -> Option<Arc<Session>> {
  ctx.userdata::<ClipboardState>().map(|s| s.0.clone())
}

/// Never reach the OS clipboard in this engine: the session's own value
/// is the clipboard (a test host's engine, a render). An image then
/// writes and reads whatever the platform, so the engine lists the
/// "clipboard-image" capability. Runs after the gui seam stored the
/// session and `Flux` exists.
pub fn set_record_only(ctx: &Ctx<'_>) {
  let Some(session) = session(ctx) else {
    return;
  };
  if session.is_record_only() {
    return;
  }
  session.set_record_only();
  if !ore::clipboard::image_available() {
    crate::add_capability(ctx, IMAGE_CAPABILITY);
  }
}

/// Define `navigator.clipboard` and the `ClipboardItem` global. Runs after
/// the navigator is installed.
pub(crate) fn init_clipboard(ctx: &Ctx<'_>) {
  Class::<ClipboardItem>::define(&ctx.globals()).expect("define ClipboardItem class");
  let clipboard = Object::new(ctx.clone()).expect("create navigator.clipboard");
  clipboard
    .set("readText", Function::new(ctx.clone(), read_text).expect("create navigator.clipboard.readText"))
    .expect("set navigator.clipboard.readText");
  clipboard
    .set("writeText", Function::new(ctx.clone(), write_text).expect("create navigator.clipboard.writeText"))
    .expect("set navigator.clipboard.writeText");
  clipboard
    .set("read", Function::new(ctx.clone(), read).expect("create navigator.clipboard.read"))
    .expect("set navigator.clipboard.read");
  clipboard
    .set("write", Function::new(ctx.clone(), write).expect("create navigator.clipboard.write"))
    .expect("set navigator.clipboard.write");
  let navigator: Object = ctx.globals().get("navigator").expect("navigator global");
  navigator.set("clipboard", clipboard).expect("set navigator.clipboard");
}

/// `ClipboardItem`: a snapshot of typed bytes, a MIME type to a
/// `Uint8Array` each. Holds only Rust data (see flux/CLAUDE.md,
/// "Persistent handles and classes").
#[derive(JsLifetime)]
#[rquickjs::class(rename = "ClipboardItem")]
pub struct ClipboardItem {
  #[qjs(skip_trace)]
  entries: Vec<(String, Arc<[u8]>)>,
}

impl<'js> Trace<'js> for ClipboardItem {
  fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

#[rquickjs::methods]
impl ClipboardItem {
  /// `new ClipboardItem({ "image/png": bytes })`: the bytes are copied,
  /// any type string is taken. Throws a `TypeError` for a non-object, an
  /// empty record, a type that is not valid text (a lone surrogate), or a
  /// value that is not a `Uint8Array`.
  #[qjs(constructor)]
  pub fn new<'js>(ctx: Ctx<'js>, record: OptArg<Value<'js>>) -> rquickjs::Result<Self> {
    let not_a_record =
      || Exception::throw_type(&ctx, &format!("new {ITEM_API}: pass an object mapping a MIME type to a Uint8Array"));
    let Some(record) = record.0.as_ref().and_then(Value::as_object) else {
      return Err(not_a_record());
    };
    let mut entries = Vec::new();
    // The keys as JS strings, converted by length and validated: a key read
    // as a Rust string straight from its atom is cut at a NUL and not
    // checked for UTF-8. The value is read with the JS key itself.
    for key in record.keys::<rquickjs::String<'js>>() {
      let key = key?;
      let Ok(kind) = key.to_string() else {
        return Err(Exception::throw_type(&ctx, &format!("new {ITEM_API}: a type must be valid text")));
      };
      let value: Value<'js> = record.get(key)?;
      let Ok(bytes) = TypedArray::<u8>::from_value(value) else {
        return Err(Exception::throw_type(&ctx, &format!("new {ITEM_API}: the value for {kind} must be a Uint8Array")));
      };
      entries.push((kind, Arc::from(bytes_of(&ctx, &bytes, &format!("new {ITEM_API}"))?)));
    }
    if entries.is_empty() {
      return Err(not_a_record());
    }
    Ok(ClipboardItem { entries })
  }

  /// The item's types, a frozen array, in the record's order.
  #[qjs(get, enumerable)]
  pub fn types<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Array<'js>> {
    let types = Array::new(ctx.clone())?;
    for (i, (kind, _)) in self.entries.iter().enumerate() {
      types.set(i, kind.as_str())?;
    }
    let object: Object = ctx.globals().get("Object")?;
    let freeze: Function = object.get("freeze")?;
    freeze.call::<_, ()>((types.clone(),))?;
    Ok(types)
  }

  /// `getType(type)`: a promise of a fresh `Uint8Array` copy of the bytes,
  /// rejected with a `TypeError` for a type that is not a string and with
  /// a `NotFoundError` for a type the item does not hold. Never throws.
  #[qjs(rename = "getType")]
  pub fn get_type<'js>(&self, ctx: Ctx<'js>, kind: OptArg<Value<'js>>) -> rquickjs::Result<Promise<'js>> {
    let (promise, resolve, reject) = Promise::new(&ctx)?;
    let api = format!("{ITEM_API}.getType");
    let kind = match string_argument(&ctx, kind.0) {
      Ok(kind) => kind,
      Err(message) => {
        reject.call::<_, ()>((type_error(&ctx, &format!("{api}: the type {message}")),))?;
        return Ok(promise);
      }
    };
    match self.entries.iter().find(|(held, _)| *held == kind) {
      Some((_, bytes)) => {
        resolve.call::<_, ()>((TypedArray::<u8>::new(ctx.clone(), bytes.to_vec())?,))?;
      }
      None => {
        let message = format!("{api}: the item holds no {kind}");
        reject.call::<_, ()>((named_error(&ctx, "NotFoundError", &message)?,))?;
      }
    }
    Ok(promise)
  }
}

/// Why a call rejects: an argument the API does not take (a `TypeError`),
/// or the session's answer.
enum Failure {
  Argument(String),
  Clipboard(ClipboardError),
}

/// A settled call: the value, or a rejection named for why it failed,
/// its message prefixed with the API name.
struct Answer<T> {
  api: &'static str,
  result: Result<T, Failure>,
}

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Answer<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let api = self.api;
    let error = match self.result {
      Ok(value) => return value.into_js(ctx),
      Err(Failure::Argument(message)) => return Err(Exception::throw_type(ctx, &format!("{api}: {message}"))),
      Err(Failure::Clipboard(error)) => error,
    };
    let message = format!("{api}: {error}");
    match error {
      ClipboardError::Unavailable => Err(ctx.throw(named_error(ctx, "NotSupportedError", &message)?)),
      ClipboardError::NotPng => Err(Exception::throw_type(ctx, &message)),
      ClipboardError::Failed(_) => Err(Exception::throw_message(ctx, &message)),
    }
  }
}

/// What `read` resolves to: no item, or one holding the PNG.
struct Items(Option<Arc<[u8]>>);

impl<'js> IntoJs<'js> for Items {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let items = Array::new(ctx.clone())?;
    if let Some(png) = self.0 {
      items.set(0, Class::instance(ctx.clone(), ClipboardItem { entries: vec![(PNG_TYPE.to_string(), png)] })?)?;
    }
    Ok(items.into_value())
  }
}

/// Hand `call` (or the argument error that stops it) to the session, the
/// engine held as work in flight from here until the answer, which `map`
/// turns into the call's value.
fn ask<T, U, C, M>(
  ctx: &Ctx<'_>,
  api: &'static str,
  call: Result<C, String>,
  map: M,
) -> Promised<impl Future<Output = Answer<U>>>
where
  T: Send + 'static,
  C: FnOnce(&Arc<Session>, Responder<T>),
  M: FnOnce(T) -> Result<U, Failure>,
{
  let hold = crate::pending::PendingOps::of(ctx).in_flight("clipboard");
  let (tx, rx) = oneshot::channel();
  let respond: Responder<T> = Box::new(move |result| {
    tx.send(result).ok();
  });
  let stopped = match (call, session(ctx)) {
    (Err(message), _) => Some(Failure::Argument(message)),
    (Ok(_), None) => {
      Some(Failure::Clipboard(ClipboardError::Failed("no clipboard session in this engine".to_string())))
    }
    (Ok(call), Some(session)) => {
      call(&session, respond);
      None
    }
  };
  Promised(async move {
    let result = match stopped {
      Some(failure) => Err(failure),
      None => rx
        .await
        .unwrap_or_else(|_| Err(ClipboardError::Failed("the platform did not answer".to_string())))
        .map_err(Failure::Clipboard)
        .and_then(map),
    };
    drop(hold);
    Answer { api, result }
  })
}

/// `readText()`: the clipboard's text, "" when it holds none.
fn read_text(ctx: Ctx<'_>) -> rquickjs::Result<Promised<impl Future<Output = Answer<String>>>> {
  let call = Ok(|session: &Arc<Session>, respond: Responder<String>| session.read_text(respond));
  Ok(ask(&ctx, READ_TEXT_API, call, Ok))
}

/// `writeText(text)`: the clipboard holds `text` from now on.
fn write_text<'js>(
  ctx: Ctx<'js>,
  text: OptArg<Value<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = Answer<()>>>> {
  let text = string_argument(&ctx, text.0).map_err(|message| format!("text {message}"));
  let call = text.map(|text| move |session: &Arc<Session>, respond: Responder<()>| session.write_text(text, respond));
  Ok(ask(&ctx, WRITE_TEXT_API, call, Ok))
}

/// `read()`: the clipboard's image as one item, or no item.
fn read(ctx: Ctx<'_>) -> rquickjs::Result<Promised<impl Future<Output = Answer<Items>>>> {
  let call = Ok(|session: &Arc<Session>, respond: Responder<Option<Image>>| session.read_image(respond));
  Ok(ask(&ctx, READ_API, call, |image| as_png(image).map(Items)))
}

/// `write(items)`: the clipboard holds the one item's PNG from now on.
fn write<'js>(
  ctx: Ctx<'js>,
  items: OptArg<Value<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = Answer<()>>>> {
  let call = one_png(&ctx, items.0)
    .map(|png| move |session: &Arc<Session>, respond: Responder<()>| session.write_image(png, respond));
  Ok(ask(&ctx, WRITE_API, call, Ok))
}

/// The PNG of `items`, an array of exactly one `ClipboardItem` with the
/// one type `image/png`; the error says what else it is. Reading the
/// array can run script (an index getter), so every read goes through
/// `argument` and a throw becomes the error.
fn one_png<'js>(ctx: &Ctx<'js>, items: Option<Value<'js>>) -> Result<Arc<[u8]>, String> {
  let Some(items) = items.as_ref().and_then(Value::as_array) else {
    return Err(format!("pass an array of one ClipboardItem holding {PNG_TYPE}"));
  };
  // The length as a number: `Array::len` asserts an int32, and a script's
  // array can be longer (`new Array(2 ** 31)`).
  let length: f64 = argument(ctx, items.as_object().get("length"))?;
  if length != 1.0 {
    return Err(format!("pass exactly one ClipboardItem, got {length}"));
  }
  let item: Value<'js> = argument(ctx, items.get(0))?;
  let Ok(item) = Class::<ClipboardItem>::from_value(&item) else {
    return Err("the item must be a ClipboardItem".to_string());
  };
  let item = item.borrow();
  match item.entries.as_slice() {
    [(kind, png)] if kind == PNG_TYPE => Ok(png.clone()),
    entries => {
      let types: Vec<&str> = entries.iter().map(|(kind, _)| kind.as_str()).collect();
      Err(format!("the item must hold the one type {PNG_TYPE}, it holds {}", types.join(", ")))
    }
  }
}

/// A string argument, or why it is not one ("must be a string", or why
/// it does not convert: a lone surrogate is no UTF-8).
fn string_argument<'js>(ctx: &Ctx<'js>, value: Option<Value<'js>>) -> Result<String, String> {
  let Some(text) = value.as_ref().and_then(Value::as_string) else {
    return Err("must be a string".to_string());
  };
  argument(ctx, text.to_string()).map_err(|e| format!("is not valid text: {e}"))
}

/// What reading an argument gave, or the message of what went wrong: a
/// conversion error, or what a script threw, taken off the context so it
/// does not leak into the next call.
fn argument<'js, T>(ctx: &Ctx<'js>, result: rquickjs::Result<T>) -> Result<T, String> {
  let result = result.catch(ctx).map_err(|error| match error {
    CaughtError::Exception(exception) => exception.message().unwrap_or_else(|| "reading it threw".to_string()),
    CaughtError::Value(_) => "reading it threw".to_string(),
    CaughtError::Error(error) => error.to_string(),
  });
  // Reading the thrown error's message can run a getter that throws again.
  if ctx.has_exception() {
    drop(ctx.catch());
  }
  result
}

/// A `TypeError` as a value, to reject with: thrown, then taken back off
/// the context.
fn type_error<'js>(ctx: &Ctx<'js>, message: &str) -> Value<'js> {
  let _ = Exception::throw_type(ctx, message);
  ctx.catch()
}

/// The clipboard's image as PNG bytes: a BMP (a Windows bitmap, or one a
/// test put) re-encoded through forge's codec, its pixels as they are.
fn as_png(image: Option<Image>) -> Result<Option<Arc<[u8]>>, Failure> {
  let Some(image) = image else {
    return Ok(None);
  };
  match image.format {
    ImageFormat::Png => Ok(Some(image.bytes)),
    ImageFormat::Bmp => {
      let failed = |e: String| {
        Failure::Clipboard(ClipboardError::Failed(format!("the clipboard's bitmap could not be converted to PNG: {e}")))
      };
      let decoded = forge::image::decode(&image.bytes, false).map_err(failed)?;
      let png = forge::image::encode_png(&decoded.data, decoded.width, decoded.height, false).map_err(failed)?;
      Ok(Some(png.into()))
    }
  }
}
