#![cfg(feature = "compile")]

mod common;

use common::run_source;

// Line delivery needs a real stdin, which belongs to the test runner; that
// path is checked by hand with the flux binary and a pipe. These cover the
// surface and the validation.

#[tokio::test]
async fn exposes_the_terminal_surface() {
  let out = run_source(
    r#"
      import { isTTY, on, once, setRawMode, write } from "flux:tty"
      console.log(typeof isTTY, typeof on, typeof once, typeof setRawMode, typeof write)
    "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "boolean function function function function");
}

#[tokio::test]
async fn rejects_an_unknown_event() {
  let out = run_source(
    r#"
      import { on } from "flux:tty"
      try {
        on("keypress", () => {})
      } catch (e) {
        console.log(e.message)
      }
    "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "Unknown tty event: keypress (expected \"line\", \"key\" or \"close\")");
}
