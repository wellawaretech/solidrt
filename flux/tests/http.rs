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
fn serve_and_fetch_round_trip() {
  let port = free_port();
  let code = format!(
    r#"
        let server = Flux.serve({{
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
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            // Stopping lets the engine go idle so the test thread finishes.
            .finally(() => server.stop());
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
    ]
  );
}

#[test]
fn serve_returns_handle_and_stops() {
  let port = free_port();
  let code = format!(
    r#"
        let server = Flux.serve({{
            port: {port},
            fetch(req) {{ return "up"; }},
        }});
        // The handle exposes Bun-like introspection.
        console.log("meta", server.port, server.hostname, server.url);

        (async () => {{
            let base = "http://127.0.0.1:{port}";
            let r1 = await fetch(base + "/");
            console.log("before", r1.status, await r1.text());

            server.stop();
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
      // port echoes the bound port; hostname is what we bind; url is derived
      format!("meta {port} 0.0.0.0 http://0.0.0.0:{port}/"),
      // server is up before stop()
      "before 200 up".to_string(),
      // after stop() the connection is refused, so fetch rejects
      "after true".to_string(),
    ]
  );
}

#[test]
fn serve_honors_hostname() {
  let port = free_port();
  let code = format!(
    r#"
        let server = Flux.serve({{
            port: {port},
            hostname: "127.0.0.1",
            fetch(req) {{ return "up"; }},
        }});
        // The configured hostname is reflected back, not the "0.0.0.0" default.
        console.log("meta", server.hostname, server.url);

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("body", r.status, await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.stop());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(
    lines,
    vec![
      format!("meta 127.0.0.1 http://127.0.0.1:{port}/"),
      "body 200 up".to_string(),
    ]
  );
}

#[test]
fn serve_error_handler() {
  let port = free_port();
  let code = format!(
    r#"
        let server = Flux.serve({{
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
            .finally(() => server.stop());
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
        // No error() handler: a thrown fetch falls back to a plaintext 500.
        let server = Flux.serve({{
            port: {port},
            fetch(req) {{ throw new Error("nope"); }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("fallback", r.status, await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.stop());
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
        let server = Flux.serve({{
            port: {port},
            hostname: "127.0.0.1",
            // The second arg is the Server handle: same introspection as the
            // value Flux.serve returned.
            fetch(req, srv) {{ return srv.url + " port=" + srv.port; }},
        }});

        (async () => {{
            let r = await fetch("http://127.0.0.1:{port}/");
            console.log("arg", await r.text());
        }})()
            .catch(e => console.error("test error: " + (e && e.message || e)))
            .finally(() => server.stop());
        "#,
  );

  let lines = serve_and_capture(&code);
  assert_eq!(lines, vec![format!("arg http://127.0.0.1:{port}/ port={port}")]);
}
