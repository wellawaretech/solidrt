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
  assert_eq!(out.lines_at(flux::LogLevel::Log), vec!["text/plain", "1", "true false", "2", "2, 3", "null", "null"]);
}

#[tokio::test]
async fn headers_init_copies_instance_and_rejects_non_string_values() {
  let out = run_source(
    r#"
            let a = new Headers({ "X-A": "1" });
            let b = new Headers(a);
            a.set("X-A", "changed");
            // b copied a's entries at construction; a's later mutation stays in a.
            console.log(b.get("x-a"));
            // A non-string value is a caller bug and throws (never stringified
            // or silently dropped).
            try {
                new Headers({ "X-N": 5 });
                console.log("no throw");
            } catch (e) {
                console.log("threw", String(e.message || e).includes("must be a string"));
            }
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.lines_at(flux::LogLevel::Log), vec!["1", "threw true"]);
}

#[tokio::test]
async fn array_buffer_transfer_family_is_removed() {
  // The vendored quickjs-ng transfer() corrupts externally backed buffers
  // (okf/upstream/quickjs-ng-transfer-external-buffers.md), so context setup
  // removes all three variants; ordinary ArrayBuffer use is untouched.
  let out = run_source(
    r#"
            let names = ["transfer", "transferToImmutable", "transferToFixedLength"];
            console.log(names.map(n => ArrayBuffer.prototype[n] === undefined).join(","));
            let buf = new ArrayBuffer(4);
            new Uint8Array(buf)[0] = 7;
            console.log(buf.byteLength, new Uint8Array(buf)[0]);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.lines_at(flux::LogLevel::Log), vec!["true,true,true", "4 7"]);
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

#[tokio::test]
async fn text_encoder_encodes_utf8() {
  let out = run_source(
    r#"
            let enc = new TextEncoder();
            console.log(enc.encoding);
            // "A" is one byte; the euro sign is three (E2 82 AC = 226,130,172).
            let bytes = enc.encode("A€");
            console.log(bytes.length);
            console.log(Array.from(bytes).join(","));
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "utf-8\n4\n65,226,130,172");
}

#[tokio::test]
async fn text_decoder_streams_split_multibyte() {
  let out = run_source(
    r#"
            let bytes = new TextEncoder().encode("a€b"); // 61, E2 82 AC, 62
            let dec = new TextDecoder();
            // Split mid euro-sign: the first chunk ends one byte into it.
            let p1 = dec.decode(bytes.slice(0, 2), { stream: true });
            let p2 = dec.decode(bytes.slice(2), { stream: true });
            console.log(p1 + p2);
            console.log(dec.encoding);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "a\u{20ac}b\nutf-8");
}

#[tokio::test]
async fn text_decoder_fatal_and_replacement() {
  let out = run_source(
    r#"
            // Non-fatal: an invalid byte becomes the replacement char U+FFFD.
            let lenient = new TextDecoder().decode(new Uint8Array([0xff]));
            console.log(lenient === "�");
            // Fatal: the same input throws instead.
            let threw = false;
            try { new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff])); }
            catch (e) { threw = true; }
            console.log(threw);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true\ntrue");
}

#[tokio::test]
async fn text_decoder_bom_and_label() {
  let out = run_source(
    r#"
            let bom = new Uint8Array([0xef, 0xbb, 0xbf, 0x68, 0x69]); // BOM + "hi"
            // A leading BOM is stripped by default.
            console.log(new TextDecoder().decode(bom));
            // ...and kept with ignoreBOM (length 3: U+FEFF, h, i).
            console.log(new TextDecoder("utf-8", { ignoreBOM: true }).decode(bom).length);
            // A non-utf-8 label is rejected.
            let msg = "no throw";
            try { new TextDecoder("utf-16"); } catch (e) { msg = String(e.message || e); }
            console.log(msg.includes("utf-8"));
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "hi\n3\ntrue");
}
