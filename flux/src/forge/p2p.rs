//! Engine-free peer-to-peer core, built on iroh.
//!
//! The scripting-engine-independent half of `flux:p2p`: the bound `Endpoint`
//! (keypair identity, dial/accept, ticket encoding, path introspection), the
//! bidirectional `Stream` (pull-based read + queued writer), and the iroh-facing
//! free functions. It names no scripting-engine types; the marshalling layer
//! (`plugins/flux/p2p.rs`) decodes JS args into these types, builds the JS
//! classes and the accept async-iterable, and encodes results back to JS.
//! Destined for the `forge` crate (see REDESIGN.md).
//!
//! "protocol" is the JS-facing name for what QUIC/iroh call the connection's
//! ALPN (RFC 7301): an opaque identifier negotiated in the handshake that both
//! selects and routes the connection. The bytes are passed through verbatim.
//!
//! A `Stream` is a byte-oriented duplex: reads are pull-based (`read_chunk` pulls
//! at most `READ_CHUNK` bytes, `Ok(None)` at end-of-stream) so the transport only
//! advances as the caller iterates; writes go through an mpsc queue drained by
//! `run_writer` (the caller spawns that task, since spawning is host-specific).
//! `Stream::new` returns the handle together with the writer's receiver, the same
//! split as the websocket `SocketSink`.

use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use tokio::sync::mpsc;

use iroh::endpoint::{presets, Connection, RecvStream, RelayMode, SendStream, TransportAddrUsage};
use iroh::{Endpoint as IrohEndpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr};

use crate::logger::Logger;

/// Read granularity: each `read_chunk` pulls at most this many bytes off a stream.
const READ_CHUNK: usize = 64 * 1024;

/// A message queued from the caller for the per-stream writer task.
pub(crate) enum WriteMsg {
  Data(Vec<u8>),
  Finish,
}

/// A bound iroh endpoint with a stable keypair. Cheaply cloned (the iroh handle
/// is itself a clone of shared state plus the 32-byte secret), so the caller can
/// move a clone into each async op rather than borrowing across an await.
#[derive(Clone)]
pub(crate) struct Endpoint {
  inner: IrohEndpoint,
  /// The 32-byte secret key, kept so it can be read back for persistence.
  secret: [u8; 32],
}

impl Endpoint {
  /// Bind an endpoint. `secret` is the 32-byte key (generated when `None`);
  /// `relay_url` selects a self-hosted relay (`None` uses the public n0 relays);
  /// `alpns` lists the protocols this endpoint will `accept`.
  ///
  /// Bind runs on a worker thread (`tokio::spawn`), not the caller's thread:
  /// iroh's bind does blocking work that otherwise stalls a single-threaded host
  /// (observed on Android, starving the render/init commands). This also turns a
  /// bind panic into a returned error instead of a silent teardown.
  pub(crate) async fn bind(
    secret: Option<[u8; 32]>,
    relay_url: Option<String>,
    alpns: Vec<Vec<u8>>,
  ) -> Result<Endpoint, String> {
    match tokio::spawn(build_endpoint(secret, relay_url, alpns)).await {
      Ok(Ok((inner, secret))) => Ok(Endpoint { inner, secret }),
      Ok(Err(e)) => Err(e),
      Err(e) => Err(format!("bind task failed: {e}")),
    }
  }

  /// This endpoint's dial address: the string peers pass to `connect`.
  pub(crate) fn id(&self) -> String {
    self.inner.id().to_string()
  }

  /// The secret key as 64 hex chars, for the caller to persist and feed back to
  /// `bind` to keep a stable identity across restarts.
  pub(crate) fn secret_key_hex(&self) -> String {
    encode_hex(&self.secret)
  }

  /// A self-contained dial token (`id|relay|ips`) carrying this endpoint's id,
  /// home relay, and direct addresses, so a peer can `connect` without relying on
  /// discovery. Waits (briefly) for the relay to be assigned before encoding;
  /// bounded, since a LAN-only endpoint without a relay still yields direct addrs.
  pub(crate) async fn ticket(&self) -> String {
    let _ = tokio::time::timeout(Duration::from_secs(3), self.inner.online()).await;
    encode_ticket(&self.inner.addr())
  }

  /// Dial a peer and open one bidirectional stream over `protocol`. `peer` is
  /// either a `ticket` (connects directly, no discovery) or a bare endpoint `id`
  /// (needs discovery to resolve the peer's address). Returns the raw iroh parts;
  /// the caller assembles a `Stream` from them.
  pub(crate) async fn connect(
    &self,
    peer: String,
    protocol: String,
  ) -> Result<(Connection, SendStream, RecvStream), String> {
    let alpn = protocol.into_bytes();
    let addr = parse_dial(&peer)?;
    let conn = self.inner.connect(addr, &alpn).await.map_err(|e| e.to_string())?;
    let (send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    Ok((conn, send, recv))
  }

  /// Accept the next incoming connection matching `alpn` and open its first
  /// bidirectional stream. `Ok(None)` once the endpoint stops accepting (closed).
  pub(crate) async fn accept_one(&self, alpn: &[u8]) -> Result<Option<(Connection, SendStream, RecvStream)>, String> {
    accept_one(&self.inner, alpn).await
  }

  /// Snapshot of how the connection to `id` is currently carried; see `ConnInfo`.
  /// iroh starts on the relay and upgrades to direct after hole-punching, so poll
  /// this to watch the path settle.
  pub(crate) async fn conn_info(&self, id: String) -> Result<ConnInfo, String> {
    let id: EndpointId = id.parse().map_err(|e| format!("invalid endpoint id: {e}"))?;
    let info = self.inner.remote_info(id).await;
    let mut addrs = Vec::new();
    let (mut has_direct, mut has_relay) = (false, false);
    if let Some(info) = info {
      for ta in info.addrs() {
        let active = matches!(ta.usage(), TransportAddrUsage::Active);
        let addr = ta.addr();
        let kind = if addr.is_relay() {
          has_relay |= active;
          "relay"
        } else if addr.is_ip() {
          has_direct |= active;
          "direct"
        } else {
          "custom"
        };
        addrs.push(AddrEntry { kind, addr: addr.to_string(), active });
      }
    }
    let path = match (has_direct, has_relay) {
      (true, true) => "mixed",
      (true, false) => "direct",
      (false, true) => "relay",
      (false, false) => "none",
    };
    Ok(ConnInfo { path, addrs })
  }

  /// Close the endpoint, ending any `accept_one` loop.
  pub(crate) async fn close(&self) {
    self.inner.close().await;
  }
}

/// One known transport address for a peer, as reported by `conn_info`.
pub struct AddrEntry {
  /// `"relay"`, `"direct"` (an IP path), or `"custom"`.
  pub kind: &'static str,
  pub addr: String,
  pub active: bool,
}

/// A snapshot of how a peer connection is currently carried. `path` is
/// `"direct"` (a direct IP path is active), `"relay"` (only a relay path is
/// active), `"mixed"` (both), or `"none"`; `addrs` lists every known transport
/// address.
pub struct ConnInfo {
  pub path: &'static str,
  pub addrs: Vec<AddrEntry>,
}

/// A single bidirectional p2p stream: a byte duplex. Reads are pull-based; writes
/// go through the writer task draining the channel `new` hands back. Holds the
/// `Connection` only to keep the QUIC connection (and thus the stream) alive for
/// the stream's lifetime.
pub(crate) struct Stream {
  conn: Connection,
  recv: RefCell<Option<RecvStream>>,
  tx: mpsc::UnboundedSender<WriteMsg>,
}

impl Stream {
  /// Build a stream and the receiver its writer task drains. The caller spawns
  /// `run_writer(send, rx, logger)` (spawning is host-specific). Owns the
  /// outgoing channel so callers never handle `WriteMsg` directly.
  pub(crate) fn new(conn: Connection, recv: RecvStream) -> (Rc<Stream>, mpsc::UnboundedReceiver<WriteMsg>) {
    let (tx, rx) = mpsc::unbounded_channel::<WriteMsg>();
    let stream = Rc::new(Stream { conn, recv: RefCell::new(Some(recv)), tx });
    (stream, rx)
  }

  /// Pull the next chunk (at most `READ_CHUNK` bytes). `Ok(None)` at end-of-stream
  /// or once the recv half is gone (closed). The recv half is taken out before the
  /// await and put back after, so no borrow is held across it.
  pub(crate) async fn read_chunk(&self) -> Result<Option<Vec<u8>>, String> {
    let Some(mut recv) = self.recv.borrow_mut().take() else {
      return Ok(None);
    };
    let mut buf = vec![0u8; READ_CHUNK];
    // iroh's read returns Ok(None) at end-of-stream, Ok(Some(n)) for n bytes read.
    let n = recv.read(&mut buf).await.map_err(|e| e.to_string())?;
    match n {
      None => Ok(None),
      Some(n) => {
        buf.truncate(n);
        *self.recv.borrow_mut() = Some(recv);
        Ok(Some(buf))
      }
    }
  }

  /// Queue bytes on the send half.
  pub(crate) fn write(&self, bytes: Vec<u8>) {
    let _ = self.tx.send(WriteMsg::Data(bytes));
  }

  /// Finish the send half (QUIC FIN) after queued writes flush. The recv half
  /// stays open for replies.
  pub(crate) fn finish(&self) {
    let _ = self.tx.send(WriteMsg::Finish);
  }

  /// Tear the stream down: finish the send half and stop reading.
  pub(crate) fn close(&self) {
    let _ = self.tx.send(WriteMsg::Finish);
    self.recv.borrow_mut().take();
  }

  /// The remote peer's endpoint id.
  pub(crate) fn remote_id(&self) -> String {
    self.conn.remote_id().to_string()
  }
}

/// Encode an `EndpointAddr` as a compact ticket string `id|relay|ip1,ip2,...`
/// (relay and ips optional). Hand-rolled rather than serde to keep the QR small.
fn encode_ticket(addr: &EndpointAddr) -> String {
  let mut relay = String::new();
  let mut ips: Vec<String> = Vec::new();
  for ta in &addr.addrs {
    match ta {
      TransportAddr::Relay(url) if relay.is_empty() => relay = url.to_string(),
      TransportAddr::Ip(sa) => ips.push(sa.to_string()),
      _ => {}
    }
  }
  format!("{}|{}|{}", addr.id, relay, ips.join(","))
}

/// Parse a dial target that is either a bare endpoint id or an `encode_ticket`
/// string. A bare id (no `|`) yields an id-only addr (needs discovery to
/// resolve); a ticket carries the relay + direct addresses, so no discovery.
fn parse_dial(s: &str) -> Result<EndpointAddr, String> {
  let mut parts = s.split('|');
  let id: EndpointId = parts.next().unwrap_or("").trim().parse().map_err(|e| format!("invalid endpoint id: {e}"))?;
  let mut addrs: Vec<TransportAddr> = Vec::new();
  if let Some(relay) = parts.next().filter(|s| !s.is_empty()) {
    addrs.push(TransportAddr::Relay(relay.parse::<RelayUrl>().map_err(|e| e.to_string())?));
  }
  if let Some(ips) = parts.next() {
    for ip in ips.split(',').filter(|s| !s.is_empty()) {
      addrs.push(TransportAddr::Ip(ip.parse::<SocketAddr>().map_err(|e| e.to_string())?));
    }
  }
  Ok(EndpointAddr::from_parts(id, addrs))
}

/// Build and bind an iroh endpoint. Returns it together with the (possibly
/// generated) secret-key bytes so the caller can expose them for persistence.
async fn build_endpoint(
  secret: Option<[u8; 32]>,
  relay_url: Option<String>,
  alpns: Vec<Vec<u8>>,
) -> Result<(IrohEndpoint, [u8; 32]), String> {
  let secret_key = match secret {
    Some(bytes) => SecretKey::from_bytes(&bytes),
    None => SecretKey::generate(),
  };
  let bytes = secret_key.to_bytes();

  let mut builder = IrohEndpoint::builder(presets::N0).secret_key(secret_key).alpns(alpns);
  if let Some(url) = relay_url {
    let relay: RelayUrl = url.parse().map_err(|e| format!("invalid relayUrl: {e}"))?;
    builder = builder.relay_mode(RelayMode::custom([relay]));
  }
  let endpoint = builder.bind().await.map_err(|e| e.to_string())?;
  Ok((endpoint, bytes))
}

/// Accept the next incoming connection matching `alpn` and open its first
/// bidirectional stream. Returns `None` once the endpoint stops accepting.
/// Non-matching or failed connections are skipped.
async fn accept_one(
  ep: &IrohEndpoint,
  alpn: &[u8],
) -> Result<Option<(Connection, SendStream, RecvStream)>, String> {
  loop {
    let Some(incoming) = ep.accept().await else {
      return Ok(None);
    };
    let conn = match incoming.await {
      Ok(conn) => conn,
      Err(_) => continue,
    };
    if conn.alpn() != alpn {
      conn.close(0u32.into(), b"alpn mismatch");
      continue;
    }
    match conn.accept_bi().await {
      Ok((send, recv)) => return Ok(Some((conn, send, recv))),
      Err(_) => continue,
    }
  }
}

/// Drain queued writes onto the send half in order, then finish it. A write
/// error or a closed queue ends the task.
pub(crate) async fn run_writer(mut send: SendStream, mut rx: mpsc::UnboundedReceiver<WriteMsg>, logger: &Logger) {
  while let Some(msg) = rx.recv().await {
    match msg {
      WriteMsg::Data(buf) => {
        if let Err(e) = send.write_all(&buf).await {
          logger.warn(&format!("[flux] p2p write error: {e}"));
          break;
        }
      }
      WriteMsg::Finish => break,
    }
  }
  let _ = send.finish();
}

fn encode_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode 64 hex chars into 32 bytes (a secret key). Engine-free; the caller
/// extracts the string from JS first.
pub(crate) fn decode_hex32(s: &str) -> Result<[u8; 32], String> {
  if s.len() != 64 {
    return Err("secretKey must be 64 hex characters (32 bytes)".to_string());
  }
  let mut out = [0u8; 32];
  for (i, slot) in out.iter_mut().enumerate() {
    *slot = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| "secretKey is not valid hex".to_string())?;
  }
  Ok(out)
}