#![cfg(feature = "compile")]

mod common;

use common::run_source;

#[tokio::test]
async fn subtle_digest_sha256() {
  let out = run_source(
    r#"
      let hex = (buf) => Array.from(new Uint8Array(buf)).map((b) => b.toString(16).padStart(2, "0")).join("");
      let bytes = new TextEncoder().encode("abc");
      let d256 = await crypto.subtle.digest("SHA-256", bytes);
      console.log(d256 instanceof ArrayBuffer, hex(d256));
      // An ArrayBuffer input and the { name } spelling are accepted too.
      let d512 = await crypto.subtle.digest({ name: "SHA-512" }, bytes.buffer);
      console.log(d512.byteLength, hex(d512).slice(0, 16));
      let d384 = await crypto.subtle.digest("sha-384", new Uint8Array(0));
      console.log(d384.byteLength);
    "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(
    out.lines_at(flux::LogLevel::Log),
    vec![
      "true ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
      "64 ddaf35a193617aba",
      "48",
    ]
  );
}

#[tokio::test]
async fn subtle_digest_rejects_unsupported() {
  let out = run_source(
    r#"
      try {
        await crypto.subtle.digest("SHA-1", new Uint8Array(0));
        console.log("no throw");
      } catch (e) {
        console.log("rejected", e.message);
      }
      try {
        await crypto.subtle.digest("SHA-256", "abc");
        console.log("no throw");
      } catch (e) {
        console.log("rejected", e.message);
      }
    "#,
  )
  .await;
  assert_eq!(
    out.lines_at(flux::LogLevel::Log),
    vec![
      "rejected crypto.subtle.digest: unsupported algorithm \"SHA-1\" (SHA-256, SHA-384, SHA-512)",
      "rejected crypto.subtle.digest: data must be a Uint8Array or ArrayBuffer",
    ]
  );
}
