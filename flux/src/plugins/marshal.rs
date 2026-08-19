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

use rquickjs::function::{FromParam, ParamRequirement, ParamsAccessor, This};
use rquickjs::promise::Promised;
use rquickjs::{qjs, ArrayBuffer, Ctx, FromJs, Function, IntoJs, Object, Value};

use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;

/// An optional argument: `opts: OptArg<Object>`, `code: OptArg<u16>`, ...
///
/// rquickjs `Opt<T>` only tolerates an ABSENT argument; an explicit
/// `undefined` is still converted into `T` and fails ("Error converting from
/// js 'undefined' into type ..."). Passing `undefined` for "not given" is
/// ordinary JS - wrappers forward their own optional parameter verbatim, and
/// the web platform reads `undefined` as "use the default" everywhere - so
/// binding params take `OptArg<T>` instead, which treats absent, `undefined`,
/// and `null` alike as `None`. Any other value must convert into `T`.
pub struct OptArg<T>(pub Option<T>);

impl<'js, T: FromJs<'js>> FromParam<'js> for OptArg<T> {
  fn param_requirement() -> ParamRequirement {
    ParamRequirement::optional()
  }

  fn from_param<'a>(params: &mut ParamsAccessor<'a, 'js>) -> rquickjs::Result<Self> {
    if params.is_empty() {
      return Ok(OptArg(None));
    }
    let ctx = params.ctx().clone();
    let value = params.arg();
    if value.is_undefined() || value.is_null() {
      return Ok(OptArg(None));
    }
    Ok(OptArg(Some(T::from_js(&ctx, value)?)))
  }
}

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

/// One step of a Rust-backed async iterator, returned owned from `next()`:
/// `Some(item)` is a value (`done: false`), `None` is the end (`done: true`).
/// Owned rather than a `Ctx`-bound object so the `next()` future stays
/// `'static`; the `IntoJs` builds the `{ value, done }` object on the JS thread.
pub struct Step<T>(pub Option<T>);

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Step<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let value = match self.0 {
      Some(v) => Some(v.into_js(ctx)?),
      None => None,
    };
    Ok(iter_result(ctx, value)?.into_value())
  }
}

/// Build an async-iterator result object `{ value, done }`. `Some(v)` is a chunk
/// (`done: false`); `None` is end-of-stream (`value: undefined, done: true`).
/// `Step` is the owned form for `next()` return types; call this directly only
/// when the value is already a JS handle.
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

/// Mark a returned promise as handled at the JS-engine level before its result is
/// read natively through `MaybePromise`/`into_future`. `PromiseFuture::poll`
/// takes a fast path: when the promise has already settled (e.g. a handler
/// synchronously returns `Promise.reject(e)`), it reads the result via
/// `JS_PromiseResult` directly and never calls `.then()`/`.catch()`. QuickJS's
/// unhandled-rejection tracker only clears when a reaction is attached, so
/// without this a rejection we genuinely route to `error()` is still reported by
/// `engine::flush_rejections` as if nobody looked at it.
///
/// The reaction must be a real no-op rejection handler, not `Undefined`:
/// `.then(_, undefined)` marks this promise handled but yields a derived promise
/// that re-rejects with the same reason and is itself unhandled, so the rejection
/// simply reappears. A no-op `onRejected` lets the derived promise resolve.
pub fn mark_observed<'js>(val: &Value<'js>) {
  let Some(promise) = val.as_promise() else { return };
  let Ok(noop) = Function::new(promise.ctx().clone(), || {}) else { return };
  let Ok(catch) = promise.catch() else { return };
  let _ = catch.call::<_, Value<'_>>((This(promise.clone()), noop));
}

/// Create an ArrayBuffer aliasing external bytes, with NO free callback:
/// QuickJS never frees or touches the bytes, on detach or at finalization.
///
/// Not `ArrayBuffer::from_source`, deliberately: its drop closure is unsound
/// against detach. `JS_DetachArrayBuffer` invokes the buffer's `free_func`
/// but does not clear it, so the finalizer invokes it AGAIN at teardown with
/// the same opaque pointer, and rquickjs's shim then double-drops its boxed
/// closure (double `Box::from_raw`) - a crash (see
/// okf/upstream/rquickjs-detach-double-free.md). With no callback registered,
/// both sites are no-ops and the bytes' lifetime is the caller's contract:
/// the wasm plugin pins the instance in its registry, the gpu plugin's write
/// lease pins the staging block in alloy's Context until end/destroy.
pub fn array_buffer_over<'js>(ctx: &Ctx<'js>, ptr: *mut u8, len: usize) -> rquickjs::Result<ArrayBuffer<'js>> {
  let value = unsafe {
    let raw = qjs::JS_NewArrayBuffer(ctx.as_raw().as_ptr(), ptr, len as _, None, std::ptr::null_mut(), false);
    Value::from_raw(ctx.clone(), raw)
  };
  if value.is_exception() {
    return Err(rquickjs::Error::Exception);
  }
  ArrayBuffer::from_value(value).ok_or(rquickjs::Error::Unknown)
}
