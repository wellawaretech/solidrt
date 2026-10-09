use rquickjs::{
  atom::PredefinedAtom,
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
use crate::pending::{Hold, PendingOps};
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, mark_observed, CopyBytes, Step};
use crate::standards_plugins::streams::readable_from;

// Re-exported so the `crate::standards_plugins::body::ByteStream` importers
// (response, request) stay unchanged; the engine-free primitive itself lives
// in `forge::stream`.
pub(crate) use forge::stream::ByteStream;

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

  /// Consume the body once into a `ReadableStream` of Uint8Array chunks
  /// (`.body`). A streamed body reads the network stream; a buffered one
  /// yields its bytes as a single chunk, so `for await (const c of msg.body)`
  /// works uniformly.
  pub(crate) fn as_readable<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    let stream = match self {
      MessageBody::Incoming(incoming) => incoming.take().ok_or_else(|| throw_consumed(ctx))?,
      MessageBody::Buffered(state) => forge::stream::from_bytes(state.take().ok_or_else(|| throw_consumed(ctx))?),
    };
    let pending = PendingOps::of(ctx);
    byte_stream_readable(ctx, stream, move || pending.in_flight("body read"))
  }

  /// Consume the body once into a read of all its bytes, for
  /// `text`/`bytes`/`arrayBuffer`/`json`: the read holds the engine as work
  /// in flight until the body is drained.
  fn read_all<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<BodyRead<'js>> {
    let source = self.take_source(ctx)?;
    let hold = PendingOps::of(ctx).in_flight("body read");
    Ok(Box::pin(source.collect(hold)))
  }
}

/// The `.body` of a Request or Response: the stream it was constructed with
/// or already handed out (`stream`), the same object on every read as on the
/// web; else one over its bytes, kept in `stream` for the next read.
pub(crate) fn body_stream<'js>(
  ctx: &Ctx<'js>,
  body: &MessageBody,
  stream: &RefCell<Option<Object<'js>>>,
) -> rquickjs::Result<Object<'js>> {
  if let Some(stream) = &*stream.borrow() {
    return Ok(stream.clone());
  }
  let readable = body.as_readable(ctx)?;
  *stream.borrow_mut() = Some(readable.clone());
  Ok(readable)
}

/// Consume a Request's or Response's body once for a reader
/// (`text`/`bytes`/`arrayBuffer`/`json`). A stream (constructed with, or
/// handed out as `.body`) is taken and drained through the engine; the read
/// holds the engine as work in flight until the body is done.
pub(crate) fn body_read_all<'js>(
  ctx: &Ctx<'js>,
  body: &MessageBody,
  stream: &RefCell<Option<Object<'js>>>,
) -> rquickjs::Result<BodyRead<'js>> {
  let Some(stream) = stream.borrow_mut().take() else {
    return body.read_all(ctx);
  };
  // Whatever the stream left in the body is its own (it was built over the
  // bytes, or the bytes are empty beside a constructed stream): taken, so a
  // later read sees the body consumed.
  let _ = body.take_source(ctx);
  let hold = PendingOps::of(ctx).in_flight("body read");
  let ctx = ctx.clone();
  Ok(Box::pin(async move {
    let bytes = drain_async_iterable(&ctx, stream).await;
    drop(hold);
    bytes
  }))
}

/// The drainable source behind a body reader (`text`/`bytes`/`json`): either
/// already-buffered bytes or a live network stream.
pub(crate) enum BodySource {
  Bytes(Vec<u8>),
  Stream(ByteStream),
}

impl BodySource {
  async fn collect(self, hold: Hold) -> Result<Vec<u8>, String> {
    match self {
      BodySource::Bytes(bytes) => Ok(bytes),
      BodySource::Stream(stream) => drain_stream(stream, hold).await,
    }
  }
}

/// Read a byte stream to EOF, concatenating chunks. `hold` is the read's hold
/// on the engine, taken where the read was asked for and kept until the body
/// is fully drained.
async fn drain_stream(mut stream: ByteStream, hold: Hold) -> Result<Vec<u8>, String> {
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
  drop(hold);
  match error {
    Some(e) => Err(e.to_string()),
    None => Ok(buf),
  }
}

/// A read of a whole body, behind `text`/`bytes`/`arrayBuffer`/`json`:
/// `'js`-bound because a JS stream body is drained through the engine.
pub(crate) type BodyRead<'js> = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + 'js>>;

async fn read_text(read: BodyRead<'_>) -> Result<String, String> {
  let bytes = read.await?;
  String::from_utf8(bytes).map_err(|e| e.to_string())
}

pub(crate) async fn collect_text(read: BodyRead<'_>) -> JsResult<String> {
  JsResult(read_text(read).await)
}

pub(crate) async fn collect_bytes(read: BodyRead<'_>) -> JsResult<JsBytes> {
  JsResult(read.await.map(JsBytes))
}

pub(crate) async fn collect_array_buffer(read: BodyRead<'_>) -> JsResult<JsArrayBuffer> {
  JsResult(read.await.map(JsArrayBuffer))
}

pub(crate) async fn collect_json(read: BodyRead<'_>) -> JsResult<JsonValue> {
  JsResult(read_text(read).await.map(JsonValue))
}

/// Return type of the iterator's `next()`: a promise resolving to one step.
type IterStepFuture = Promised<Pin<Box<dyn Future<Output = JsResult<Step<JsBytes>>>>>>;

/// A `ReadableStream` over a native byte stream: `byte_stream_iterable`
/// wrapped by `ReadableStream.from`, so the stream pulls one chunk per read
/// and cancelling it drops the native stream. What `.body` and a child's
/// `stdout`/`stderr` hand to JS.
pub(crate) fn byte_stream_readable<'js>(
  ctx: &Ctx<'js>,
  stream: ByteStream,
  hold: impl Fn() -> Hold + 'static,
) -> rquickjs::Result<Object<'js>> {
  let iter = byte_stream_iterable(ctx, stream, hold)?;
  readable_from(ctx, iter)
}

/// Build a Rust-backed JS async-iterator over a network byte stream, the
/// underlying source of `byte_stream_readable`. Each `next()` pulls one chunk
/// (a Uint8Array) from `stream`, resolving `{ value, done }`; `return()` drops
/// the stream, so a consumer that stops early (a `break`, a stream cancel)
/// releases the socket or pipe behind it now, not at GC; and
/// `[Symbol.asyncIterator]()` returns the object itself. The structural dual
/// of `drive_async_iterable` (JS-produces -> Rust-consumes): here Rust
/// produces and JS consumes. Pull-based, so the network only advances as JS
/// pulls; `hold` is taken for each read and released when it lands, so an
/// abandoned iterator holds nothing. The caller decides its class: a
/// response body is work in flight, a running child's output is standing.
fn byte_stream_iterable<'js>(
  ctx: &Ctx<'js>,
  stream: ByteStream,
  hold: impl Fn() -> Hold + 'static,
) -> rquickjs::Result<Object<'js>> {
  let cell = Rc::new(RefCell::new(Some(stream)));
  // Set by `return()`: a `next()` in flight at that moment drops the stream
  // when its chunk lands instead of putting it back.
  let closed = Rc::new(Cell::new(false));
  let iter = Object::new(ctx.clone())?;

  let next_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let cell = cell.clone();
      let closed = closed.clone();
      move |_ctx: Ctx<'_>| -> rquickjs::Result<IterStepFuture> {
        let cell = cell.clone();
        let closed = closed.clone();
        let hold = hold();
        Ok(Promised(Box::pin(async move {
          // Take the stream out so no RefCell borrow is held across the await. A
          // concurrent (un-awaited) next() finding it gone just reports done.
          let Some(mut stream) = cell.borrow_mut().take() else {
            return JsResult(Ok(Step(None)));
          };
          let item = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
          drop(hold);
          JsResult(match item {
            Some(Ok(chunk)) => {
              if !closed.get() {
                *cell.borrow_mut() = Some(stream);
              }
              Ok(Step(Some(JsBytes(chunk.to_vec()))))
            }
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(Step(None)),
          })
        })))
      }
    }),
  )?;
  iter.set("next", next_fn)?;

  let return_fn = Function::new(
    ctx.clone(),
    MutFn::from(move |_ctx: Ctx<'_>| -> rquickjs::Result<Step<JsBytes>> {
      closed.set(true);
      cell.borrow_mut().take();
      Ok(Step(None))
    }),
  )?;
  iter.set("return", return_fn)?;
  attach_async_iterator(ctx, &iter)?;

  Ok(iter)
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
    return Ok(ta.copy_bytes());
  }
  Err(rquickjs::Error::new_from_js_message("body", for_class, "must be string, Uint8Array, null, or undefined"))
}

/// True if `val` is an async-iterable (has a `Symbol.asyncIterator` method), e.g.
/// the object an `async function*` generator returns. Primitives (strings, null,
/// numbers) are not objects, so they are false without a property read.
pub(crate) fn is_async_iterable<'js>(val: &Value<'js>) -> rquickjs::Result<bool> {
  let Some(obj) = val.as_object() else {
    return Ok(false);
  };
  let method: Value<'js> = obj.get(PredefinedAtom::SymbolAsyncIterator)?;
  Ok(method.is_function())
}

/// Parse a Response body value into either buffered bytes or, when it is an
/// async-iterable, a `ReadableStream` to be drained later (the value itself
/// when it is one, else `ReadableStream.from` over it, so `.body` is one
/// shape whatever was given). Otherwise falls back to the buffered
/// `extract_body_value` rules (string, Uint8Array, null/undefined).
pub(crate) fn extract_streaming_body<'js>(val: &Value<'js>) -> rquickjs::Result<(Vec<u8>, Option<Object<'js>>)> {
  if is_async_iterable(val)? {
    let obj = val.clone().into_object().expect("async iterable is an object");
    return Ok((Vec::new(), Some(readable_from(val.ctx(), obj)?)));
  }
  Ok((extract_body_value(val, "Response")?, None))
}

/// Drive a JS async-iterable body chunk by chunk, handing the bytes of each
/// non-empty chunk (a string or Uint8Array) to `sink`, until the iterator is
/// done or `sink` returns false (the consumer is gone). When the drive stops
/// before the end the iterator's `return()` is called, so a `ReadableStream`
/// behind it is cancelled rather than left locked. `Err` carries the message
/// of a throw, a rejection, or a chunk of the wrong type.
///
/// Touches JS values, so it runs on the QuickJS executor (a `ctx.spawn` task
/// or a `Promised` future).
async fn drive_async_iterable<'js>(
  ctx: &Ctx<'js>,
  iterable: Object<'js>,
  mut sink: impl AsyncFnMut(Vec<u8>) -> bool,
) -> Result<(), String> {
  let get_iter: Function<'js> = iterable
    .get(PredefinedAtom::SymbolAsyncIterator)
    .map_err(|e| format!("body is not async-iterable: {}", format_js_error(ctx, e)))?;
  let iter: Object<'js> = get_iter
    .call((This(iterable),))
    .map_err(|e| format!("could not get async iterator: {}", format_js_error(ctx, e)))?;
  let next: Function<'js> = iter.get("next").map_err(|e| format!("iterator has no next(): {e}"))?;

  loop {
    let step: Value<'js> =
      next.call((This(iter.clone()),)).map_err(|e| format!("iterator next() threw: {}", format_js_error(ctx, e)))?;
    mark_observed(&step);
    let result = MaybePromise::from_value(step)
      .into_future::<Value<'js>>()
      .await
      .map_err(|e| format!("iterator rejected: {}", format_js_error(ctx, e)))?;
    let Some(obj) = result.into_object() else {
      return Err("iterator result was not an object".to_string());
    };
    if obj.get("done").unwrap_or(true) {
      return Ok(());
    }
    let value: Value<'js> = obj.get("value").map_err(|e| format!("could not read chunk value: {e}"))?;
    let chunk = match extract_body_value(&value, "stream chunk") {
      Ok(b) => b,
      Err(e) => {
        return_iterator(ctx, &iter);
        return Err(format!("chunk must be a string or Uint8Array: {e}"));
      }
    };
    if chunk.is_empty() {
      continue;
    }
    if !sink(chunk).await {
      return_iterator(ctx, &iter);
      return Ok(());
    }
  }
}

/// Call the iterator's `return()`, if it has one, for a drive that stops
/// before the end. Its outcome is nobody's to observe: a rejection is
/// marked handled, a throw is taken off the context.
fn return_iterator<'js>(ctx: &Ctx<'js>, iter: &Object<'js>) {
  let Ok(Some(ret)) = iter.get::<_, Option<Function<'js>>>("return") else {
    return;
  };
  match ret.call::<_, Value<'js>>((This(iter.clone()),)) {
    Ok(result) => mark_observed(&result),
    Err(_) => {
      let _ = ctx.catch();
    }
  }
}

/// Drive a JS async-iterable body into `tx`, chunk by chunk, until the
/// iterator is done, an error occurs, or the consumer drops the receiver.
/// What the HTTP server's streamed responses and a streamed fetch request
/// body run under `ctx.spawn`; an error is logged, since no caller is left
/// to receive it.
pub(crate) async fn pump_async_iterable<'js>(
  ctx: Ctx<'js>,
  iterable: Object<'js>,
  tx: mpsc::Sender<Vec<u8>>,
  logger: Logger,
) {
  // A send error means the consumer is gone (e.g. the connection closed).
  let drive = drive_async_iterable(&ctx, iterable, async |chunk| tx.send(chunk).await.is_ok());
  if let Err(message) = drive.await {
    logger.warn(&format!("[flux] stream: {message}"));
  }
}

/// Read a JS async-iterable body to its end, concatenating the chunks: what
/// `text`/`bytes`/`arrayBuffer`/`json` do on a stream-bodied Response.
pub(crate) async fn drain_async_iterable<'js>(ctx: &Ctx<'js>, iterable: Object<'js>) -> Result<Vec<u8>, String> {
  let mut buf = Vec::new();
  drive_async_iterable(ctx, iterable, async |chunk| {
    buf.extend_from_slice(&chunk);
    true
  })
  .await?;
  Ok(buf)
}

pub struct JsBytes(pub Vec<u8>);

impl<'js> IntoJs<'js> for JsBytes {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    crate::plugins::value::bytes_into_js(ctx, self.0)
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
        // Started here, not in the promise's future: what it holds is held
        // from the call on.
        let fetch = fetch_bytes();
        Ok(Promised(async move {
          JsResult(match fetch.await {
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
        // Started here, not in the promise's future: what it holds is held
        // from the call on.
        let fetch = fetch_bytes();
        Ok(Promised(async move { JsResult(fetch.await.map(JsBytes)) }))
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
        // Started here, not in the promise's future: what it holds is held
        // from the call on.
        let fetch = fetch_bytes();
        Ok(Promised(async move { JsResult(fetch.await.map(JsArrayBuffer)) }))
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
        // Started here, not in the promise's future: what it holds is held
        // from the call on.
        let fetch = fetch_bytes();
        Ok(Promised(async move {
          JsResult(match fetch.await {
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
