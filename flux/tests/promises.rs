#![cfg(feature = "compile")]

mod common;

use common::run_source;

#[tokio::test]
async fn promise_resolve() {
  let out = run_source("Promise.resolve('resolved').then(v => console.log(v))").await;
  assert_eq!(out.log(), "resolved");
}

#[tokio::test]
async fn promise_then_chain() {
  let out = run_source(
    r#"
            Promise.resolve(1)
                .then(v => v + 1)
                .then(v => v * 3)
                .then(v => console.log(v))
            "#,
  )
  .await;
  assert_eq!(out.log(), "6");
}

#[tokio::test]
async fn promise_catch() {
  let out = run_source(
    r#"
            Promise.reject(new Error('boom'))
                .catch(e => console.log(e.message))
            "#,
  )
  .await;
  assert_eq!(out.log(), "boom");
}

#[tokio::test]
async fn promise_all() {
  let out = run_source(
    r#"
            Promise.all([
                Promise.resolve('a'),
                Promise.resolve('b'),
                Promise.resolve('c'),
            ]).then(v => console.log(v.join(',')))
            "#,
  )
  .await;
  assert_eq!(out.log(), "a,b,c");
}

#[tokio::test]
async fn async_function() {
  let out = run_source(
    r#"
            (async () => {
                let a = await Promise.resolve('hello');
                let b = await Promise.resolve(' world');
                console.log(a + b);
            })()
            "#,
  )
  .await;
  assert_eq!(out.log(), "hello world");
}

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
  assert!(
    !out.has_error(),
    "a synchronously-handled rejection should not be reported, got errors: {:?}",
    out.errors()
  );
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
  assert!(
    !out.has_error(),
    "a caught async rejection should not be reported, got errors: {:?}",
    out.errors()
  );
}
