#![cfg(feature = "compile")]

mod common;

use common::LogSink;
use flux::{FluxEngine, LogLevel};
use std::time::{Duration, Instant};

/// Grab a currently-free TCP port by binding an ephemeral one and releasing it.
/// There is a small race before the engine rebinds it, acceptable for tests.
fn free_port() -> u16 {
  let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
  listener.local_addr().expect("local addr").port()
}

/// Run a server script on a detached engine thread and poll the captured log
/// until the script logs the "DONE" sentinel (or an error is reported), or the
/// timeout elapses. `Flux.serve` holds the engine's pending count open forever,
/// so `eval_source` never returns and the thread is intentionally left running;
/// it dies when the test process exits.
fn serve_and_capture(code: &str) -> Vec<String> {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let code = code.to_string();
  std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
    rt.block_on(engine.eval_source(&code));
  });

  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    let cap = sink.captured();
    let done = cap.lines_at(LogLevel::Log).iter().any(|l| *l == "DONE");
    if done || cap.has_error() || Instant::now() >= deadline {
      // Application log lines only; drop the "[flux] serve ..." access log.
      let mut lines: Vec<String> =
        cap.lines_at(LogLevel::Log).into_iter().filter(|l| !l.starts_with("[flux]")).map(|l| l.to_string()).collect();
      // Surface any reported errors so a failing assert shows them.
      for e in cap.lines_at(LogLevel::Error) {
        lines.push(format!("ERROR: {e}"));
      }
      return lines;
    }
    std::thread::sleep(Duration::from_millis(20));
  }
}

#[test]
fn serve_and_fetch_round_trip() {
  let port = free_port();
  let code = format!(
    r#"
        Flux.serve({{
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

            console.log("DONE");
        }})().catch(e => console.error("test error: " + (e && e.message || e)));
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
      "DONE",
    ]
  );
}
