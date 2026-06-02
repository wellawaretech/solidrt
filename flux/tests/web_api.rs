#![cfg(feature = "compile")]

mod common;

use common::run_source;

// Web-API objects (Headers, Request, Response, body) tested in isolation, with
// no server involved. These all resolve their bodies synchronously, so the
// engine loop drains and run_source returns normally.

#[tokio::test]
async fn headers_case_insensitive_and_multi_value() {
  let out = run_source(
    r#"
            let h = new Headers({ "Content-Type": "text/plain", "X-A": "1" });
            console.log(h.get("content-type"));
            console.log(h.get("X-A"));
            console.log(h.has("x-a"), h.has("nope"));
            h.set("X-A", "2");
            console.log(h.get("x-a"));
            h.append("X-A", "3");
            console.log(h.get("x-a"));
            h.delete("x-a");
            console.log(h.get("x-a"));
            console.log(h.get("missing"));
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  // A missing header returns null (WHATWG Headers.get semantics), logged as "null".
  assert_eq!(
    out.lines_at(flux::LogLevel::Log),
    vec!["text/plain", "1", "true false", "2", "2, 3", "null", "null"]
  );
}

#[tokio::test]
async fn response_status_headers_and_body() {
  let out = run_source(
    r#"
            let r = new Response("hello", { status: 201, statusText: "Created", headers: { "X-T": "v" } });
            console.log(r.status, r.statusText, r.ok);
            console.log(r.headers.get("x-t"));
            console.log(await r.text());
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "201 Created true\nv\nhello");
}

#[tokio::test]
async fn response_defaults_and_ok_range() {
  let out = run_source(
    r#"
            let ok = new Response();
            console.log(ok.status, ok.ok);
            let bad = new Response("x", { status: 404 });
            console.log(bad.status, bad.ok);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "200 true\n404 false");
}

#[tokio::test]
async fn response_json_static_sets_content_type() {
  let out = run_source(
    r#"
            let r = Response.json({ a: 1, b: "two" });
            console.log(r.status, r.headers.get("content-type"));
            let j = await r.json();
            console.log(j.a, j.b);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "200 application/json\n1 two");
}

#[tokio::test]
async fn request_method_uppercased_and_body() {
  let out = run_source(
    r#"
            let req = new Request("http://x/y", { method: "post", body: "data", headers: { "X-H": "h" } });
            console.log(req.method, req.url);
            console.log(req.headers.get("x-h"));
            console.log(await req.text());
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "POST http://x/y\nh\ndata");
}

#[tokio::test]
async fn request_json_body() {
  let out = run_source(
    r#"
            let req = new Request("http://x", { method: "POST", body: JSON.stringify({ n: 5 }) });
            let j = await req.json();
            console.log(j.n);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "5");
}

#[tokio::test]
async fn body_is_consume_once() {
  let out = run_source(
    r#"
            let r = new Response("once");
            console.log(await r.text());
            let msg = "no throw";
            try {
                await r.text();
            } catch (e) {
                msg = String(e.message || e);
            }
            console.log(msg);
            "#,
  )
  .await;
  assert_eq!(out.log(), "once\nBody already consumed");
}

#[tokio::test]
async fn response_rejects_invalid_body_type() {
  let out = run_source(
    r#"
            let msg = "no throw";
            try {
                new Response(123);
            } catch (e) {
                msg = String(e.message || e);
            }
            console.log(msg);
            "#,
  )
  .await;
  assert!(out.log().contains("must be string"), "got: {}", out.log());
}