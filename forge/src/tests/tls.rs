use std::future::poll_fn;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::net::*;
use crate::stream::ByteStream;
use crate::tls::*;

// The fixtures and how they were made: data/tls/README.md. The fetch tests
// use them too.
pub(super) const CA: &[u8] = include_bytes!("data/tls/ca.pem");
pub(super) const SERVER: &[u8] = include_bytes!("data/tls/server.pem");
pub(super) const SERVER_KEY: &[u8] = include_bytes!("data/tls/server-key.pem");
pub(super) const CLIENT: &[u8] = include_bytes!("data/tls/client.pem");
pub(super) const CLIENT_KEY: &[u8] = include_bytes!("data/tls/client-key.pem");
pub(super) const CLIENT_PIN: &str = include_str!("data/tls/client.pin");
pub(super) const SELF_SIGNED: &[u8] = include_bytes!("data/tls/self-signed.pem");
pub(super) const SELF_SIGNED_KEY: &[u8] = include_bytes!("data/tls/self-signed-key.pem");
pub(super) const SELF_SIGNED_PIN: &str = include_str!("data/tls/self-signed.pin");

/// How long a test waits for something that must not happen.
const QUIET_MS: u64 = 300;
const CONNECT_MS: u64 = 5000;

async fn next_chunk(stream: &mut ByteStream) -> Result<Option<Vec<u8>>, String> {
  match poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
    Some(Ok(chunk)) => Ok(Some(chunk.to_vec())),
    Some(Err(e)) => Err(e.to_string()),
    None => Ok(None),
  }
}

async fn soon<T>(fut: impl std::future::Future<Output = T>) -> T {
  tokio::time::timeout(Duration::from_secs(5), fut).await.expect("within the budget")
}

fn port_of(listener: &Listener) -> u16 {
  listener.local_addr().rsplit(':').next().and_then(|p| p.parse().ok()).expect("listener port")
}

pub(super) fn server_config_of(cert: &[u8], key: &[u8], client_auth: ClientAuth, alpn: &[&str]) -> Arc<ServerConfig> {
  server_config(&ServerOptions {
    cert: cert.to_vec(),
    key: key.to_vec(),
    alpn: alpn.iter().map(|p| p.to_string()).collect(),
    client_auth,
  })
  .expect("server config")
}

fn private_ca_client() -> ClientOptions {
  ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() }
}

fn with_client_cert(opts: ClientOptions) -> ClientOptions {
  ClientOptions { cert: Some(CLIENT.to_vec()), key: Some(CLIENT_KEY.to_vec()), ..opts }
}

/// Connect `client` to a `listen_tls` listener over `server`, as `name`:
/// the client's result, and the connection the listener accepted (if any
/// by `QUIET_MS` after the client's handshake).
async fn handshake(
  server: Arc<ServerConfig>,
  client: &ClientOptions,
  name: &str,
) -> (Result<Conn, String>, Option<Conn>, Listener) {
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let port = port_of(&listener);
  let config = client_config(client).expect("client config");
  let connect = connect_tls("127.0.0.1", port, CONNECT_MS, config, server_name(name).expect("name"));
  let (connected, accepted) = {
    // The listener takes in the TCP connection only while an accept is
    // pending, so both run together.
    let accept = listener.accept();
    tokio::pin!(connect, accept);
    let mut accepted = None;
    let mut accept_done = false;
    let connected = loop {
      tokio::select! {
        c = &mut connect => break c,
        a = &mut accept, if !accept_done => {
          accept_done = true;
          accepted = a.ok().flatten();
        }
      }
    };
    // A failed handshake leaves the accept pending: give the server a moment.
    if !accept_done {
      let late = tokio::time::timeout(Duration::from_millis(QUIET_MS), &mut accept).await;
      accepted = late.ok().and_then(|a| a.ok().flatten());
    }
    (connected, accepted)
  };
  (connected, accepted, listener)
}

#[tokio::test]
async fn round_trip_with_a_private_ca_and_alpn() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &["x-other", "x-test"]);
  let client = ClientOptions { alpn: vec!["x-test".to_string()], ..private_ca_client() };
  let (client, server, _listener) = handshake(server, &client, "localhost").await;
  let client = client.expect("client handshake");
  let server = server.expect("accepted conn");

  assert_eq!(client.alpn_protocol().as_deref(), Some("x-test"));
  assert_eq!(server.alpn_protocol().as_deref(), Some("x-test"));
  assert!(client.peer_key().is_some(), "the client sees the server's key");
  assert_eq!(server.peer_key(), None, "no client certificate was asked for");

  client.write(b"ping".to_vec()).await.expect("client write");
  assert_eq!(soon(next_chunk(&mut server.readable())).await, Ok(Some(b"ping".to_vec())));
  server.write(b"pong".to_vec()).await.expect("server write");
  assert_eq!(soon(next_chunk(&mut client.readable())).await, Ok(Some(b"pong".to_vec())));
}

#[tokio::test]
async fn the_default_roots_reject_a_private_ca() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let (client, server, _listener) = handshake(server, &ClientOptions::default(), "localhost").await;
  let err = client.err().expect("the handshake fails");
  assert!(err.contains("invalid peer certificate"), "{err}");
  assert!(server.is_none(), "a failed handshake never reaches accept");
}

#[tokio::test]
async fn the_name_is_checked_against_the_certificate() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let (client, _, _listener) = handshake(server, &private_ca_client(), "example.com").await;
  let err = client.err().expect("the handshake fails");
  assert!(err.contains("invalid peer certificate"), "{err}");
}

#[tokio::test]
async fn mutual_tls_with_ca_certs() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::Chain(vec![CA.to_vec()]), &[]);
  let (client, server, _listener) = handshake(server, &with_client_cert(private_ca_client()), "localhost").await;
  client.expect("client handshake");
  let server = server.expect("accepted conn");
  assert_eq!(server.peer_key().as_deref(), Some(CLIENT_PIN));
}

#[tokio::test]
async fn a_missing_client_certificate_is_rejected() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::Chain(vec![CA.to_vec()]), &[]);
  let (client, server, _listener) = handshake(server, &private_ca_client(), "localhost").await;
  assert!(server.is_none(), "the server drops the connection");
  // TLS 1.3 finishes the client's side of the handshake before the server
  // checks the certificate, so the client hears of it on its first read.
  if let Ok(client) = client {
    let read = soon(next_chunk(&mut client.readable())).await;
    let err = read.err().expect("the first read fails");
    assert!(err.contains("CertificateRequired"), "{err}");
  }
}

#[tokio::test]
async fn any_client_key_reports_the_peer_key() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::AnyKey, &[]);
  let (client, server, _listener) = handshake(server, &with_client_cert(private_ca_client()), "localhost").await;
  client.expect("client handshake");
  assert_eq!(server.expect("accepted conn").peer_key().as_deref(), Some(CLIENT_PIN));
}

#[tokio::test]
async fn a_pin_replaces_the_chain_and_the_name_check() {
  let server = server_config_of(SELF_SIGNED, SELF_SIGNED_KEY, ClientAuth::None, &[]);
  let pin = parse_pin(SELF_SIGNED_PIN).expect("pin");
  // An address the certificate does not name: the key is the identity.
  let opts = ClientOptions { pin: Some(pin), ..Default::default() };
  let (client, server, _listener) = handshake(server, &opts, "10.1.2.3").await;
  let client = client.expect("client handshake");
  assert_eq!(client.peer_key().as_deref(), Some(SELF_SIGNED_PIN));
  assert!(server.is_some());
}

#[tokio::test]
async fn a_pin_mismatch_fails_the_handshake() {
  let server = server_config_of(SELF_SIGNED, SELF_SIGNED_KEY, ClientAuth::None, &[]);
  let opts = ClientOptions { pin: Some(parse_pin(CLIENT_PIN).expect("pin")), ..Default::default() };
  let (client, server, _listener) = handshake(server, &opts, "localhost").await;
  let err = client.err().expect("the handshake fails");
  assert!(err.contains("does not match the pin"), "{err}");
  assert!(server.is_none());
}

#[test]
fn pins_parse_from_hex_only() {
  assert_eq!(parse_pin(SELF_SIGNED_PIN).map(|p| p.len()), Ok(PIN_LEN));
  assert!(parse_pin("abc").is_err());
  assert!(parse_pin(&"zz".repeat(PIN_LEN)).is_err());
  let opts = ClientOptions { pin: Some([0; PIN_LEN]), ..private_ca_client() };
  assert!(client_config(&opts).is_err(), "a pin excludes caCerts");
  let opts = ClientOptions { cert: Some(CLIENT.to_vec()), ..Default::default() };
  assert!(client_config(&opts).is_err(), "a cert needs its key");
}

#[tokio::test]
async fn a_stalled_client_does_not_hold_up_the_next_accept() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let port = port_of(&listener);
  // Connects and never sends its ClientHello.
  let _stalled = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.expect("raw connect");
  let config = client_config(&private_ca_client()).expect("client config");
  let name = server_name("localhost").expect("name");
  let (client, accepted) =
    tokio::join!(connect_tls("127.0.0.1", port, CONNECT_MS, config, name), soon(listener.accept()));
  client.expect("client handshake");
  assert!(accepted.expect("accept").is_some());
}

#[tokio::test]
async fn half_close_sends_close_notify_and_keeps_reading() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let (client, server, _listener) = handshake(server, &private_ca_client(), "localhost").await;
  let client = client.expect("client handshake");
  let server = server.expect("accepted conn");

  client.write(b"request".to_vec()).await.expect("write");
  client.close_write().await.expect("half-close");
  let mut from_client = server.readable();
  assert_eq!(soon(next_chunk(&mut from_client)).await, Ok(Some(b"request".to_vec())));
  assert_eq!(soon(next_chunk(&mut from_client)).await, Ok(None), "close_notify ends the server's read");
  // TLS 1.3: the half-closed side still reads the answer.
  server.write(b"answer".to_vec()).await.expect("answer");
  assert_eq!(soon(next_chunk(&mut client.readable())).await, Ok(Some(b"answer".to_vec())));
}

#[tokio::test]
async fn a_peer_that_skips_close_notify_ends_the_read() {
  let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
  let port = listener.local_addr().expect("addr").port();
  let acceptor = TlsAcceptor::from(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]));
  let server = tokio::spawn(async move {
    let (tcp, _) = listener.accept().await.expect("accept");
    let mut tls = acceptor.accept(tcp).await.expect("server handshake");
    tls.write_all(b"last words").await.expect("write");
    tls.flush().await.expect("flush");
    // Drop the TCP stream under the TLS session: FIN, no close_notify.
    let (tcp, _) = tls.into_inner();
    drop(tcp);
  });
  let config = client_config(&private_ca_client()).expect("client config");
  let client = connect_tls("127.0.0.1", port, CONNECT_MS, config, server_name("localhost").expect("name"))
    .await
    .expect("client handshake");
  server.await.expect("server task");
  let mut view = client.readable();
  assert_eq!(soon(next_chunk(&mut view)).await, Ok(Some(b"last words".to_vec())));
  assert_eq!(soon(next_chunk(&mut view)).await, Ok(None));
}

/// A STARTTLS server: answers one plain-text line with `OK`, then does the
/// TLS handshake on the same socket and echoes one chunk over it.
async fn starttls_server() -> u16 {
  let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
  let port = listener.local_addr().expect("addr").port();
  let acceptor = TlsAcceptor::from(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]));
  tokio::spawn(async move {
    let (mut tcp, _) = listener.accept().await.expect("accept");
    let mut line = [0u8; 10];
    tcp.read_exact(&mut line).await.expect("STARTTLS line");
    assert_eq!(&line, b"STARTTLS\r\n");
    tcp.write_all(b"OK\r\n").await.expect("OK");
    let mut tls = acceptor.accept(tcp).await.expect("server handshake");
    let mut buf = [0u8; 64];
    let n = tls.read(&mut buf).await.expect("read over TLS");
    tls.write_all(&buf[..n]).await.expect("echo");
    tls.flush().await.expect("flush");
  });
  port
}

#[tokio::test]
async fn start_tls_upgrades_after_a_plain_exchange() {
  let port = starttls_server().await;
  let conn = connect("127.0.0.1", port, CONNECT_MS).await.expect("connect");
  conn.write(b"STARTTLS\r\n".to_vec()).await.expect("write");
  {
    // Read the answer through a view the app then lets go of.
    let mut view = conn.readable();
    assert_eq!(soon(next_chunk(&mut view)).await, Ok(Some(b"OK\r\n".to_vec())));
  }
  let config = client_config(&private_ca_client()).expect("client config");
  let tls =
    start_tls(&conn, CONNECT_MS, config, Some(server_name("localhost").expect("name"))).await.expect("startTls");
  assert!(conn.write(b"x".to_vec()).await.is_err(), "the old Conn is spent");
  assert_eq!(soon(next_chunk(&mut conn.readable())).await, Ok(None));

  tls.write(b"secret".to_vec()).await.expect("write over TLS");
  assert_eq!(soon(next_chunk(&mut tls.readable())).await, Ok(Some(b"secret".to_vec())));
}

#[tokio::test]
async fn start_tls_ends_a_pending_read_as_end_of_stream() {
  let port = starttls_server().await;
  let conn = connect("127.0.0.1", port, CONNECT_MS).await.expect("connect");
  conn.write(b"STARTTLS\r\n".to_vec()).await.expect("write");
  let mut view = conn.readable();
  assert_eq!(soon(next_chunk(&mut view)).await, Ok(Some(b"OK\r\n".to_vec())));
  let config = client_config(&private_ca_client()).expect("client config");
  let name = Some(server_name("localhost").expect("name"));
  // The app's read is still pending when the upgrade starts.
  let (pending, tls) = tokio::join!(next_chunk(&mut view), start_tls(&conn, CONNECT_MS, config, name));
  assert_eq!(pending, Ok(None));
  let tls = tls.expect("startTls");
  tls.write(b"after".to_vec()).await.expect("write over TLS");
  assert_eq!(soon(next_chunk(&mut tls.readable())).await, Ok(Some(b"after".to_vec())));
}

#[tokio::test]
async fn start_tls_refuses_a_tls_or_half_closed_conn() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let (client, _server, _listener) = handshake(server, &private_ca_client(), "localhost").await;
  let client = client.expect("client handshake");
  let config = client_config(&private_ca_client()).expect("client config");
  let name = Some(server_name("localhost").expect("name"));
  let err = start_tls(&client, CONNECT_MS, config.clone(), name.clone()).await.err().expect("refused");
  assert!(err.contains("TLS already"), "{err}");

  let listener = listen("127.0.0.1", 0).await.expect("listen");
  let (plain, _peer) = tokio::join!(connect("127.0.0.1", port_of(&listener), CONNECT_MS), listener.accept());
  let plain = plain.expect("connect");
  plain.close_write().await.expect("half-close");
  let err = start_tls(&plain, CONNECT_MS, config, name).await.err().expect("refused");
  assert!(err.contains("write side is closed"), "{err}");
}
