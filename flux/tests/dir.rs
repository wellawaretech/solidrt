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

#[tokio::test]
async fn realpath_resolves_and_rejects_missing() {
  let dir = TempDir::new();
  std::fs::create_dir(dir.as_path().join("sub")).expect("create sub");
  let expected = std::fs::canonicalize(dir.as_path().join("sub")).expect("canonical sub");
  let dotted = dir.join("sub/../sub");
  let missing = dir.join("nope");

  let code = r#"
            import { realpath } from "flux:fs";
            console.log(await realpath("__DOTTED__"));
            try {
                await realpath("__MISSING__");
                console.log("no error");
            } catch (e) {
                console.log("rejected");
            }
            "#
  .replace("__DOTTED__", &dotted)
  .replace("__MISSING__", &missing);

  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), format!("{}\nrejected", expected.to_string_lossy()));
}

// watch(callback) sees a created file and the target name of a rename (an
// editor's atomic save); the unsubscribe stops the watch and lets the engine
// go idle right away.
#[tokio::test]
async fn watch_reports_changes_and_unsubscribe_lets_engine_idle() {
  let dir = TempDir::new();
  let code = r#"
            import { dir } from "flux:fs";
            import { file } from "flux:fs";
            let seen = [];
            let off = dir("__DIR__").watch((e) => {
              seen.push(e.kind + ":" + e.path.split("/").pop());
              if (e.kind === "rename" && e.path.endsWith("a.txt")) {
                off();
                console.log(seen.includes("create:a.tmp"), seen.includes("rename:a.txt"));
              }
            });
            await file("__DIR__/a.tmp").write("x");
            "#
  .replace("__DIR__", &dir.path());

  let dir_path = dir.as_path().to_path_buf();
  let renamer = tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    std::fs::rename(dir_path.join("a.tmp"), dir_path.join("a.txt")).expect("rename a.tmp");
  });
  let out = tokio::time::timeout(std::time::Duration::from_secs(5), run_source(&code))
    .await
    .expect("engine stayed alive after the watch was unsubscribed");
  renamer.await.expect("renamer");
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true true");
}

#[tokio::test]
async fn watch_throws_for_a_missing_directory() {
  let dir = TempDir::new();
  let code = r#"
            import { dir } from "flux:fs";
            try {
              dir("__DIR__/no-such-dir").watch(() => {});
              console.log("no throw");
            } catch (e) {
              console.log(e instanceof Error, e.message.startsWith("watch "));
            }
            "#
  .replace("__DIR__", &dir.path());
  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true true");
}
