//! Engine-free websocket-server core.
//!
//! The scripting-engine-independent half of the `flux:http` websocket server:
//! the outgoing frame queue with backpressure accounting (`SocketSink`), the
//! per-server pub/sub registry (`Topics`), the writer-queue message type
//! (`OutMsg`), the readyState/protocol constants, and close-frame parsing. It
//! names no scripting-engine types; the marshalling layer
//! (`plugins/flux/websocket.rs`) drives these from the read/write loops and
//! dispatches the JS callbacks. Destined for the `forge` crate (see REDESIGN.md).
//! `OutMsg`, `parse_close`, and `CLOSE_GRACE` are also shared with the
//! web-standard WebSocket *client* (`plugins/websocket.rs`).

use fastwebsockets::{FragmentCollectorRead, Frame, OpCode, WebSocketError, WebSocketRead, WebSocketWrite};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch, Notify};

use crate::http::wait_for_stop;
use crate::logger::Logger;

/// Web-standard readyState values. A server socket is born OPEN (the handle is
/// only created after the handshake), so there is no CONNECTING state here.
pub const OPEN: u8 = 1;
pub const CLOSING: u8 = 2;
pub const CLOSED: u8 = 3;

/// How long a closing socket waits for the peer's close echo (or remaining
/// frames) before giving up and dropping the connection, so a dead peer cannot
/// stall server shutdown. Shared with the client (plugins::websocket).
pub const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// Bytes of queued-but-unwritten frames above which `send` reports backpressure
/// (-1) and a later `drain` callback is armed. Matches Bun's default.
pub const DEFAULT_BACKPRESSURE_LIMIT: usize = 1024 * 1024;

/// Control frames (ping/pong) carry at most 125 payload bytes (RFC 6455 5.5).
pub const MAX_CONTROL_PAYLOAD: usize = 125;

/// A frame queued for the writer task: messages and closes from JS, plus the
/// read half's obligated sends (pong replies, close echoes). Shared with the
/// client (plugins::websocket), which runs its own writer loop.
pub enum OutMsg {
  Frame(OpCode, Vec<u8>),
  Close(u16, String),
  /// The reader finished; stop the writer.
  End,
}

/// The plain-Rust outgoing half of one socket: the writer queue plus its
/// accounting and pub/sub membership. Shared (`Rc`) between the JS handle, the
/// reader and writer tasks, and the pub/sub topic registry, which must hold
/// sockets without JS lifetimes.
///
/// Holding `topics` makes a `topics <-> sink` reference cycle while the socket
/// is subscribed to anything; `unsubscribe_all` (run when the socket closes)
/// breaks it by dropping the registry's `Rc<SocketSink>`, so the close path must
/// always reach it.
pub struct SocketSink {
  id: u64,
  tx: mpsc::UnboundedSender<OutMsg>,
  state: Cell<u8>,
  /// Total payload bytes queued for the writer but not yet written.
  queued: Cell<usize>,
  /// True once a send exceeded `limit`; cleared (and `drain` fired) when the
  /// writer empties the queue.
  backpressured: Cell<bool>,
  limit: usize,
  /// The server's pub/sub registry, so the socket can (un)subscribe itself.
  topics: Topics,
  /// The topics this socket joined, so closing can unsubscribe them all.
  subscribed: RefCell<HashSet<String>>,
}

impl SocketSink {
  /// Build a sink and the receiver its writer task drains. Owns the outgoing
  /// channel so callers never handle `OutMsg` directly.
  pub fn new(topics: Topics, limit: usize) -> (Rc<Self>, mpsc::UnboundedReceiver<OutMsg>) {
    let id = topics.next_id();
    let (tx, rx) = mpsc::unbounded_channel();
    let sink = Rc::new(SocketSink {
      id,
      tx,
      state: Cell::new(OPEN),
      queued: Cell::new(0),
      backpressured: Cell::new(false),
      limit,
      topics,
      subscribed: RefCell::new(HashSet::new()),
    });
    (sink, rx)
  }

  pub fn state(&self) -> u8 {
    self.state.get()
  }

  pub fn is_open(&self) -> bool {
    self.state.get() == OPEN
  }

  /// Queue a frame for the writer, with Bun's send return values: -1 when the
  /// queue exceeds the backpressure limit (frame still queued; `drain` will fire
  /// once it empties), 0 when the socket is no longer open (dropped), otherwise
  /// the number of payload bytes queued.
  pub fn enqueue(&self, opcode: OpCode, payload: Vec<u8>) -> i32 {
    if self.state.get() != OPEN {
      return 0;
    }
    let len = payload.len();
    if self.tx.send(OutMsg::Frame(opcode, payload)).is_err() {
      return 0;
    }
    let queued = self.queued.get() + len;
    self.queued.set(queued);
    if queued > self.limit {
      self.backpressured.set(true);
      return -1;
    }
    len as i32
  }

  /// Queue an obligated frame from the read half (a pong reply or close echo),
  /// counting its bytes like any other queued frame. `Err` means the writer is
  /// gone, which ends the read loop.
  pub fn send_obligated(&self, opcode: OpCode, payload: Vec<u8>) -> Result<(), ()> {
    self.queued.set(self.queued.get() + payload.len());
    self.tx.send(OutMsg::Frame(opcode, payload)).map_err(|_| ())
  }

  /// Begin a graceful close: if not already closing, mark the socket CLOSING and
  /// queue a close frame. Returns true when this call initiated the close (the
  /// caller then arms its close-grace deadline), false if already closing.
  pub fn begin_close(&self, code: u16, reason: String) -> bool {
    if self.state.get() >= CLOSING {
      return false;
    }
    self.state.set(CLOSING);
    let _ = self.tx.send(OutMsg::Close(code, reason));
    true
  }

  /// Mark the socket CLOSING without queueing a frame (the writer's own close
  /// path, where the close frame is written directly). Never lowers the state.
  pub fn set_closing(&self) {
    self.state.set(CLOSING.max(self.state.get()));
  }

  /// Mark the socket fully CLOSED (the read loop ended).
  pub fn mark_closed(&self) {
    self.state.set(CLOSED);
  }

  /// Tell the writer to stop once the queue drains.
  pub fn send_end(&self) {
    let _ = self.tx.send(OutMsg::End);
  }

  /// Account for `len` bytes just written by the writer; `ok` is whether the
  /// write succeeded. Returns true when the queue has just emptied while
  /// backpressured and open, i.e. the `drain` callback should fire (which also
  /// clears the backpressured flag).
  pub fn on_written(&self, len: usize, ok: bool) -> bool {
    let left = self.queued.get().saturating_sub(len);
    self.queued.set(left);
    if ok && left == 0 && self.backpressured.get() && self.state.get() == OPEN {
      self.backpressured.set(false);
      true
    } else {
      false
    }
  }

  /// Join a topic so `publish(topic)` reaches this socket. No-op on a closing or
  /// closed socket. Takes `&Rc<Self>` because the registry holds the socket by
  /// `Rc` (without a JS lifetime).
  pub fn subscribe(self: &Rc<Self>, topic: &str) {
    if !self.is_open() {
      return;
    }
    self.topics.subscribe(topic, self);
    self.subscribed.borrow_mut().insert(topic.to_string());
  }

  /// Leave a topic. Closing the socket unsubscribes everything automatically.
  pub fn unsubscribe(&self, topic: &str) {
    self.topics.unsubscribe(topic, self.id);
    self.subscribed.borrow_mut().remove(topic);
  }

  pub fn is_subscribed(&self, topic: &str) -> bool {
    self.subscribed.borrow().contains(topic)
  }

  /// Publish a pre-encoded message to every subscriber of `topic` except this
  /// socket. Returns the number of sockets the message was queued to.
  pub fn publish(&self, topic: &str, opcode: OpCode, payload: Vec<u8>) -> i32 {
    self.topics.publish(topic, opcode, payload, Some(self.id))
  }

  /// Drop all topic subscriptions (the socket closed). Breaks the
  /// `topics <-> sink` cycle; see the struct doc.
  pub fn unsubscribe_all(&self) {
    for topic in self.subscribed.borrow_mut().drain() {
      self.topics.unsubscribe(&topic, self.id);
    }
  }
}

/// The per-server pub/sub registry: topic name -> subscribed sockets by id. All
/// access happens on the JS thread. Sockets are removed by `unsubscribe` and
/// automatically when they close; an entry is dropped with its last subscriber.
#[derive(Clone, Default)]
pub struct Topics {
  inner: Rc<TopicsInner>,
}

#[derive(Default)]
struct TopicsInner {
  map: RefCell<HashMap<String, HashMap<u64, Rc<SocketSink>>>>,
  next_id: Cell<u64>,
}

impl Topics {
  pub fn next_id(&self) -> u64 {
    let id = self.inner.next_id.get();
    self.inner.next_id.set(id + 1);
    id
  }

  pub fn subscribe(&self, topic: &str, sink: &Rc<SocketSink>) {
    self.inner.map.borrow_mut().entry(topic.to_string()).or_default().insert(sink.id, sink.clone());
  }

  pub fn unsubscribe(&self, topic: &str, id: u64) {
    let mut map = self.inner.map.borrow_mut();
    if let Some(subs) = map.get_mut(topic) {
      subs.remove(&id);
      if subs.is_empty() {
        map.remove(topic);
      }
    }
  }

  pub fn subscriber_count(&self, topic: &str) -> usize {
    self.inner.map.borrow().get(topic).map_or(0, HashMap::len)
  }

  /// Publish a pre-encoded message to every subscriber of `topic`, except the
  /// `exclude`d socket (the publisher, for `ws.publish`). Returns the number of
  /// sockets the message was queued to; closed sockets are skipped. The caller
  /// (marshalling) has already turned the JS value into `(opcode, payload)`.
  pub fn publish(&self, topic: &str, opcode: OpCode, payload: Vec<u8>, exclude: Option<u64>) -> i32 {
    let mut delivered = 0;
    if let Some(subs) = self.inner.map.borrow().get(topic) {
      for (id, sink) in subs {
        if Some(*id) == exclude {
          continue;
        }
        if sink.enqueue(opcode, payload.clone()) != 0 {
          delivered += 1;
        }
      }
    }
    delivered
  }
}

/// Extract (code, reason) from a close frame payload: a big-endian u16 followed
/// by an optional UTF-8 reason. An empty payload means no status (1005).
pub fn parse_close(payload: &[u8]) -> (u16, String) {
  if payload.len() >= 2 {
    (u16::from_be_bytes([payload[0], payload[1]]), String::from_utf8_lossy(&payload[2..]).into_owned())
  } else {
    (1005, String::new())
  }
}

// ---- Server loops ----------------------------------------------------------

/// The engine-bound half of a server websocket: the in-loop callbacks
/// (`message`/`pong`/`drain`/`close`) the read/write loops fire. A scripting host
/// forwards them to script functions; a pure-Rust host implements them with
/// closures. The `Handle` (the host's per-socket object, e.g. the script's
/// `ServerWebSocket`) is built by the host before the loops start, so its
/// construction is not part of this trait - the loops only pass it back.
pub trait WsDispatch {
  /// The per-socket handle the callbacks receive.
  type Handle;

  fn on_text(&self, handle: &Self::Handle, text: String);
  fn on_binary(&self, handle: &Self::Handle, bytes: Vec<u8>);
  fn on_pong(&self, handle: &Self::Handle, bytes: Vec<u8>);
  fn on_drain(&self, handle: &Self::Handle);
  fn on_close(&self, handle: &Self::Handle, code: u16, reason: String);
}

/// Drive the read half until the peer closes, errors, or the server shuts down.
/// Owns the close-grace state machine and forwards the read half's obligated
/// sends (pong replies, close echoes) to the writer; decoded data frames go to
/// `dispatch`. On exit it ends the writer queue, drops topic subscriptions, and
/// fires `on_close`. Engine-free: generic over `WsDispatch`.
pub async fn run_reader<R, D>(
  read_half: WebSocketRead<R>,
  sink: Rc<SocketSink>,
  close_notify: Rc<Notify>,
  mut shutdown_rx: watch::Receiver<bool>,
  dispatch: &D,
  handle: &D::Handle,
  logger: &Logger,
) where
  R: AsyncRead + Unpin,
  D: WsDispatch,
{
  let mut reader = FragmentCollectorRead::new(read_half);

  // Forward the read half's obligated sends to the writer, counting their bytes
  // like any other queued frame. A send error means the writer is gone, which
  // ends the read loop.
  let obligated_sink = sink.clone();
  let mut send_obligated = move |frame: Frame<'_>| {
    let res =
      obligated_sink.send_obligated(frame.opcode, frame.payload.into()).map_err(|()| WebSocketError::ConnectionClosed);
    std::future::ready(res)
  };

  // (code, reason) reported to the close callback.
  let mut close_info = (1006u16, String::new());
  // Once closing (server shutdown or ws.close()), keep reading only until the
  // peer's close echo, bounded by a grace deadline so a silent peer cannot keep
  // the socket (and the runtime) alive forever.
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
          OpCode::Text => dispatch.on_text(handle, String::from_utf8_lossy(&frame.payload).into_owned()),
          OpCode::Binary => dispatch.on_binary(handle, frame.payload.into()),
          OpCode::Pong => dispatch.on_pong(handle, frame.payload.into()),
          OpCode::Close => {
            close_info = parse_close(&frame.payload);
            break;
          }
          _ => {}
        }
      }
      _ = wait_for_stop(&mut shutdown_rx), if !closing => {
        closing = true;
        // 1001 Going Away: the server is shutting down.
        let _ = sink.begin_close(1001, String::new());
        grace.as_mut().reset(tokio::time::Instant::now() + CLOSE_GRACE);
      }
      // ws.close() was called from the host: arm the grace deadline.
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

  sink.mark_closed();
  sink.send_end();
  sink.unsubscribe_all();
  let (code, reason) = close_info;
  dispatch.on_close(handle, code, reason);
}

/// Drain the writer queue, writing frames to the socket and accounting the
/// queued bytes, until the reader ends the queue (`OutMsg::End`). Fires
/// `on_drain` when a backpressured queue empties. Engine-free: generic over
/// `WsDispatch`.
pub async fn run_writer<W, D>(
  mut ws: WebSocketWrite<W>,
  mut rx: mpsc::UnboundedReceiver<OutMsg>,
  sink: Rc<SocketSink>,
  dispatch: &D,
  handle: &D::Handle,
  logger: &Logger,
) where
  W: AsyncWrite + Unpin,
  D: WsDispatch,
{
  // After a close frame goes out nothing more may be sent; queued frames that
  // arrive later (including a redundant close echo) are dropped, but their bytes
  // still leave the queue accounting.
  let mut sent_close = false;
  while let Some(msg) = rx.recv().await {
    let res = match msg {
      OutMsg::Frame(opcode, payload) => {
        let len = payload.len();
        let res = if sent_close {
          Ok(())
        } else {
          sent_close = opcode == OpCode::Close;
          ws.write_frame(Frame::new(true, opcode, None, payload.into())).await
        };
        if sink.on_written(len, res.is_ok()) {
          dispatch.on_drain(handle);
        }
        res
      }
      OutMsg::Close(code, reason) if !sent_close => {
        sent_close = true;
        sink.set_closing();
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
