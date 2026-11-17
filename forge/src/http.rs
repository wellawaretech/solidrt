//! Engine-free HTTP server core.
//!
//! The scripting-engine-independent foundation of the `flux:http` server:
//! routing, request and reply plumbing, connection lifecycle, and graceful
//! shutdown. It names no scripting-engine types, and no hyper type crosses
//! its boundary: a request reaches the host as `RequestParts`, the host
//! answers with a `Reply`, and a websocket upgrade travels as an opaque
//! `UpgradeHandle`. Everything that must call back into a script is
//! expressed generically: an async `Fn(RequestParts) -> Reply` for request
//! dispatch, an `FnMut` for spawning connection tasks. The marshalling layer
//! (flux `forge_plugins/serve.rs`) monomorphizes those generics with its
//! engine-bound types and owns every `ctx.spawn`.
//!
//! A connection is plain HTTP/1.1 (`serve_connection`) or HTTPS
//! (`serve_tls_connection`): the handshake runs in the connection's own task,
//! then `h2` by ALPN is served as HTTP/2 and anything else as HTTP/1.1.

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Frame, Incoming};
use hyper::server::conn::{http1, http2};
use hyper::service::{service_fn, Service};
use hyper::upgrade::OnUpgrade;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use percent_encoding::percent_decode_str;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_rustls::TlsAcceptor;

use crate::logger::Logger;
use crate::stream::{to_byte_stream, ByteStream};
use crate::tls;

// ---- Response bodies -------------------------------------------------------

/// One boxed body type for every serve response, so buffered (`Full`) and
/// streamed (`ChannelBody`) responses share a single hyper body type. Bodies
/// never produce an error, hence `Infallible`.
pub(crate) type ResBody = BoxBody<Bytes, Infallible>;

fn full_body(bytes: Vec<u8>) -> ResBody {
  Full::new(Bytes::from(bytes)).boxed()
}

/// A streamed response body: hyper pulls frames as the producer task sends bytes.
/// The stream ends (EOF) when the sender is dropped (producer finished or errored).
struct ChannelBody {
  rx: mpsc::Receiver<Vec<u8>>,
}

impl Body for ChannelBody {
  type Data = Bytes;
  type Error = Infallible;

  fn poll_frame(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
    match self.rx.poll_recv(cx) {
      Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(Frame::data(Bytes::from(bytes))))),
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
    }
  }
}

/// Build a streamed response body and the sender that feeds it. The producer task
/// sends chunks; dropping the sender ends the body (EOF). Used for responses
/// whose bytes are produced after the handler returns (chunked transfer
/// encoding, no Content-Length).
fn channel_body() -> (mpsc::Sender<Vec<u8>>, ResBody) {
  let (tx, rx) = mpsc::channel::<Vec<u8>>(16);
  (tx, ChannelBody { rx }.boxed())
}

fn text_response(status: StatusCode, body: &str) -> HyperResponse<ResBody> {
  HyperResponse::builder()
    .status(status)
    .header("Content-Type", "text/plain")
    .body(full_body(body.as_bytes().to_vec()))
    .expect("build response")
}

/// Assemble a hyper response from already-extracted parts and an (already boxed)
/// body. Defaults the Content-Type to text/plain when the headers don't set one.
/// Shared by buffered and streamed responses alike.
fn build_response(status: u16, headers: &[(String, String)], body: ResBody) -> HyperResponse<ResBody> {
  let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
  let mut builder = HyperResponse::builder().status(status);
  let mut has_content_type = false;
  for (k, v) in headers {
    if k.eq_ignore_ascii_case("content-type") {
      has_content_type = true;
    }
    builder = builder.header(k.as_str(), v.as_str());
  }
  if !has_content_type {
    builder = builder.header("Content-Type", "text/plain");
  }
  builder.body(body).unwrap_or_else(|_| text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"))
}

/// What a request handler answers with, opaque to the host: a buffered body
/// (`full`, `text`), a streamed one (`streamed`, fed through the returned
/// sender), or the 101 of an accepted websocket upgrade
/// (`websocket::Handshake::into_parts`).
pub struct Reply(HyperResponse<ResBody>);

impl Reply {
  /// A plain-text reply; an invalid status code becomes 500.
  pub fn text(status: u16, body: &str) -> Reply {
    Reply(text_response(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), body))
  }

  /// A buffered reply. Content-Type defaults to text/plain when `headers` do
  /// not set one.
  pub fn full(status: u16, headers: &[(String, String)], body: Vec<u8>) -> Reply {
    Reply(build_response(status, headers, full_body(body)))
  }

  /// A streamed reply (chunked transfer encoding): the producer sends chunks
  /// through the sender and drops it to end the body.
  pub fn streamed(status: u16, headers: &[(String, String)]) -> (mpsc::Sender<Vec<u8>>, Reply) {
    let (tx, body) = channel_body();
    (tx, Reply(build_response(status, headers, body)))
  }

  pub(crate) fn from_hyper(response: HyperResponse<ResBody>) -> Reply {
    Reply(response)
  }
}

// ---- Requests --------------------------------------------------------------

/// A request's websocket upgrade capability, carried on `RequestParts` for
/// the host to hand back through `websocket::accept_upgrade`. Opaque: it wraps
/// hyper's upgrade handle.
pub struct UpgradeHandle(OnUpgrade);

impl UpgradeHandle {
  pub(crate) fn into_inner(self) -> OnUpgrade {
    self.0
  }
}

/// An incoming request as the host sees it. The `body` is the request stream,
/// read incrementally by the handler rather than buffered up front, so large
/// uploads stay constant-memory; a handler that never reads it just drops the
/// unread stream.
pub struct RequestParts {
  pub method: String,
  pub url: String,
  pub headers: Vec<(String, String)>,
  pub body: ByteStream,
  /// Present on every HTTP/1 request hyper can upgrade; consumed by the
  /// host's `server.upgrade(req)`.
  pub upgrade: Option<UpgradeHandle>,
  /// The connection's peer, for `server.requestIP(req)` and `ws.remoteAddr`.
  pub remote: Option<Remote>,
  /// The client certificate's key in the `peerKey` form (`tls::peer_key`),
  /// for `server.peerKey(req)`. `None` without a client certificate or on a
  /// plain connection.
  pub peer_key: Option<String>,
}

impl RequestParts {
  fn from_hyper(mut req: HyperRequest<Incoming>, remote: Option<Remote>, peer_key: Option<String>) -> RequestParts {
    let method = req.method().as_str().to_string();
    // HTTP/2 carries the target as `:scheme`, `:authority` and `:path`, which
    // hyper joins into an absolute URI and sends no `Host` header; the host
    // sees what HTTP/1.1 sends, the origin form (`/x?y`) plus a `Host`, so
    // routes and handlers read the same either way.
    let url = match (req.version(), req.uri().path_and_query()) {
      (hyper::Version::HTTP_2, Some(path)) => path.to_string(),
      _ => req.uri().to_string(),
    };
    let mut headers: Vec<(String, String)> = req
      .headers()
      .iter()
      .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string())))
      .collect();
    if req.version() == hyper::Version::HTTP_2 && !headers.iter().any(|(name, _)| name == "host") {
      if let Some(authority) = req.uri().authority() {
        headers.insert(0, ("host".to_string(), authority.to_string()));
      }
    }
    let upgrade = req.extensions_mut().remove::<OnUpgrade>().map(UpgradeHandle);
    let body = to_byte_stream(req.into_body().into_data_stream());
    RequestParts { method, url, headers, body, upgrade, remote, peer_key }
  }
}

// ---- Routing ---------------------------------------------------------------

/// One `/`-delimited segment of a route pattern.
enum Segment {
  Literal(String),
  Param(String),
  Wildcard,
}

/// Split a pattern like `/users/:id/*` into segments and compute its match tier
/// (0 = exact, 1 = has a `:param`, 2 = has a `*`).
fn parse_pattern(pattern: &str) -> (Vec<Segment>, u8) {
  let mut segments = Vec::new();
  let mut tier = 0u8;
  for part in pattern.split('/').filter(|s| !s.is_empty()) {
    if part == "*" {
      segments.push(Segment::Wildcard);
      tier = 2;
    } else if let Some(name) = part.strip_prefix(':') {
      segments.push(Segment::Param(name.to_string()));
      tier = tier.max(1);
    } else {
      segments.push(Segment::Literal(part.to_string()));
    }
  }
  (segments, tier)
}

/// Match request path segments against a pattern, capturing `:param` values. A
/// trailing `*` matches the remaining segments (including none).
fn match_segments(segments: &[Segment], path: &[&str]) -> Option<Vec<(String, String)>> {
  let mut params = Vec::new();
  let mut i = 0;
  for seg in segments {
    match seg {
      Segment::Wildcard => return Some(params),
      Segment::Literal(lit) => {
        if path.get(i) != Some(&lit.as_str()) {
          return None;
        }
        i += 1;
      }
      Segment::Param(name) => {
        // Decode per-segment so an encoded %2F stays inside the value rather
        // than acting as a path separator. Lossy: invalid UTF-8 won't reject.
        let value = percent_decode_str(path.get(i)?).decode_utf8_lossy().into_owned();
        params.push((name.clone(), value));
        i += 1;
      }
    }
  }
  // Without a wildcard the path must be fully consumed (no extra segments).
  (i == path.len()).then_some(params)
}

/// A registered route: its compiled pattern, a match `tier` (0 = exact, 1 = has a
/// `:param`, 2 = has a `*`), and the handler payload to run. Generic over the
/// handler type `H` so the engine-free router carries whatever the marshalling
/// layer needs (a script function, a static snapshot, a per-method table).
pub struct Route<H> {
  segments: Vec<Segment>,
  tier: u8,
  pub handler: H,
}

impl<H> Route<H> {
  /// Compile `pattern` and pair it with its handler payload.
  pub fn new(pattern: &str, handler: H) -> Self {
    let (segments, tier) = parse_pattern(pattern);
    Route { segments, tier, handler }
  }
}

/// A compiled route table. Routes are pre-sorted by `tier` so exact patterns beat
/// `:param` patterns beat `*`; within a tier, registration order is kept.
pub struct RouteTable<H> {
  routes: Vec<Route<H>>,
}

impl<H> RouteTable<H> {
  /// Build a table from routes, sorting by tier. A stable sort keeps registration
  /// order within each tier.
  pub fn from_routes(mut routes: Vec<Route<H>>) -> Self {
    routes.sort_by_key(|r| r.tier);
    RouteTable { routes }
  }

  /// Return the first matching route's handler and its captured path params.
  pub fn lookup(&self, path: &str) -> Option<(&H, Vec<(String, String)>)> {
    let path_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for route in &self.routes {
      if let Some(params) = match_segments(&route.segments, &path_segs) {
        return Some((&route.handler, params));
      }
    }
    None
  }
}

// ---- Server lifecycle ------------------------------------------------------

/// Shutdown signal shared between a server handle, its accept loop, and each
/// connection task. A `watch` channel latches (once true it stays true) and
/// broadcasts to every subscriber. Held in an `Arc` so the accept loop keeps it
/// alive independent of any handle's lifetime: dropping the handle leaves the
/// server running.
pub struct ServerShared {
  shutdown: watch::Sender<bool>,
}

impl ServerShared {
  pub fn new() -> Arc<Self> {
    let (shutdown, _) = watch::channel(false);
    Arc::new(ServerShared { shutdown })
  }

  pub fn subscribe(&self) -> watch::Receiver<bool> {
    self.shutdown.subscribe()
  }

  /// Signal shutdown. A send error means there are no live subscribers (the loop
  /// already exited), i.e. already stopped.
  pub fn stop(&self) {
    let _ = self.shutdown.send(true);
  }
}

/// Resolve once a stop has been signalled (value `true`). A dropped sender also
/// resolves it: nothing can signal a stop anymore, so treat it as one.
pub async fn wait_for_stop(rx: &mut watch::Receiver<bool>) {
  let _ = rx.wait_for(|&stop| stop).await;
}

/// Bind a non-blocking TCP listener for the HTTP server, registered with tokio.
/// Returns a descriptive message on failure for the caller to surface.
pub fn bind_listener(hostname: &str, port: u16) -> Result<TcpListener, String> {
  let addr = format!("{hostname}:{port}");
  let listener = std::net::TcpListener::bind(&addr).map_err(|e| format!("serve: failed to bind {addr}: {e}"))?;
  listener.set_nonblocking(true).map_err(|e| format!("serve: failed to configure listener on {addr}: {e}"))?;
  TcpListener::from_std(listener).map_err(|e| format!("serve: failed to register listener on {addr}: {e}"))
}

/// The peer on the other side of an accepted connection: a TCP peer address, or
/// a p2p peer's endpoint id. Carried onto requests and upgraded sockets so the
/// marshalling layer can expose it (`server.requestIP`, `ws.remoteAddress`).
#[derive(Clone)]
pub enum Remote {
  Ip(std::net::SocketAddr),
  Peer(String),
}

/// Spawns hyper's per-stream HTTP/2 tasks through the host's spawner,
/// which runs them on the host's (single-threaded) executor; hyper's
/// `Executor` has no `Send` bound, and the host's request futures are not
/// `Send`. Generic over the lifetime `'a` of what it spawns: a host's
/// spawner may take futures that borrow from it (a script engine's spawn
/// takes futures bound to the engine's lifetime), so it is not tied to
/// `'static`.
#[derive(Clone)]
pub struct LocalExecutor<'a>(Rc<dyn Fn(LocalTask<'a>) + 'a>);

/// A task `LocalExecutor` hands to the host's spawner.
pub type LocalTask<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

impl<'a> LocalExecutor<'a> {
  pub fn new(spawn: impl Fn(LocalTask<'a>) + 'a) -> Self {
    LocalExecutor(Rc::new(spawn))
  }
}

impl<'a, Fut: Future<Output = ()> + 'a> hyper::rt::Executor<Fut> for LocalExecutor<'a> {
  fn execute(&self, fut: Fut) {
    (self.0)(Box::pin(fut))
  }
}

/// The HTTP version a connection is served with.
pub enum Protocol {
  Http1,
  Http2,
}

impl Protocol {
  /// The protocol of an agreed ALPN: `h2` is HTTP/2; `http/1.1`, or no
  /// ALPN at all, is HTTP/1.1.
  pub fn from_alpn(alpn: Option<&str>) -> Protocol {
    match alpn {
      Some("h2") => Protocol::Http2,
      _ => Protocol::Http1,
    }
  }
}

/// The hyper service both protocols serve: each request becomes
/// `RequestParts` (with the connection's `remote` and `peer_key`) for the
/// host's `handler`.
fn service<H, Fut>(
  handler: H,
  remote: Option<Remote>,
  peer_key: Option<String>,
) -> impl Service<
  HyperRequest<Incoming>,
  Response = HyperResponse<ResBody>,
  Error = Infallible,
  Future = impl Future<Output = Result<HyperResponse<ResBody>, Infallible>>,
>
where
  H: Fn(RequestParts) -> Fut,
  Fut: Future<Output = Reply>,
{
  service_fn(move |req: HyperRequest<Incoming>| {
    let reply = handler(RequestParts::from_hyper(req, remote.clone(), peer_key.clone()));
    async move { Ok::<_, Infallible>(reply.await.0) }
  })
}

/// Whether a connection error is the peer having hung up: an I/O error,
/// anywhere in the chain, that says the other side closed or reset the
/// connection. A server that asked the connection to close expects this.
fn peer_hung_up(e: &hyper::Error) -> bool {
  let mut source = std::error::Error::source(e);
  while let Some(err) = source {
    if let Some(io) = err.downcast_ref::<std::io::Error>() {
      return matches!(
        io.kind(),
        std::io::ErrorKind::BrokenPipe
          | std::io::ErrorKind::ConnectionReset
          | std::io::ErrorKind::ConnectionAborted
          | std::io::ErrorKind::NotConnected
          | std::io::ErrorKind::UnexpectedEof
      );
    }
    source = err.source();
  }
  false
}

/// Run a hyper connection `conn` to its end. On a stop signal, finish any
/// in-flight request then close (`graceful` asks the connection to); an idle
/// keep-alive connection has nothing in flight, so it closes promptly.
/// Closing is a handshake with the peer (an HTTP/2 GOAWAY and PING, the TLS
/// close_notify), and a peer that hangs up before it ends has done what the
/// server asked: its reset or broken pipe ends the connection without a
/// report. Every other error, and any error before the stop, is reported.
async fn drive<C>(conn: C, graceful: impl FnOnce(Pin<&mut C>), logger: Logger, mut shutdown_rx: watch::Receiver<bool>)
where
  C: Future<Output = Result<(), hyper::Error>>,
{
  tokio::pin!(conn);
  tokio::select! {
    res = conn.as_mut() => {
      if let Err(e) = res {
        logger.warn(&format!("[flux] serve connection error: {e}"));
      }
    }
    _ = wait_for_stop(&mut shutdown_rx) => {
      graceful(conn.as_mut());
      match conn.as_mut().await {
        Err(e) if !peer_hung_up(&e) => logger.warn(&format!("[flux] serve connection error: {e}")),
        _ => {}
      }
    }
  }
}

/// Serve one accepted connection: run HTTP/1 with websocket upgrades and graceful
/// shutdown. On a stop signal, finish any in-flight request then close; an idle
/// keep-alive connection has nothing in flight, so it closes promptly. Generic
/// over the request `handler` so the engine-free core never names the host's
/// (script-bound) types, and over the `io` so any byte duplex serves (an accepted
/// TCP socket, a p2p connection's `ConnIo`). `remote` rides onto every request
/// the connection carries. Plain HTTP is HTTP/1.1 only (no h2c).
pub async fn serve_connection<I, H, Fut>(
  io: I,
  remote: Option<Remote>,
  handler: H,
  logger: Logger,
  shutdown_rx: watch::Receiver<bool>,
) where
  I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
  H: Fn(RequestParts) -> Fut,
  Fut: Future<Output = Reply>,
{
  let service = service(handler, remote, None);
  // with_upgrades keeps the connection alive past a 101 response so hyper can
  // hand the raw stream to the websocket tasks.
  let conn = http1::Builder::new().serve_connection(TokioIo::new(io), service).with_upgrades();
  drive(conn, |conn| conn.graceful_shutdown(), logger, shutdown_rx).await;
}

/// Serve one accepted TCP connection over TLS. The handshake runs here, in
/// the connection's own task, bounded by `tls::HANDSHAKE_TIMEOUT`, so a
/// stalled client holds up no other connection; a failed or timed out
/// handshake is logged and drops the connection. Then the protocol agreed by
/// ALPN decides: `h2` is served by hyper's HTTP/2 server, its streams spawned
/// through `exec`; `http/1.1` (or no ALPN) as `serve_connection` does, with
/// websocket upgrades. The upgrade is HTTP/1.1 only (no RFC 8441 extended
/// CONNECT), so a browser opens its WebSocket on a separate HTTP/1.1
/// connection. Both shut down gracefully on the stop signal; a stop during
/// the handshake drops the connection. The client certificate's key, if any,
/// rides onto every request as `peer_key`.
pub async fn serve_tls_connection<'a, H, Fut>(
  stream: TcpStream,
  config: Arc<tls::ServerConfig>,
  remote: Option<Remote>,
  handler: H,
  logger: Logger,
  mut shutdown_rx: watch::Receiver<bool>,
  exec: LocalExecutor<'a>,
) where
  H: Fn(RequestParts) -> Fut + 'a,
  Fut: Future<Output = Reply> + 'a,
{
  let accept = TlsAcceptor::from(config).accept(stream);
  // A stop during the handshake drops the connection: nothing was served
  // on it yet, and a server that closed answers no new request.
  let handshake = tokio::select! {
    r = tokio::time::timeout(tls::HANDSHAKE_TIMEOUT, accept) => r,
    _ = wait_for_stop(&mut shutdown_rx) => return,
  };
  let stream = match handshake {
    Ok(Ok(stream)) => stream,
    failed => {
      let reason = match failed {
        Ok(Err(e)) => tls::handshake_error(&e),
        _ => format!("timed out after {:?}", tls::HANDSHAKE_TIMEOUT),
      };
      let peer = match &remote {
        Some(Remote::Ip(addr)) => addr.to_string(),
        Some(Remote::Peer(id)) => id.clone(),
        None => "an unknown peer".to_string(),
      };
      logger.warn(&format!("[flux] serve TLS handshake with {peer}: {reason}"));
      return;
    }
  };
  let (_, session) = stream.get_ref();
  let protocol = Protocol::from_alpn(tls::alpn_protocol(session).as_deref());
  let service = service(handler, remote, tls::peer_key(session));
  let io = TokioIo::new(stream);
  match protocol {
    Protocol::Http2 => {
      let conn = http2::Builder::new(exec).serve_connection(io, service);
      drive(conn, |conn| conn.graceful_shutdown(), logger, shutdown_rx).await;
    }
    Protocol::Http1 => {
      let conn = http1::Builder::new().serve_connection(io, service).with_upgrades();
      drive(conn, |conn| conn.graceful_shutdown(), logger, shutdown_rx).await;
    }
  }
}

/// Accept connections until shutdown, handing each accepted socket and its
/// peer to `on_conn` (which the marshalling layer uses to spawn a connection
/// task). Owns the accept/stop `select!` and accept-error logging; the spawn
/// itself stays with the caller so the engine-free core never spawns
/// engine-bound tasks.
pub async fn accept_loop<F>(
  listener: TcpListener,
  logger: Logger,
  mut shutdown_rx: watch::Receiver<bool>,
  mut on_conn: F,
) where
  F: FnMut(Option<Remote>, TcpStream),
{
  loop {
    tokio::select! {
      accepted = listener.accept() => {
        match accepted {
          Ok((sock, _)) => {
            let remote = sock.peer_addr().ok().map(Remote::Ip);
            on_conn(remote, sock)
          }
          Err(e) => logger.warn(&format!("[flux] serve accept error: {e}")),
        }
      }
      _ = wait_for_stop(&mut shutdown_rx) => break,
    }
  }
}

#[cfg(feature = "p2p")]
/// The p2p sibling of `accept_loop`: accept incoming connections on `endpoint`
/// whose ALPN matches `alpn` until shutdown (or the endpoint closes), handing
/// each connection's first bi-stream to `on_conn` as a byte duplex plus the
/// remote peer. One QUIC connection carries one HTTP connection, mirroring how
/// the dev tunnel client dials (lattice/src/go/tunnel.rs). The shutdown only
/// stops accepting; the endpoint stays open for its owner.
pub async fn accept_loop_p2p<F>(
  endpoint: crate::p2p::Endpoint,
  alpn: Vec<u8>,
  logger: Logger,
  mut shutdown_rx: watch::Receiver<bool>,
  mut on_conn: F,
) where
  F: FnMut(Remote, crate::p2p::ConnIo),
{
  loop {
    tokio::select! {
      accepted = endpoint.accept_io(&alpn) => {
        match accepted {
          Ok(Some((remote, io))) => on_conn(Remote::Peer(remote), io),
          Ok(None) => break,
          Err(e) => logger.warn(&format!("[flux] serve p2p accept error: {e}")),
        }
      }
      _ = wait_for_stop(&mut shutdown_rx) => break,
    }
  }
}
