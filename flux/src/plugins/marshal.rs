//! Shared marshalling helpers.
//!
//! Cross-cutting glue the plugins' async methods repeat. Per-API decoding (this
//! plugin's specific argument/result surface) stays in each plugin; only the
//! uniform value/async plumbing lives here: `with_pending` bridges a fallible
//! native future to a JS promise, and `iter_result` + `attach_async_iterator`
//! build the Rust-backed async-iterables (fetch/p2p byte streams, the p2p accept
//! iterator). More (an `object_builder` HRTB coercion, an actor request/reply
//! bridge) land here as the plugins shrink.

use std::future::Future;

use rquickjs::promise::Promised;
use rquickjs::{Ctx, Function, IntoJs, Object, Value};

use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;

/// Bridge a fallible native async op to a JS promise. Holds a `PendingOps` for
/// the op's whole duration (so the engine loop stays alive until it resolves)
/// and wraps the outcome in `JsResult` (so an `Err(String)` rejects as a clean
/// JS `Error`, with no `IO Error:` prefix).
///
/// Collapses the block every async method repeated:
/// ```ignore
/// let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
/// Ok(Promised(async move {
///   pending.hold();
///   let r = work().await;
///   pending.release();
///   JsResult(r)
/// }))
/// ```
/// into `Ok(with_pending(&ctx, async move { work().await }))`.
pub fn with_pending<'js, T, F>(ctx: &Ctx<'js>, fut: F) -> Promised<impl Future<Output = JsResult<T>>>
where
  F: Future<Output = Result<T, String>>,
{
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  Promised(async move {
    pending.hold();
    let r = fut.await;
    pending.release();
    JsResult(r)
  })
}

/// Build an async-iterator result object `{ value, done }`. `Some(v)` is a chunk
/// (`done: false`); `None` is end-of-stream (`value: undefined, done: true`).
/// The shape every Rust-backed `next()` returns (fetch/p2p byte streams, the p2p
/// accept iterator), so callers only decide chunk-vs-end.
pub fn iter_result<'js>(ctx: &Ctx<'js>, value: Option<Value<'js>>) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  match value {
    Some(v) => {
      obj.set("value", v)?;
      obj.set("done", false)?;
    }
    None => {
      obj.set("value", Value::new_undefined(ctx.clone()))?;
      obj.set("done", true)?;
    }
  }
  Ok(obj)
}

/// Make `obj` its own async-iterator: `obj[Symbol.asyncIterator]()` returns
/// `obj`, so `for await (const x of obj)` drives its `next()`. Generic over the
/// JS handle type (an `Object` iterator, a `Class` instance like `P2pStream`).
pub fn attach_async_iterator<'js, T>(ctx: &Ctx<'js>, obj: &T) -> rquickjs::Result<()>
where
  T: IntoJs<'js> + Clone,
{
  let attach: Function = ctx.eval("(o) => { o[Symbol.asyncIterator] = function () { return this; }; }")?;
  attach.call::<_, ()>((obj.clone(),))?;
  Ok(())
}