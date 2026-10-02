#![cfg(feature = "compile")]

mod common;

use common::run_source;

// The promise tests are flux/tests/promises.test.ts, run by sol test. What
// stays here is what a test inside the engine cannot observe: under
// flux:test an uncaught rejection fails the test, so whether and when the
// host is told is only visible from here.
//
// The host promise rejection tracker reports a rejection only if it is still
// unhandled once the job queue drains, so a rejection handled a microtask later
// (e.g. `.catch()`) must not be reported. See engine::flush_rejections.

#[tokio::test]
async fn unhandled_rejection_is_reported() {
  let out = run_source("Promise.reject(new Error('boom-unhandled'))").await;
  assert!(
    out.errors().contains("boom-unhandled"),
    "expected an unhandled rejection to be reported, got errors: {:?}",
    out.errors()
  );
}

#[tokio::test]
async fn handled_rejection_is_not_reported() {
  let out = run_source("Promise.reject(new Error('boom-handled')).catch(() => {})").await;
  assert!(!out.has_error(), "a synchronously-handled rejection should not be reported, got errors: {:?}", out.errors());
}

#[tokio::test]
async fn unhandled_async_throw_is_reported() {
  let out = run_source(
    r#"
            async function f() { throw new Error('boom-async-await') }
            f()
            "#,
  )
  .await;
  assert!(
    out.errors().contains("boom-async-await"),
    "expected an unhandled async rejection to be reported, got errors: {:?}",
    out.errors()
  );
}

#[tokio::test]
async fn handled_async_throw_is_not_reported() {
  let out = run_source(
    r#"
            async function g() { throw new Error('boom-async-caught') }
            g().catch(() => {})
            "#,
  )
  .await;
  assert!(!out.has_error(), "a caught async rejection should not be reported, got errors: {:?}", out.errors());
}
