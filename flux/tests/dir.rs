#![cfg(feature = "compile")]

mod common;

use common::{run_source, TempDir};

// The dir(), realpath and glob tests are flux/tests/dir.test.ts, run by sol
// test. What stays here is what a test inside the engine cannot observe:
// the host drops a test's engine at the test's end, so whether the engine
// went idle by itself is only visible from here.

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
