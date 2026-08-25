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

#[tokio::test]
async fn alive_sees_own_process_and_not_a_free_pid() {
  let out = run_source(
    r#"
      import { alive, pid } from "flux:process"
      console.log(alive(pid), alive(4294967294))
    "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true false");
}

#[tokio::test]
async fn env_is_a_snapshot_of_the_environment() {
  std::env::set_var("FLUX_TEST_ENV", "yes");
  let out = run_source(
    r#"
      import { env } from "flux:process"
      console.log(env.FLUX_TEST_ENV, env.FLUX_TEST_UNSET === undefined)
    "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "yes true");
}
