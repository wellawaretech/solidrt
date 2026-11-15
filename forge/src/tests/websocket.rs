use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use fastwebsockets::{FragmentCollector, OpCode};
use hyper_util::rt::TokioIo;

use super::tls::{server_config_of, CA, SELF_SIGNED, SELF_SIGNED_KEY, SELF_SIGNED_PIN, SERVER, SERVER_KEY};
use crate::logger::default_logger;
use crate::tls::{client_config, parse_pin, ClientAuth, ClientConfig, ClientOptions, ServerConfig};
use crate::websocket::*;

#[test]
fn wss_urls_parse_with_their_default_port() {
  let target = parse_ws_url("wss://example.com/chat?room=1").expect("wss parses");
  assert!(target.tls);
  assert_eq!((target.host.as_str(), target.port, target.path.as_str()), ("example.com", 443, "/chat?room=1"));
  let target = parse_ws_url("ws://example.com").expect("ws parses");
  assert!(!target.tls);
  assert_eq!(target.port, 80);
  assert!(parse_ws_url("https://example.com/").is_err());
  assert!(target.protocols.is_empty(), "a parsed target offers no subprotocol");
}

#[test]
fn offered_subprotocols_are_distinct_tokens() {
  let offer = |list: &[&str]| check_protocols(&list.iter().map(|p| p.to_string()).collect::<Vec<_>>());
  assert_eq!(offer(&[]), Ok(()));
  assert_eq!(offer(&["chat", "v2.json", "a+b"]), Ok(()));
  assert_eq!(offer(&[""]), Err("a subprotocol must not be empty".to_string()));
  assert_eq!(offer(&["a b"]), Err("invalid subprotocol: a b".to_string()));
  assert_eq!(offer(&["a,b"]), Err("invalid subprotocol: a,b".to_string()));
  assert_eq!(offer(&["chat", "Chat", "chat"]), Err("duplicate subprotocol: chat".to_string()));
}

/// What a test server answers to the client's subprotocol offer.
#[derive(Clone, Copy)]
enum Pick {
  /// The first protocol offered, or none when none was.
  First,
  /// No `Sec-WebSocket-Protocol` header.
  Nothing,
  /// `UNOFFERED`, whatever the client offered.
  Unoffered,
}

/// The pick of `Pick::Unoffered`: a protocol no test offers.
const UNOFFERED: &str = "unoffered";

/// The ALPN list of the test servers. `h2` comes first, so a client that
/// offered `h2` would land on a protocol it cannot upgrade.
const SERVER_ALPN: [&str; 2] = ["h2", "http/1.1"];

/// A TLS WebSocket server on 127.0.0.1 with the private CA's certificate,
/// echoing text with no subprotocol picked unless one is offered.
async fn wss_echo_server() -> u16 {
  wss_echo_server_over(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &SERVER_ALPN), Pick::First).await
}

/// A TLS WebSocket server on 127.0.0.1 over `config` that echoes text
/// messages: the TLS acceptor in front of a hyper HTTP/1.1 upgrade, whose
/// 101 answers the subprotocol offer as `pick` says.
async fn wss_echo_server_over(config: Arc<ServerConfig>, pick: Pick) -> u16 {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
  let port = listener.local_addr().expect("addr").port();
  let acceptor = tokio_rustls::TlsAcceptor::from(config);
  tokio::spawn(async move {
    while let Ok((tcp, _)) = listener.accept().await {
      let acceptor = acceptor.clone();
      tokio::spawn(async move {
        let Ok(tls) = acceptor.accept(tcp).await else { return };
        assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"http/1.1".as_slice()), "the client offers http/1.1 only");
        let service = hyper::service::service_fn(move |mut req| async move {
          let offered = req.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok());
          let first = offered.and_then(|v| v.split(',').next()).map(|p| p.trim().to_string());
          let (mut response, upgrade) = fastwebsockets::upgrade::upgrade(&mut req).expect("an upgrade request");
          let answer = match pick {
            Pick::First => first,
            Pick::Nothing => None,
            Pick::Unoffered => Some(UNOFFERED.to_string()),
          };
          if let Some(answer) = answer {
            let value = hyper::header::HeaderValue::from_str(&answer).expect("a token header value");
            response.headers_mut().insert("sec-websocket-protocol", value);
          }
          tokio::spawn(async move {
            let mut ws = FragmentCollector::new(upgrade.await.expect("upgraded"));
            while let Ok(frame) = ws.read_frame().await {
              match frame.opcode {
                OpCode::Text => {
                  let echo = fastwebsockets::Frame::text(frame.payload);
                  if ws.write_frame(echo).await.is_err() {
                    return;
                  }
                }
                OpCode::Close => return,
                _ => {}
              }
            }
          });
          Ok::<_, std::convert::Infallible>(response)
        });
        let _ =
          hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tls), service).with_upgrades().await;
      });
    }
  });
  port
}

/// The events a client socket reports, as text, and what it does on them:
/// on open it sends `hello`, on the first text it closes.
struct Recorder {
  socket: Rc<ClientSocket>,
  events: RefCell<Vec<String>>,
}

impl ClientDispatch for Recorder {
  fn on_open(&self) {
    self.events.borrow_mut().push("open".to_string());
    self.socket.send(Kind::Text, b"hello".to_vec());
  }
  fn on_text(&self, text: String) {
    self.events.borrow_mut().push(format!("text: {text}"));
    let _ = self.socket.close(None, None);
  }
  fn on_binary(&self, _bytes: Vec<u8>) {}
  fn on_error(&self, message: String) {
    self.events.borrow_mut().push(format!("error: {message}"));
  }
  fn on_close(&self, code: u16, _reason: String, was_clean: bool) {
    self.events.borrow_mut().push(format!("close: {code} {was_clean}"));
  }
}

/// Run a client to `url` with `tls` until its socket closes; its events.
pub(super) async fn run(url: &str, tls: Arc<ClientConfig>) -> Vec<String> {
  run_offering(url, tls, &[]).await.0
}

/// Run a client to `url` with `tls` offering `protocols` until its socket
/// closes; its events and the subprotocol it agreed on.
async fn run_offering(url: &str, tls: Arc<ClientConfig>, protocols: &[&str]) -> (Vec<String>, String) {
  let local = tokio::task::LocalSet::new();
  let mut target = parse_ws_url(url).expect("url parses");
  target.protocols = protocols.iter().map(|p| p.to_string()).collect();
  let socket = ClientSocket::new();
  let agreed = socket.clone();
  let recorder = Recorder { socket: socket.clone(), events: RefCell::new(Vec::new()) };
  local
    .run_until(async {
      let logger = default_logger();
      let spawn_writer = |writer: ClientWriter| {
        tokio::task::spawn_local(async move { writer.run(&default_logger()).await });
      };
      let done = run_client(socket, target, Some(tls), &recorder, spawn_writer, &logger);
      tokio::time::timeout(Duration::from_secs(5), done).await.expect("the socket closes");
    })
    .await;
  (recorder.events.into_inner(), agreed.protocol())
}

/// A client config that trusts the private CA.
fn private_ca() -> Arc<ClientConfig> {
  client_config(&ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() }).expect("client config")
}

#[tokio::test]
async fn wss_echoes_through_a_private_ca() {
  let port = wss_echo_server().await;
  let events = run(&format!("wss://localhost:{port}/echo"), private_ca()).await;
  assert_eq!(events, ["open", "text: hello", "close: 1000 true"]);
}

#[tokio::test]
async fn an_untrusted_wss_server_reports_error_then_close() {
  let port = wss_echo_server().await;
  let tls = client_config(&ClientOptions::default()).expect("client config");
  let events = run(&format!("wss://localhost:{port}/echo"), tls).await;
  assert_eq!(events.len(), 2, "{events:?}");
  assert!(events[0].starts_with("error: connect failed: TLS handshake: invalid peer certificate"), "{events:?}");
  assert_eq!(events[1], "close: 1006 false");
}

#[tokio::test]
async fn wss_echoes_to_a_self_signed_server_through_a_pin() {
  let server = server_config_of(SELF_SIGNED, SELF_SIGNED_KEY, ClientAuth::None, &SERVER_ALPN);
  let port = wss_echo_server_over(server, Pick::First).await;
  let pin = parse_pin(SELF_SIGNED_PIN).expect("pin");
  let tls = client_config(&ClientOptions { pin: Some(pin), ..Default::default() }).expect("client config");
  // The pin replaces the name check, so an address the certificate does
  // not name passes.
  let events = run(&format!("wss://127.0.0.1:{port}/echo"), tls).await;
  assert_eq!(events, ["open", "text: hello", "close: 1000 true"]);
}

#[tokio::test]
async fn the_server_picks_one_of_the_offered_subprotocols() {
  let port = wss_echo_server().await;
  let (events, protocol) = run_offering(&format!("wss://localhost:{port}/echo"), private_ca(), &["chat", "json"]).await;
  assert_eq!(events, ["open", "text: hello", "close: 1000 true"]);
  assert_eq!(protocol, "chat");
}

#[tokio::test]
async fn a_server_that_picks_no_subprotocol_agrees_on_none() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &SERVER_ALPN);
  let port = wss_echo_server_over(server, Pick::Nothing).await;
  let (events, protocol) = run_offering(&format!("wss://localhost:{port}/echo"), private_ca(), &["chat"]).await;
  assert_eq!(events, ["open", "text: hello", "close: 1000 true"]);
  assert_eq!(protocol, "");
}

#[tokio::test]
async fn a_pick_that_was_not_offered_fails_the_connection() {
  let server = server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &SERVER_ALPN);
  let port = wss_echo_server_over(server, Pick::Unoffered).await;
  let url = format!("wss://localhost:{port}/echo");
  let error = format!("error: connect failed: the server picked a subprotocol that was not offered: {UNOFFERED}");
  let failed = (vec![error, "close: 1006 false".to_string()], String::new());
  assert_eq!(run_offering(&url, private_ca(), &["chat"]).await, failed);
  // A pick when nothing was offered fails as well.
  assert_eq!(run_offering(&url, private_ca(), &[]).await, failed);
}
