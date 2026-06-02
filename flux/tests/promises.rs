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
