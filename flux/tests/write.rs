#![cfg(feature = "compile")]

mod common;

use common::{run_source, TempDir};

// file(path).write(data) where data is a string or Uint8Array. These verify the
// written bytes directly from Rust, keeping the focus on write itself.

#[tokio::test]
async fn writes_string() {
  let dir = TempDir::new();
  let file = dir.join("s.txt");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write("content here");
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(std::fs::read_to_string(&file).expect("read back"), "content here");
}

#[tokio::test]
async fn writes_uint8array() {
  let dir = TempDir::new();
  let file = dir.join("bytes.bin");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write(new Uint8Array([1, 2, 3, 4]));
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(std::fs::read(&file).expect("read back"), vec![1u8, 2, 3, 4]);
}

#[tokio::test]
async fn overwrites_existing_file() {
  let dir = TempDir::new();
  let file = dir.join("over.txt");
  std::fs::write(&file, "old contents").expect("seed file");
  let code = r#"
            import { file } from "flux:fs";
            await file("__FILE__").write("new");
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(std::fs::read_to_string(&file).expect("read back"), "new");
}

#[tokio::test]
async fn rejects_invalid_data_type() {
  let dir = TempDir::new();
  let file = dir.join("never.txt");
  let code = r#"
            import { file } from "flux:fs";
            let msg = "no throw";
            try {
                await file("__FILE__").write(123);
            } catch (e) {
                msg = String(e.message || e);
            }
            console.log(msg);
            "#
  .replace("__FILE__", &file);

  let out = run_source(&code).await;
  assert!(out.log().contains("data must be string or Uint8Array"), "got: {}", out.log());
  assert!(!std::path::Path::new(&file).exists(), "file should not have been created");
}