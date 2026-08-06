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

/// Run a server script to completion and return its captured log lines. The
/// script is expected to call `server.close()` once its work is done: that lets
/// the engine go idle and `eval_source` return, so we just wait for the thread
/// to finish (with a watchdog timeout so a broken close fails instead of hangs).
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

  done_rx.recv_timeout(Duration::from_secs(10)).expect("engine did not exit; did the script call server.close()?");

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
fn serve_and_fetch_round_trip() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            async fetch(req) {{
                if (req.url === "/json") return Response.json({{ ok: true, where: req.url }});
                if (req.url === "/custom") {{
                    return new Response("made", {{ status: 418, headers: {{ "X-Brewed-By": "flux" }} }});
                }}
                if (req.url === "/echo") {{
                    let body = await req.text();
                    return new Response(req.method + ":" + body + ":" + (req.headers.get("x-demo") || "none"));
                }}
                return "hello";
            }},
        }});

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            let r1 = await fetch(base + "/");
            console.log("root", r1.status, r1.ok, r1.headers.get("content-type"), await r1.text());

            let r2 = await fetch(base + "/json");
            let j = await r2.json();
            console.log("json", r2.status, r2.headers.get("content-type"), j.ok, j.where);

            let r3 = await fetch(base + "/custom");
            console.log("custom", r3.status, r3.statusText, r3.ok, r3.headers.get("x-brewed-by"), await r3.text());

            let r4 = await fetch(base + "/echo", {{ method: "POST", body: "hi", headers: {{ "X-Demo": "abc" }} }});
            console.log("echo", await r4.text());

            let r5 = await fetch(base + "/echo", {{ method: "POST", body: "hi", headers: new Headers({{ "X-Demo": "inst" }}) }});
            console.log("echo-headers", await r5.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            // Stopping lets the engine go idle so the test thread finishes.
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(
    lines,
    vec![
      // string return -> 200 text/plain
      "root 200 true text/plain hello",
      // Response.json -> 200 application/json, body parsed back to an object
      "json 200 application/json true /json",
      // custom status + header round-trip; 418 resolves to its canonical reason
      // phrase and ok is false (not 2xx)
      "custom 418 I'm a teapot false flux made",
      // POST body and a request header reach the handler; method is uppercased
      "echo POST:hi:abc",
      // a Headers instance works as the headers option (entries live in Rust,
      // so it must be recognized, not iterated as a plain object)
      "echo-headers POST:hi:inst",
    ]
  );
}

#[tokio::test]
async fn fetch_rejects_unsupported_header_and_body_values() {
  // Both throw synchronously at the call site (caller bugs, not environmental
  // failures), before any network activity - so no server is needed.
  let out = common::run_source(
    r#"
            try {
                fetch("http://127.0.0.1:9/", { method: "POST", body: { a: 1 } });
                console.log("no throw");
            } catch (e) {
                console.log("body", String(e.message || e).includes("Fetch body"));
            }
            try {
                fetch("http://127.0.0.1:9/", { headers: { "X-N": 5 } });
                console.log("no throw");
            } catch (e) {
                console.log("headers", String(e.message || e).includes("must be a string"));
            }
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.lines_at(flux::LogLevel::Log), vec!["body true", "headers true"]);
}

#[test]
fn serve_returns_handle_and_stops() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            fetch(req) {{ return "up"; }},
        }});
        // The handle exposes Bun-like introspection.
        console.log("meta", server.port, server.host, server.url);

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            let r1 = await fetch(base + "/");
            console.log("before", r1.status, await r1.text());

            server.close();
            // Let the accept loop drop the listener and connections drain.
            await new Promise(r => setTimeout(r, 200));

            // After stop() the listener is closed and the pooled keep-alive
            // connection was gracefully shut down, so a fresh dial is refused.
            let refused = false;
            try {{
                let r2 = await fetch(base + "/");
                await r2.text();
            }} catch (e) {{
                refused = true;
            }}
            console.log("after", refused);
        }})().catch(e => console.error("test error: " + (e && e.message || e)));
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(
    lines,
    vec![
      // port echoes the bound port; host is what we bind; url is derived
      format!("meta {port} 0.0.0.0 http://0.0.0.0:{port}/"),
      // server is up before stop()
      "before 200 up".to_string(),
      // after stop() the connection is refused, so fetch rejects
      "after true".to_string(),
    ]
  );
}

#[test]
fn serve_honors_host() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            host: "127.0.0.1",
            fetch(req) {{ return "up"; }},
        }});
        // The configured host is reflected back, not the "0.0.0.0" default.
        console.log("meta", server.host, server.url);

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("body", r.status, await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec![format!("meta 127.0.0.1 http://127.0.0.1:{port}/"), "body 200 up".to_string(),]);
}

#[test]
fn serve_error_handler() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            fetch(req) {{
                if (req.url === "/throw") throw new Error("boom");
                if (req.url === "/reject") return Promise.reject(new Error("async boom"));
                return "ok";
            }},
            // Receives the thrown value; its returned Response (status included)
            // is sent instead of the default 500.
            error(err) {{ return new Response("handled: " + err.message, {{ status: 502 }}); }},
        }});

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            let r1 = await fetch(base + "/throw");
            console.log("throw", r1.status, await r1.text());
            let r2 = await fetch(base + "/reject");
            console.log("reject", r2.status, await r2.text());
            let r3 = await fetch(base + "/");
            console.log("ok", r3.status, await r3.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(
    lines,
    vec![
      // sync throw routes through error(); custom status passes through
      "throw 502 handled: boom".to_string(),
      // a rejected promise reaches error() the same way
      "reject 502 handled: async boom".to_string(),
      // a successful request is untouched by error()
      "ok 200 ok".to_string(),
    ]
  );
}

#[test]
fn serve_error_default_500() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        // No error() handler: a thrown fetch falls back to a plaintext 500.
        let server = serve({{
            port: {port},
            fetch(req) {{ throw new Error("nope"); }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("fallback", r.status, await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["fallback 500 Internal Server Error".to_string()]);
}

#[test]
fn serve_passes_server_arg() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            host: "127.0.0.1",
            // The second arg is the Server handle: same introspection as the
            // value serve() returned.
            fetch(req, srv) {{ return srv.url + " port=" + srv.port; }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("arg", await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec![format!("arg http://127.0.0.1:{port}/ port={port}")]);
}

#[test]
fn serve_routes() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            routes: {{
                "/":          () => "root",
                "/version":   Response.json({{ v: 1 }}),          // static Response
                "/users/me":  () => "me",                        // exact beats :id
                "/users/:id": (req) => "user " + req.params.id,  // path param
                "/files/*":   () => "wild",                      // trailing wildcard
            }},
            fetch(req) {{ return "fallback " + req.url; }},
        }});

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            console.log("root", await (await fetch(base + "/")).text());
            let v = await fetch(base + "/version");
            console.log("version", v.headers.get("content-type"), (await v.json()).v);
            console.log("me", await (await fetch(base + "/users/me")).text());
            console.log("id", await (await fetch(base + "/users/42")).text());
            console.log("wild", await (await fetch(base + "/files/a/b/c")).text());
            console.log("fallback", await (await fetch(base + "/nope")).text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(
    lines,
    vec![
      "root root".to_string(),
      // static Response keeps its content-type and body across requests
      "version application/json 1".to_string(),
      // exact "/users/me" wins over "/users/:id"
      "me me".to_string(),
      // :id is captured into req.params
      "id user 42".to_string(),
      // "/files/*" matches the remaining segments
      "wild wild".to_string(),
      // unmatched paths fall through to fetch (label + handler body)
      "fallback fallback /nope".to_string(),
    ]
  );
}

#[test]
fn serve_routes_decodes_params() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        let server = serve({{
            port: {port},
            routes: {{
                "/users/:id":  (req) => req.params.id,
                "/files/:name": (req) => req.params.name,
            }},
        }});

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            // %20 -> space
            console.log("space", await (await fetch(base + "/users/john%20doe")).text());
            // %2F stays inside the param value; still one segment matching :name
            console.log("slash", await (await fetch(base + "/files/a%2Fb")).text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["space john doe".to_string(), "slash a/b".to_string()]);
}

#[test]
fn serve_routes_404_without_fetch() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        // routes but no fetch: an unmatched path is a 404.
        let server = serve({{
            port: {port},
            routes: {{ "/hit": () => "hit" }},
        }});

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            let r1 = await fetch(base + "/hit");
            console.log("hit", r1.status, await r1.text());
            let r2 = await fetch(base + "/miss");
            console.log("miss", r2.status, await r2.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["hit 200 hit".to_string(), "miss 404 Not Found".to_string()]);
}

#[test]
fn serve_routes_per_method() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        // A route value can be a per-method object; the request method picks the
        // handler, and an unlisted method is a 405 with an Allow header.
        let server = serve({{
            port: {port},
            routes: {{
                "/api": {{
                    GET:  () => "got",
                    POST: () => "posted",
                }},
            }},
        }});

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            console.log("get", await (await fetch(base + "/api")).text());
            console.log("post", await (await fetch(base + "/api", {{ method: "POST" }})).text());
            let d = await fetch(base + "/api", {{ method: "DELETE" }});
            console.log("delete", d.status, d.headers.get("allow"), await d.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(
    lines,
    vec![
      "get got".to_string(),
      "post posted".to_string(),
      // unlisted method -> 405 with Allow listing the registered methods in order
      "delete 405 GET, POST Method Not Allowed".to_string(),
    ]
  );
}

#[test]
fn serve_streams_async_iterable() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";
        // A handler can return a Response whose body is an async generator; each
        // yielded chunk is streamed to the client (chunked transfer encoding).
        async function* chunks() {{
            yield "Hello, ";
            yield "streamed ";
            yield "world";
        }}

        let server = serve({{
            port: {port},
            fetch() {{
                return new Response(chunks(), {{ headers: {{ "Content-Type": "text/plain" }} }});
            }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("ct", r.headers.get("content-type"));
            // The client reassembles the streamed chunks into the full body.
            console.log("body", await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["ct text/plain".to_string(), "body Hello, streamed world".to_string()]);
}

#[test]
fn serve_receives_streamed_request_body() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";

        // fetch can stream a request body from an async generator; the server
        // collects the chunked body and sees the full payload.
        async function* parts() {{
            yield "strea";
            yield "med ";
            yield "request";
        }}

        let server = serve({{
            port: {port},
            async fetch(req) {{ return new Response(req.method + ":" + await req.text()); }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/upload", {{ method: "POST", body: parts() }});
            console.log("echo", await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["echo POST:streamed request".to_string()]);
}

#[test]
fn fetch_iterates_response_body_stream() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";

        // The server streams a chunked response; the client consumes it lazily by
        // iterating `response.body` (a Rust-backed async-iterable of Uint8Array
        // chunks), reassembling the payload as the chunks arrive.
        async function* chunks() {{
            yield "Hello, ";
            yield "streamed ";
            yield "world";
        }}

        let server = serve({{
            port: {port},
            fetch() {{ return new Response(chunks()); }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            let dec = new TextDecoder();
            let text = "";
            for await (const chunk of r.body) {{
                text += dec.decode(chunk, {{ stream: true }});
            }}
            text += dec.decode();
            console.log("iterated", text);
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["iterated Hello, streamed world".to_string()]);
}

#[test]
fn serve_iterates_request_body_stream() {
  let port = free_port();
  let code = format!(
    r#"
        import {{ serve }} from "flux:http";

        // The client streams a request body; the server reads it incrementally by
        // iterating `req.body` (a Rust-backed async-iterable of Uint8Array chunks)
        // rather than the framework buffering the whole upload up front.
        async function* parts() {{
            yield "incre";
            yield "mental ";
            yield "upload";
        }}

        let server = serve({{
            port: {port},
            async fetch(req) {{
                let dec = new TextDecoder();
                let text = "";
                for await (const chunk of req.body) {{
                    text += dec.decode(chunk, {{ stream: true }});
                }}
                text += dec.decode();
                return new Response("got:" + text);
            }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/upload", {{ method: "POST", body: parts() }});
            console.log("resp", await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.close());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec!["resp got:incremental upload".to_string()]);
}

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
    cap.lines_at(flux::LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(String::from).collect();
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
    cap.lines_at(flux::LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(String::from).collect();
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
    cap.lines_at(flux::LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(String::from).collect();
  let expected = vec!["pub 1 2 2".to_string(), "closed 0".to_string(), "closed 0".to_string()];
  assert_eq!(lines, expected);
}
