#![cfg(feature = "compile")]

mod common;

use common::{run_source, LogSink};
use flux::{FluxEngine, LogLevel};
use std::time::Duration;

/// Grab a currently-free TCP port by binding an ephemeral one and releasing it.
/// There is a small race before the engine rebinds it, acceptable for tests.
fn free_port() -> u16 {
  let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
  listener.local_addr().expect("local addr").port()
}

/// Run a server script to completion and return its captured log lines. The
/// script is expected to call `server.stop()` once its work is done: that lets
/// the engine go idle and `eval_source` return, so we just wait for the thread
/// to finish (with a watchdog timeout so a broken stop fails instead of hangs).
fn serve_and_capture(code: &str) -> Vec<String> {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let code = code.to_string();
  let (done_tx, done_rx) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
    rt.block_on(engine.eval_source(&code));
    let _ = done_tx.send(());
  });

  done_rx.recv_timeout(Duration::from_secs(10)).expect("engine did not exit; did the script call server.stop()?");

  let cap = sink.captured();
  // Application log lines only; drop the "[flux] serve ..." access log.
  let mut lines: Vec<String> =
    cap.lines_at(LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(|l| l.to_string()).collect();
  // Surface any reported errors so a failing assert shows them.
  for e in cap.lines_at(LogLevel::Error) {
    lines.push(format!("ERROR: {e}"));
  }
  lines
}

#[test]
fn client_echo_round_trip() {
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
                message(ws, message) {{
                    if (typeof message === "string") ws.send("echo: " + message);
                    else ws.send(message);
                }},
            }},
        }});

        let ws = new WebSocket("ws://127.0.0.1:{port}/chat");
        console.log("connecting:", ws.readyState, ws.url);
        try {{
            ws.send("too early");
        }} catch (e) {{
            console.log("connecting send throws");
        }}
        ws.onopen = () => {{
            console.log("open:", ws.readyState);
            ws.send("hello");
        }};
        ws.onmessage = (event) => {{
            if (typeof event.data === "string") {{
                console.log("text:", event.data);
                ws.send(new Uint8Array([1, 2, 3]));
            }} else {{
                console.log("binary:", event.data.length, event.data[0], event.data[2]);
                ws.close(4100, "done");
            }}
        }};
        ws.onclose = (event) => {{
            console.log("close:", event.code, event.reason, event.wasClean, ws.readyState);
            server.stop();
        }};
    "#
  );
  let lines = serve_and_capture(&code);
  let expected = vec![
    format!("connecting: 0 ws://127.0.0.1:{port}/chat"),
    "connecting send throws".to_string(),
    "open: 1".to_string(),
    "text: echo: hello".to_string(),
    "binary: 3 1 3".to_string(),
    "close: 4100 done true 3".to_string(),
  ];
  assert_eq!(lines, expected);
}

#[tokio::test]
async fn client_connect_refused() {
  let port = free_port();
  let code = format!(
    r#"
        let ws = new WebSocket("ws://127.0.0.1:{port}/");
        ws.onerror = (event) => console.log("error:", event.type, typeof event.message);
        ws.onclose = (event) => console.log("close:", event.code, event.wasClean, ws.readyState);
    "#
  );
  let cap = run_source(&code).await;
  let lines: Vec<&str> = cap.lines_at(LogLevel::Log);
  assert_eq!(lines, vec!["error: error string", "close: 1006 false 3"]);
}

#[tokio::test]
async fn client_rejects_bad_urls() {
  let code = r#"
        for (let url of ["wss://example.com/", "http://example.com/", "ws://"]) {
            try {
                new WebSocket(url);
                console.log("accepted:", url);
            } catch (e) {
                console.log("rejected:", url);
            }
        }
    "#;
  let cap = run_source(code).await;
  assert_eq!(
    cap.lines_at(LogLevel::Log),
    vec!["rejected: wss://example.com/", "rejected: http://example.com/", "rejected: ws://"]
  );
}
