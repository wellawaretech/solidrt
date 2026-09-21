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
use std::ptr::NonNull;

use rquickjs::atom::PredefinedAtom;
use rquickjs::function::{FromParam, ParamRequirement, ParamsAccessor, This};
use rquickjs::promise::Promised;
use rquickjs::{qjs, ArrayBuffer, Ctx, Exception, FromJs, Function, IntoJs, Object, TypedArray, Value};

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
/// `obj`, so `for await (const x of obj)` drives its `next()`. A `Class`
/// instance (`P2pStream`, `NetConn`) derefs to its `Object`.
pub fn attach_async_iterator<'js>(ctx: &Ctx<'js>, obj: &Object<'js>) -> rquickjs::Result<()> {
  obj.set(PredefinedAtom::SymbolAsyncIterator, Function::new(ctx.clone(), |this: This<Value<'js>>| this.0)?)
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

/// A JS buffer whose bytes native code can read in place: an ArrayBuffer, or
/// a typed array (its own viewed range, not the whole buffer behind it).
///
/// The borrow is the engine's memory. It stays valid only while no JS runs:
/// a script can write into, detach or (resizable buffers) move the backing
/// store. Every accessor below hands out a slice for synchronous native use
/// during the binding call, which is the one pattern the plugins have, so
/// the unsafety lives here and nowhere else.
pub trait JsBytes {
  /// The viewed bytes, `None` when the buffer is detached.
  fn raw_bytes(&self) -> Option<NonNull<[u8]>>;
}

impl JsBytes for ArrayBuffer<'_> {
  fn raw_bytes(&self) -> Option<NonNull<[u8]>> {
    self.as_raw()
  }
}

impl<T> JsBytes for TypedArray<'_, T> {
  fn raw_bytes(&self) -> Option<NonNull<[u8]>> {
    self.as_raw()
  }
}

/// The element types a typed array can be viewed as; each is plain data
/// that any byte pattern is valid for, which is what `elements` relies on.
pub trait Element: Copy {}

macro_rules! elements {
  ($($t:ty),*) => { $(impl Element for $t {})* };
}

elements!(u8, i8, u16, i16, u32, i32, f32, f64, u64, i64);

/// The buffer's bytes, `None` when it is detached.
pub fn bytes<'a>(buf: &'a impl JsBytes) -> Option<&'a [u8]> {
  // SAFETY: see `JsBytes`; a typed array's view is exactly `len` bytes.
  buf.raw_bytes().map(|raw| unsafe { raw.as_ref() })
}

/// The typed array's elements, `None` when its buffer is detached.
pub fn elements<'a, T: Element>(ta: &'a TypedArray<'_, T>) -> Option<&'a [T]> {
  // SAFETY: see `JsBytes`; a typed array's view starts element-aligned and
  // spans a whole number of elements, and `Element` types accept any bytes.
  bytes(ta).map(|b| unsafe { std::slice::from_raw_parts(b.as_ptr().cast::<T>(), b.len() / std::mem::size_of::<T>()) })
}

/// The typed array's elements for writing (an `out` parameter), `None` when
/// its buffer is detached.
pub fn elements_mut<'a, T: Element>(ta: &'a TypedArray<'_, T>) -> Option<&'a mut [T]> {
  // SAFETY: as for `elements`; the engine hands out no other reference to
  // its bytes while JS is not running.
  ta.raw_bytes().map(|raw| unsafe {
    std::slice::from_raw_parts_mut(raw.as_ptr().cast::<T>(), raw.len() / std::mem::size_of::<T>())
  })
}

/// `bytes`, throwing `"<api>: detached buffer"` for a detached buffer.
pub fn bytes_of<'a>(ctx: &Ctx<'_>, buf: &'a impl JsBytes, api: &str) -> rquickjs::Result<&'a [u8]> {
  bytes(buf).ok_or_else(|| Exception::throw_message(ctx, &format!("{api}: detached buffer")))
}

/// `elements`, throwing `"<api>: detached buffer"` for a detached buffer.
pub fn elements_of<'a, T: Element>(ctx: &Ctx<'_>, ta: &'a TypedArray<'_, T>, api: &str) -> rquickjs::Result<&'a [T]> {
  elements(ta).ok_or_else(|| Exception::throw_message(ctx, &format!("{api}: detached buffer")))
}

/// `elements_mut`, throwing `"<api>: detached buffer"` for a detached buffer.
pub fn elements_mut_of<'a, T: Element>(
  ctx: &Ctx<'_>,
  ta: &'a TypedArray<'_, T>,
  api: &str,
) -> rquickjs::Result<&'a mut [T]> {
  elements_mut(ta).ok_or_else(|| Exception::throw_message(ctx, &format!("{api}: detached buffer")))
}

/// Copy a buffer's bytes out of the engine; empty when the buffer is
/// detached. For bytes that outlive the binding call (a request body, a
/// file write, a subprocess payload).
pub trait CopyBytes {
  fn copy_bytes(&self) -> Vec<u8>;
}

impl<B: JsBytes> CopyBytes for B {
  fn copy_bytes(&self) -> Vec<u8> {
    bytes(self).map(<[u8]>::to_vec).unwrap_or_default()
  }
}

/// `JS_NewArrayBuffer` max_len sentinel for a fixed-length buffer: QuickJS
/// refuses to resize it or transfer it to a different length.
const FIXED_LENGTH: qjs::size_t = 0;

/// Create an ArrayBuffer aliasing external bytes, with NO realloc/free
/// callback: QuickJS never frees, moves or resizes the bytes, on detach or at
/// finalization, and rejects `resize`/length-changing `transfer` on it.
///
/// Not `ArrayBuffer::from_source`, deliberately: that hands ownership of the
/// bytes to the buffer (its source is dropped when the buffer is freed).
/// Here the bytes belong to someone else and outlive the view: the wasm
/// plugin pins the instance in its registry, the gpu plugin's write lease
/// pins the staging block in alloy's Context until end/destroy.
pub fn array_buffer_over<'js>(ctx: &Ctx<'js>, ptr: *mut u8, len: usize) -> rquickjs::Result<ArrayBuffer<'js>> {
  let value = unsafe {
    let raw =
      qjs::JS_NewArrayBuffer(ctx.as_raw().as_ptr(), ptr, len as _, FIXED_LENGTH, None, std::ptr::null_mut(), false);
    Value::from_raw(ctx.clone(), raw)
  };
  if value.is_exception() {
    return Err(rquickjs::Error::Exception);
  }
  ArrayBuffer::from_value(value).ok_or(rquickjs::Error::Unknown)
}
