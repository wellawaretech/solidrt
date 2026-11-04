//! `navigator.share`: the web's share call, a thin marshalling layer over
//! `ore::share`'s `Session`, the session this engine keeps. The web's
//! shape (`{ title, text, url, files }`) with simplified semantics: no
//! user-gesture rule, no `canShare` (the "share" capability says it), and
//! `navigator.share` defined only where the capability is listed, so the
//! web's own check (`if (navigator.share)`) agrees with it. `files` holds
//! `flux:fs` file handles, the runtime's one file model. The call resolves
//! once the sheet accepted the share and rejects with the web's names: an
//! `AbortError` when the user closed it without picking a target, an
//! `InvalidStateError` while another sheet is open, a `TypeError` for
//! data it cannot share. It holds the engine as work in flight until the
//! sheet closes, so `settle` waits on it.

use std::future::Future;
use std::sync::Arc;

use ore::share::{Session, ShareError, ShareRequest, SharedFile};
use rquickjs::promise::Promised;
use rquickjs::{Array, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Value};
use tokio::sync::oneshot;

use crate::plugins::js_error::named_error;
use crate::plugins::marshal::string_opt;

/// The capability `navigator.share` is listed under: where the platform
/// has a share sheet (`ore::share::available`), or where the session
/// records only.
pub(crate) const CAPABILITY: &str = "share";

const API: &str = "navigator.share";

#[derive(Clone, JsLifetime)]
struct ShareState(#[qjs(skip_trace)] Arc<Session>);

/// Store the share session in userdata. Runs at engine init, before the
/// navigator is installed.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(ShareState(Session::new())).expect("store share state");
}

/// This engine's share session, for the embedder's test verb and control
/// surface (the shares the app asked for).
pub fn session(ctx: &Ctx<'_>) -> Option<Arc<Session>> {
  ctx.userdata::<ShareState>().map(|s| s.0.clone())
}

/// Define `navigator.share` where the platform has a sheet. Runs after
/// the navigator is installed.
pub(crate) fn init_share(ctx: &Ctx<'_>) {
  if ore::share::available() {
    define(ctx);
  }
}

fn define(ctx: &Ctx<'_>) {
  let navigator: Object = ctx.globals().get("navigator").expect("navigator global");
  navigator
    .set("share", Function::new(ctx.clone(), share).expect("create navigator.share"))
    .expect("set navigator.share");
}

/// Never show a sheet in this engine: every share is logged and resolves
/// (a test host's engine, a render). Every call then resolves, so the
/// engine defines `navigator.share` and lists the "share" capability
/// whether or not the platform has a sheet.
pub fn set_record_only(ctx: &Ctx<'_>) {
  let Some(session) = session(ctx) else {
    return;
  };
  if session.is_record_only() {
    return;
  }
  session.set_record_only();
  if !ore::share::available() {
    define(ctx);
    crate::add_capability(ctx, CAPABILITY);
  }
}

/// `navigator.share(data)`: the data read into a request and handed to the
/// session.
fn share<'js>(ctx: Ctx<'js>, data: Value<'js>) -> rquickjs::Result<Promised<impl Future<Output = Shared>>> {
  let Some(data) = data.as_object() else {
    return Err(Exception::throw_type(&ctx, &format!("{API}: pass {{ title, text, url, files }}")));
  };
  let request = ShareRequest {
    title: string_opt(&ctx, data, "title", API)?,
    text: string_opt(&ctx, data, "text", API)?,
    url: string_opt(&ctx, data, "url", API)?,
    files: files(&ctx, data)?,
  };
  if request.title.is_none() && request.text.is_none() && request.url.is_none() && request.files.is_empty() {
    return Err(Exception::throw_type(&ctx, &format!("{API}: nothing to share: set title, text, url or files")));
  }
  let Some(session) = session(&ctx) else {
    return Err(Exception::throw_message(&ctx, &format!("{API}: no share session in this engine")));
  };
  let hold = crate::pending::PendingOps::of(&ctx).in_flight("share");
  let (tx, rx) = oneshot::channel();
  session.share(
    request,
    Box::new(move |result| {
      tx.send(result).ok();
    }),
  );
  Ok(Promised(async move {
    let result = rx.await.unwrap_or_else(|_| Err(ShareError::Failed("the platform did not answer".to_string())));
    drop(hold);
    Shared(result)
  }))
}

/// The `files` field: `flux:fs` file handles, read for where each file is
/// and the name to show for it.
fn files<'js>(ctx: &Ctx<'js>, data: &Object<'js>) -> rquickjs::Result<Vec<SharedFile>> {
  let Some(files) = data.get::<_, Option<Array<'js>>>("files")? else {
    return Ok(Vec::new());
  };
  let not_a_file =
    || Exception::throw_type(ctx, &format!("{API}: files must be flux:fs file handles, as file() returns"));
  let mut out = Vec::new();
  for file in files.iter::<Value<'js>>() {
    let file = file?;
    let file = file.as_object().ok_or_else(not_a_file)?;
    let location: String = file.get::<_, Option<String>>("path")?.ok_or_else(not_a_file)?;
    let name = file.get::<_, Option<String>>("name")?.unwrap_or_default();
    out.push(SharedFile { location, name });
  }
  Ok(out)
}

/// A settled call: undefined, or a rejection with the web's name for why
/// the share did not go out.
struct Shared(Result<(), ShareError>);

impl<'js> IntoJs<'js> for Shared {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let error = match self.0 {
      Ok(()) => return Ok(Value::new_undefined(ctx.clone())),
      Err(error) => error,
    };
    let message = format!("{API}: {error}");
    let value = match error {
      ShareError::Cancelled => named_error(ctx, "AbortError", &message)?,
      ShareError::Busy => named_error(ctx, "InvalidStateError", &message)?,
      ShareError::Unavailable => named_error(ctx, "NotSupportedError", &message)?,
      ShareError::NotShareable(_) => return Err(Exception::throw_type(ctx, &message)),
      ShareError::Failed(_) => return Err(Exception::throw_message(ctx, &message)),
    };
    Err(ctx.throw(value))
  }
}
