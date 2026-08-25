#![cfg(feature = "compile")]

mod common;

use common::run_source;
use std::time::Duration;

// A signal listener holds the engine loop open; unsubscribing the last one
// must let it go idle right away, not on the next delivery of the signal.
#[tokio::test]
async fn signal_unsubscribe_lets_engine_idle() {
  let run = run_source(
    r#"
      import { on, pid } from "flux:process"
      console.log("pid", typeof pid, pid > 0)
      let off = on("SIGUSR1", () => {})
      setTimeout(() => { off(); console.log("off") }, 50)
    "#,
  );
  let out = tokio::time::timeout(Duration::from_secs(5), run)
    .await
    .expect("engine stayed alive after the last signal listener was removed");
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.lines_at(flux::LogLevel::Log), vec!["pid number true", "off"]);
}
