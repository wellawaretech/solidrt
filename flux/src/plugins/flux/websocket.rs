use fastwebsockets::upgrade::{is_upgrade_request, upgrade as accept_upgrade, UpgradeFut};
use fastwebsockets::{FragmentCollectorRead, Frame, OpCode, WebSocketError, WebSocketWrite};
use http_body_util::BodyExt;
use hyper::Request as HyperRequest;
use rquickjs::class::Trace;
use rquickjs::function::{IntoArgs, Opt};
use rquickjs::{Class, Ctx, Function, JsLifetime, Object, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use tokio::io::AsyncWrite;
use tokio::sync::{mpsc, watch, Notify};

use crate::logger::{format_js_error, Logger};
use crate::pending::PendingOps;
use crate::plugins::body::{extract_body_value, JsBytes};
use crate::plugins::flux::serve::{wait_for_stop, ResBody};

/// Web-standard readyState values. A server socket is born OPEN (the class is
/// only created after the handshake), so there is no CONNECTING state here.
const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 3;

/// How long a closing socket waits for the peer's close echo (or remaining
/// frames) before giving up and dropping the connection, so a dead peer cannot
/// stall server shutdown.
const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// The `websocket` option callbacks of `serve`. One set per server, shared by
/// all sockets.
pub(crate) struct WsHandlers<'js> {
  open: Option<Function<'js>>,
  message: Option<Function<'js>>,
  close: Option<Function<'js>>,
}

/// Parse the `websocket: { open?, message?, close? }` serve option.
pub(crate) fn parse_ws_handlers<'js>(opts: &Object<'js>) -> rquickjs::Result<Option<Rc<WsHandlers<'js>>>> {
  let Some(obj): Option<Object<'js>> = opts.get("websocket")? else {
    return Ok(None);
  };
  Ok(Some(Rc::new(WsHandlers {
    open: obj.get("open").ok(),
    message: obj.get("message").ok(),
    close: obj.get("close").ok(),
  })))
}

/// A serve Request's upgrade capability, stored on the Request while its handler
/// runs. `server.upgrade(req)` moves it from `Ready` to `Accepted`; serve then
/// sends the held 101 response once the handler returns.
pub(crate) enum ServeUpgrade {
  /// Not upgraded yet: the hyper upgrade handle taken from the incoming request.
  Ready(hyper::upgrade::OnUpgrade),
  /// Handshake accepted: the 101 response to send, and the future resolving to
  /// the raw socket once hyper releases the connection.
  Accepted { response: hyper::Response<ResBody>, socket: UpgradeFut },
}

/// Validate a websocket upgrade and produce the 101 response plus the socket
/// future. The original hyper request was already split into parts by serve, so
/// rebuild a minimal one from the extracted headers (fastwebsockets validates
/// `Sec-WebSocket-Key`/`-Version` from it) and re-attach the upgrade handle
/// where `hyper::upgrade::on` looks for it.
pub(crate) fn try_upgrade(
  headers: &[(String, String)],
  on_upgrade: hyper::upgrade::OnUpgrade,
) -> Result<ServeUpgrade, String> {
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
  Ok(ServeUpgrade::Accepted { response: response.map(BodyExt::boxed), socket })
}

/// A frame queued for the writer task: messages and closes from JS, plus the
/// read half's obligated sends (pong replies, close echoes).
enum OutMsg {
  Frame(OpCode, Vec<u8>),
  Close(u16, String),
  /// The reader finished; stop the writer.
  End,
}

/// The per-connection handle passed to the `websocket` callbacks.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "ServerWebSocket")]
pub(crate) struct ServerWebSocket {
  #[qjs(skip_trace)]
  tx: mpsc::UnboundedSender<OutMsg>,
  #[qjs(skip_trace)]
  state: Rc<Cell<u8>>,
  /// Wakes the read loop when `close()` starts a close, so it can arm the
  /// close-grace deadline (`notify_one` stores a permit, so no race with a
  /// loop that is not currently waiting).
  #[qjs(skip_trace)]
  closing: Rc<Notify>,
}

#[rquickjs::methods]
impl ServerWebSocket {
  /// Queue a message: a string sends a text frame, a Uint8Array a binary frame.
  /// Returns the number of payload bytes queued, or 0 when the socket is no
  /// longer open. The queue is unbounded for now; backpressure reporting
  /// (Bun's -1) is planned together with the `drain` callback.
  pub fn send(&self, data: Value<'_>) -> rquickjs::Result<usize> {
    if self.state.get() != OPEN {
      return Ok(0);
    }
    let (opcode, payload) = match data.as_string() {
      Some(s) => (OpCode::Text, s.to_string()?.into_bytes()),
      None => (OpCode::Binary, extract_body_value(&data, "ServerWebSocket")?),
    };
    let len = payload.len();
    if self.tx.send(OutMsg::Frame(opcode, payload)).is_err() {
      return Ok(0);
    }
    Ok(len)
  }

  /// Send a close frame (default 1000). The connection finishes once the peer
  /// echoes the close (or the grace period expires).
  pub fn close(&self, code: Opt<u16>, reason: Opt<String>) {
    if self.state.get() >= CLOSING {
      return;
    }
    self.state.set(CLOSING);
    let _ = self.tx.send(OutMsg::Close(code.0.unwrap_or(1000), reason.0.unwrap_or_default()));
    self.closing.notify_one();
  }

  #[qjs(get, rename = "readyState")]
  pub fn ready_state(&self) -> u8 {
    self.state.get()
  }
}

/// Run an accepted socket: spawn its writer, then drive the read loop until the
/// peer closes, errors, or the server shuts down. Holds a pending op per task so
/// the runtime stays alive while the socket is open. Runs on the JS executor:
/// the callbacks are JS functions.
pub(crate) fn spawn_socket<'js>(
  ctx: &Ctx<'js>,
  socket: UpgradeFut,
  handlers: Rc<WsHandlers<'js>>,
  shutdown_rx: watch::Receiver<bool>,
  logger: Logger,
) {
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  pending.hold();
  let ctx2 = ctx.clone();
  ctx.spawn(async move {
    run_socket(ctx2, socket, handlers, shutdown_rx, &logger, &pending).await;
    pending.release();
  });
}

async fn run_socket<'js>(
  ctx: Ctx<'js>,
  socket: UpgradeFut,
  handlers: Rc<WsHandlers<'js>>,
  mut shutdown_rx: watch::Receiver<bool>,
  logger: &Logger,
  pending: &PendingOps,
) {
  let ws = match socket.await {
    Ok(ws) => ws,
    Err(e) => {
      logger.warn(&format!("[flux] websocket upgrade failed: {e}"));
      return;
    }
  };
  let (read_half, write_half) = ws.split(tokio::io::split);
  let mut reader = FragmentCollectorRead::new(read_half);
  let (tx, rx) = mpsc::unbounded_channel::<OutMsg>();
  let state = Rc::new(Cell::new(OPEN));

  pending.hold();
  let writer_state = state.clone();
  let writer_logger = logger.clone();
  let writer_pending = pending.clone();
  ctx.spawn(async move {
    run_writer(write_half, rx, writer_state, &writer_logger).await;
    writer_pending.release();
  });

  let close_notify = Rc::new(Notify::new());
  let socket_handle = ServerWebSocket { tx: tx.clone(), state: state.clone(), closing: close_notify.clone() };
  let ws_class = match Class::instance(ctx.clone(), socket_handle) {
    Ok(c) => c,
    Err(e) => {
      logger.warn(&format!("[flux] websocket: could not create socket handle: {e}"));
      let _ = tx.send(OutMsg::End);
      return;
    }
  };

  call_callback(&ctx, &handlers.open, (ws_class.clone(),), "open", logger);

  // Forward the read half's obligated sends (pong replies, close echoes) to the
  // writer. A send error means the writer is gone, which ends the read loop.
  let obligated_tx = tx.clone();
  let mut send_obligated = move |frame: Frame<'_>| {
    let res = obligated_tx
      .send(OutMsg::Frame(frame.opcode, frame.payload.into()))
      .map_err(|_| WebSocketError::ConnectionClosed);
    std::future::ready(res)
  };

  // (code, reason) reported to the close callback.
  let mut close_info = (1006u16, String::new());
  // Once closing (server shutdown or ws.close()), keep reading only until the
  // peer's close echo, bounded by a grace deadline so a silent peer cannot
  // keep the socket (and the runtime) alive forever.
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
            break;
          }
        };
        match frame.opcode {
          OpCode::Text => {
            let text = String::from_utf8_lossy(&frame.payload).into_owned();
            call_callback(&ctx, &handlers.message, (ws_class.clone(), text), "message", logger);
          }
          OpCode::Binary => {
            let bytes = JsBytes(frame.payload.into());
            call_callback(&ctx, &handlers.message, (ws_class.clone(), bytes), "message", logger);
          }
          OpCode::Close => {
            close_info = parse_close(&frame.payload);
            break;
          }
          _ => {}
        }
      }
      _ = wait_for_stop(&mut shutdown_rx), if !closing => {
        closing = true;
        state.set(CLOSING);
        // 1001 Going Away: the server is shutting down.
        let _ = tx.send(OutMsg::Close(1001, String::new()));
        grace.as_mut().reset(tokio::time::Instant::now() + CLOSE_GRACE);
      }
      // ws.close() was called from JS: arm the grace deadline.
      _ = close_notify.notified(), if !closing => {
        closing = true;
        grace.as_mut().reset(tokio::time::Instant::now() + CLOSE_GRACE);
      }
      _ = grace.as_mut(), if closing => {
        logger.warn("[flux] websocket close timed out; dropping connection");
        break;
      }
    }
  }

  state.set(CLOSED);
  let _ = tx.send(OutMsg::End);
  let (code, reason) = close_info;
  call_callback(&ctx, &handlers.close, (ws_class, code, reason), "close", logger);
}

async fn run_writer<W: AsyncWrite + Unpin>(
  mut ws: WebSocketWrite<W>,
  mut rx: mpsc::UnboundedReceiver<OutMsg>,
  state: Rc<Cell<u8>>,
  logger: &Logger,
) {
  // After a close frame goes out nothing more may be sent; queued frames that
  // arrive later (including a redundant close echo) are dropped.
  let mut sent_close = false;
  while let Some(msg) = rx.recv().await {
    let res = match msg {
      OutMsg::Frame(opcode, payload) if !sent_close => {
        sent_close = opcode == OpCode::Close;
        ws.write_frame(Frame::new(true, opcode, None, payload.into())).await
      }
      OutMsg::Close(code, reason) if !sent_close => {
        sent_close = true;
        state.set(CLOSING.max(state.get()));
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

/// Invoke an optional websocket callback, logging a throw instead of
/// propagating it (there is no JS caller to propagate to).
fn call_callback<'js, A: IntoArgs<'js>>(
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

/// Extract (code, reason) from a close frame payload: a big-endian u16 followed
/// by an optional UTF-8 reason. An empty payload means no status (1005).
fn parse_close(payload: &[u8]) -> (u16, String) {
  if payload.len() >= 2 {
    (u16::from_be_bytes([payload[0], payload[1]]), String::from_utf8_lossy(&payload[2..]).into_owned())
  } else {
    (1005, String::new())
  }
}
