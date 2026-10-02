#![cfg(feature = "compile")]

mod common;

use common::run_source;

// The timer and microtask tests are flux/tests/time.test.ts, run by sol
// test. What stays here is what a test inside the engine cannot observe:
// under flux:test an uncaught error fails the test, so the report itself is
// only visible to the host.

#[tokio::test]
async fn queue_microtask_throw_is_reported() {
  let out = run_source(r#"queueMicrotask(() => { throw new Error("boom"); });"#).await;
  assert!(out.has_error(), "expected throwing microtask to be reported as uncaught");
}
