use bytes::Bytes;
use rquickjs::{
  function::{MutFn, This},
  promise::{MaybePromise, Promised},
  Ctx, Function, IntoJs, Object, TypedArray, Value,
};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::io;
use std::rc::Rc;
use tokio::sync::mpsc;

use crate::logger::Logger;

/// In-memory body buffer shared by Response and Request. Consume-once semantics:
/// `take` returns the bytes once, then subsequent calls return None.
pub(crate) struct BodyState {
  bytes: RefCell<Option<Vec<u8>>>,
}

impl BodyState {
  pub(crate) fn new(bytes: Vec<u8>) -> Self {
    Self { bytes: RefCell::new(Some(bytes)) }
  }

  pub(crate) fn empty() -> Self {
    Self { bytes: RefCell::new(Some(Vec::new())) }
  }

  /// Peek a copy of the bytes without consuming. Returns None if already consumed.
  pub(crate) fn peek(&self) -> Option<Vec<u8>> {
    self.bytes.borrow().clone()
  }

  /// Consume the bytes. Returns None if already consumed.
  pub(crate) fn take(&self) -> Option<Vec<u8>> {
    self.bytes.borrow_mut().take()
  }
}

pub(crate) fn body_text(state: &BodyState, ctx: &Ctx<'_>) -> rquickjs::Result<String> {
  let bytes = state.take().ok_or_else(|| throw_consumed(ctx))?;
  String::from_utf8(bytes).map_err(utf8_err)
}

pub(crate) fn body_bytes(state: &BodyState, ctx: &Ctx<'_>) -> rquickjs::Result<JsBytes> {
  let bytes = state.take().ok_or_else(|| throw_consumed(ctx))?;
  Ok(JsBytes(bytes))
}

pub(crate) fn body_json(state: &BodyState, ctx: &Ctx<'_>) -> rquickjs::Result<JsonValue> {
  let text = body_text(state, ctx)?;
  Ok(JsonValue(text))
}

/// Extract bytes from a JS value (string, Uint8Array, null/undefined).
pub(crate) fn extract_body_value<'js>(val: &Value<'js>, for_class: &'static str) -> rquickjs::Result<Vec<u8>> {
  if val.is_null() || val.is_undefined() {
    return Ok(Vec::new());
  }
  if let Some(s) = val.as_string() {
    return Ok(s.to_string()?.into_bytes());
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  Err(rquickjs::Error::new_from_js_message("body", for_class, "must be string, Uint8Array, null, or undefined"))
}

/// True if `val` is an async-iterable (has a `Symbol.asyncIterator` method), e.g.
/// the object an `async function*` generator returns. Primitives (strings, null,
/// numbers) are not objects, so they short-circuit to false without evaluating.
pub(crate) fn is_async_iterable<'js>(ctx: &Ctx<'js>, val: &Value<'js>) -> rquickjs::Result<bool> {
  if val.as_object().is_none() {
    return Ok(false);
  }
  let probe: Function<'js> = ctx.eval("(o) => typeof o[Symbol.asyncIterator] === 'function'")?;
  probe.call((val.clone(),))
}

/// Parse a Response body value into either buffered bytes or, when it is an
/// async-iterable, a stream source object to be drained later (see
/// `pump_async_iterable`). Otherwise falls back to the buffered `extract_body_value`
/// rules (string, Uint8Array, null/undefined).
pub(crate) fn extract_streaming_body<'js>(
  ctx: &Ctx<'js>,
  val: &Value<'js>,
) -> rquickjs::Result<(Vec<u8>, Option<Object<'js>>)> {
  if is_async_iterable(ctx, val)? {
    let obj = val.clone().into_object().expect("async iterable is an object");
    return Ok((Vec::new(), Some(obj)));
  }
  Ok((extract_body_value(val, "Response")?, None))
}

/// Drive a JS async-iterable body, sending each yielded chunk to `tx` as bytes
/// until the iterator is done, an error occurs, or the consumer drops the
/// receiver. Chunks must be strings or Uint8Arrays; empty chunks are skipped.
///
/// Touches JS values, so it must run on the QuickJS executor (spawn via
/// `ctx.spawn`). Shared by the HTTP server's streamed responses today; the same
/// shape fits a streamed fetch request body (a different sink) later.
pub(crate) async fn pump_async_iterable<'js>(
  ctx: Ctx<'js>,
  iterable: Object<'js>,
  tx: mpsc::Sender<Bytes>,
  logger: Logger,
) {
  let get_iter: Function<'js> = match ctx.eval("(o) => o[Symbol.asyncIterator]()") {
    Ok(f) => f,
    Err(e) => {
      logger.warn(&format!("[flux] stream: body is not async-iterable: {e}"));
      return;
    }
  };
  let iter: Object<'js> = match get_iter.call((iterable,)) {
    Ok(i) => i,
    Err(e) => {
      logger.warn(&format!("[flux] stream: could not get async iterator: {e}"));
      return;
    }
  };
  let next: Function<'js> = match iter.get("next") {
    Ok(n) => n,
    Err(e) => {
      logger.warn(&format!("[flux] stream: iterator has no next(): {e}"));
      return;
    }
  };

  loop {
    let step: Value<'js> = match next.call((This(iter.clone()),)) {
      Ok(v) => v,
      Err(e) => {
        logger.warn(&format!("[flux] stream: iterator next() threw: {e}"));
        break;
      }
    };
    let result = match MaybePromise::from_value(step).into_future::<Value<'js>>().await {
      Ok(v) => v,
      Err(e) => {
        logger.warn(&format!("[flux] stream: iterator rejected: {e}"));
        break;
      }
    };
    let Some(obj) = result.into_object() else {
      logger.warn("[flux] stream: iterator result was not an object");
      break;
    };
    if obj.get("done").unwrap_or(true) {
      break;
    }
    let value: Value<'js> = match obj.get("value") {
      Ok(v) => v,
      Err(e) => {
        logger.warn(&format!("[flux] stream: could not read chunk value: {e}"));
        break;
      }
    };
    let chunk = match extract_body_value(&value, "stream chunk") {
      Ok(b) => b,
      Err(e) => {
        logger.warn(&format!("[flux] stream: chunk must be a string or Uint8Array: {e}"));
        break;
      }
    };
    if chunk.is_empty() {
      continue;
    }
    // A send error means the consumer is gone (e.g. the connection closed).
    if tx.send(Bytes::from(chunk)).await.is_err() {
      break;
    }
  }
}

pub struct JsBytes(pub Vec<u8>);

impl<'js> IntoJs<'js> for JsBytes {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    TypedArray::<u8>::new(ctx.clone(), self.0).map(|ta| ta.into_value())
  }
}

pub struct JsonValue(pub String);

impl<'js> IntoJs<'js> for JsonValue {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    ctx.json_parse(self.0)
  }
}

pub(crate) fn throw_consumed(ctx: &Ctx<'_>) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), "Body already consumed").expect("create error string").into())
}

fn utf8_err(e: std::string::FromUtf8Error) -> rquickjs::Error {
  rquickjs::Error::Io(io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Attach text(), bytes(), json() methods to obj.
///
/// fetch_bytes is invoked lazily on each method call to obtain the body bytes.
/// If consume_once is true, calling any of the three methods more than once
/// throws "Body already consumed" (web fetch semantics). If false, methods can
/// be called repeatedly (file-like semantics).
pub fn attach_body<'js, F, Fut>(
  ctx: &Ctx<'js>,
  obj: &Object<'js>,
  fetch_bytes: F,
  consume_once: bool,
) -> rquickjs::Result<()>
where
  F: Fn() -> Fut + Clone + 'static,
  Fut: Future<Output = rquickjs::Result<Vec<u8>>> + 'static,
{
  let consumed = Rc::new(Cell::new(false));

  let text_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let consumed = consumed.clone();
      let fetch_bytes = fetch_bytes.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        if consume_once && consumed.get() {
          return Err(throw_consumed(&ctx));
        }
        consumed.set(true);
        let fetch = fetch_bytes.clone();
        Ok(Promised(async move {
          let bytes = fetch().await?;
          String::from_utf8(bytes).map_err(utf8_err)
        }))
      }
    }),
  )
  .expect("create text function");

  let bytes_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let consumed = consumed.clone();
      let fetch_bytes = fetch_bytes.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        if consume_once && consumed.get() {
          return Err(throw_consumed(&ctx));
        }
        consumed.set(true);
        let fetch = fetch_bytes.clone();
        Ok(Promised(async move {
          let bytes = fetch().await?;
          Ok::<JsBytes, rquickjs::Error>(JsBytes(bytes))
        }))
      }
    }),
  )
  .expect("create bytes function");

  let json_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let consumed = consumed.clone();
      let fetch_bytes = fetch_bytes.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        if consume_once && consumed.get() {
          return Err(throw_consumed(&ctx));
        }
        consumed.set(true);
        let fetch = fetch_bytes.clone();
        Ok(Promised(async move {
          let bytes = fetch().await?;
          let text = String::from_utf8(bytes).map_err(utf8_err)?;
          Ok::<JsonValue, rquickjs::Error>(JsonValue(text))
        }))
      }
    }),
  )
  .expect("create json function");

  obj.set("text", text_fn)?;
  obj.set("bytes", bytes_fn)?;
  obj.set("json", json_fn)?;

  Ok(())
}
