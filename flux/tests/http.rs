#![cfg(feature = "compile")]

mod common;

use common::LogSink;
use flux::{FluxEngine, LogLevel};
use std::time::Duration;

/// Grab a currently-free TCP port by binding an ephemeral one and releasing it.
/// There is a small race before the engine rebinds it, acceptable for tests.
fn free_port() -> u16 {
  let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
  listener.local_addr().expect("local addr").port()
}

// The server's websocket wire, driven by an independent raw client: flux's
// own WebSocket would test the codec against itself. The module tests of
// flux:http (serve and fetch) are flux/tests/http.test.ts, run by sol test;
// what stays here is what a test inside the engine cannot observe.

/// Minimal raw WebSocket client for driving the server's websocket path without
/// pulling a client crate into the dev-dependencies. Frames are small (< 126
/// bytes) so only the short length form is implemented.
mod ws_client {
  use std::io::{Read, Write};
  use std::net::TcpStream;
  use std::time::Duration;

  pub fn connect(port: u16) -> TcpStream {
    // The engine thread binds the listener; retry until it is up.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
      match TcpStream::connect(("127.0.0.1", port)) {
        Ok(s) => {
          s.set_read_timeout(Some(Duration::from_secs(5))).expect("set read timeout");
          return s;
        }
        Err(e) if std::time::Instant::now() < deadline => {
          let _ = e;
          std::thread::sleep(Duration::from_millis(20));
        }
        Err(e) => panic!("connect to server: {e}"),
      }
    }
  }

  /// Perform the upgrade handshake; returns the full 101 response head so
  /// tests can assert on extra headers.
  pub fn handshake(s: &mut TcpStream, port: u16) -> String {
    let req = format!(
      "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\
       Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    s.write_all(req.as_bytes()).expect("write handshake");
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
      s.read_exact(&mut byte).expect("read handshake response");
      buf.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&buf).into_owned();
    assert!(head.starts_with("HTTP/1.1 101"), "expected 101, got: {head}");
    head
  }

  /// Send one masked frame (client frames must be masked).
  pub fn send(s: &mut TcpStream, opcode: u8, payload: &[u8]) {
    assert!(payload.len() < 126, "test frames use the short length form");
    let mask = [0x12u8, 0x34, 0x56, 0x78];
    let mut frame = vec![0x80 | opcode, 0x80 | payload.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    s.write_all(&frame).expect("write frame");
  }

  /// Read one (unmasked) server frame, returning (opcode, payload).
  pub fn read(s: &mut TcpStream) -> (u8, Vec<u8>) {
    let mut head = [0u8; 2];
    s.read_exact(&mut head).expect("read frame header");
    let len = (head[1] & 0x7F) as usize;
    assert!(len < 126, "test frames use the short length form");
    let mut payload = vec![0u8; len];
    s.read_exact(&mut payload).expect("read frame payload");
    (head[0] & 0x0F, payload)
  }
}

#[test]
fn serve_websocket_echo_and_close() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";

        let server = serve({{
            port: {port},
            fetch(req, server) {{
                if (server.upgrade(req)) return;
                return "not a websocket";
            }},
            websocket: {{
                open(ws) {{
                    console.log("open", ws.readyState);
                    ws.send("welcome");
                }},
                message(ws, m) {{
                    if (m === "bye") {{ ws.close(4001, "done"); return; }}
                    if (typeof m === "string") ws.send("echo:" + m);
                    else ws.send(m);
                }},
                close(ws, code, reason) {{
                    console.log("close", code, reason, ws.readyState);
                    server.close();
                }},
            }},
        }});
        "#,
  );

  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let (done_tx, done_rx) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
    rt.block_on(engine.eval_source(&code));
    let _ = done_tx.send(());
  });

  let mut s = ws_client::connect(port);
  ws_client::handshake(&mut s, port);

  assert_eq!(ws_client::read(&mut s), (0x1, b"welcome".to_vec()));

  ws_client::send(&mut s, 0x1, b"hello");
  assert_eq!(ws_client::read(&mut s), (0x1, b"echo:hello".to_vec()));

  ws_client::send(&mut s, 0x2, &[1, 2, 3, 250]);
  assert_eq!(ws_client::read(&mut s), (0x2, vec![1, 2, 3, 250]));

  ws_client::send(&mut s, 0x1, b"bye");
  let (opcode, payload) = ws_client::read(&mut s);
  assert_eq!(opcode, 0x8, "expected a close frame");
  assert_eq!(u16::from_be_bytes([payload[0], payload[1]]), 4001);
  assert_eq!(&payload[2..], b"done");
  // Echo the close so the server sees a clean shutdown, then stop().
  ws_client::send(&mut s, 0x8, &payload);

  done_rx.recv_timeout(Duration::from_secs(10)).expect("engine did not exit after server.close()");

  let cap = sink.captured();
  let lines: Vec<String> =
    cap.lines_at(LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(String::from).collect();
  assert_eq!(lines, vec!["open 1".to_string(), "close 4001 done 3".to_string()]);
}

#[test]
fn serve_websocket_data_drain_ping() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";

        let server = serve({{
            port: {port},
            fetch(req, server) {{
                if (server.upgrade(req, {{ data: {{ uid: 7 }}, headers: {{ "X-Extra": "yes" }} }})) return;
                return "not a websocket";
            }},
            websocket: {{
                // A tiny limit so the second send exceeds it (-1) and drain fires
                // once the writer empties the queue.
                backpressureLimit: 1,
                open(ws) {{
                    console.log("open", JSON.stringify(ws.data), ws.send("a"), ws.send("bb"));
                }},
                drain(ws) {{
                    console.log("drain");
                }},
                message(ws, m) {{
                    if (m === "ping-me") console.log("ping ret", ws.ping("xy"));
                }},
                pong(ws, payload) {{
                    console.log("pong", new TextDecoder().decode(payload));
                }},
                close(ws, code, reason) {{
                    console.log("close", code, reason);
                    server.close();
                }},
            }},
        }});
        "#,
  );

  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let (done_tx, done_rx) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
    rt.block_on(engine.eval_source(&code));
    let _ = done_tx.send(());
  });

  let mut s = ws_client::connect(port);
  let head = ws_client::handshake(&mut s, port);
  assert!(head.to_lowercase().contains("x-extra: yes"), "missing upgrade header in: {head}");

  // The two open() sends arrive in order; receiving them means the writer
  // drained the queue, so the drain callback has fired.
  assert_eq!(ws_client::read(&mut s), (0x1, b"a".to_vec()));
  assert_eq!(ws_client::read(&mut s), (0x1, b"bb".to_vec()));

  // ws.ping() goes out as a ping control frame; answer it and the pong
  // callback fires server-side.
  ws_client::send(&mut s, 0x1, b"ping-me");
  assert_eq!(ws_client::read(&mut s), (0x9, b"xy".to_vec()));
  ws_client::send(&mut s, 0xA, b"xy");

  // A client ping is answered automatically (never surfaces to JS).
  ws_client::send(&mut s, 0x9, b"pp");
  assert_eq!(ws_client::read(&mut s), (0xA, b"pp".to_vec()));

  // Client-initiated close: the protocol layer echoes it, close() fires.
  let mut close_payload = 1000u16.to_be_bytes().to_vec();
  close_payload.extend_from_slice(b"ok");
  ws_client::send(&mut s, 0x8, &close_payload);
  assert_eq!(ws_client::read(&mut s), (0x8, close_payload));

  done_rx.recv_timeout(Duration::from_secs(10)).expect("engine did not exit after server.close()");

  let cap = sink.captured();
  let lines: Vec<String> =
    cap.lines_at(LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(String::from).collect();
  let expected = vec![
    "open {\"uid\":7} 1 -1".to_string(),
    "drain".to_string(),
    "ping ret -1".to_string(),
    "drain".to_string(),
    "pong xy".to_string(),
    "close 1000 ok".to_string(),
  ];
  assert_eq!(lines, expected);
}

#[test]
fn serve_websocket_pubsub() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";

        let closed = 0;
        let server = serve({{
            port: {port},
            fetch(req, server) {{
                if (server.upgrade(req)) return;
                return "not a websocket";
            }},
            websocket: {{
                message(ws, m) {{
                    if (m === "join") {{
                        ws.subscribe("room");
                        ws.send("joined:" + server.subscriberCount("room"));
                    }} else if (m === "shout") {{
                        // ws.publish excludes the publisher; server.publish reaches all.
                        console.log("pub", ws.publish("room", "from-peer"), server.publish("room", "to-all"), server.subscriberCount("room"));
                    }} else if (m === "leave") {{
                        ws.unsubscribe("room");
                        ws.send("left:" + ws.isSubscribed("room") + ":" + server.subscriberCount("room"));
                    }}
                }},
                close(ws, code, reason) {{
                    closed += 1;
                    console.log("closed", server.subscriberCount("room"));
                    if (closed === 2) server.close();
                }},
            }},
        }});
        "#,
  );

  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let (done_tx, done_rx) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
    rt.block_on(engine.eval_source(&code));
    let _ = done_tx.send(());
  });

  let mut a = ws_client::connect(port);
  ws_client::handshake(&mut a, port);
  let mut b = ws_client::connect(port);
  ws_client::handshake(&mut b, port);

  ws_client::send(&mut a, 0x1, b"join");
  assert_eq!(ws_client::read(&mut a), (0x1, b"joined:1".to_vec()));
  ws_client::send(&mut b, 0x1, b"join");
  assert_eq!(ws_client::read(&mut b), (0x1, b"joined:2".to_vec()));

  // A publishes: A only sees the server-wide publish, B sees both.
  ws_client::send(&mut a, 0x1, b"shout");
  assert_eq!(ws_client::read(&mut a), (0x1, b"to-all".to_vec()));
  assert_eq!(ws_client::read(&mut b), (0x1, b"from-peer".to_vec()));
  assert_eq!(ws_client::read(&mut b), (0x1, b"to-all".to_vec()));

  ws_client::send(&mut b, 0x1, b"leave");
  assert_eq!(ws_client::read(&mut b), (0x1, b"left:false:1".to_vec()));

  // A closes while still subscribed: the socket is auto-unsubscribed before
  // the close callback runs, so it logs a count of 0.
  ws_client::send(&mut a, 0x8, &1000u16.to_be_bytes());
  assert_eq!(ws_client::read(&mut a).0, 0x8);
  ws_client::send(&mut b, 0x8, &1000u16.to_be_bytes());
  assert_eq!(ws_client::read(&mut b).0, 0x8);

  done_rx.recv_timeout(Duration::from_secs(10)).expect("engine did not exit after server.close()");

  let cap = sink.captured();
  let lines: Vec<String> =
    cap.lines_at(LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(String::from).collect();
  let expected = vec!["pub 1 2 2".to_string(), "closed 0".to_string(), "closed 0".to_string()];
  assert_eq!(lines, expected);
}
