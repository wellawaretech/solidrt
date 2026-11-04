use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use fastwebsockets::{FragmentCollector, OpCode};
use hyper_util::rt::TokioIo;

use super::tls::{server_config_of, CA, SERVER, SERVER_KEY};
use crate::logger::default_logger;
use crate::tls::{client_config, ClientAuth, ClientConfig, ClientOptions};
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
}

/// A TLS WebSocket server on 127.0.0.1 that echoes one text message: the
/// TLS acceptor in front of a hyper HTTP/1.1 upgrade. It offers `h2` first,
/// so a client that offered `h2` would land on a protocol it cannot upgrade.
async fn wss_echo_server() -> u16 {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
  let port = listener.local_addr().expect("addr").port();
  let acceptor =
    tokio_rustls::TlsAcceptor::from(server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &["h2", "http/1.1"]));
  tokio::spawn(async move {
    while let Ok((tcp, _)) = listener.accept().await {
      let acceptor = acceptor.clone();
      tokio::spawn(async move {
        let Ok(tls) = acceptor.accept(tcp).await else { return };
        assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"http/1.1".as_slice()), "the client offers http/1.1 only");
        let service = hyper::service::service_fn(|mut req| async move {
          let (response, upgrade) = fastwebsockets::upgrade::upgrade(&mut req).expect("an upgrade request");
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
async fn run(url: &str, tls: Arc<ClientConfig>) -> Vec<String> {
  let local = tokio::task::LocalSet::new();
  let target = parse_ws_url(url).expect("url parses");
  let socket = ClientSocket::new();
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
  recorder.events.into_inner()
}

#[tokio::test]
async fn wss_echoes_through_a_private_ca() {
  let port = wss_echo_server().await;
  let tls = client_config(&ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() }).expect("client config");
  let events = run(&format!("wss://localhost:{port}/echo"), tls).await;
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
