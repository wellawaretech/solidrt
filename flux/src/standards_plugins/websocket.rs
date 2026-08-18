use bytes::Bytes;
use fastwebsockets::{handshake, FragmentCollectorRead, Frame, OpCode, WebSocketError, WebSocketWrite};
use http_body_util::Empty;
use hyper::header::{CONNECTION, HOST, UPGRADE};
use hyper::Request as HyperRequest;
use rquickjs::class::{Trace, Tracer};
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use tokio::io::AsyncWrite;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Notify};

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::marshal::OptArg;
use crate::forge_plugins::websocket::{call_callback, message_payload};
use crate::standards_plugins::body::JsBytes;
use forge::websocket::{parse_close, OutMsg, CLOSE_GRACE};

/// Web-standard readyState values. Unlike a server socket, a client starts in
/// CONNECTING while the TCP connect and handshake are in flight.
const CONNECTING: u8 = 0;
const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 3;

/// RFC 6455: a close reason must fit in the close frame with its 2-byte code.
const MAX_CLOSE_REASON: usize = 123;

/// The web-standard event handler properties. Stored behind a RefCell so the
/// connection task reads whatever is assigned at the moment an event fires.
#[derive(Default)]
struct Handlers<'js> {
  open: Option<Function<'js>>,
  message: Option<Function<'js>>,
  error: Option<Function<'js>>,
  close: Option<Function<'js>>,
}

/// State shared between the JS handle and the connection task.
struct WsShared<'js> {
  state: Cell<u8>,
  /// Writer queue, populated once the handshake completes.
  tx: RefCell<Option<mpsc::UnboundedSender<OutMsg>>>,
  /// Wakes the connection task when `close()` starts a close: during CONNECTING
  /// it aborts the attempt, after that it arms the close-grace deadline
  /// (`notify_one` stores a permit, so no race with a task not yet waiting).
  closing: Notify,
  handlers: RefCell<Handlers<'js>>,
}

unsafe impl<'js> JsLifetime<'js> for WsShared<'js> {
  type Changed<'to> = WsShared<'to>;
}

/// The web-standard `WebSocket` client global. Stage 1: `ws://` only, handler
/// properties (no addEventListener), plain-object events.
#[rquickjs::class(rename = "WebSocket")]
pub(crate) struct WebSocket<'js> {
  shared: Rc<WsShared<'js>>,
  url: String,
}

unsafe impl<'js> JsLifetime<'js> for WebSocket<'js> {
  type Changed<'to> = WebSocket<'to>;
}

impl<'js> Trace<'js> for WebSocket<'js> {
  fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
    let h = self.shared.handlers.borrow();
    for f in [&h.open, &h.message, &h.error, &h.close].into_iter().flatten() {
      f.trace(tracer);
    }
  }
}

/// A handler property value for JS: the function, or null when unset.
fn handler_value<'js>(ctx: &Ctx<'js>, f: &Option<Function<'js>>) -> Value<'js> {
  f.clone().map_or_else(|| Value::new_null(ctx.clone()), Function::into_value)
}

#[rquickjs::methods]
impl<'js> WebSocket<'js> {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>, url: String) -> rquickjs::Result<Self> {
    let (host, port, path) = parse_ws_url(&ctx, &url)?;
    let shared = Rc::new(WsShared {
      state: Cell::new(CONNECTING),
      tx: RefCell::new(None),
      closing: Notify::new(),
      handlers: RefCell::new(Handlers::default()),
    });
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    pending.hold();
    let logger = ctx.logger();
    let task_shared = shared.clone();
    let ctx2 = ctx.clone();
    ctx.spawn(async move {
      run_client(ctx2, task_shared, host, port, path, &logger).await;
      pending.release();
    });
    Ok(WebSocket { shared, url })
  }

  /// Queue a message: a string sends a text frame, a Uint8Array a binary frame.
  /// Throws while CONNECTING; a send on a closing or closed socket is dropped.
  pub fn send(&self, ctx: Ctx<'js>, data: Value<'js>) -> rquickjs::Result<()> {
    match self.shared.state.get() {
      CONNECTING => Err(Exception::throw_message(&ctx, "WebSocket is still in CONNECTING state")),
      OPEN => {
        let (opcode, payload) = message_payload(&data)?;
        if let Some(tx) = &*self.shared.tx.borrow() {
          let _ = tx.send(OutMsg::Frame(opcode, payload));
        }
        Ok(())
      }
      _ => Ok(()),
    }
  }

  /// Start the closing handshake (default code 1000). During CONNECTING this
  /// aborts the attempt; the close event then reports an unclean 1006.
  pub fn close(&self, ctx: Ctx<'js>, code: OptArg<u16>, reason: OptArg<String>) -> rquickjs::Result<()> {
    if let Some(c) = code.0 {
      if c != 1000 && !(3000..=4999).contains(&c) {
        return Err(Exception::throw_message(&ctx, "close code must be 1000 or in the range 3000-4999"));
      }
    }
    let reason = reason.0.unwrap_or_default();
    if reason.len() > MAX_CLOSE_REASON {
      return Err(Exception::throw_message(&ctx, "close reason must be 123 bytes or fewer"));
    }
    let state = self.shared.state.get();
    if state >= CLOSING {
      return Ok(());
    }
    self.shared.state.set(CLOSING);
    if state == OPEN {
      if let Some(tx) = &*self.shared.tx.borrow() {
        let _ = tx.send(OutMsg::Close(code.0.unwrap_or(1000), reason));
      }
    }
    self.shared.closing.notify_one();
    Ok(())
  }

  #[qjs(get, rename = "readyState")]
  pub fn ready_state(&self) -> u8 {
    self.shared.state.get()
  }

  #[qjs(get)]
  pub fn url(&self) -> String {
    self.url.clone()
  }

  #[qjs(get, rename = "onopen")]
  pub fn onopen(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.shared.handlers.borrow().open)
  }

  #[qjs(set, rename = "onopen")]
  pub fn set_onopen(&self, value: Value<'js>) {
    self.shared.handlers.borrow_mut().open = value.into_function();
  }

  #[qjs(get, rename = "onmessage")]
  pub fn onmessage(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.shared.handlers.borrow().message)
  }

  #[qjs(set, rename = "onmessage")]
  pub fn set_onmessage(&self, value: Value<'js>) {
    self.shared.handlers.borrow_mut().message = value.into_function();
  }

  #[qjs(get, rename = "onerror")]
  pub fn onerror(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.shared.handlers.borrow().error)
  }

  #[qjs(set, rename = "onerror")]
  pub fn set_onerror(&self, value: Value<'js>) {
    self.shared.handlers.borrow_mut().error = value.into_function();
  }

  #[qjs(get, rename = "onclose")]
  pub fn onclose(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.shared.handlers.borrow().close)
  }

  #[qjs(set, rename = "onclose")]
  pub fn set_onclose(&self, value: Value<'js>) {
    self.shared.handlers.borrow_mut().close = value.into_function();
  }
}

/// Parse a ws:// URL into (host, port, path-and-query). wss:// is recognized
/// but rejected until TLS support lands.
fn parse_ws_url(ctx: &Ctx<'_>, url: &str) -> rquickjs::Result<(String, u16, String)> {
  let throw = |msg: &str| Err(Exception::throw_message(ctx, msg));
  if url.starts_with("wss://") {
    return throw("wss:// is not supported yet (no TLS)");
  }
  let Some(rest) = url.strip_prefix("ws://") else {
    return throw("WebSocket URL must start with ws://");
  };
  if rest.contains('#') {
    return throw("WebSocket URL must not contain a fragment");
  }
  let (authority, path) = match rest.find(['/', '?']) {
    Some(i) if rest.as_bytes()[i] == b'?' => (&rest[..i], format!("/{}", &rest[i..])),
    Some(i) => (&rest[..i], rest[i..].to_string()),
    None => (rest, "/".to_string()),
  };
  // Split an optional port; a bracketed IPv6 host keeps its colons.
  let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
    let Some(end) = v6.find(']') else {
      return throw("invalid WebSocket URL: unterminated IPv6 host");
    };
    match &v6[end + 1..] {
      "" => (&v6[..end], None),
      p => match p.strip_prefix(':') {
        Some(p) => (&v6[..end], Some(p)),
        None => return throw("invalid WebSocket URL authority"),
      },
    }
  } else {
    match authority.rsplit_once(':') {
      Some((h, p)) => (h, Some(p)),
      None => (authority, None),
    }
  };
  if host.is_empty() {
    return throw("WebSocket URL is missing a host");
  }
  let port = match port {
    Some(p) => match p.parse::<u16>() {
      Ok(p) => p,
      Err(_) => return throw("invalid WebSocket URL port"),
    },
    None => 80,
  };
  Ok((host.to_string(), port, path))
}

/// Ties hyper's connection task to the tokio runtime (the future is Send, so
/// this works on both current-thread and multi-thread runtimes).
struct SpawnExecutor;

impl<Fut> hyper::rt::Executor<Fut> for SpawnExecutor
where
  Fut: Future + Send + 'static,
  Fut::Output: Send + 'static,
{
  fn execute(&self, fut: Fut) {
    tokio::task::spawn(fut);
  }
}

/// Build a `{ type }` event object and pass it to `build` for event-specific
/// fields, then invoke the handler with it. Event-building failures are logged,
/// not propagated (there is no JS caller).
fn fire_event<'js>(
  ctx: &Ctx<'js>,
  handler: &Option<Function<'js>>,
  what: &str,
  build: impl FnOnce(&Object<'js>) -> rquickjs::Result<()>,
  logger: &Logger,
) {
  if handler.is_none() {
    return;
  }
  let event = Object::new(ctx.clone()).and_then(|o| {
    o.set("type", what)?;
    build(&o)?;
    Ok(o)
  });
  match event {
    Ok(event) => call_callback(ctx, handler, (event,), what, logger),
    Err(e) => logger.warn(&format!("[flux] websocket: could not build {what} event: {e}")),
  }
}

/// Fire `error` (with a message) and then `close`, the order the web standard
/// prescribes for a connection that failed.
fn fire_error_close<'js>(ctx: &Ctx<'js>, shared: &WsShared<'js>, message: &str, logger: &Logger) {
  let error = shared.handlers.borrow().error.clone();
  fire_event(ctx, &error, "error", |o| o.set("message", message), logger);
  fire_close(ctx, shared, 1006, String::new(), false, logger);
}

fn fire_close<'js>(
  ctx: &Ctx<'js>,
  shared: &WsShared<'js>,
  code: u16,
  reason: String,
  was_clean: bool,
  logger: &Logger,
) {
  let close = shared.handlers.borrow().close.clone();
  fire_event(
    ctx,
    &close,
    "close",
    |o| {
      o.set("code", code)?;
      o.set("reason", reason)?;
      o.set("wasClean", was_clean)
    },
    logger,
  );
}

/// Connect, handshake, then drive the socket until it closes: the client-side
/// mirror of the server's run_socket. Runs on the JS executor (the handlers are
/// JS functions); the constructor holds a pending op for its whole lifetime.
async fn run_client<'js>(
  ctx: Ctx<'js>,
  shared: Rc<WsShared<'js>>,
  host: String,
  port: u16,
  path: String,
  logger: &Logger,
) {
  // IPv6 hosts get their brackets back for the Host header.
  let authority = if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
  let connect = async {
    let stream = TcpStream::connect((host.as_str(), port)).await.map_err(|e| e.to_string())?;
    let req = HyperRequest::builder()
      .method("GET")
      .uri(&path)
      .header(HOST, &authority)
      .header(UPGRADE, "websocket")
      .header(CONNECTION, "upgrade")
      .header("Sec-WebSocket-Key", handshake::generate_key())
      .header("Sec-WebSocket-Version", "13")
      .body(Empty::<Bytes>::new())
      .map_err(|e| e.to_string())?;
    handshake::client(&SpawnExecutor, req, stream).await.map_err(|e| e.to_string())
  };
  tokio::pin!(connect);

  let ws = tokio::select! {
    res = &mut connect => match res {
      Ok((ws, _response)) => ws,
      Err(e) => {
        shared.state.set(CLOSED);
        logger.warn(&format!("[flux] websocket connect failed: {e}"));
        fire_error_close(&ctx, &shared, &format!("connect failed: {e}"), logger);
        return;
      }
    },
    // close() during CONNECTING aborts the attempt.
    _ = shared.closing.notified() => {
      shared.state.set(CLOSED);
      fire_close(&ctx, &shared, 1006, String::new(), false, logger);
      return;
    }
  };

  let (read_half, write_half) = ws.split(tokio::io::split);
  let mut reader = FragmentCollectorRead::new(read_half);
  let (tx, rx) = mpsc::unbounded_channel::<OutMsg>();
  *shared.tx.borrow_mut() = Some(tx.clone());
  shared.state.set(OPEN);

  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  pending.hold();
  let writer_logger = logger.clone();
  ctx.spawn(async move {
    run_writer(write_half, rx, &writer_logger).await;
    pending.release();
  });

  let open = shared.handlers.borrow().open.clone();
  fire_event(&ctx, &open, "open", |_| Ok(()), logger);

  // Forward the read half's obligated sends (pong replies, close echoes) to the
  // writer. A send error means the writer is gone, which ends the read loop.
  let obligated_tx = tx.clone();
  let mut send_obligated = move |frame: Frame<'_>| {
    let res = obligated_tx
      .send(OutMsg::Frame(frame.opcode, frame.payload.into()))
      .map_err(|_| WebSocketError::ConnectionClosed);
    std::future::ready(res)
  };

  // (code, reason) reported to the close event; wasClean stays false unless the
  // peer completes the closing handshake with a close frame.
  let mut close_info = (1006u16, String::new());
  let mut was_clean = false;
  let mut read_error: Option<String> = None;
  let grace = tokio::time::sleep(CLOSE_GRACE);
  tokio::pin!(grace);
  let mut closing = false;
  loop {
    tokio::select! {
      frame = reader.read_frame(&mut send_obligated) => {
        let frame = match frame {
          Ok(f) => f,
          Err(e) => {
            if !matches!(e, WebSocketError::ConnectionClosed | WebSocketError::UnexpectedEOF) {
              logger.warn(&format!("[flux] websocket read error: {e}"));
            }
            read_error = Some(e.to_string());
            break;
          }
        };
        match frame.opcode {
          OpCode::Text => {
            let text = String::from_utf8_lossy(&frame.payload).into_owned();
            let message = shared.handlers.borrow().message.clone();
            fire_event(&ctx, &message, "message", |o| o.set("data", text), logger);
          }
          OpCode::Binary => {
            let bytes = JsBytes(frame.payload.into());
            let message = shared.handlers.borrow().message.clone();
            fire_event(&ctx, &message, "message", |o| o.set("data", bytes), logger);
          }
          OpCode::Close => {
            close_info = parse_close(&frame.payload);
            was_clean = true;
            break;
          }
          _ => {}
        }
      }
      // ws.close() was called from JS: arm the grace deadline.
      _ = shared.closing.notified(), if !closing => {
        closing = true;
        grace.as_mut().reset(tokio::time::Instant::now() + CLOSE_GRACE);
      }
      _ = grace.as_mut(), if closing => {
        logger.warn("[flux] websocket close timed out; dropping connection");
        break;
      }
    }
  }

  shared.state.set(CLOSED);
  let _ = tx.send(OutMsg::End);
  shared.tx.borrow_mut().take();
  if let Some(e) = read_error {
    let error = shared.handlers.borrow().error.clone();
    fire_event(&ctx, &error, "error", |o| o.set("message", e), logger);
  }
  let (code, reason) = close_info;
  fire_close(&ctx, &shared, code, reason, was_clean, logger);
}

/// Drain the writer queue onto the socket: the client-side mirror of the
/// server's run_writer, without the backpressure/drain accounting. After a
/// close frame goes out nothing more may be sent; later frames are dropped.
async fn run_writer<W: AsyncWrite + Unpin>(
  mut ws: WebSocketWrite<W>,
  mut rx: mpsc::UnboundedReceiver<OutMsg>,
  logger: &Logger,
) {
  let mut sent_close = false;
  while let Some(msg) = rx.recv().await {
    let res = match msg {
      OutMsg::Frame(opcode, payload) if !sent_close => {
        sent_close = opcode == OpCode::Close;
        ws.write_frame(Frame::new(true, opcode, None, payload.into())).await
      }
      OutMsg::Close(code, reason) if !sent_close => {
        sent_close = true;
        ws.write_frame(Frame::close(code, reason.as_bytes())).await
      }
      OutMsg::End => break,
      _ => Ok(()),
    };
    if let Err(e) = res {
      logger.warn(&format!("[flux] websocket write error: {e}"));
      break;
    }
  }
}

pub(crate) fn init_websocket(ctx: &Ctx<'_>) {
  Class::<WebSocket>::define(&ctx.globals()).expect("define WebSocket class");
  let ctor: Object = ctx.globals().get("WebSocket").expect("WebSocket constructor");
  let proto: Object = ctor.get("prototype").expect("WebSocket prototype");
  for (name, value) in [("CONNECTING", CONNECTING), ("OPEN", OPEN), ("CLOSING", CLOSING), ("CLOSED", CLOSED)] {
    ctor.set(name, value).expect("set WebSocket constant");
    proto.set(name, value).expect("set WebSocket constant");
  }
}
