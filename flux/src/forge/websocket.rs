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

use fastwebsockets::OpCode;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Web-standard readyState values. A server socket is born OPEN (the handle is
/// only created after the handshake), so there is no CONNECTING state here.
pub(crate) const OPEN: u8 = 1;
pub(crate) const CLOSING: u8 = 2;
pub(crate) const CLOSED: u8 = 3;

/// How long a closing socket waits for the peer's close echo (or remaining
/// frames) before giving up and dropping the connection, so a dead peer cannot
/// stall server shutdown. Shared with the client (plugins::websocket).
pub(crate) const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// Bytes of queued-but-unwritten frames above which `send` reports backpressure
/// (-1) and a later `drain` callback is armed. Matches Bun's default.
pub(crate) const DEFAULT_BACKPRESSURE_LIMIT: usize = 1024 * 1024;

/// Control frames (ping/pong) carry at most 125 payload bytes (RFC 6455 5.5).
pub(crate) const MAX_CONTROL_PAYLOAD: usize = 125;

/// A frame queued for the writer task: messages and closes from JS, plus the
/// read half's obligated sends (pong replies, close echoes). Shared with the
/// client (plugins::websocket), which runs its own writer loop.
pub(crate) enum OutMsg {
  Frame(OpCode, Vec<u8>),
  Close(u16, String),
  /// The reader finished; stop the writer.
  End,
}

/// The plain-Rust outgoing half of one socket: the writer queue plus its
/// accounting. Shared (`Rc`) between the JS handle, the reader and writer tasks,
/// and the pub/sub topic registry, which must hold sockets without JS lifetimes.
pub(crate) struct SocketSink {
  id: u64,
  tx: mpsc::UnboundedSender<OutMsg>,
  state: Cell<u8>,
  /// Total payload bytes queued for the writer but not yet written.
  queued: Cell<usize>,
  /// True once a send exceeded `limit`; cleared (and `drain` fired) when the
  /// writer empties the queue.
  backpressured: Cell<bool>,
  limit: usize,
}

impl SocketSink {
  pub(crate) fn new(id: u64, tx: mpsc::UnboundedSender<OutMsg>, limit: usize) -> Self {
    SocketSink { id, tx, state: Cell::new(OPEN), queued: Cell::new(0), backpressured: Cell::new(false), limit }
  }

  pub(crate) fn id(&self) -> u64 {
    self.id
  }

  pub(crate) fn state(&self) -> u8 {
    self.state.get()
  }

  pub(crate) fn is_open(&self) -> bool {
    self.state.get() == OPEN
  }

  /// Queue a frame for the writer, with Bun's send return values: -1 when the
  /// queue exceeds the backpressure limit (frame still queued; `drain` will fire
  /// once it empties), 0 when the socket is no longer open (dropped), otherwise
  /// the number of payload bytes queued.
  pub(crate) fn enqueue(&self, opcode: OpCode, payload: Vec<u8>) -> i32 {
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
  pub(crate) fn send_obligated(&self, opcode: OpCode, payload: Vec<u8>) -> Result<(), ()> {
    self.queued.set(self.queued.get() + payload.len());
    self.tx.send(OutMsg::Frame(opcode, payload)).map_err(|_| ())
  }

  /// Begin a graceful close: if not already closing, mark the socket CLOSING and
  /// queue a close frame. Returns true when this call initiated the close (the
  /// caller then arms its close-grace deadline), false if already closing.
  pub(crate) fn begin_close(&self, code: u16, reason: String) -> bool {
    if self.state.get() >= CLOSING {
      return false;
    }
    self.state.set(CLOSING);
    let _ = self.tx.send(OutMsg::Close(code, reason));
    true
  }

  /// Mark the socket CLOSING without queueing a frame (the writer's own close
  /// path, where the close frame is written directly). Never lowers the state.
  pub(crate) fn set_closing(&self) {
    self.state.set(CLOSING.max(self.state.get()));
  }

  /// Mark the socket fully CLOSED (the read loop ended).
  pub(crate) fn mark_closed(&self) {
    self.state.set(CLOSED);
  }

  /// Tell the writer to stop once the queue drains.
  pub(crate) fn send_end(&self) {
    let _ = self.tx.send(OutMsg::End);
  }

  /// Account for `len` bytes just written by the writer; `ok` is whether the
  /// write succeeded. Returns true when the queue has just emptied while
  /// backpressured and open, i.e. the `drain` callback should fire (which also
  /// clears the backpressured flag).
  pub(crate) fn on_written(&self, len: usize, ok: bool) -> bool {
    let left = self.queued.get().saturating_sub(len);
    self.queued.set(left);
    if ok && left == 0 && self.backpressured.get() && self.state.get() == OPEN {
      self.backpressured.set(false);
      true
    } else {
      false
    }
  }
}

/// The per-server pub/sub registry: topic name -> subscribed sockets by id. All
/// access happens on the JS thread. Sockets are removed by `unsubscribe` and
/// automatically when they close; an entry is dropped with its last subscriber.
#[derive(Clone, Default)]
pub(crate) struct Topics {
  inner: Rc<TopicsInner>,
}

#[derive(Default)]
struct TopicsInner {
  map: RefCell<HashMap<String, HashMap<u64, Rc<SocketSink>>>>,
  next_id: Cell<u64>,
}

impl Topics {
  pub(crate) fn next_id(&self) -> u64 {
    let id = self.inner.next_id.get();
    self.inner.next_id.set(id + 1);
    id
  }

  pub(crate) fn subscribe(&self, topic: &str, sink: &Rc<SocketSink>) {
    self.inner.map.borrow_mut().entry(topic.to_string()).or_default().insert(sink.id, sink.clone());
  }

  pub(crate) fn unsubscribe(&self, topic: &str, id: u64) {
    let mut map = self.inner.map.borrow_mut();
    if let Some(subs) = map.get_mut(topic) {
      subs.remove(&id);
      if subs.is_empty() {
        map.remove(topic);
      }
    }
  }

  pub(crate) fn subscriber_count(&self, topic: &str) -> usize {
    self.inner.map.borrow().get(topic).map_or(0, HashMap::len)
  }

  /// Publish a pre-encoded message to every subscriber of `topic`, except the
  /// `exclude`d socket (the publisher, for `ws.publish`). Returns the number of
  /// sockets the message was queued to; closed sockets are skipped. The caller
  /// (marshalling) has already turned the JS value into `(opcode, payload)`.
  pub(crate) fn publish(&self, topic: &str, opcode: OpCode, payload: Vec<u8>, exclude: Option<u64>) -> i32 {
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
pub(crate) fn parse_close(payload: &[u8]) -> (u16, String) {
  if payload.len() >= 2 {
    (u16::from_be_bytes([payload[0], payload[1]]), String::from_utf8_lossy(&payload[2..]).into_owned())
  } else {
    (1005, String::new())
  }
}