use std::future::poll_fn;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::rustls;
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

pub(super) fn server_config_of(cert: &[u8], key: &[u8], client_auth: ClientAuth, alpn: &[&str]) -> ServerTls {
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
  server: ServerTls,
  client: &ClientOptions,
  name: &str,
) -> (Result<Conn, String>, Option<Conn>, Listener) {
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let port = port_of(&listener);
  let config = client_config(client).expect("client config");
  let identity = Some(client_identity(client));
  let connect = connect_tls("127.0.0.1", port, CONNECT_MS, config, server_name(name).expect("name"), identity);
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
    tokio::join!(connect_tls("127.0.0.1", port, CONNECT_MS, config, name, None), soon(listener.accept()));
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
  let acceptor = TlsAcceptor::from(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]).config);
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
  let client = connect_tls("127.0.0.1", port, CONNECT_MS, config, server_name("localhost").expect("name"), None)
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
  starttls_server_over(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]).config).await
}

/// `starttls_server` over `config`: a config shared with a `listen_tls`
/// listener shares its session cache too.
async fn starttls_server_over(config: Arc<ServerConfig>) -> u16 {
  let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
  let port = listener.local_addr().expect("addr").port();
  let acceptor = TlsAcceptor::from(config);
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

/// The client role of `start_tls` over `config`, verifying `name`.
fn as_client(config: Arc<ClientConfig>, name: &str) -> TlsRole {
  TlsRole::Client { config, server_name: Some(server_name(name).expect("name")), identity: None }
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
  let tls = start_tls(&conn, CONNECT_MS, as_client(config, "localhost")).await.expect("startTls");
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
  // The app's read is still pending when the upgrade starts.
  let (pending, tls) =
    tokio::join!(next_chunk(&mut view), start_tls(&conn, CONNECT_MS, as_client(config, "localhost")));
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
  let err = start_tls(&client, CONNECT_MS, as_client(config.clone(), "localhost")).await.err().expect("refused");
  assert!(err.contains("TLS already"), "{err}");

  let listener = listen("127.0.0.1", 0).await.expect("listen");
  let (plain, _peer) = tokio::join!(connect("127.0.0.1", port_of(&listener), CONNECT_MS), listener.accept());
  let plain = plain.expect("connect");
  plain.close_write().await.expect("half-close");
  let err = start_tls(&plain, CONNECT_MS, as_client(config, "localhost")).await.err().expect("refused");
  assert!(err.contains("write side is closed"), "{err}");
}

#[tokio::test]
async fn start_tls_upgrades_both_sides_of_a_plain_connection() {
  let listener = listen("127.0.0.1", 0).await.expect("listen");
  let (client, server) = tokio::join!(connect("127.0.0.1", port_of(&listener), CONNECT_MS), listener.accept());
  let client = client.expect("connect");
  let server = server.expect("accept").expect("accepted conn");
  client.write(b"STARTTLS\r\n".to_vec()).await.expect("request");
  assert_eq!(soon(next_chunk(&mut server.readable())).await, Ok(Some(b"STARTTLS\r\n".to_vec())));
  server.write(b"OK\r\n".to_vec()).await.expect("answer");
  assert_eq!(soon(next_chunk(&mut client.readable())).await, Ok(Some(b"OK\r\n".to_vec())));

  let as_server = TlsRole::Server(server_config_of(SERVER, SERVER_KEY, ClientAuth::AnyKey, &[]).config);
  let config = client_config(&with_client_cert(private_ca_client())).expect("client config");
  let (server_tls, client_tls) = tokio::join!(
    start_tls(&server, CONNECT_MS, as_server),
    start_tls(&client, CONNECT_MS, as_client(config, "localhost"))
  );
  let server_tls = server_tls.expect("server startTls");
  let client_tls = client_tls.expect("client startTls");
  assert_eq!(server_tls.peer_key().as_deref(), Some(CLIENT_PIN));
  assert!(client_tls.peer_key().is_some(), "the client sees the server's key");

  client_tls.write(b"up".to_vec()).await.expect("client write over TLS");
  assert_eq!(soon(next_chunk(&mut server_tls.readable())).await, Ok(Some(b"up".to_vec())));
  server_tls.write(b"down".to_vec()).await.expect("server write over TLS");
  assert_eq!(soon(next_chunk(&mut client_tls.readable())).await, Ok(Some(b"down".to_vec())));
  for old in [&client, &server] {
    assert!(old.write(b"x".to_vec()).await.is_err(), "the old Conn is spent");
    assert_eq!(soon(next_chunk(&mut old.readable())).await, Ok(None));
  }
}

// ---- generated certificates --------------------------------------------------

/// A certificate for `names` from `generate_certificate`, signed by
/// `issuer` or self-signed.
fn generated(names: &[&str], is_ca: bool, issuer: Option<&GeneratedCertificate>) -> GeneratedCertificate {
  let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
  generate_certificate(&CertificateRequest {
    names: &names,
    valid_days: DEFAULT_CERT_VALID_DAYS,
    is_ca,
    issuer: issuer.map(|i| (i.cert.as_bytes(), i.key.as_bytes())),
  })
  .expect("generate certificate")
}

fn served(made: &GeneratedCertificate) -> ServerTls {
  server_config_of(made.cert.as_bytes(), made.key.as_bytes(), ClientAuth::None, &[])
}

fn trusting(ca: &GeneratedCertificate) -> ClientOptions {
  ClientOptions { ca_certs: vec![ca.cert.clone().into_bytes()], ..Default::default() }
}

#[tokio::test]
async fn a_generated_self_signed_certificate_is_trusted_by_its_pin() {
  let made = generated(&["localhost", "127.0.0.1"], false, None);
  assert!(made.cert.starts_with("-----BEGIN CERTIFICATE-----"), "{}", made.cert);
  assert!(made.key.starts_with("-----BEGIN PRIVATE KEY-----"), "a PKCS#8 key: {}", made.key);
  let opts = ClientOptions { pin: Some(parse_pin(&made.pin).expect("pin")), ..Default::default() };
  let (client, accepted, _listener) = handshake(served(&made), &opts, "127.0.0.1").await;
  assert_eq!(client.expect("client handshake").peer_key(), Some(made.pin.clone()));
  assert!(accepted.is_some());

  let (client, accepted, _listener) = handshake(served(&made), &ClientOptions::default(), "localhost").await;
  let err = client.err().expect("the default roots reject it");
  assert!(err.contains("invalid peer certificate"), "{err}");
  assert!(accepted.is_none());
}

#[tokio::test]
async fn a_generated_ca_issues_a_leaf_a_ca_certs_client_trusts() {
  let ca = generated(&["test-ca"], true, None);
  let leaf = generated(&["localhost"], false, Some(&ca));
  let (client, accepted, _listener) = handshake(served(&leaf), &trusting(&ca), "localhost").await;
  let client = client.expect("client handshake");
  assert_eq!(client.peer_key(), Some(leaf.pin.clone()), "the pin is the key the client sees");
  assert!(accepted.is_some());

  let stranger = generated(&["localhost"], false, None);
  let (client, _, _listener) = handshake(served(&stranger), &trusting(&ca), "localhost").await;
  let err = client.err().expect("a certificate the CA did not issue fails");
  assert!(err.contains("invalid peer certificate"), "{err}");
}

#[tokio::test]
async fn an_ip_name_verifies_by_ip() {
  let ca = generated(&["test-ca"], true, None);
  let leaf = generated(&["127.0.0.1"], false, Some(&ca));
  let (client, _, _listener) = handshake(served(&leaf), &trusting(&ca), "127.0.0.1").await;
  client.expect("the IP address is the name");
  let (client, _, _listener) = handshake(served(&leaf), &trusting(&ca), "localhost").await;
  let err = client.err().expect("a DNS name the certificate does not hold fails");
  assert!(err.contains("invalid peer certificate"), "{err}");
}

#[test]
fn generate_certificate_errors_name_the_field() {
  let ca = generated(&["test-ca"], true, None);
  let names = vec!["localhost".to_string()];
  let request = |names: &[String], valid_days: u32, issuer: Option<(&[u8], &[u8])>| {
    let request = CertificateRequest { names, valid_days, is_ca: false, issuer };
    generate_certificate(&request).err().expect("an error")
  };
  let err = request(&[], DEFAULT_CERT_VALID_DAYS, None);
  assert!(err.starts_with("names: "), "{err}");
  let err = request(&names, 0, None);
  assert!(err.starts_with("validDays: "), "{err}");
  let err = request(&names, u32::MAX, None);
  assert!(err.starts_with("validDays: "), "{err}");
  let err = request(&names, DEFAULT_CERT_VALID_DAYS, Some((b"not a pem", ca.key.as_bytes())));
  assert!(err.starts_with("issuer.cert: "), "{err}");
  let err = request(&names, DEFAULT_CERT_VALID_DAYS, Some((ca.cert.as_bytes(), b"not a pem")));
  assert!(err.starts_with("issuer.key: "), "{err}");
  let err = request(&names, DEFAULT_CERT_VALID_DAYS, Some((ca.cert.as_bytes(), SELF_SIGNED_KEY)));
  assert_eq!(err, "issuer.key: does not belong to issuer.cert");
  for bad in ["", "my server", "http://x", "a..b", "*"] {
    let err = request(&[bad.to_string()], DEFAULT_CERT_VALID_DAYS, None);
    assert!(err.starts_with("names: not a DNS name"), "{bad:?}: {err}");
  }
  let wildcard =
    CertificateRequest { names: &["*.example.test".to_string()], valid_days: 1, is_ca: false, issuer: None };
  generate_certificate(&wildcard).expect("a wildcard name");
}

// ---- renewal -----------------------------------------------------------------

fn pinned_to(made: &GeneratedCertificate) -> ClientOptions {
  ClientOptions { pin: Some(parse_pin(&made.pin).expect("pin")), ..Default::default() }
}

/// Why a full handshake by `client` with `listener` fails (no session to
/// resume, so the certificate is checked), the accept running beside it.
async fn refusal(listener: &Listener, client: &ClientOptions) -> String {
  let config = client_config_without_resumption(client).expect("client config");
  let name = server_name("localhost").expect("name");
  let connect = connect_tls("127.0.0.1", port_of(listener), CONNECT_MS, config, name, None);
  let refused = tokio::select! {
    connected = connect => connected.err(),
    // A failed handshake never reaches accept: only a passing one ends here.
    _ = listener.accept() => None,
  };
  refused.expect("the handshake fails")
}

#[tokio::test]
async fn a_listener_presents_a_new_certificate_to_new_handshakes() {
  let first = generated(&["localhost"], false, None);
  let second = generated(&["localhost"], false, None);
  let listener = listen_tls("127.0.0.1", 0, served(&first)).await.expect("listen_tls");
  let configs = ClientConfigs::new();
  let (before, accepted) = greeted(&listener, &configs, &pinned_to(&first)).await;
  listener.set_certificate(second.cert.as_bytes(), second.key.as_bytes()).expect("set certificate");
  let (after, _accepted) = greeted(&listener, &configs, &pinned_to(&second)).await;
  assert_eq!(after.peer_key(), Some(second.pin.clone()), "a new handshake gets the new certificate");
  assert_eq!(before.peer_key(), Some(first.pin.clone()));
  before.write(b"still open".to_vec()).await.expect("write");
  assert_eq!(soon(next_chunk(&mut accepted.readable())).await, Ok(Some(b"still open".to_vec())));
  let err = refusal(&listener, &pinned_to(&first)).await;
  assert!(err.contains("does not match the pin"), "the old key is gone: {err}");
}

#[tokio::test]
async fn a_bad_pair_leaves_the_certificate_in_place() {
  let first = generated(&["localhost"], false, None);
  let second = generated(&["localhost"], false, None);
  let listener = listen_tls("127.0.0.1", 0, served(&first)).await.expect("listen_tls");
  let err = listener.set_certificate(second.cert.as_bytes(), first.key.as_bytes()).err().expect("a mismatch");
  assert_eq!(err, "key: does not belong to cert");
  let err = listener.set_certificate(b"not a pem", second.key.as_bytes()).err().expect("no certificate");
  assert!(err.starts_with("cert: "), "{err}");
  let err = listener.set_certificate(second.cert.as_bytes(), b"not a pem").err().expect("no key");
  assert!(err.starts_with("key: "), "{err}");
  let (client, _accepted) = greeted(&listener, &ClientConfigs::new(), &pinned_to(&first)).await;
  assert_eq!(client.peer_key(), Some(first.pin.clone()), "the first pair still serves");

  let options = ServerOptions {
    cert: SERVER.to_vec(),
    key: SELF_SIGNED_KEY.to_vec(),
    alpn: Vec::new(),
    client_auth: ClientAuth::None,
  };
  let err = server_config(&options).err().expect("a mismatch at the start");
  assert_eq!(err, "key: does not belong to cert");
  let plain = listen("127.0.0.1", 0).await.expect("listen");
  let err = plain.set_certificate(first.cert.as_bytes(), first.key.as_bytes()).err().expect("a plain listener");
  assert!(err.contains("no certificate"), "{err}");
}

// ---- session resumption ------------------------------------------------------

/// What a resumption test's server sends first, after each handshake.
const GREETING: &[u8] = b"hello";

/// Connect over the config `configs` keeps for `client` to `listener` as
/// "localhost" and accept it there; the server then greets and the client
/// reads the greeting, which also takes in the server's TLS 1.3 session
/// ticket. The client's and the server's `Conn`.
async fn greeted(listener: &Listener, configs: &ClientConfigs, client: &ClientOptions) -> (Conn, Conn) {
  let config = configs.config_for(client).expect("client config");
  greeted_over(listener, config, client).await
}

/// `greeted` over `config`, built from `client`.
async fn greeted_over(listener: &Listener, config: Arc<ClientConfig>, client: &ClientOptions) -> (Conn, Conn) {
  let name = server_name("localhost").expect("name");
  let identity = Some(client_identity(client));
  let connect = connect_tls("127.0.0.1", port_of(listener), CONNECT_MS, config, name, identity);
  let (client, server) = tokio::join!(connect, soon(listener.accept()));
  let client = client.expect("client handshake");
  let server = server.expect("accept").expect("accepted conn");
  server.write(GREETING.to_vec()).await.expect("greeting");
  assert_eq!(soon(next_chunk(&mut client.readable())).await, Ok(Some(GREETING.to_vec())));
  (client, server)
}

/// Two connections with equal options to `server`: the first a full
/// handshake, the second resumed, on both sides.
async fn second_connection_resumes(server: ServerTls) {
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let configs = ClientConfigs::new();
  let (client, accepted) = greeted(&listener, &configs, &private_ca_client()).await;
  assert!(!client.resumed() && !accepted.resumed(), "the first handshake is a full one");
  let (client, accepted) = greeted(&listener, &configs, &private_ca_client()).await;
  assert!(client.resumed(), "the client resumed");
  assert!(accepted.resumed(), "the server resumed");
  assert!(client.peer_key().is_some(), "a resumed session keeps the server's certificate");
}

#[test]
fn client_configs_are_keyed_by_the_options() {
  let configs = ClientConfigs::new();
  let config_for = |opts: &ClientOptions| configs.config_for(opts).expect("client config");
  let base = || ClientOptions { alpn: vec!["x-a".to_string()], ..private_ca_client() };
  let config = config_for(&base());
  assert!(Arc::ptr_eq(&config, &config_for(&base())), "equal options share a config");
  let other_alpn = ClientOptions { alpn: vec!["x-b".to_string()], ..base() };
  assert!(!Arc::ptr_eq(&config, &config_for(&other_alpn)), "another ALPN list");
  let pinned = ClientOptions { pin: Some(parse_pin(SELF_SIGNED_PIN).expect("pin")), ..Default::default() };
  let other_pin = ClientOptions { pin: Some(parse_pin(CLIENT_PIN).expect("pin")), ..Default::default() };
  assert!(!Arc::ptr_eq(&config_for(&pinned), &config_for(&other_pin)), "another pin");
  assert!(!Arc::ptr_eq(&config, &config_for(&with_client_cert(base()))), "an added client certificate");
  let bad = ClientOptions { cert: Some(CLIENT.to_vec()), ..Default::default() };
  assert!(configs.config_for(&bad).is_err(), "a config that does not build is an error");
}

#[test]
fn client_configs_drop_the_least_recently_used() {
  let configs = ClientConfigs::new();
  let config_for = |i: usize| {
    let opts = ClientOptions { alpn: vec![format!("x-{i}")], ..Default::default() };
    configs.config_for(&opts).expect("client config")
  };
  let first = config_for(0);
  let second = config_for(1);
  for i in 2..CLIENT_CONFIG_KEYS {
    config_for(i);
  }
  // The first is used again, so the second is now the least recent.
  assert!(Arc::ptr_eq(&first, &config_for(0)));
  config_for(CLIENT_CONFIG_KEYS);
  assert!(Arc::ptr_eq(&first, &config_for(0)), "a recent config stays");
  assert!(!Arc::ptr_eq(&second, &config_for(1)), "the least recent was dropped");
}

#[tokio::test]
async fn equal_options_resume_over_tls13() {
  second_connection_resumes(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[])).await;
}

#[tokio::test]
async fn equal_options_resume_over_tls12() {
  let options =
    ServerOptions { cert: SERVER.to_vec(), key: SERVER_KEY.to_vec(), alpn: Vec::new(), client_auth: ClientAuth::None };
  let server = server_config_with_versions(&options, &[&rustls::version::TLS12]).expect("server config");
  second_connection_resumes(server).await;
}

#[tokio::test]
async fn other_options_do_a_full_handshake() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &["x-a", "x-b"]);
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let configs = ClientConfigs::new();
  let alpn = |p: &str| ClientOptions { alpn: vec![p.to_string()], ..private_ca_client() };
  let (client, _accepted) = greeted(&listener, &configs, &alpn("x-a")).await;
  assert!(!client.resumed());
  let (client, accepted) = greeted(&listener, &configs, &alpn("x-b")).await;
  assert_eq!(client.alpn_protocol().as_deref(), Some("x-b"));
  assert!(!client.resumed() && !accepted.resumed(), "another ALPN list is a full handshake");
}

#[tokio::test]
async fn start_tls_resumes_a_connect_tls_session() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let listener = listen_tls("127.0.0.1", 0, server.clone()).await.expect("listen_tls");
  let configs = ClientConfigs::new();
  let (first, _accepted) = greeted(&listener, &configs, &private_ca_client()).await;
  assert!(!first.resumed());

  let port = starttls_server_over(server.config).await;
  let conn = connect("127.0.0.1", port, CONNECT_MS).await.expect("connect");
  conn.write(b"STARTTLS\r\n".to_vec()).await.expect("write");
  assert_eq!(soon(next_chunk(&mut conn.readable())).await, Ok(Some(b"OK\r\n".to_vec())));
  let config = configs.config_for(&private_ca_client()).expect("client config");
  let tls = start_tls(&conn, CONNECT_MS, as_client(config, "localhost")).await.expect("startTls");
  assert!(tls.resumed(), "the upgrade resumed the connectTls session");
  assert!(!conn.resumed(), "plain TCP never resumes");
  tls.write(b"secret".to_vec()).await.expect("write over TLS");
  assert_eq!(soon(next_chunk(&mut tls.readable())).await, Ok(Some(b"secret".to_vec())));
}

#[tokio::test]
async fn a_config_without_resumption_does_a_full_handshake() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let configs = ClientConfigs::new();
  let (first, _accepted) = greeted(&listener, &configs, &private_ca_client()).await;
  assert!(!first.resumed());
  let config = client_config_without_resumption(&private_ca_client()).expect("client config");
  let (client, accepted) = greeted_over(&listener, config, &private_ca_client()).await;
  assert!(!client.resumed() && !accepted.resumed(), "an opted-out connection is a full handshake");
  let (client, accepted) = greeted(&listener, &configs, &private_ca_client()).await;
  assert!(client.resumed() && accepted.resumed(), "the cached config still resumes");
}

#[tokio::test]
async fn a_client_conn_records_its_options_identity() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &[]);
  let listener = listen_tls("127.0.0.1", 0, server).await.expect("listen_tls");
  let configs = ClientConfigs::new();
  let options = private_ca_client();
  let (client, accepted) = greeted(&listener, &configs, &options).await;
  assert_eq!(client.client_identity(), Some(client_identity(&options)));
  let other = ClientOptions { alpn: vec!["x-other".to_string()], ..private_ca_client() };
  assert_ne!(client.client_identity(), Some(client_identity(&other)), "other options, another identity");
  assert_eq!(accepted.client_identity(), None, "an accepted connection is no client");
  let plain = connect("127.0.0.1", port_of(&listener), CONNECT_MS).await.expect("connect");
  assert_eq!(plain.client_identity(), None, "plain TCP is no TLS client");
}
