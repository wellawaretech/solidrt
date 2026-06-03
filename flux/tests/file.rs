#![cfg(feature = "compile")]

mod common;

use common::{run_source, TempDir};

// file(path) -> object with text()/bytes()/json()/exists()/stat()/write(). Bodies
// are read from disk on each call (re-readable, unlike a Response). Each test
// uses file(path).write() to lay down its fixture, then reads it back.

#[tokio::test]
async fn read_text_and_path() {
  let dir = TempDir::new();
  let file = dir.join("hello.txt");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write("hello world");
            let f = file("__FILE__");
            console.log(f.path === "__FILE__");
            console.log(await f.text());
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true\nhello world");
}

#[tokio::test]
async fn read_bytes_as_uint8array() {
  let dir = TempDir::new();
  let file = dir.join("bytes.bin");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write(new Uint8Array([10, 20, 30]));
            let f = file("__FILE__");
            let b = await f.bytes();
            console.log(b instanceof Uint8Array, b.length, b[0], b[1], b[2]);
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true 3 10 20 30");
}

#[tokio::test]
async fn read_json() {
  let dir = TempDir::new();
  let file = dir.join("data.json");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write(JSON.stringify({ n: 7, s: "x" }));
            let f = file("__FILE__");
            let j = await f.json();
            console.log(j.n, j.s);
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "7 x");
}

#[tokio::test]
async fn exists_true_for_file_false_for_missing() {
  let dir = TempDir::new();
  let present = dir.join("present.txt");
  let missing = dir.join("missing.txt");
  let code = r#"
            import { file } from "flux:fs";
            await file("__PRESENT__").write("x");
            let p = file("__PRESENT__");
            let m = file("__MISSING__");
            console.log(await p.exists(), await m.exists());
            "#
  .replace("__PRESENT__", &present)
  .replace("__MISSING__", &missing);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true false");
}

#[tokio::test]
async fn stat_reports_size_and_type() {
  let dir = TempDir::new();
  let file = dir.join("sized.txt");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write("12345");
            let f = file("__FILE__");
            let s = await f.stat();
            console.log(s.size, s.type);
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "5 file");
}

#[tokio::test]
async fn text_is_rereadable() {
  let dir = TempDir::new();
  let file = dir.join("again.txt");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write("again");
            let f = file("__FILE__");
            console.log(await f.text());
            console.log(await f.text());
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  // A file body is NOT consume-once, so a second read succeeds.
  assert_eq!(out.log(), "again\nagain");
}

#[tokio::test]
async fn stat_on_missing_rejects() {
  let dir = TempDir::new();
  let missing = dir.join("nope.txt");
  let code = r#"
            import { file } from "flux:fs";
            let f = file("__MISSING__");
            let msg = "no error";
            try {
                await f.stat();
            } catch (e) {
                msg = "rejected";
            }
            console.log(msg);
            "#
  .replace("__MISSING__", &missing);

  let out = run_source(&code).await;
  assert_eq!(out.log(), "rejected");
}