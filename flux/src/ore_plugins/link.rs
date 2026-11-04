//! `flux:link`: `openLink` and `registerProtocolHandler`, a thin
//! marshalling layer over `ore::link`'s `Session`, the session this engine
//! keeps. The link always goes out to the platform's opener; the call
//! resolves once the opener accepted it and rejects when the platform
//! refuses it (`NotSupportedError` where there is no opener at all). The
//! session logs every link, which the embedder reads back for a test or an
//! agent, and reaches no platform when set to record only. A call holds
//! the engine as work in flight until the platform answers, so `settle`
//! waits on it. The registration reads the app's own scheme the embedder
//! set (`set_own_scheme`).

use std::future::Future;
use std::sync::Arc;

use ore::link::{LinkError, OwnScheme, Session};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Exception, Function, IntoJs, JsLifetime, Value};
use tokio::sync::oneshot;

use crate::plugins::js_error::named_error;

/// The capability `openLink` is listed under, only where the platform has
/// an opener (`ore::link::available`), or where the session records only.
pub(crate) const CAPABILITY: &str = "link";

#[derive(Clone, JsLifetime)]
struct LinkState(#[qjs(skip_trace)] Arc<Session>);

/// Store the link session in userdata. Runs at engine init, before any
/// module import.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(LinkState(Session::new())).expect("store link state");
}

/// This engine's link session, for the embedder's test verb and control
/// surface (the links the app asked to open).
pub fn session(ctx: &Ctx<'_>) -> Option<Arc<Session>> {
  ctx.userdata::<LinkState>().map(|s| s.0.clone())
}

/// What the embedder knows of the app's own scheme, for
/// `registerProtocolHandler`; `OwnScheme::Unknown` until set.
pub fn set_own_scheme(ctx: &Ctx<'_>, own: OwnScheme) {
  if let Some(session) = session(ctx) {
    session.set_own_scheme(own);
  }
}

/// Never reach the platform in this engine: every link is logged and the
/// call resolves, and a registration writes nothing (a test host's engine,
/// a render). Every call then resolves, so the engine lists the "link"
/// capability whether or not the platform has an opener.
pub fn set_record_only(ctx: &Ctx<'_>) {
  let Some(session) = session(ctx) else {
    return;
  };
  if session.is_record_only() {
    return;
  }
  session.set_record_only();
  if !ore::link::available() {
    crate::add_capability(ctx, CAPABILITY);
  }
}

/// The `flux:link` module.
pub struct LinkModule;

impl ModuleDef for LinkModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("openLink")?;
    decl.declare("registerProtocolHandler")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("openLink", Function::new(ctx.clone(), open_link)?)?;
    exports.export("registerProtocolHandler", Function::new(ctx.clone(), register_protocol_handler)?)?;
    Ok(())
  }
}

/// `openLink(link)`: a string with a scheme (`https:`, `mailto:`, the
/// app's own), handed to the session.
fn open_link<'js>(ctx: Ctx<'js>, link: Value<'js>) -> rquickjs::Result<Promised<impl Future<Output = Opened>>> {
  let Some(link) = link.as_string().map(|s| s.to_string()).transpose()? else {
    return Err(Exception::throw_type(&ctx, "openLink: pass the link as a string"));
  };
  if ore::link::scheme(&link).is_none() {
    return Err(Exception::throw_type(
      &ctx,
      &format!("openLink: the link needs a scheme, like https://example.com, got {link:?}"),
    ));
  }
  if link.contains('\0') {
    return Err(Exception::throw_type(&ctx, "openLink: the link must not contain a NUL character"));
  }
  let Some(session) = session(&ctx) else {
    return Err(Exception::throw_message(&ctx, "openLink: no link session in this engine"));
  };
  let hold = crate::pending::PendingOps::of(&ctx).in_flight("openLink");
  let (tx, rx) = oneshot::channel();
  session.open(
    link,
    Box::new(move |result| {
      tx.send(result).ok();
    }),
  );
  Ok(Promised(async move {
    let result = rx.await.unwrap_or_else(|_| Err(LinkError::Failed("the platform did not answer".to_string())));
    drop(hold);
    Opened(result)
  }))
}

/// `registerProtocolHandler()`: make this executable the handler for the
/// app's own scheme, as the session's `OwnScheme` says. Throws with why
/// it could not.
fn register_protocol_handler(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let Some(session) = session(&ctx) else {
    return Err(Exception::throw_message(&ctx, "registerProtocolHandler: no link session in this engine"));
  };
  session.register_handler().map_err(|e| Exception::throw_message(&ctx, &format!("registerProtocolHandler: {e}")))
}

/// A settled call: undefined, or a rejection named for why the link was
/// not opened.
struct Opened(Result<(), LinkError>);

impl<'js> IntoJs<'js> for Opened {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self.0 {
      Ok(()) => Ok(Value::new_undefined(ctx.clone())),
      Err(error @ LinkError::Unavailable) => {
        Err(ctx.throw(named_error(ctx, "NotSupportedError", &format!("openLink: {error}"))?))
      }
      Err(LinkError::Failed(message)) => Err(Exception::throw_message(ctx, &format!("openLink: {message}"))),
    }
  }
}
