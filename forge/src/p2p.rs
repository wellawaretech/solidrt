//! Engine-free peer-to-peer core, built on iroh.
//!
//! The scripting-engine-independent half of `flux:p2p`: the bound `Endpoint`
//! (keypair identity, dial/accept, ticket encoding, path introspection), the
//! bidirectional `Stream` (a byte duplex), and the iroh-facing free functions. It names no scripting-engine types, and no iroh types cross
//! its boundary: `connect`/`accept_one` hand back an assembled `Stream`, and
//! `connect_io`/`accept_io` a byte duplex. The marshalling layer (flux
//! `forge_plugins/p2p.rs`) decodes JS args into these types, builds the JS
//! classes and the accept async-iterable, and encodes results back to JS.
//!
//! "protocol" is the JS-facing name for what QUIC/iroh call the connection's
//! ALPN (RFC 7301): an opaque identifier negotiated in the handshake that both
//! selects and routes the connection. The bytes are passed through verbatim.
//!
//! A `Stream` is a byte-oriented duplex with the `net::Conn` shape: the read
//! half is a `ByteStream` view (`readable()`) that advances only as it is
//! polled; a write awaits the transport under a lock, so a fast producer sees
//! QUIC's flow control as backpressure and a failed write reaches the caller.
//! Nothing runs in the background.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::sync::Mutex;
use url::Url;

use iroh::endpoint::{presets, Connection, RecvStream, RelayMode, SendStream, TransportAddrUsage, VarInt};
use iroh::{
  Endpoint as IrohEndpoint, EndpointAddr, EndpointId, RelayConfig, RelayMap, RelayUrl, SecretKey, TransportAddr,
  Watcher,
};

use crate::stream::{holding, read_half, ByteStream, Closing, ReadSlot};
use crate::Value;

#[cfg(feature = "p2p-relay")]
pub mod relay;
#[cfg(feature = "p2p-relay")]
pub use relay::Relay;

/// Read granularity: each read off a `Stream` pulls at most this many bytes.
const READ_CHUNK: usize = 64 * 1024;

/// The application error code an aborted stream resets its send half with.
/// Nothing reads it: the peer sees the reset itself.
const RESET_CODE: u32 = 0;

/// How long `ticket` waits for the addresses it encodes (the relay coming
/// online, or the first direct address) before encoding what it has.
const TICKET_WAIT: Duration = Duration::from_secs(3);

/// What a write reports once the stream was closed (not aborted).
const STREAM_CLOSED: &str = "stream is closed";

/// A bound iroh endpoint with a stable keypair. Cheaply cloned (the iroh handle
/// is itself a clone of shared state plus the 32-byte secret), so the caller can
/// move a clone into each async op rather than borrowing across an await.
#[derive(Clone)]
pub struct Endpoint {
  inner: IrohEndpoint,
  /// The 32-byte secret key, kept so it can be read back for persistence.
  secret: [u8; 32],
  /// The configured relay, if any: `ticket` waits for the home relay only
  /// then, and writes the relay's token into the ticket.
  relay: Option<RelayAccess>,
}

/// A configured relay as an app or a ticket names it: the URL proper, and
/// the token the relay admits (`relay::Relay` issues one), carried as the
/// URL's user part on the way in and out (`http://<token>@host:port/`).
#[derive(Clone)]
struct RelayAccess {
  url: RelayUrl,
  token: Option<String>,
}

impl Endpoint {
  /// Bind an endpoint. `secret` is the 32-byte key (generated when `None`);
  /// `relay_url` selects a self-hosted relay (`None`: no relay at all), its
  /// token as the URL's user part when the relay admits one;
  /// `alpns` lists the protocols this endpoint will `accept`. No public
  /// infrastructure is ever used: no public relay and no address publishing or
  /// lookup, so nothing about the endpoint leaves the machine except the
  /// ticket, the sole carrier of addressing (its direct IPs, plus the relay
  /// when one is configured). `bind_port` pins the UDP
  /// port (IPv4-only) so the ticket's direct address stays fixed across
  /// restarts; paired with a persisted `secret` this makes the whole ticket
  /// stable, so a client can re-dial the old ticket after a restart. `None`
  /// lets iroh pick an ephemeral port. `relay_only` binds no UDP socket at
  /// all: every packet goes through the relay (which must then be given),
  /// for a device whose UDP sends fail
  /// (okf/upstream/noq-udp-android-eio-ecn-cmsg-send-stall.md).
  ///
  /// Bind runs on a worker thread (`tokio::spawn`), not the caller's thread:
  /// iroh's bind does blocking work that otherwise stalls a single-threaded host
  /// (observed on Android, starving the render/init commands). This also turns a
  /// bind panic into a returned error instead of a silent teardown.
  pub async fn bind(
    secret: Option<[u8; 32]>,
    relay_url: Option<String>,
    alpns: Vec<Vec<u8>>,
    bind_port: Option<u16>,
    relay_only: bool,
  ) -> Result<Endpoint, String> {
    let relay = relay_url.as_deref().map(parse_relay_url).transpose()?;
    if relay_only && relay.is_none() {
      return Err("paths \"relay\" needs a relayUrl".to_string());
    }
    match tokio::spawn(build_endpoint(secret, relay.clone(), alpns, bind_port, relay_only)).await {
      Ok(Ok((inner, secret))) => Ok(Endpoint { inner, secret, relay }),
      Ok(Err(e)) => Err(e),
      Err(e) => Err(format!("bind task failed: {e}")),
    }
  }

  /// This endpoint's dial address: the string peers pass to `connect`.
  pub fn id(&self) -> String {
    self.inner.id().to_string()
  }

  /// The secret key as 64 hex chars, for the caller to persist and feed back to
  /// `bind` to keep a stable identity across restarts.
  pub fn secret_key_hex(&self) -> String {
    encode_hex(&self.secret)
  }

  /// A self-contained dial token (`id|relay|ips`) carrying this endpoint's id,
  /// home relay (with its token, so the peer can use it too), and direct
  /// addresses, so a peer can `connect` without any lookup. Waits (bounded) for what makes it dialable: the relay coming
  /// online when one is configured, else the first direct address (without a
  /// relay `online` never resolves).
  pub async fn ticket(&self) -> String {
    if self.relay.is_some() {
      let _ = tokio::time::timeout(TICKET_WAIT, self.inner.online()).await;
    } else {
      let _ = tokio::time::timeout(TICKET_WAIT, self.direct_addr_known()).await;
    }
    encode_ticket(&self.inner.addr(), self.relay.as_ref().and_then(|relay| relay.token.as_deref()))
  }

  /// Resolves once the endpoint knows at least one direct address.
  async fn direct_addr_known(&self) {
    let mut watcher = self.inner.watch_addr();
    let mut addr = watcher.get();
    while addr.ip_addrs().next().is_none() {
      addr = match watcher.updated().await {
        Ok(addr) => addr,
        Err(_disconnected) => return,
      };
    }
  }

  /// Dial a peer and open one bidirectional stream over `protocol`. `peer` is a
  /// `ticket`. A bare endpoint `id` parses, but with no address lookup
  /// registered it only reaches a peer this endpoint already knows a path to.
  pub async fn connect(&self, peer: String, protocol: String) -> Result<Stream, String> {
    let (conn, send, recv) = self.connect_raw(peer, protocol).await?;
    Ok(Stream::open(conn, send, recv))
  }

  /// Like `connect`, but packaged as a byte duplex (see `accept_io`): for a
  /// byte-oriented consumer such as the dev tunnel's TCP forwarder.
  pub async fn connect_io(&self, peer: String, protocol: String) -> Result<ConnIo, String> {
    let (conn, send, recv) = self.connect_raw(peer, protocol).await?;
    Ok(ConnIo { io: tokio::io::join(recv, send), conn })
  }

  async fn connect_raw(&self, peer: String, protocol: String) -> Result<(Connection, SendStream, RecvStream), String> {
    let alpn = protocol.into_bytes();
    let addr = parse_dial(&peer)?;
    let conn = self.inner.connect(addr, &alpn).await.map_err(|e| e.to_string())?;
    let (send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    Ok((conn, send, recv))
  }

  /// Accept the next incoming connection matching `alpn` and open its first
  /// bidirectional stream as a `Stream`: it arrives with the dialer's first
  /// bytes, since QUIC announces a stream by its data. `Ok(None)` once the
  /// endpoint stops accepting (closed).
  pub async fn accept_one(&self, alpn: &[u8]) -> Result<Option<Stream>, String> {
    Ok(accept_one(&self.inner, alpn).await?.map(|(conn, send, recv)| Stream::open(conn, send, recv)))
  }

  /// Like `accept_one`, but packaged as a byte duplex: the remote peer's id plus
  /// a `ConnIo` over the connection's first bi-stream, ready for a byte-oriented
  /// consumer (the HTTP server serves it like an accepted TCP socket).
  pub async fn accept_io(&self, alpn: &[u8]) -> Result<Option<(String, ConnIo)>, String> {
    match accept_one(&self.inner, alpn).await? {
      Some((conn, send, recv)) => {
        let remote = conn.remote_id().to_string();
        Ok(Some((remote, ConnIo { io: tokio::io::join(recv, send), conn })))
      }
      None => Ok(None),
    }
  }

  /// Snapshot of how the connection to `id` is currently carried; see `ConnInfo`.
  /// iroh starts on the relay and upgrades to direct after hole-punching, so poll
  /// this to watch the path settle.
  pub async fn conn_info(&self, id: String) -> Result<ConnInfo, String> {
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
  pub async fn close(&self) {
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

impl From<AddrEntry> for Value {
  fn from(e: AddrEntry) -> Value {
    Value::map([("kind", Value::from(e.kind)), ("addr", Value::from(e.addr)), ("active", Value::from(e.active))])
  }
}

/// `{ path, addrs: [{ kind, addr, active }] }`.
impl From<ConnInfo> for Value {
  fn from(c: ConnInfo) -> Value {
    Value::map([("path", Value::from(c.path)), ("addrs", Value::list(c.addrs))])
  }
}

/// A single bidirectional p2p stream: a byte duplex. The read half is read
/// through the `readable()` view; writes await the send half under a lock.
/// Holds the `Connection` to keep the QUIC connection (and thus the stream)
/// alive for the stream's lifetime; the read view holds its own handle, so a
/// consumer still reading keeps it up after the `Stream` is gone.
pub struct Stream {
  conn: Connection,
  recv: ReadSlot<RecvStream>,
  send: Mutex<Option<SendStream>>,
  closing: Closing,
}

impl Stream {
  /// Assemble a stream from an opened bi-stream.
  fn open(conn: Connection, send: SendStream, recv: RecvStream) -> Stream {
    Stream { conn, recv: ReadSlot::new(recv), send: Mutex::new(Some(send)), closing: Closing::new() }
  }

  /// The recv half as a `ByteStream`: at most `READ_CHUNK` bytes per chunk,
  /// pulled as the consumer polls, ending at end-of-stream or when the stream
  /// is closed (as an error when aborted). One view per stream: dropping it
  /// releases the recv half, which tells the peer to stop sending (QUIC
  /// STOP_SENDING), while the send half stays usable.
  pub fn readable(&self) -> ByteStream {
    holding(read_half(&self.recv, &self.closing, READ_CHUNK), self.conn.clone())
  }

  /// Write all of `bytes` on the send half, serialized behind the lock and
  /// paced by QUIC flow control. Errors once the send half was finished, the
  /// stream closed or aborted, or when the peer stopped reading or went away.
  pub async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
    let mut guard = self.send.lock().await;
    let Some(send) = guard.as_mut() else {
      return Err(self.write_closed());
    };
    let r = tokio::select! {
      r = send.write_all(&bytes) => r.map_err(|e| e.to_string()),
      _ = self.closing.closed() => Err(self.closing.write_error(STREAM_CLOSED)),
    };
    if self.closing.is_closed() {
      // close()/abort() could not reach the locked send half; end it here.
      if let Some(send) = guard.take() {
        self.end_send(send);
      }
    }
    r
  }

  /// Half-close: finish the send half (QUIC FIN) after the writes so far,
  /// keeping the recv half open for replies. After it, `write` errors.
  /// Idempotent, and a no-op once the stream is closed.
  pub async fn finish(&self) -> Result<(), String> {
    let mut guard = self.send.lock().await;
    let Some(mut send) = guard.take() else {
      return Ok(());
    };
    if self.closing.is_closed() {
      self.end_send(send);
      return Err(self.closing.write_error(STREAM_CLOSED));
    }
    send.finish().map_err(|e| e.to_string())
  }

  /// Tear the stream down gracefully: a pending read ends, the recv half is
  /// released and the send half finished (QUIC FIN). Idempotent.
  pub fn close(&self) {
    self.closing.close();
    self.release();
  }

  /// Tear the stream down with an error: as `close()`, except that the send
  /// half is reset instead of finished, so the peer's read fails rather than
  /// ending cleanly, and a pending or later read here reports `reason`.
  /// Idempotent (the first end wins).
  pub fn abort(&self, reason: String) {
    self.closing.abort(reason);
    self.release();
  }

  fn release(&self) {
    self.recv.close();
    if let Ok(mut guard) = self.send.try_lock() {
      if let Some(send) = guard.take() {
        self.end_send(send);
      }
    } // else a write is in flight; its closed branch ends the half
  }

  /// End a send half the way the stream ended: reset after an abort, else
  /// finished (which dropping an unfinished half does).
  fn end_send(&self, mut send: SendStream) {
    if self.closing.reason().is_some() {
      let _ = send.reset(VarInt::from_u32(RESET_CODE));
    }
  }

  fn write_closed(&self) -> String {
    if self.closing.is_closed() {
      self.closing.write_error(STREAM_CLOSED)
    } else {
      "send half is finished".to_string()
    }
  }

  /// The remote peer's endpoint id.
  pub fn remote_id(&self) -> String {
    self.conn.remote_id().to_string()
  }
}

/// One connection as a tokio byte duplex: the joined recv/send halves of its
/// first bi-stream, holding the `Connection` so the QUIC connection lives
/// exactly as long as the io handle. Built by `Endpoint::accept_io`.
pub struct ConnIo {
  conn: Connection,
  io: tokio::io::Join<RecvStream, SendStream>,
}

impl ConnIo {
  /// A future resolving once the QUIC connection fully closes (peer close or
  /// idle timeout). Await it after the io has been consumed and served: bytes
  /// written to QUIC are only guaranteed delivery while a connection handle is
  /// alive, so dropping the io right after the final write can discard the
  /// unacknowledged tail. The future holds its own handle, keeping the
  /// connection up until the peer has read everything and closed.
  pub fn closed(&self) -> impl std::future::Future<Output = ()> {
    let conn = self.conn.clone();
    async move {
      conn.closed().await;
    }
  }
}

impl tokio::io::AsyncRead for ConnIo {
  fn poll_read(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    std::pin::Pin::new(&mut self.io).poll_read(cx, buf)
  }
}

impl tokio::io::AsyncWrite for ConnIo {
  fn poll_write(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &[u8],
  ) -> std::task::Poll<std::io::Result<usize>> {
    std::pin::Pin::new(&mut self.io).poll_write(cx, buf)
  }

  fn poll_flush(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    std::pin::Pin::new(&mut self.io).poll_flush(cx)
  }

  fn poll_shutdown(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    std::pin::Pin::new(&mut self.io).poll_shutdown(cx)
  }
}

/// Encode an `EndpointAddr` as a compact ticket string `id|relay|ip1,ip2,...`
/// (relay and ips optional; the relay with `token` as its user part).
/// Hand-rolled rather than serde to keep the QR small.
fn encode_ticket(addr: &EndpointAddr, token: Option<&str>) -> String {
  let mut relay = String::new();
  let mut ips: Vec<String> = Vec::new();
  for ta in &addr.addrs {
    match ta {
      TransportAddr::Relay(url) if relay.is_empty() => relay = relay_with_token(url, token),
      TransportAddr::Ip(sa) => ips.push(sa.to_string()),
      _ => {}
    }
  }
  format!("{}|{}|{}", addr.id, relay, ips.join(","))
}

/// Parse a dial target that is either a bare endpoint id or an `encode_ticket`
/// string. A bare id (no `|`) yields an id-only addr (with no lookup registered,
/// it dials only a peer already known); a ticket carries the relay + direct
/// addresses.
fn parse_dial(s: &str) -> Result<EndpointAddr, String> {
  let mut parts = s.split('|');
  let id: EndpointId = parts.next().unwrap_or("").trim().parse().map_err(|e| format!("invalid endpoint id: {e}"))?;
  let mut addrs: Vec<TransportAddr> = Vec::new();
  if let Some(relay) = parts.next().filter(|s| !s.is_empty()) {
    // The token in the ticket is for the dialer's own relay config; the
    // peer's relay address is the URL proper.
    addrs.push(TransportAddr::Relay(parse_relay_url(relay)?.url));
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
///
/// Always built from `presets::Minimal` (crypto provider only), never
/// `presets::N0`: N0 uses n0's public relays and registers a pkarr publisher +
/// DNS lookup against n0's public server, announcing the endpoint's addresses
/// off-machine. The only relay is the self-hosted one `relay_url` names.
async fn build_endpoint(
  secret: Option<[u8; 32]>,
  relay: Option<RelayAccess>,
  alpns: Vec<Vec<u8>>,
  bind_port: Option<u16>,
  relay_only: bool,
) -> Result<(IrohEndpoint, [u8; 32]), String> {
  let secret_key = match secret {
    Some(bytes) => SecretKey::from_bytes(&bytes),
    None => SecretKey::generate(),
  };
  let bytes = secret_key.to_bytes();

  let mut builder = IrohEndpoint::builder(presets::Minimal).secret_key(secret_key).alpns(alpns);
  // IPv4 only: `clear_ip_transports` drops the pre-configured unspecified
  // sockets (one per family), then `bind_addr` binds exactly one IPv4 socket,
  // on the pinned port when there is one (so the ticket's direct address is
  // stable across restarts, paired with a persisted key) or any free port.
  // iroh prefers an IPv6 path when both exist, and a phone's global IPv6
  // privacy addresses rotate under a connection (observed 2026-10-16: a
  // stream stalled after its first packets on such a path), while the LAN
  // path every device here shares is IPv4.
  // Relay-only: no IP transport at all, so nothing is sent over UDP.
  builder = builder.clear_ip_transports();
  if !relay_only {
    builder = builder
      .bind_addr((std::net::Ipv4Addr::UNSPECIFIED, bind_port.unwrap_or(0)))
      .map_err(|e| format!("invalid bind port: {e}"))?;
  }
  let relay_mode = match relay {
    Some(RelayAccess { url, token }) => {
      // No QUIC address discovery port: the self-hosted relay (`relay`)
      // serves plain HTTP only, and an entry with the default QUIC port
      // (what `RelayMode::custom` writes) makes iroh probe it and wait out
      // the timeouts before the HTTP probe counts, 3 s before the endpoint
      // is online (measured 2026-10-17).
      let mut config = RelayConfig::new(url, None);
      if let Some(token) = token {
        config = config.with_auth_token(token);
      }
      RelayMode::Custom(RelayMap::from(config))
    }
    None => RelayMode::Disabled,
  };
  builder = builder.relay_mode(relay_mode);
  let endpoint = builder.bind().await.map_err(|e| e.to_string())?;
  Ok((endpoint, bytes))
}

/// Accept the next incoming connection matching `alpn` and open its first
/// bidirectional stream. Returns `None` once the endpoint stops accepting.
/// Non-matching or failed connections are skipped.
async fn accept_one(ep: &IrohEndpoint, alpn: &[u8]) -> Result<Option<(Connection, SendStream, RecvStream)>, String> {
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

/// A relay URL as an app or a ticket gives it, `http://<token>@host:port/`
/// when the relay admits a token: the URL proper and the token.
fn parse_relay_url(s: &str) -> Result<RelayAccess, String> {
  let mut url = Url::parse(s).map_err(|e| format!("invalid relay URL: {e}"))?;
  let token = (!url.username().is_empty()).then(|| url.username().to_string());
  url.set_username("").map_err(|_| format!("invalid relay URL: {s}"))?;
  Ok(RelayAccess { url: RelayUrl::from(url), token })
}

/// A relay URL as a ticket carries it: `token` as its user part, so the peer
/// dialing the ticket can configure the same relay.
fn relay_with_token(url: &RelayUrl, token: Option<&str>) -> String {
  let mut url: Url = (**url).clone();
  if let Some(token) = token {
    let _ = url.set_username(token);
  }
  url.to_string()
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode 64 hex chars into 32 bytes (a secret key). Engine-free; the caller
/// extracts the string from JS first.
pub fn decode_hex32(s: &str) -> Result<[u8; 32], String> {
  if s.len() != 64 {
    return Err("secretKey must be 64 hex characters (32 bytes)".to_string());
  }
  let mut out = [0u8; 32];
  for (i, slot) in out.iter_mut().enumerate() {
    *slot = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| "secretKey is not valid hex".to_string())?;
  }
  Ok(out)
}
