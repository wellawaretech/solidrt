use fastwebsockets::upgrade::{is_upgrade_request, upgrade as accept_upgrade, UpgradeFut};
use fastwebsockets::OpCode;
use http_body_util::BodyExt;
use hyper::header::{HeaderName, HeaderValue};
use hyper::Request as HyperRequest;
use rquickjs::class::{Trace, Tracer};
use rquickjs::function::{IntoArgs, Opt};
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::{watch, Notify};

use forge::http::ResBody;
use forge::websocket::{
  run_reader, run_writer, SocketSink, Topics, WsDispatch, DEFAULT_BACKPRESSURE_LIMIT, MAX_CONTROL_PAYLOAD,
};
use crate::logger::{format_js_error, Logger};
use crate::pending::PendingOps;
use crate::plugins::body::{extract_body_value, JsBytes};

/// The `websocket` option callbacks of `serve`. One set per server, shared by
/// all sockets. No `ping` callback: incoming pings are answered automatically
/// by the protocol layer and never surface.
pub(crate) struct WsHandlers<'js> {
  open: Option<Function<'js>>,
  message: Option<Function<'js>>,
  drain: Option<Function<'js>>,
  pong: Option<Function<'js>>,
  close: Option<Function<'js>>,
  backpressure_limit: usize,
}

/// Parse the `websocket: { open?, message?, drain?, pong?, close?,
/// backpressureLimit? }` serve option.
pub(crate) fn parse_ws_handlers<'js>(opts: &Object<'js>) -> rquickjs::Result<Option<Rc<WsHandlers<'js>>>> {
  let Some(obj): Option<Object<'js>> = opts.get("websocket")? else {
    return Ok(None);
  };
  let limit: Option<f64> = obj.get("backpressureLimit").ok();
  Ok(Some(Rc::new(WsHandlers {
    open: obj.get("open").ok(),
    message: obj.get("message").ok(),
    drain: obj.get("drain").ok(),
    pong: obj.get("pong").ok(),
    close: obj.get("close").ok(),
    backpressure_limit: limit.map_or(DEFAULT_BACKPRESSURE_LIMIT, |l| l.max(0.0) as usize),
  })))
}

/// A serve Request's upgrade capability, stored on the Request while its handler
/// runs. `server.upgrade(req)` moves it from `Ready` to `Accepted`; serve then
/// sends the held 101 response once the handler returns.
pub(crate) enum ServeUpgrade<'js> {
  /// Not upgraded yet: the hyper upgrade handle taken from the incoming request.
  Ready(hyper::upgrade::OnUpgrade),
  /// Handshake accepted: the 101 response to send, the future resolving to the
  /// raw socket once hyper releases the connection, and the user value destined
  /// for `ws.data`.
  Accepted { response: hyper::Response<ResBody>, socket: UpgradeFut, data: Option<Value<'js>> },
}

unsafe impl<'js> JsLifetime<'js> for ServeUpgrade<'js> {
  type Changed<'to> = ServeUpgrade<'to>;
}

impl<'js> ServeUpgrade<'js> {
  /// Trace the held `data` value (called from the owning Request's Trace impl)
  /// so it survives GC between `upgrade()` and the handler returning.
  pub(crate) fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
    if let ServeUpgrade::Accepted { data: Some(d), .. } = self {
      d.trace(tracer);
    }
  }
}

/// Validate a websocket upgrade and produce the 101 response plus the socket
/// future. The original hyper request was already split into parts by serve, so
/// rebuild a minimal one from the extracted headers (fastwebsockets validates
/// `Sec-WebSocket-Key`/`-Version` from it) and re-attach the upgrade handle
/// where `hyper::upgrade::on` looks for it. `extra_headers` (from
/// `upgrade(req, { headers })`) are appended to the 101; an invalid one fails
/// the upgrade rather than silently dropping it.
pub(crate) fn try_upgrade<'js>(
  headers: &[(String, String)],
  on_upgrade: hyper::upgrade::OnUpgrade,
  extra_headers: &[(String, String)],
  data: Option<Value<'js>>,
) -> Result<ServeUpgrade<'js>, String> {
  let mut builder = HyperRequest::builder();
  for (k, v) in headers {
    builder = builder.header(k.as_str(), v.as_str());
  }
  let mut req = builder.body(()).map_err(|e| format!("invalid headers: {e}"))?;
  if !is_upgrade_request(&req) {
    return Err("not a websocket upgrade request".to_string());
  }
  req.extensions_mut().insert(on_upgrade);
  let (response, socket) = accept_upgrade(&mut req).map_err(|e| e.to_string())?;
  let mut response = response.map(BodyExt::boxed);
  for (k, v) in extra_headers {
    let name = HeaderName::from_bytes(k.as_bytes()).map_err(|e| format!("invalid header name {k}: {e}"))?;
    let value = HeaderValue::from_str(v).map_err(|e| format!("invalid header value for {k}: {e}"))?;
    response.headers_mut().append(name, value);
  }
  Ok(ServeUpgrade::Accepted { response, socket, data })
}

/// Split a message value into its frame opcode and payload bytes: a string
/// sends a text frame, a Uint8Array a binary frame. Shared by send and publish
/// (and the client's send).
pub(crate) fn message_payload<'js>(data: &Value<'js>) -> rquickjs::Result<(OpCode, Vec<u8>)> {
  match data.as_string() {
    Some(s) => Ok((OpCode::Text, s.to_string()?.into_bytes())),
    None => Ok((OpCode::Binary, extract_body_value(data, "ServerWebSocket")?)),
  }
}

/// The per-connection handle passed to the `websocket` callbacks.
#[derive(JsLifetime)]
#[rquickjs::class(rename = "ServerWebSocket")]
pub(crate) struct ServerWebSocket<'js> {
  sink: Rc<SocketSink>,
  /// Wakes the read loop when `close()` starts a close, so it can arm the
  /// close-grace deadline (`notify_one` stores a permit, so no race with a
  /// loop that is not currently waiting).
  closing: Rc<Notify>,
  /// The user value from `upgrade(req, { data })`; undefined when not given.
  data: RefCell<Value<'js>>,
}

impl<'js> Trace<'js> for ServerWebSocket<'js> {
  fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
    self.data.borrow().trace(tracer);
  }
}

impl<'js> ServerWebSocket<'js> {
  /// Extract an optional control-frame payload (string or Uint8Array) and
  /// enforce the RFC 6455 control-frame size limit.
  fn control_payload(ctx: &Ctx<'js>, data: Opt<Value<'js>>) -> rquickjs::Result<Vec<u8>> {
    let payload = match data.0 {
      Some(v) => extract_body_value(&v, "ServerWebSocket")?,
      None => Vec::new(),
    };
    if payload.len() > MAX_CONTROL_PAYLOAD {
      return Err(Exception::throw_message(ctx, "ping/pong payload must be 125 bytes or fewer"));
    }
    Ok(payload)
  }
}

#[rquickjs::methods]
impl<'js> ServerWebSocket<'js> {
  /// Queue a message: a string sends a text frame, a Uint8Array a binary frame.
  /// Returns the bytes queued, 0 if the socket is no longer open, or -1 when
  /// the queue exceeds backpressureLimit (the message is still queued and
  /// `drain` fires once the queue empties).
  pub fn send(&self, data: Value<'js>) -> rquickjs::Result<i32> {
    let (opcode, payload) = message_payload(&data)?;
    Ok(self.sink.enqueue(opcode, payload))
  }

  /// Send a ping control frame (the peer's reply surfaces in the `pong`
  /// callback). Same return values as `send`.
  pub fn ping(&self, ctx: Ctx<'js>, data: Opt<Value<'js>>) -> rquickjs::Result<i32> {
    Ok(self.sink.enqueue(OpCode::Ping, Self::control_payload(&ctx, data)?))
  }

  /// Send an unsolicited pong control frame. Same return values as `send`.
  pub fn pong(&self, ctx: Ctx<'js>, data: Opt<Value<'js>>) -> rquickjs::Result<i32> {
    Ok(self.sink.enqueue(OpCode::Pong, Self::control_payload(&ctx, data)?))
  }

  /// Join a topic; `server.publish(topic)` and peers' `ws.publish(topic)` then
  /// reach this socket. No-op on a closing or closed socket.
  pub fn subscribe(&self, topic: String) {
    self.sink.subscribe(&topic);
  }

  /// Leave a topic. Closing the socket unsubscribes everything automatically.
  pub fn unsubscribe(&self, topic: String) {
    self.sink.unsubscribe(&topic);
  }

  #[qjs(rename = "isSubscribed")]
  pub fn is_subscribed(&self, topic: String) -> bool {
    self.sink.is_subscribed(&topic)
  }

  /// Publish to every subscriber of `topic` except this socket. Returns the
  /// number of sockets the message was queued to.
  pub fn publish(&self, topic: String, data: Value<'js>) -> rquickjs::Result<i32> {
    let (opcode, payload) = message_payload(&data)?;
    Ok(self.sink.publish(&topic, opcode, payload))
  }

  /// Send a close frame (default 1000). The connection finishes once the peer
  /// echoes the close (or the grace period expires).
  pub fn close(&self, code: Opt<u16>, reason: Opt<String>) {
    if self.sink.begin_close(code.0.unwrap_or(1000), reason.0.unwrap_or_default()) {
      self.closing.notify_one();
    }
  }

  #[qjs(get, rename = "readyState")]
  pub fn ready_state(&self) -> u8 {
    self.sink.state()
  }

  #[qjs(get)]
  pub fn data(&self) -> Value<'js> {
    self.data.borrow().clone()
  }

  #[qjs(set, rename = "data")]
  pub fn set_data(&self, value: Value<'js>) {
    *self.data.borrow_mut() = value;
  }
}

/// The marshalling `WsDispatch`: forwards the read/write loops' in-loop
/// callbacks to their JS functions. One per socket, shared (`Rc`) by the reader
/// and writer tasks. The handle it receives is the `ServerWebSocket` class.
struct JsDispatch<'js> {
  ctx: Ctx<'js>,
  handlers: Rc<WsHandlers<'js>>,
  logger: Logger,
}

impl<'js> WsDispatch for JsDispatch<'js> {
  type Handle = Class<'js, ServerWebSocket<'js>>;

  fn on_text(&self, handle: &Self::Handle, text: String) {
    call_callback(&self.ctx, &self.handlers.message, (handle.clone(), text), "message", &self.logger);
  }

  fn on_binary(&self, handle: &Self::Handle, bytes: Vec<u8>) {
    call_callback(&self.ctx, &self.handlers.message, (handle.clone(), JsBytes(bytes)), "message", &self.logger);
  }

  fn on_pong(&self, handle: &Self::Handle, bytes: Vec<u8>) {
    call_callback(&self.ctx, &self.handlers.pong, (handle.clone(), JsBytes(bytes)), "pong", &self.logger);
  }

  fn on_drain(&self, handle: &Self::Handle) {
    call_callback(&self.ctx, &self.handlers.drain, (handle.clone(),), "drain", &self.logger);
  }

  fn on_close(&self, handle: &Self::Handle, code: u16, reason: String) {
    call_callback(&self.ctx, &self.handlers.close, (handle.clone(), code, reason), "close", &self.logger);
  }
}

/// Run an accepted socket: build the handle, fire `open`, spawn the writer task,
/// then drive the read loop (both loops are engine-free, in forge) until the
/// peer closes, errors, or the server shuts down. Holds a pending op per task so
/// the runtime stays alive while the socket is open. Runs on the JS executor:
/// the callbacks are JS functions.
pub(crate) fn spawn_socket<'js>(
  ctx: &Ctx<'js>,
  socket: UpgradeFut,
  handlers: Rc<WsHandlers<'js>>,
  shutdown_rx: watch::Receiver<bool>,
  logger: Logger,
  data: Option<Value<'js>>,
  topics: Topics,
) {
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  pending.hold();
  let ctx2 = ctx.clone();
  ctx.spawn(async move {
    run_socket(ctx2, socket, handlers, shutdown_rx, logger, &pending, data, topics).await;
    pending.release();
  });
}

#[allow(clippy::too_many_arguments)]
async fn run_socket<'js>(
  ctx: Ctx<'js>,
  socket: UpgradeFut,
  handlers: Rc<WsHandlers<'js>>,
  shutdown_rx: watch::Receiver<bool>,
  logger: Logger,
  pending: &PendingOps,
  data: Option<Value<'js>>,
  topics: Topics,
) {
  let ws = match socket.await {
    Ok(ws) => ws,
    Err(e) => {
      logger.warn(&format!("[flux] websocket upgrade failed: {e}"));
      return;
    }
  };
  let (read_half, write_half) = ws.split(tokio::io::split);
  let (sink, rx) = SocketSink::new(topics, handlers.backpressure_limit);
  let close_notify = Rc::new(Notify::new());

  let socket_handle = ServerWebSocket {
    sink: sink.clone(),
    closing: close_notify.clone(),
    data: RefCell::new(data.unwrap_or_else(|| Value::new_undefined(ctx.clone()))),
  };
  let handle = match Class::instance(ctx.clone(), socket_handle) {
    Ok(c) => c,
    Err(e) => {
      logger.warn(&format!("[flux] websocket: could not create socket handle: {e}"));
      return;
    }
  };

  let dispatch = Rc::new(JsDispatch { ctx: ctx.clone(), handlers: handlers.clone(), logger: logger.clone() });

  call_callback(&ctx, &handlers.open, (handle.clone(),), "open", &logger);

  // The writer is its own task so the reader's close-grace deadline can end the
  // connection (and fire `close`) even if a wedged peer stalls writes.
  pending.hold();
  let writer_dispatch = dispatch.clone();
  let writer_handle = handle.clone();
  let writer_sink = sink.clone();
  let writer_logger = logger.clone();
  let writer_pending = pending.clone();
  ctx.spawn(async move {
    run_writer(write_half, rx, writer_sink, &*writer_dispatch, &writer_handle, &writer_logger).await;
    writer_pending.release();
  });

  run_reader(read_half, sink, close_notify, shutdown_rx, &*dispatch, &handle, &logger).await;
}

/// Invoke an optional websocket callback, logging a throw instead of
/// propagating it (there is no JS caller to propagate to).
pub(crate) fn call_callback<'js, A: IntoArgs<'js>>(
  ctx: &Ctx<'js>,
  f: &Option<Function<'js>>,
  args: A,
  what: &str,
  logger: &Logger,
) {
  let Some(f) = f else {
    return;
  };
  if let Err(e) = f.call::<A, Value<'js>>(args) {
    logger.warn(&format!("[flux] websocket {what} handler error: {}", format_js_error(ctx, e)));
  }
}