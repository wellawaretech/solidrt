#![cfg(feature = "compile")]

mod common;

use common::run_source;
use std::time::Duration;

// The liveness and signal tests of flux:process are flux/tests/process.test.ts,
// run by sol test. What stays here is what a test inside the engine cannot
// observe: the engine going idle, a variable the test set in its own
// environment, and the process ending.

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

// exit ends the process inside the call, so these run the flux binary on a
// script from stdin and read its status and output.
fn run_flux(source: &str) -> std::process::Output {
  use std::io::Write;
  use std::process::{Command, Stdio};
  let mut child = Command::new(env!("CARGO_BIN_EXE_flux"))
    .arg("-")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("start the flux binary");
  child.stdin.take().expect("stdin is piped").write_all(source.as_bytes()).expect("write the script");
  child.wait_with_output().expect("wait for the flux binary")
}

#[test]
fn exit_ends_the_process_with_the_code() {
  let out = run_flux(
    r#"
      import { exit } from "flux:process"
      setTimeout(() => console.log("never"), 0)
      console.log("before")
      exit(3)
      console.log("after")
    "#,
  );
  assert_eq!(out.status.code(), Some(3));
  assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "before");
}

#[test]
fn exit_defaults_to_success() {
  let out = run_flux(
    r#"
      import { exit } from "flux:process"
      exit()
    "#,
  );
  assert_eq!(out.status.code(), Some(0));
}

#[test]
fn exit_refuses_a_code_no_os_carries() {
  let out = run_flux(
    r#"
      import { exit } from "flux:process"
      for (let code of [256, -1, 1.5]) {
        try { exit(code) } catch (e) { console.log(e.message) }
      }
    "#,
  );
  let printed = String::from_utf8_lossy(&out.stdout);
  let lines: Vec<&str> = printed.lines().collect();
  assert_eq!(
    lines,
    vec![
      "exit: code 256 is not an integer in 0..255",
      "exit: code -1 is not an integer in 0..255",
      "exit: code 1.5 is not an integer in 0..255",
    ]
  );
  assert_eq!(out.status.code(), Some(0));
}

// Without the host's say (run_source sets no ProcessExit, like a windowed
// app's engine and like an isolate) exit throws and the process goes on.
#[tokio::test]
async fn exit_throws_where_the_host_did_not_allow_it() {
  let out = run_source(
    r#"
      import { exit } from "flux:process"
      try { exit(0) } catch (e) { console.log(e.message.startsWith("exit: this host does not end through flux:process")) }
      console.log("still here")
    "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.lines_at(flux::LogLevel::Log), vec!["true", "still here"]);
}
