//! Shared marshalling helpers.
//!
//! Cross-cutting glue the plugins' async methods repeat. Per-API decoding (this
//! plugin's specific argument/result surface) stays in each plugin; only the
//! uniform value/async plumbing lives here. The first helper bridges a fallible
//! native future to a JS promise; more (an `object_builder` HRTB coercion, an
//! actor request/reply bridge) land here as the plugins shrink.

use std::future::Future;

use rquickjs::promise::Promised;
use rquickjs::Ctx;

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