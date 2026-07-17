use bytes::Bytes;
use rquickjs::{
  function::{MutFn, This},
  promise::{MaybePromise, Promised},
  ArrayBuffer, Ctx, Function, IntoJs, Object, TypedArray, Value,
};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use tokio::sync::mpsc;

use crate::logger::{format_js_error, Logger};
use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, iter_result};

// Re-exported so existing `crate::plugins::standards::body::{ByteStream, to_byte_stream}`
// importers (response, request, serve, subprocess) stay unchanged; the
// engine-free primitive itself now lives in `forge::stream`.
pub(crate) use forge::stream::{to_byte_stream, ByteStream};

/// In-memory body buffer shared by Response and Request. Consume-once semantics:
/// `take` returns the bytes once, then subsequent calls return None.
pub(crate) struct BodyState {
  bytes: RefCell<Option<Vec<u8>>>,
}

impl BodyState {
  pub(crate) fn new(bytes: Vec<u8>) -> Self {
    Self { bytes: RefCell::new(Some(bytes)) }
  }

  /// Consume the bytes. Returns None if already consumed.
  pub(crate) fn take(&self) -> Option<Vec<u8>> {
    self.bytes.borrow_mut().take()
  }
}

/// A streamed message body read from the network (a fetch response, or an
/// incoming server request). Consume-once: the first reader `take`s the stream
/// (drained by text/bytes/json, or iterated by `.body`); later access sees `None`
/// and throws "Body already consumed".
pub(crate) struct IncomingBody {
  stream: Rc<RefCell<Option<ByteStream>>>,
}

impl IncomingBody {
  pub(crate) fn new(stream: ByteStream) -> Self {
    Self { stream: Rc::new(RefCell::new(Some(stream))) }
  }

  pub(crate) fn take(&self) -> Option<ByteStream> {
    self.stream.borrow_mut().take()
  }
}

/// The body of a `Request` or `Response`: either buffered bytes (a JS-constructed
/// body, a server static response) or a live network stream (a fetch response, an
/// incoming server request). Shared so both message types read bodies identically;
/// the only extra case is a `Response`'s outgoing `async function*`, which lives in
/// `Response::stream`, not here.
pub(crate) enum MessageBody {
  Buffered(BodyState),
  Incoming(IncomingBody),
}

impl MessageBody {
  pub(crate) fn buffered(bytes: Vec<u8>) -> Self {
    MessageBody::Buffered(BodyState::new(bytes))
  }

  pub(crate) fn incoming(stream: ByteStream) -> Self {
    MessageBody::Incoming(IncomingBody::new(stream))
  }

  /// Consume the body once into a drainable source for `text`/`bytes`/`json`.
  pub(crate) fn take_source(&self, ctx: &Ctx<'_>) -> rquickjs::Result<BodySource> {
    match self {
      MessageBody::Buffered(state) => state.take().map(BodySource::Bytes).ok_or_else(|| throw_consumed(ctx)),
      MessageBody::Incoming(incoming) => incoming.take().map(BodySource::Stream).ok_or_else(|| throw_consumed(ctx)),
    }
  }

  /// Consume the body once into an async-iterable of Uint8Array chunks (`.body`).
  /// A streamed body iterates the network stream; a buffered one yields its bytes
  /// as a single chunk, so `for await (const c of msg.body)` works uniformly.
  pub(crate) fn as_async_iterable<'js>(&self, ctx: &Ctx<'js>, pending: PendingOps) -> rquickjs::Result<Value<'js>> {
    match self {
      MessageBody::Incoming(incoming) => {
        let stream = incoming.take().ok_or_else(|| throw_consumed(ctx))?;
        Ok(byte_stream_iterable(ctx, stream, pending)?.into_value())
      }
      MessageBody::Buffered(state) => {
        let bytes = state.take().ok_or_else(|| throw_consumed(ctx))?;
        buffered_async_iterable(ctx, bytes)
      }
    }
  }
}

/// The drainable source behind a body reader (`text`/`bytes`/`json`): either
/// already-buffered bytes or a live network stream.
pub(crate) enum BodySource {
  Bytes(Vec<u8>),
  Stream(ByteStream),
}

impl BodySource {
  async fn collect(self, pending: PendingOps) -> Result<Vec<u8>, String> {
    match self {
      BodySource::Bytes(bytes) => Ok(bytes),
      BodySource::Stream(stream) => drain_stream(stream, pending).await,
    }
  }
}

/// Read a byte stream to EOF, concatenating chunks. Holds a pending op across the
/// network reads so the engine stays alive until the body is fully drained.
async fn drain_stream(mut stream: ByteStream, pending: PendingOps) -> Result<Vec<u8>, String> {
  pending.hold();
  let mut buf = Vec::new();
  let mut error = None;
  while let Some(item) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
    match item {
      Ok(chunk) => buf.extend_from_slice(&chunk),
      Err(e) => {
        error = Some(e);
        break;
      }
    }
  }
  pending.release();
  match error {
    Some(e) => Err(e.to_string()),
    None => Ok(buf),
  }
}

async fn collect_text_raw(source: BodySource, pending: PendingOps) -> Result<String, String> {
  let bytes = source.collect(pending).await?;
  String::from_utf8(bytes).map_err(|e| e.to_string())
}

pub(crate) async fn collect_text(source: BodySource, pending: PendingOps) -> JsResult<String> {
  JsResult(collect_text_raw(source, pending).await)
}

pub(crate) async fn collect_bytes(source: BodySource, pending: PendingOps) -> JsResult<JsBytes> {
  JsResult(source.collect(pending).await.map(JsBytes))
}

pub(crate) async fn collect_array_buffer(source: BodySource, pending: PendingOps) -> JsResult<JsArrayBuffer> {
  JsResult(source.collect(pending).await.map(JsArrayBuffer))
}

pub(crate) async fn collect_json(source: BodySource, pending: PendingOps) -> JsResult<JsonValue> {
  JsResult(collect_text_raw(source, pending).await.map(JsonValue))
}

/// One step of a Rust-backed byte async-iterator. Returned owned (not as a JS
/// `Object` tied to a borrowed `Ctx`) so the iterator future stays `'static` and
/// dodges the lifetime tangle of returning a `Ctx`-bound value (mirrors how
/// `attach_body`'s closures return owned `JsBytes`/`JsonValue`).
enum IterStep {
  Chunk(Vec<u8>),
  Done,
}

/// Return type of the iterator's `next()`: a promise resolving to one `IterStep`.
type IterStepFuture = Promised<Pin<Box<dyn Future<Output = JsResult<IterStep>>>>>;

impl<'js> IntoJs<'js> for IterStep {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let value = match self {
      IterStep::Chunk(bytes) => Some(TypedArray::<u8>::new(ctx.clone(), bytes)?.into_value()),
      IterStep::Done => None,
    };
    Ok(iter_result(ctx, value)?.into_value())
  }
}

/// Build a Rust-backed JS async-iterable over a network byte stream. Each `next()`
/// pulls one chunk (a Uint8Array) from `stream`, resolving `{ value, done }`;
/// `[Symbol.asyncIterator]()` returns the object itself, so `for await` works.
/// The structural dual of `pump_async_iterable` (JS-produces -> Rust-consumes):
/// here Rust produces and JS consumes. Pull-based, so the network only advances
/// as JS pulls; a `pending` op is held only across each in-flight read, so an
/// abandoned iterator leaks nothing.
pub(crate) fn byte_stream_iterable<'js>(
  ctx: &Ctx<'js>,
  stream: ByteStream,
  pending: PendingOps,
) -> rquickjs::Result<Object<'js>> {
  let cell = Rc::new(RefCell::new(Some(stream)));
  let iter = Object::new(ctx.clone())?;

  let next_fn = Function::new(
    ctx.clone(),
    MutFn::from(move |_ctx: Ctx<'_>| -> rquickjs::Result<IterStepFuture> {
      let cell = cell.clone();
      let pending = pending.clone();
      Ok(Promised(Box::pin(async move {
        // Take the stream out so no RefCell borrow is held across the await. A
        // concurrent (un-awaited) next() finding it gone just reports done.
        let Some(mut stream) = cell.borrow_mut().take() else {
          return JsResult(Ok(IterStep::Done));
        };
        pending.hold();
        let item = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
        pending.release();
        JsResult(match item {
          Some(Ok(chunk)) => {
            *cell.borrow_mut() = Some(stream);
            Ok(IterStep::Chunk(chunk.to_vec()))
          }
          Some(Err(e)) => Err(e.to_string()),
          None => Ok(IterStep::Done),
        })
      })))
    }),
  )?;
  iter.set("next", next_fn)?;
  attach_async_iterator(ctx, &iter)?;

  Ok(iter)
}

/// Wrap already-buffered bytes in a single-chunk async-iterable, so `response.body`
/// behaves uniformly for buffered and streamed responses. An empty body yields
/// nothing.
pub(crate) fn buffered_async_iterable<'js>(ctx: &Ctx<'js>, bytes: Vec<u8>) -> rquickjs::Result<Value<'js>> {
  let ta = TypedArray::<u8>::new(ctx.clone(), bytes)?;
  let wrap: Function = ctx.eval("(b) => (async function* () { if (b.length) yield b; })()")?;
  wrap.call((ta,))
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
      logger.warn(&format!("[flux] stream: could not get async iterator: {}", format_js_error(&ctx, e)));
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
        logger.warn(&format!("[flux] stream: iterator next() threw: {}", format_js_error(&ctx, e)));
        break;
      }
    };
    let result = match MaybePromise::from_value(step).into_future::<Value<'js>>().await {
      Ok(v) => v,
      Err(e) => {
        logger.warn(&format!("[flux] stream: iterator rejected: {}", format_js_error(&ctx, e)));
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

/// Body bytes surfaced to JS as an `ArrayBuffer` (the standard `arrayBuffer()`
/// result), where `JsBytes` surfaces them as a `Uint8Array`.
pub struct JsArrayBuffer(pub Vec<u8>);

impl<'js> IntoJs<'js> for JsArrayBuffer {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    ArrayBuffer::new(ctx.clone(), self.0).map(|ab| ab.into_value())
  }
}

pub struct JsonValue(pub String);

impl<'js> IntoJs<'js> for JsonValue {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    ctx.json_parse(self.0)
  }
}

pub(crate) fn throw_consumed(ctx: &Ctx<'_>) -> rquickjs::Error {
  throw_msg(ctx, "Body already consumed")
}

pub(crate) fn throw_msg(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

/// Attach text(), bytes(), json() methods to obj.
///
/// fetch_bytes is invoked lazily on each method call to obtain the body bytes.
/// Its error is a plain message string, surfaced to JS as a clean `Error`.
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
  Fut: Future<Output = Result<Vec<u8>, String>> + 'static,
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
          JsResult(match fetch().await {
            Ok(bytes) => String::from_utf8(bytes).map_err(|e| e.to_string()),
            Err(msg) => Err(msg),
          })
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
        Ok(Promised(async move { JsResult(fetch().await.map(JsBytes)) }))
      }
    }),
  )
  .expect("create bytes function");

  let array_buffer_fn = Function::new(
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
        Ok(Promised(async move { JsResult(fetch().await.map(JsArrayBuffer)) }))
      }
    }),
  )
  .expect("create arrayBuffer function");

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
          JsResult(match fetch().await {
            Ok(bytes) => String::from_utf8(bytes).map(JsonValue).map_err(|e| e.to_string()),
            Err(msg) => Err(msg),
          })
        }))
      }
    }),
  )
  .expect("create json function");

  obj.set("text", text_fn)?;
  obj.set("bytes", bytes_fn)?;
  obj.set("arrayBuffer", array_buffer_fn)?;
  obj.set("json", json_fn)?;

  Ok(())
}
