//! The HTTPS server of `forge::http`, built from its pieces the way flux:http's
//! `serve` builds it: `bind_listener`, `ServerShared`, `accept_loop`, and a
//! `serve_tls_connection` task per socket, all on a `LocalSet` with
//! `spawn_local`, the HTTP/2 streams spawned through a `LocalExecutor`.

use std::future::{poll_fn, Future};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::task::{spawn_local, LocalSet};

use super::tls::{server_config_of, CA, CLIENT, CLIENT_KEY, CLIENT_PIN, SERVER, SERVER_KEY};
use super::websocket::run;
use crate::fetch::{send, Client};
use crate::http::*;
use crate::logger::default_logger;
use crate::stream::ByteStream;
use crate::tls::{client_config, ClientAuth, ClientOptions, ServerConfig};
use crate::websocket::{
  accept_upgrade, run_reader, run_writer, Kind, PendingSocket, SocketSink, Topics, WsDispatch,
  DEFAULT_BACKPRESSURE_LIMIT,
};

/// The protocols the server accepts by ALPN, as flux:http's `serve` does.
const SERVE_ALPN: [&str; 2] = ["h2", "http/1.1"];

/// How long a round trip may take beside a stalled client: well under the
/// handshake limit, so a handshake that waited on the stalled one fails it.
const STALL_BUDGET: Duration = Duration::from_secs(2);

/// Start an HTTPS server on 127.0.0.1 over `config`, answering with
/// `handler`; its port and its shutdown signal. Must run on a `LocalSet`.
fn https_server<H, Fut>(config: Arc<ServerConfig>, handler: H) -> (u16, Arc<ServerShared>)
where
  H: Fn(RequestParts) -> Fut + Clone + 'static,
  Fut: Future<Output = Reply> + 'static,
{
  let listener = bind_listener("127.0.0.1", 0).expect("bind");
  let port = listener.local_addr().expect("bound address").port();
  let shared = ServerShared::new();
  let loop_shared = shared.clone();
  spawn_local(async move {
    let exec = LocalExecutor::new(|fut| {
      spawn_local(fut);
    });
    let shutdown_rx = loop_shared.subscribe();
    let on_conn = move |remote, sock| {
      let conn = serve_tls_connection(
        sock,
        config.clone(),
        remote,
        handler.clone(),
        default_logger(),
        loop_shared.subscribe(),
        exec.clone(),
      );
      spawn_local(conn);
    };
    accept_loop(listener, default_logger(), shutdown_rx, on_conn).await;
  });
  (port, shared)
}

/// Answers every request with its method, url and `Host` header: over h2
/// the server synthesizes the header from `:authority`, so both protocols
/// read the same.
async fn echo_request(parts: RequestParts) -> Reply {
  let host = parts.headers.iter().find(|(name, _)| name == "host").map(|(_, v)| v.as_str()).unwrap_or("-");
  Reply::text(200, &format!("{} {} {host}", parts.method, parts.url))
}

/// Answers every request with the client certificate's key, empty without one.
async fn echo_peer_key(parts: RequestParts) -> Reply {
  Reply::text(200, parts.peer_key.as_deref().unwrap_or(""))
}

fn ca_server(client_auth: ClientAuth) -> Arc<ServerConfig> {
  server_config_of(SERVER, SERVER_KEY, client_auth, &SERVE_ALPN)
}

fn private_ca() -> ClientOptions {
  ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() }
}

/// A fetch client (offering `h2` and `http/1.1`) over `opts`.
fn fetch_client(opts: &ClientOptions) -> Client {
  Client::with_tls("forge-test", &client_config(opts).expect("client config")).expect("fetch client")
}

async fn body_text(mut body: ByteStream) -> String {
  let mut bytes = Vec::new();
  while let Some(chunk) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
    bytes.extend_from_slice(&chunk.expect("body chunk"));
  }
  String::from_utf8(bytes).expect("utf-8 body")
}

/// GET `url` through `client`: whether it came over HTTP/2, and the body.
async fn get(client: &Client, url: &str) -> Result<(bool, String), String> {
  let resp = send(client, "GET", url, vec![], None).await?;
  assert_eq!(resp.status, 200);
  Ok((resp.http2, body_text(resp.body).await))
}

async fn local<F: Future>(fut: F) -> F::Output {
  LocalSet::new().run_until(fut).await
}

#[tokio::test]
async fn h2_round_trip_with_a_private_ca() {
  local(async {
    let (port, shared) = https_server(ca_server(ClientAuth::None), echo_request);
    let (http2, body) =
      get(&fetch_client(&private_ca()), &format!("https://127.0.0.1:{port}/x?y=1")).await.expect("fetch");
    assert!(http2, "the client agreed h2");
    assert_eq!(body, format!("GET /x?y=1 127.0.0.1:{port}"));
    shared.stop();
  })
  .await;
}

#[tokio::test]
async fn http1_round_trip_with_a_private_ca() {
  local(async {
    let (port, shared) = https_server(ca_server(ClientAuth::None), echo_request);
    let opts = ClientOptions { alpn: vec!["http/1.1".to_string()], ..private_ca() };
    let config = client_config(&opts).expect("client config");
    let client =
      reqwest::Client::builder().http1_only().use_preconfigured_tls((*config).clone()).build().expect("reqwest client");
    let resp = client.get(format!("https://127.0.0.1:{port}/x")).send().await.expect("fetch");
    assert_eq!(resp.version(), reqwest::Version::HTTP_11);
    assert_eq!(resp.text().await.expect("body"), format!("GET /x 127.0.0.1:{port}"));
    shared.stop();
  })
  .await;
}

#[tokio::test]
async fn any_client_cert_reports_the_peer_key() {
  local(async {
    let (port, shared) = https_server(ca_server(ClientAuth::AnyKey), echo_peer_key);
    let url = format!("https://127.0.0.1:{port}/");
    let with_cert = ClientOptions { cert: Some(CLIENT.to_vec()), key: Some(CLIENT_KEY.to_vec()), ..private_ca() };
    let (http2, body) = get(&fetch_client(&with_cert), &url).await.expect("fetch with a client certificate");
    assert!(http2);
    assert_eq!(body, CLIENT_PIN);
    // `AnyKey` requires a certificate: rustls refuses the handshake of a
    // client without one (over TLS 1.3 the client hears of it on its first
    // read), so the request fails rather than reaching the handler.
    assert!(get(&fetch_client(&private_ca()), &url).await.is_err(), "the server refuses a client without one");
    shared.stop();
  })
  .await;
}

#[tokio::test]
async fn a_stalled_client_does_not_hold_up_the_next_handshake() {
  local(async {
    let (port, shared) = https_server(ca_server(ClientAuth::None), echo_request);
    // Connects and never sends its ClientHello.
    let _stalled = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.expect("raw connect");
    let url = format!("https://127.0.0.1:{port}/");
    let client = fetch_client(&private_ca());
    let round_trip = tokio::time::timeout(STALL_BUDGET, get(&client, &url));
    let (_, body) = round_trip.await.expect("within the budget").expect("fetch");
    assert_eq!(body, format!("GET / 127.0.0.1:{port}"));
    shared.stop();
  })
  .await;
}

/// Echoes every text message back on its socket.
struct Echo;

impl WsDispatch for Echo {
  type Handle = Rc<SocketSink>;

  fn on_text(&self, sink: &Rc<SocketSink>, text: String) {
    sink.enqueue(Kind::Text, text.into_bytes());
  }
  fn on_binary(&self, _sink: &Rc<SocketSink>, _bytes: Vec<u8>) {}
  fn on_pong(&self, _sink: &Rc<SocketSink>, _bytes: Vec<u8>) {}
  fn on_drain(&self, _sink: &Rc<SocketSink>) {}
  fn on_close(&self, _sink: &Rc<SocketSink>, _code: u16, _reason: String) {}
}

/// Run an accepted upgrade's socket with `Echo`, as the host's socket tasks do.
async fn echo_socket(socket: PendingSocket, shutdown_rx: watch::Receiver<bool>) {
  let (read, write) = socket.accept().await.expect("upgraded");
  let (sink, queue) = SocketSink::new(Topics::default(), DEFAULT_BACKPRESSURE_LIMIT);
  let logger = default_logger();
  let writer = run_writer(write, queue, sink.clone(), &Echo, &sink, &logger);
  let reader = run_reader(read, sink.clone(), Rc::new(Notify::new()), shutdown_rx, &Echo, &sink, &logger);
  tokio::join!(writer, reader);
}

#[tokio::test]
async fn a_websocket_upgrades_over_http1_on_the_https_server() {
  local(async {
    // The sockets' own stop signal: the handler exists before the server.
    let sockets = ServerShared::new();
    let upgrade_sockets = sockets.clone();
    let upgrade = move |parts: RequestParts| {
      let sockets = upgrade_sockets.clone();
      async move {
        let handle = parts.upgrade.expect("an HTTP/1.1 request can upgrade");
        let (reply, socket) = accept_upgrade(&parts.headers, handle, &[]).expect("accept upgrade").into_parts();
        spawn_local(echo_socket(socket, sockets.subscribe()));
        reply
      }
    };
    let (port, server) = https_server(ca_server(ClientAuth::None), upgrade);
    // The client offers `http/1.1` only; the server would prefer `h2`.
    let tls = client_config(&private_ca()).expect("client config");
    let events = run(&format!("wss://localhost:{port}/echo"), tls).await;
    assert_eq!(events, ["open", "text: hello", "close: 1000 true"]);
    server.stop();
    sockets.stop();
  })
  .await;
}
