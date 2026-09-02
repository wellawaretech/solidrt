//! `navigator.clipboard`: the web clipboard surface, text only. The standard
//! shape (`readText`/`writeText`, both promises) with solidrt semantics: no
//! permissions model and no clipboard events - a single known app does not ask
//! itself for permission; `readText` resolves "" on an empty clipboard; every
//! failure rejects rather than throwing (an async binding never throws
//! synchronously). Backed by SDL's clipboard, which belongs to the platform
//! loop's thread, so each call sends an `AlloyCommand` whose responder feeds a
//! oneshot the promise awaits. The one gui-gated standards module: installed
//! from `gui::install` (the seam that has the command channel), so `navigator`
//! simply does not exist on a headless build - absence is the availability
//! check.

use std::sync::mpsc::Sender;

use rquickjs::{function::MutFn, promise::Promised, Ctx, Function, Object, Value};
use tokio::sync::oneshot;

use alloy::AlloyCommand;

use crate::plugins::marshal::with_pending;

/// Send a clipboard command whose responder resolves the returned receiver;
/// a dead platform loop becomes an Err the promise rejects with.
fn roundtrip<T: Send + 'static>(
  cmd_tx: &Sender<AlloyCommand>,
  make: impl FnOnce(Box<dyn FnOnce(Result<T, String>) + Send>) -> AlloyCommand,
  what: &str,
) -> Result<oneshot::Receiver<Result<T, String>>, String> {
  let (tx, rx) = oneshot::channel();
  let respond = Box::new(move |result| {
    tx.send(result).ok();
  });
  cmd_tx.send(make(respond)).map(|_| rx).map_err(|_| format!("{what}: platform loop is gone"))
}

/// Install the `navigator` global with its `clipboard.readText`/`writeText`.
pub fn init_clipboard(ctx: &Ctx<'_>, cmd_tx: Sender<AlloyCommand>) {
  let read_tx = cmd_tx.clone();
  let read_fn = Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
      let rx = roundtrip(&read_tx, AlloyCommand::GetClipboardText, "navigator.clipboard.readText");
      Ok(with_pending(&ctx, async move {
        match rx {
          Ok(rx) => rx
            .await
            .unwrap_or_else(|_| Err("navigator.clipboard.readText: platform loop did not answer".into())),
          Err(e) => Err(e),
        }
      }))
    }),
  )
  .expect("failed to create navigator.clipboard.readText");

  let write_fn = Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>, text: Value<'_>| -> rquickjs::Result<Promised<_>> {
      let rx = text
        .as_string()
        .and_then(|s| s.to_string().ok())
        .ok_or_else(|| "navigator.clipboard.writeText: text must be a string".to_string())
        .and_then(|text| {
          roundtrip(&cmd_tx, |respond| AlloyCommand::SetClipboardText(text, respond), "navigator.clipboard.writeText")
        });
      Ok(with_pending(&ctx, async move {
        match rx {
          Ok(rx) => rx
            .await
            .unwrap_or_else(|_| Err("navigator.clipboard.writeText: platform loop did not answer".into())),
          Err(e) => Err(e),
        }
      }))
    }),
  )
  .expect("failed to create navigator.clipboard.writeText");

  let clipboard = Object::new(ctx.clone()).expect("failed to create navigator.clipboard");
  clipboard.set("readText", read_fn).expect("failed to set navigator.clipboard.readText");
  clipboard.set("writeText", write_fn).expect("failed to set navigator.clipboard.writeText");
  let navigator = Object::new(ctx.clone()).expect("failed to create navigator");
  navigator.set("clipboard", clipboard).expect("failed to set navigator.clipboard");
  ctx.globals().set("navigator", navigator).expect("failed to set navigator global");
}
