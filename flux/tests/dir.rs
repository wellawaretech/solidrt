#![cfg(feature = "compile")]

mod common;

use common::{run_source, TempDir};

// dir(path) -> object with entries() and exists(). Fixtures are laid down
// from Rust (the API has no mkdir), then listed/checked from JS.

#[tokio::test]
async fn entries_lists_names_and_types() {
  let dir = TempDir::new();
  std::fs::write(dir.as_path().join("a.txt"), "a").expect("write a.txt");
  std::fs::write(dir.as_path().join("b.txt"), "b").expect("write b.txt");
  std::fs::create_dir(dir.as_path().join("sub")).expect("create sub");

  let code = r#"
            import { dir } from "flux:fs";
            let d = dir("__DIR__");
            let es = await d.entries();
            es.sort((a, b) => (a.name < b.name ? -1 : 1));
            console.log(es.map(e => e.name + ":" + e.type).join(","));
            "#
  .replace("__DIR__", &dir.path());

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "a.txt:file,b.txt:file,sub:directory");
}

#[tokio::test]
async fn exists_true_for_dir_false_for_file_and_missing() {
  let dir = TempDir::new();
  let file = dir.join("f.txt");
  std::fs::write(&file, "x").expect("write f.txt");
  let missing = dir.join("no-such-dir");

  let code = r#"
            import { dir } from "flux:fs";
            let d = dir("__DIR__");
            let asFile = dir("__FILE__");
            let missing = dir("__MISSING__");
            console.log(await d.exists(), await asFile.exists(), await missing.exists());
            "#
  .replace("__DIR__", &dir.path())
  .replace("__FILE__", &file)
  .replace("__MISSING__", &missing);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  // exists() is is_dir, so a file path and a missing path are both false.
  assert_eq!(out.log(), "true false false");
}

#[tokio::test]
async fn entries_on_missing_rejects() {
  let dir = TempDir::new();
  let missing = dir.join("no-such-dir");
  let code = r#"
            import { dir } from "flux:fs";
            let d = dir("__MISSING__");
            let msg = "no error";
            try {
                await d.entries();
            } catch (e) {
                msg = "rejected";
            }
            console.log(msg);
            "#
  .replace("__MISSING__", &missing);

  let out = run_source(&code).await;
  assert_eq!(out.log(), "rejected");
}