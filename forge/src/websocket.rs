//! Engine-free websocket core.
//!
//! Both halves of the websocket protocol flux exposes, with no fastwebsockets
//! or hyper type crossing the boundary:
//!
//! - Server (the `websocket` option of `flux:http` serve): the upgrade
//!   handshake (`accept_upgrade` -> `Handshake` -> `PendingSocket`), the
//!   outgoing frame queue with backpressure accounting (`SocketSink`), the
//!   per-server pub/sub registry (`Topics`), and the read/write loops
//!   (`run_reader`, `run_writer`) generic over a `WsDispatch` the host
//!   implements.
//! - Client (the web-standard `WebSocket` global): `parse_ws_url`, the shared
//!   `ClientSocket` state, and `run_client`, which drives connect, handshake
//!   and the frame loop against a `ClientDispatch`; the host spawns the
//!   `ClientWriter` it is handed.
//!
//! It names no scripting-engine types; the marshalling layer (flux
//! `forge_plugins/websocket.rs` and `standards_plugins/websocket.rs`) builds
//! the JS handles and forwards the callbacks.

use fastwebsockets::upgrade::{is_upgrade_request, upgrade, UpgradeFut};
use fastwebsockets::{
  handshake, FragmentCollectorRead, Frame, OpCode, WebSocket, WebSocketError, WebSocketRead, WebSocketWrite,
};
use http_body_util::{BodyExt, Empty};
use hyper::header::{HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use hyper::upgrade::Upgraded;
use hyper::Request as HyperRequest;
use hyper_util::rt::TokioIo;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch, Notify};

use crate::http::{wait_for_stop, Reply, UpgradeHandle};
use crate::logger::Logger;

/// Web-standard readyState values. A server socket is born OPEN (its handle
/// is only created after the handshake); a client starts CONNECTING while the
/// TCP connect and handshake are in flight.
pub const CONNECTING: u8 = 0;
pub const OPEN: u8 = 1;
pub const CLOSING: u8 = 2;
pub const CLOSED: u8 = 3;

/// How long a closing socket waits for the peer's close echo (or remaining
/// frames) before giving up and dropping the connection, so a dead peer cannot
/// stall server shutdown. Shared by the server loops and the client.
pub const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// Bytes of queued-but-unwritten frames above which `send` reports backpressure
/// (-1) and a later `drain` callback is armed. Matches Bun's default.
pub const DEFAULT_BACKPRESSURE_LIMIT: usize = 1024 * 1024;

/// Control frames (ping/pong) carry at most 125 payload bytes (RFC 6455 5.5).
pub const MAX_CONTROL_PAYLOAD: usize = 125;

/// RFC 6455: a close reason must fit in the close frame with its 2-byte code.
const MAX_CLOSE_REASON: usize = 123;

/// The frame kinds a host queues on purpose: the two data frames, and the two
/// control frames it may send itself (a ping, an unsolicited pong). Close
/// frames go through `begin_close`/`close`; pong replies to incoming pings are
/// the read loop's own business and never surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
  Text,
  Binary,
  Ping,
  Pong,
}

impl Kind {
  fn opcode(self) -> OpCode {
    match self {
      Kind::Text => OpCode::Text,
      Kind::Binary => OpCode::Binary,
      Kind::Ping => OpCode::Ping,
      Kind::Pong => OpCode::Pong,
    }
  }
}

/// A frame queued for a writer task: messages and closes from the host, plus
/// the read half's obligated sends (pong replies, close echoes).
enum OutMsg {
  Frame(OpCode, Vec<u8>),
  Close(u16, String),
  /// The reader finished; stop the writer.
  End,
}

// ---- Socket halves ---------------------------------------------------------

/// The upgraded connection both sides run on: hyper's post-101 stream, on
/// the server (`PendingSocket::accept`) and the client (`run_client`) alike.
type Io = TokioIo<Upgraded>;

/// The read half of an upgraded socket: what `run_reader` drives.
pub struct SocketRead(WebSocketRead<ReadHalf<Io>>);

/// The write half of an upgraded socket: what `run_writer` drives.
pub struct SocketWrite(WebSocketWrite<WriteHalf<Io>>);

fn split_socket(ws: WebSocket<Io>) -> (SocketRead, SocketWrite) {
  let (read, write) = ws.split(tokio::io::split);
  (SocketRead(read), SocketWrite(write))
}

// ---- Server upgrade --------------------------------------------------------

/// An accepted upgrade: the 101 reply the server sends once the handler
/// returns, and the socket that materializes when hyper releases the
/// connection to it.
pub struct Handshake {
  reply: Reply,
  socket: PendingSocket,
}

impl Handshake {
  pub fn into_parts(self) -> (Reply, PendingSocket) {
    (self.reply, self.socket)
  }
}

/// The socket of an accepted upgrade, ready once hyper hands over the
/// connection (after the 101 went out).
pub struct PendingSocket(UpgradeFut);

impl PendingSocket {
  pub async fn accept(self) -> Result<(SocketRead, SocketWrite), String> {
    let ws = self.0.await.map_err(|e| e.to_string())?;
    Ok(split_socket(ws))
  }
}

/// Validate a websocket upgrade and produce the 101 reply plus the pending
/// socket. The incoming request was already split into parts by the server
/// core, so a minimal one is rebuilt from the extracted `headers`
/// (fastwebsockets validates `Sec-WebSocket-Key`/`-Version` from it) with
/// the upgrade `handle` re-attached where hyper looks for it. `extra_headers`
/// (from `upgrade(req, { headers })`) are appended to the 101; an invalid one
/// fails the upgrade rather than silently dropping it.
pub fn accept_upgrade(
  headers: &[(String, String)],
  handle: UpgradeHandle,
  extra_headers: &[(String, String)],
) -> Result<Handshake, String> {
  let mut builder = HyperRequest::builder();
  for (k, v) in headers {
    builder = builder.header(k.as_str(), v.as_str());
  }
  let mut req = builder.body(()).map_err(|e| format!("invalid headers: {e}"))?;
  if !is_upgrade_request(&req) {
    return Err("not a websocket upgrade request".to_string());
  }
  req.extensions_mut().insert(handle.into_inner());
  let (response, socket) = upgrade(&mut req).map_err(|e| e.to_string())?;
  let mut response = response.map(BodyExt::boxed);
  for (k, v) in extra_headers {
    let name = HeaderName::from_bytes(k.as_bytes()).map_err(|e| format!("invalid header name {k}: {e}"))?;
    let value = HeaderValue::from_str(v).map_err(|e| format!("invalid header value for {k}: {e}"))?;
    response.headers_mut().append(name, value);
  }
  Ok(Handshake { reply: Reply::from_hyper(response), socket: PendingSocket(socket) })
}

// ---- Server sink and pub/sub -----------------------------------------------

/// The plain-Rust outgoing half of one server socket: the writer queue plus
/// its accounting and pub/sub membership. Shared (`Rc`) between the host's
/// handle, the reader and writer tasks, and the pub/sub topic registry, which
/// must hold sockets without host lifetimes.
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

/// The writer task's end of a `SocketSink`'s queue, opaque to the host: it
/// only passes it on to `run_writer`.
pub struct SinkQueue(mpsc::UnboundedReceiver<OutMsg>);

impl SocketSink {
  /// Build a sink and the queue its writer task drains.
  pub fn new(topics: Topics, limit: usize) -> (Rc<Self>, SinkQueue) {
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
    (sink, SinkQueue(rx))
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
  pub fn enqueue(&self, kind: Kind, payload: Vec<u8>) -> i32 {
    if self.state.get() != OPEN {
      return 0;
    }
    let len = payload.len();
    if self.tx.send(OutMsg::Frame(kind.opcode(), payload)).is_err() {
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
  fn send_obligated(&self, opcode: OpCode, payload: Vec<u8>) -> Result<(), ()> {
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
  fn set_closing(&self) {
    self.state.set(CLOSING.max(self.state.get()));
  }

  /// Mark the socket fully CLOSED (the read loop ended).
  fn mark_closed(&self) {
    self.state.set(CLOSED);
  }

  /// Tell the writer to stop once the queue drains.
  fn send_end(&self) {
    let _ = self.tx.send(OutMsg::End);
  }

  /// Account for `len` bytes just written by the writer; `ok` is whether the
  /// write succeeded. Returns true when the queue has just emptied while
  /// backpressured and open, i.e. the `drain` callback should fire (which also
  /// clears the backpressured flag).
  fn on_written(&self, len: usize, ok: bool) -> bool {
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
  /// `Rc` (without a host lifetime).
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

  /// Publish a message to every subscriber of `topic` except this socket.
  /// Returns the number of sockets the message was queued to.
  pub fn publish(&self, topic: &str, kind: Kind, payload: Vec<u8>) -> i32 {
    self.topics.publish(topic, kind, payload, Some(self.id))
  }

  /// Drop all topic subscriptions (the socket closed). Breaks the
  /// `topics <-> sink` cycle; see the struct doc.
  fn unsubscribe_all(&self) {
    for topic in self.subscribed.borrow_mut().drain() {
      self.topics.unsubscribe(&topic, self.id);
    }
  }
}

/// The per-server pub/sub registry: topic name -> subscribed sockets by id. All
/// access happens on the host's thread. Sockets are removed by `unsubscribe`
/// and automatically when they close; an entry is dropped with its last
/// subscriber.
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
  fn next_id(&self) -> u64 {
    let id = self.inner.next_id.get();
    self.inner.next_id.set(id + 1);
    id
  }

  fn subscribe(&self, topic: &str, sink: &Rc<SocketSink>) {
    self.inner.map.borrow_mut().entry(topic.to_string()).or_default().insert(sink.id, sink.clone());
  }

  fn unsubscribe(&self, topic: &str, id: u64) {
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

  /// Publish a message to every subscriber of `topic`, except the `exclude`d
  /// socket (the publisher, for `ws.publish`). Returns the number of sockets
  /// the message was queued to; closed sockets are skipped.
  pub fn publish(&self, topic: &str, kind: Kind, payload: Vec<u8>, exclude: Option<u64>) -> i32 {
    let mut delivered = 0;
    if let Some(subs) = self.inner.map.borrow().get(topic) {
      for (id, sink) in subs {
        if Some(*id) == exclude {
          continue;
        }
        if sink.enqueue(kind, payload.clone()) != 0 {
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

/// The host-bound half of a server websocket: the in-loop callbacks
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
pub async fn run_reader<D>(
  read_half: SocketRead,
  sink: Rc<SocketSink>,
  close_notify: Rc<Notify>,
  mut shutdown_rx: watch::Receiver<bool>,
  dispatch: &D,
  handle: &D::Handle,
  logger: &Logger,
) where
  D: WsDispatch,
{
  let mut reader = FragmentCollectorRead::new(read_half.0);

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
/// queued bytes, until the reader ends the queue. Fires `on_drain` when a
/// backpressured queue empties. Engine-free: generic over `WsDispatch`.
pub async fn run_writer<D>(
  write_half: SocketWrite,
  queue: SinkQueue,
  sink: Rc<SocketSink>,
  dispatch: &D,
  handle: &D::Handle,
  logger: &Logger,
) where
  D: WsDispatch,
{
  let mut ws = write_half.0;
  let mut rx = queue.0;
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

// ---- Client ----------------------------------------------------------------

/// A parsed `ws://` target: what `run_client` dials.
pub struct WsTarget {
  pub host: String,
  pub port: u16,
  /// Path and query, always starting with `/`.
  pub path: String,
}

/// Parse a `ws://` URL. `wss://` is recognized but rejected until TLS support
/// lands; `Err` is the message to surface.
pub fn parse_ws_url(url: &str) -> Result<WsTarget, String> {
  if url.starts_with("wss://") {
    return Err("wss:// is not supported yet (no TLS)".to_string());
  }
  let Some(rest) = url.strip_prefix("ws://") else {
    return Err("WebSocket URL must start with ws://".to_string());
  };
  if rest.contains('#') {
    return Err("WebSocket URL must not contain a fragment".to_string());
  }
  let (authority, path) = match rest.find(['/', '?']) {
    Some(i) if rest.as_bytes()[i] == b'?' => (&rest[..i], format!("/{}", &rest[i..])),
    Some(i) => (&rest[..i], rest[i..].to_string()),
    None => (rest, "/".to_string()),
  };
  // Split an optional port; a bracketed IPv6 host keeps its colons.
  let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
    let Some(end) = v6.find(']') else {
      return Err("invalid WebSocket URL: unterminated IPv6 host".to_string());
    };
    match &v6[end + 1..] {
      "" => (&v6[..end], None),
      p => match p.strip_prefix(':') {
        Some(p) => (&v6[..end], Some(p)),
        None => return Err("invalid WebSocket URL authority".to_string()),
      },
    }
  } else {
    match authority.rsplit_once(':') {
      Some((h, p)) => (h, Some(p)),
      None => (authority, None),
    }
  };
  if host.is_empty() {
    return Err("WebSocket URL is missing a host".to_string());
  }
  let port = match port {
    Some(p) => p.parse::<u16>().map_err(|_| "invalid WebSocket URL port".to_string())?,
    None => 80,
  };
  Ok(WsTarget { host: host.to_string(), port, path })
}

/// State shared between the host's client handle and `run_client`: the
/// readyState, the writer queue (present once the handshake completes), and
/// the close signal.
pub struct ClientSocket {
  state: Cell<u8>,
  tx: RefCell<Option<mpsc::UnboundedSender<OutMsg>>>,
  /// Wakes the connection task when `close()` starts a close: during CONNECTING
  /// it aborts the attempt, after that it arms the close-grace deadline
  /// (`notify_one` stores a permit, so no race with a task not yet waiting).
  closing: Notify,
}

impl ClientSocket {
  pub fn new() -> Rc<ClientSocket> {
    Rc::new(ClientSocket { state: Cell::new(CONNECTING), tx: RefCell::new(None), closing: Notify::new() })
  }

  pub fn state(&self) -> u8 {
    self.state.get()
  }

  /// Queue a frame. Only an OPEN socket sends; a frame on any other state is
  /// dropped (the host decides how to report a send while CONNECTING).
  pub fn send(&self, kind: Kind, payload: Vec<u8>) {
    if self.state.get() != OPEN {
      return;
    }
    if let Some(tx) = &*self.tx.borrow() {
      let _ = tx.send(OutMsg::Frame(kind.opcode(), payload));
    }
  }

  /// Start the closing handshake (default code 1000). During CONNECTING this
  /// aborts the attempt (the close event then reports an unclean 1006); on a
  /// closing or closed socket it is a no-op. `Err` is the message for a code
  /// or reason the web API rejects.
  pub fn close(&self, code: Option<u16>, reason: Option<String>) -> Result<(), String> {
    if let Some(c) = code {
      if c != 1000 && !(3000..=4999).contains(&c) {
        return Err("close code must be 1000 or in the range 3000-4999".to_string());
      }
    }
    let reason = reason.unwrap_or_default();
    if reason.len() > MAX_CLOSE_REASON {
      return Err(format!("close reason must be {MAX_CLOSE_REASON} bytes or fewer"));
    }
    let state = self.state.get();
    if state >= CLOSING {
      return Ok(());
    }
    self.state.set(CLOSING);
    if state == OPEN {
      if let Some(tx) = &*self.tx.borrow() {
        let _ = tx.send(OutMsg::Close(code.unwrap_or(1000), reason));
      }
    }
    self.closing.notify_one();
    Ok(())
  }
}

/// The host-bound half of a client socket: the web-standard events, in the
/// order the standard prescribes (`error` before `close` on a failure).
pub trait ClientDispatch {
  fn on_open(&self);
  fn on_text(&self, text: String);
  fn on_binary(&self, bytes: Vec<u8>);
  fn on_error(&self, message: String);
  fn on_close(&self, code: u16, reason: String, was_clean: bool);
}

/// The client's writer task, handed to the host by `run_client` once the
/// handshake completes (spawning is host-specific). The client's mirror of
/// the server's `run_writer`, without the backpressure/drain accounting.
pub struct ClientWriter {
  ws: SocketWrite,
  rx: mpsc::UnboundedReceiver<OutMsg>,
}

impl ClientWriter {
  /// Drain the queue onto the socket. After a close frame goes out nothing
  /// more may be sent; later frames are dropped.
  pub async fn run(self, logger: &Logger) {
    let mut ws = self.ws.0;
    let mut rx = self.rx;
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
}

/// Ties hyper's client connection task to the tokio runtime (the future is
/// Send, so this works on both current-thread and multi-thread runtimes).
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

/// Connect, handshake, then drive the socket until it closes: the client-side
/// mirror of the server's `run_reader`. `socket` carries the state the host's
/// handle reads and writes; `spawn_writer` receives the writer task once the
/// socket is OPEN. Events go to `dispatch`; a failed connect reports `error`
/// then an unclean `close`.
pub async fn run_client<D, S>(
  socket: Rc<ClientSocket>,
  target: WsTarget,
  dispatch: &D,
  spawn_writer: S,
  logger: &Logger,
) where
  D: ClientDispatch,
  S: FnOnce(ClientWriter),
{
  let WsTarget { host, port, path } = target;
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
      .body(Empty::<bytes::Bytes>::new())
      .map_err(|e| e.to_string())?;
    handshake::client(&SpawnExecutor, req, stream).await.map_err(|e| e.to_string())
  };
  tokio::pin!(connect);

  let ws = tokio::select! {
    res = &mut connect => match res {
      Ok((ws, _response)) => ws,
      Err(e) => {
        socket.state.set(CLOSED);
        logger.warn(&format!("[flux] websocket connect failed: {e}"));
        dispatch.on_error(format!("connect failed: {e}"));
        dispatch.on_close(1006, String::new(), false);
        return;
      }
    },
    // close() during CONNECTING aborts the attempt.
    _ = socket.closing.notified() => {
      socket.state.set(CLOSED);
      dispatch.on_close(1006, String::new(), false);
      return;
    }
  };

  let (read_half, write_half) = split_socket(ws);
  let mut reader = FragmentCollectorRead::new(read_half.0);
  let (tx, rx) = mpsc::unbounded_channel::<OutMsg>();
  *socket.tx.borrow_mut() = Some(tx.clone());
  socket.state.set(OPEN);
  spawn_writer(ClientWriter { ws: write_half, rx });
  dispatch.on_open();

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
          OpCode::Text => dispatch.on_text(String::from_utf8_lossy(&frame.payload).into_owned()),
          OpCode::Binary => dispatch.on_binary(frame.payload.into()),
          OpCode::Close => {
            close_info = parse_close(&frame.payload);
            was_clean = true;
            break;
          }
          _ => {}
        }
      }
      // close() was called from the host: arm the grace deadline.
      _ = socket.closing.notified(), if !closing => {
        closing = true;
        grace.as_mut().reset(tokio::time::Instant::now() + CLOSE_GRACE);
      }
      _ = grace.as_mut(), if closing => {
        logger.warn("[flux] websocket close timed out; dropping connection");
        break;
      }
    }
  }

  socket.state.set(CLOSED);
  let _ = tx.send(OutMsg::End);
  socket.tx.borrow_mut().take();
  if let Some(e) = read_error {
    dispatch.on_error(e);
  }
  let (code, reason) = close_info;
  dispatch.on_close(code, reason, was_clean);
}
