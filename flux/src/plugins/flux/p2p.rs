//! The `flux:p2p` module: peer-to-peer connectivity for flux, built on iroh.
//!
//! Stage 1 scope (deliberately minimal):
//! - `Endpoint.create(opts)` binds an iroh endpoint. Identity is a keypair; an
//!   ephemeral one is generated unless `secretKey` (64 hex chars) is supplied.
//!   The endpoint is dialable by its `id` over iroh's public (n0) relay and
//!   discovery infrastructure; pass `relayUrl` to use a self-hosted relay
//!   instead.
//! - `endpoint.connect(id, protocol)` dials a peer BY ID (not address) and opens
//!   one bidirectional stream.
//! - `endpoint.accept(protocol)` is an async-iterable of incoming streams whose
//!   protocol matches; the accepting endpoint must list that protocol in
//!   `opts.protocols`.
//!
//! "protocol" is the JS-facing name for what QUIC/iroh call the connection's
//! ALPN (RFC 7301): an opaque identifier negotiated in the handshake that both
//! selects and routes the connection. The bytes are passed through verbatim.
//! - A stream is a byte-oriented duplex: read with `for await (chunk of stream)`
//!   (the same async-iterable idiom as HTTP bodies in body.rs), write with
//!   `stream.write(bytes)` and end the send half with `stream.finish()` (the
//!   same mpsc-to-writer idiom as websocket.rs).
//!
//! Out of scope for stage 1: unidirectional streams, multiple streams per peer
//! (one `connect` == one stream for now), gossip/blobs, and key persistence (the
//! caller stores the `secretKey` getter value itself).
//!
//! The iroh-facing operations are kept as free functions (`build_endpoint`,
//! `accept_one`, `run_writer`) so the rendertree-style "engine-independent core"
//! split holds: only the thin class layer touches rquickjs. When WebStreams land
//! the read/write surface can be swapped here without disturbing that core.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, Exception, Function, JsLifetime, Object, TypedArray, Value};
use tokio::sync::mpsc;

use std::net::SocketAddr;

use iroh::endpoint::{presets, Connection, RecvStream, RelayMode, SendStream, TransportAddrUsage};
use iroh::{Endpoint as IrohEndpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr};

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::body::extract_body_value;

/// Read granularity: each `next()` pulls at most this many bytes off a stream.
const READ_CHUNK: usize = 64 * 1024;

/// A message queued from JS for the per-stream writer task.
enum WriteMsg {
  Data(Vec<u8>),
  Finish,
}

/// Map any iroh/io error into a JS-visible error (surfaces as a promise
/// rejection). We flatten to `io::Error` like body.rs does, rather than throwing
/// (no `Ctx` is needed and the message is preserved).
fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
  std::io::Error::other(e.to_string())
}

fn encode_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex32(ctx: &Ctx<'_>, s: &str) -> rquickjs::Result<[u8; 32]> {
  if s.len() != 64 {
    return Err(Exception::throw_message(ctx, "secretKey must be 64 hex characters (32 bytes)"));
  }
  let mut out = [0u8; 32];
  for (i, slot) in out.iter_mut().enumerate() {
    *slot = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
      .map_err(|_| Exception::throw_message(ctx, "secretKey is not valid hex"))?;
  }
  Ok(out)
}

/// The `flux:p2p` `Endpoint`: a bound iroh endpoint with a stable keypair.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Endpoint")]
pub struct P2pEndpoint {
  #[qjs(skip_trace)]
  inner: IrohEndpoint,
  /// The 32-byte secret key, kept so `secretKey` can be read back for persistence.
  #[qjs(skip_trace)]
  secret: [u8; 32],
}

#[rquickjs::methods]
impl P2pEndpoint {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<P2pEndpoint> {
    Err(Exception::throw_message(&ctx, "use Endpoint.create() to bind a p2p endpoint"))
  }

  /// Bind an endpoint. `opts`: `{ secretKey?, relayUrl?, protocols? }`.
  /// `secretKey` is 64 hex chars (omit for an ephemeral key); `relayUrl` selects
  /// a self-hosted relay (omit for the public n0 relays); `protocols` lists the
  /// protocols this endpoint will `accept`.
  #[qjs(static)]
  pub fn create<'js>(
    ctx: Ctx<'js>,
    opts: Opt<Object<'js>>,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<P2pEndpoint>>>> {
    let (secret, relay_url, alpns) = parse_create_opts(&ctx, opts.0)?;
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    Ok(Promised(async move {
      pending.hold();
      // Run bind on a worker thread, not the engine's JS thread: iroh's bind does
      // blocking work that otherwise stalls the JS thread (observed on Android,
      // starving the render/init commands).
      let r = match tokio::spawn(build_endpoint(secret, relay_url, alpns)).await {
        Ok(inner) => inner,
        Err(e) => Err(std::io::Error::other(format!("bind task failed: {e}"))),
      };
      pending.release();
      let (inner, secret) = r.map_err(rquickjs::Error::Io)?;
      Ok(P2pEndpoint { inner, secret })
    }))
  }

  /// This endpoint's dial address: the string peers pass to `connect`.
  #[qjs(get)]
  pub fn id(&self) -> String {
    self.inner.id().to_string()
  }

  /// The secret key as 64 hex chars, for the caller to persist and feed back to
  /// `create` to keep a stable identity across restarts.
  #[qjs(get, rename = "secretKey")]
  pub fn secret_key(&self) -> String {
    encode_hex(&self.secret)
  }

  /// A self-contained dial token (`id|relay|ips`) carrying this endpoint's id,
  /// home relay, and direct addresses, so a peer can `connect` without relying
  /// on discovery. Waits (briefly) for the relay to be assigned before encoding.
  pub fn ticket<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<String>>>> {
    let ep = self.inner.clone();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    Ok(Promised(async move {
      pending.hold();
      // Wait until the endpoint has a relay so the ticket includes it; bounded,
      // since a LAN-only endpoint without a relay still yields direct addresses.
      let _ = tokio::time::timeout(std::time::Duration::from_secs(3), ep.online()).await;
      let ticket = encode_ticket(&ep.addr());
      pending.release();
      Ok(ticket)
    }))
  }

  /// Dial a peer and open one bidirectional stream over `protocol`. `peer` is
  /// either a `ticket` (preferred: connects directly, no discovery) or a bare
  /// endpoint `id` (needs discovery to resolve the peer's address).
  pub fn connect<'js>(
    &self,
    ctx: Ctx<'js>,
    peer: String,
    protocol: String,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Class<'js, P2pStream>>>>> {
    let ep = self.inner.clone();
    let alpn = protocol.into_bytes();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      pending.hold();
      let r = async {
        let addr = parse_dial(&peer)?;
        let conn = ep.connect(addr, &alpn).await.map_err(io_err)?;
        let (send, recv) = conn.open_bi().await.map_err(io_err)?;
        Ok::<_, std::io::Error>((conn, send, recv))
      }
      .await;
      pending.release();
      let (conn, send, recv) = r.map_err(rquickjs::Error::Io)?;
      P2pStream::create(&ctx2, conn, send, recv)
    }))
  }

  /// An async-iterable of incoming streams whose protocol matches `protocol`.
  /// Iterating ends (`done`) when the endpoint is closed.
  pub fn accept<'js>(&self, ctx: Ctx<'js>, protocol: String) -> rquickjs::Result<Object<'js>> {
    let ep = self.inner.clone();
    let alpn = Rc::new(protocol.into_bytes());
    let iter = Object::new(ctx.clone())?;

    let next_fn = Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> rquickjs::Result<AcceptStep<'js>> {
      let ep = ep.clone();
      let alpn = alpn.clone();
      let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
      let ctx2 = ctx.clone();
      Ok(Promised(Box::pin(async move {
        pending.hold();
        let r = accept_one(&ep, &alpn).await;
        pending.release();
        let obj = Object::new(ctx2.clone())?;
        match r.map_err(rquickjs::Error::Io)? {
          Some((conn, send, recv)) => {
            let stream = P2pStream::create(&ctx2, conn, send, recv)?;
            obj.set("value", stream)?;
            obj.set("done", false)?;
          }
          None => {
            obj.set("value", Value::new_undefined(ctx2.clone()))?;
            obj.set("done", true)?;
          }
        }
        Ok(obj)
      })))
    })?;
    iter.set("next", next_fn)?;

    let attach: Function = ctx.eval("(o) => { o[Symbol.asyncIterator] = function () { return this; }; }")?;
    attach.call::<_, ()>((iter.clone(),))?;
    Ok(iter)
  }

  /// Snapshot of how the connection to `id` is currently carried. Resolves to
  /// `{ path, addrs }` where `path` is `"direct"` (a direct IP path is active),
  /// `"relay"` (only a relay path is active), `"mixed"` (both), or `"none"`, and
  /// `addrs` lists every known transport address as `{ kind, addr, active }`.
  /// iroh starts on the relay and upgrades to direct after hole-punching, so
  /// poll this to watch the path settle.
  #[qjs(rename = "connInfo")]
  pub fn conn_info<'js>(
    &self,
    ctx: Ctx<'js>,
    id: String,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Object<'js>>>>> {
    let ep = self.inner.clone();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      let id: EndpointId = id.parse().map_err(|e| rquickjs::Error::Io(io_err(e)))?;
      pending.hold();
      let info = ep.remote_info(id).await;
      pending.release();

      let obj = Object::new(ctx2.clone())?;
      let addrs = Array::new(ctx2.clone())?;
      let (mut has_direct, mut has_relay) = (false, false);
      if let Some(info) = info {
        for (i, ta) in info.addrs().enumerate() {
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
          let entry = Object::new(ctx2.clone())?;
          entry.set("kind", kind)?;
          entry.set("addr", addr.to_string())?;
          entry.set("active", active)?;
          addrs.set(i, entry)?;
        }
      }
      let path = match (has_direct, has_relay) {
        (true, true) => "mixed",
        (true, false) => "direct",
        (false, true) => "relay",
        (false, false) => "none",
      };
      obj.set("path", path)?;
      obj.set("addrs", addrs)?;
      Ok(obj)
    }))
  }

  /// Close the endpoint, ending any `accept` iteration.
  pub fn close<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<()>>>> {
    let ep = self.inner.clone();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    Ok(Promised(async move {
      pending.hold();
      ep.close().await;
      pending.release();
      Ok(())
    }))
  }
}

/// `next()` of the `accept` async-iterable: a promise resolving to an iterator
/// result object (boxed so the closure has a nameable return type).
type AcceptStep<'js> = Promised<Pin<Box<dyn Future<Output = rquickjs::Result<Object<'js>>> + 'js>>>;

/// A single bidirectional p2p stream: a byte duplex. It is its own async
/// iterator (`for await` reads the recv half); writes go through a writer task.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "P2pStream")]
pub struct P2pStream {
  /// Held only to keep the QUIC connection (and thus the stream) alive for this
  /// stream's lifetime; dropped with the JS object.
  #[qjs(skip_trace)]
  conn: Connection,
  #[qjs(skip_trace)]
  recv: Rc<RefCell<Option<RecvStream>>>,
  #[qjs(skip_trace)]
  tx: mpsc::UnboundedSender<WriteMsg>,
}

impl P2pStream {
  /// Build the JS stream object: spawn its writer task and make it iterable.
  fn create<'js>(
    ctx: &Ctx<'js>,
    conn: Connection,
    send: SendStream,
    recv: RecvStream,
  ) -> rquickjs::Result<Class<'js, P2pStream>> {
    let (tx, rx) = mpsc::unbounded_channel::<WriteMsg>();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let logger = ctx.logger();
    pending.hold();
    ctx.spawn(async move {
      run_writer(send, rx, &logger).await;
      pending.release();
    });

    let inst = Class::instance(ctx.clone(), P2pStream { conn, recv: Rc::new(RefCell::new(Some(recv))), tx })?;
    let attach: Function = ctx.eval("(o) => { o[Symbol.asyncIterator] = function () { return this; }; }")?;
    attach.call::<_, ()>((inst.clone(),))?;
    Ok(inst)
  }
}

#[rquickjs::methods]
impl P2pStream {
  /// Async-iterator step: resolve `{ value: Uint8Array, done: false }` for the
  /// next chunk, or `{ done: true }` at end-of-stream. Pull-based, so the
  /// transport only advances as JS iterates.
  pub fn next<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Object<'js>>>>> {
    let cell = self.recv.clone();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      let obj = Object::new(ctx2.clone())?;
      // Take the recv half out so no borrow is held across the await; if it is
      // gone (closed, or a concurrent step took it), report done.
      let Some(mut recv) = cell.borrow_mut().take() else {
        obj.set("value", Value::new_undefined(ctx2.clone()))?;
        obj.set("done", true)?;
        return Ok(obj);
      };
      pending.hold();
      let mut buf = vec![0u8; READ_CHUNK];
      // iroh's inherent read returns Ok(None) at end-of-stream, Ok(Some(n)) for
      // n bytes read.
      let n = recv.read(&mut buf).await;
      pending.release();
      match n {
        Ok(None) => {
          obj.set("value", Value::new_undefined(ctx2.clone()))?;
          obj.set("done", true)?;
        }
        Ok(Some(n)) => {
          buf.truncate(n);
          *cell.borrow_mut() = Some(recv);
          obj.set("value", TypedArray::<u8>::new(ctx2.clone(), buf)?)?;
          obj.set("done", false)?;
        }
        Err(e) => return Err(rquickjs::Error::Io(e.into())),
      }
      Ok(obj)
    }))
  }

  /// Queue bytes (string or Uint8Array) on the send half.
  pub fn write(&self, data: Value<'_>) -> rquickjs::Result<()> {
    let bytes = extract_body_value(&data, "P2pStream.write")?;
    let _ = self.tx.send(WriteMsg::Data(bytes));
    Ok(())
  }

  /// Finish the send half (QUIC FIN) after any queued writes flush. The recv
  /// half stays open for replies.
  pub fn finish(&self) -> rquickjs::Result<()> {
    let _ = self.tx.send(WriteMsg::Finish);
    Ok(())
  }

  /// Tear the stream down: finish the send half and stop reading.
  pub fn close(&self) -> rquickjs::Result<()> {
    let _ = self.tx.send(WriteMsg::Finish);
    self.recv.borrow_mut().take();
    Ok(())
  }

  /// The remote peer's endpoint id.
  #[qjs(get, rename = "remoteId")]
  pub fn remote_id(&self) -> String {
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
fn parse_dial(s: &str) -> Result<EndpointAddr, std::io::Error> {
  let mut parts = s.split('|');
  let id: EndpointId = parts.next().unwrap_or("").trim().parse().map_err(io_err)?;
  let mut addrs: Vec<TransportAddr> = Vec::new();
  if let Some(relay) = parts.next().filter(|s| !s.is_empty()) {
    addrs.push(TransportAddr::Relay(relay.parse().map_err(io_err)?));
  }
  if let Some(ips) = parts.next() {
    for ip in ips.split(',').filter(|s| !s.is_empty()) {
      addrs.push(TransportAddr::Ip(ip.parse::<SocketAddr>().map_err(io_err)?));
    }
  }
  Ok(EndpointAddr::from_parts(id, addrs))
}

/// Build and bind an iroh endpoint. Returns it together with the (possibly
/// generated) secret-key bytes so the class can expose them for persistence.
async fn build_endpoint(
  secret: Option<[u8; 32]>,
  relay_url: Option<String>,
  alpns: Vec<Vec<u8>>,
) -> Result<(IrohEndpoint, [u8; 32]), std::io::Error> {
  let secret_key = match secret {
    Some(bytes) => SecretKey::from_bytes(&bytes),
    None => SecretKey::generate(),
  };
  let bytes = secret_key.to_bytes();

  let mut builder = IrohEndpoint::builder(presets::N0).secret_key(secret_key).alpns(alpns);
  if let Some(url) = relay_url {
    let relay: RelayUrl = url.parse().map_err(io_err)?;
    builder = builder.relay_mode(RelayMode::custom([relay]));
  }
  log::warn!("[p2p] build_endpoint: binding...");
  let endpoint = builder.bind().await.map_err(io_err)?;
  log::warn!("[p2p] build_endpoint: bind returned");
  Ok((endpoint, bytes))
}

/// Accept the next incoming connection matching `alpn` and open its first
/// bidirectional stream. Returns `None` once the endpoint stops accepting.
/// Non-matching or failed connections are skipped.
async fn accept_one(
  ep: &IrohEndpoint,
  alpn: &[u8],
) -> Result<Option<(Connection, SendStream, RecvStream)>, std::io::Error> {
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
async fn run_writer(mut send: SendStream, mut rx: mpsc::UnboundedReceiver<WriteMsg>, logger: &Logger) {
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

pub struct P2pModule;

impl ModuleDef for P2pModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("Endpoint")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let ctor = Class::<P2pEndpoint>::create_constructor(ctx)?.expect("Endpoint class has a constructor");
    exports.export("Endpoint", ctor)?;
    Ok(())
  }
}

/// Parse the `create` options object into native config. `opts` may be absent.
fn parse_create_opts<'js>(
  ctx: &Ctx<'js>,
  opts: Option<Object<'js>>,
) -> rquickjs::Result<(Option<[u8; 32]>, Option<String>, Vec<Vec<u8>>)> {
  let Some(opts) = opts else {
    return Ok((None, None, Vec::new()));
  };
  let secret = match opts.get::<_, Option<String>>("secretKey")? {
    Some(s) => Some(decode_hex32(ctx, &s)?),
    None => None,
  };
  let relay_url = opts.get::<_, Option<String>>("relayUrl")?;
  let alpns = opts
    .get::<_, Option<Vec<String>>>("protocols")?
    .unwrap_or_default()
    .into_iter()
    .map(String::into_bytes)
    .collect();
  Ok((secret, relay_url, alpns))
}