#![cfg(feature = "compile")]

use flux::{FluxEngine, LogLevel};
use std::sync::{Arc, Mutex};

fn capture_log() -> (
  Arc<Mutex<Vec<(LogLevel, String)>>>,
  impl Fn(LogLevel, &str) + Send + Sync + 'static,
) {
  let log = Arc::new(Mutex::new(Vec::<(LogLevel, String)>::new()));
  let log2 = log.clone();
  let f = move |level: LogLevel, msg: &str| {
    log2.lock().unwrap().push((level, msg.to_string()));
  };
  (log, f)
}

fn log_output(log: &[(LogLevel, String)]) -> String {
  log
    .iter()
    .filter(|(l, _)| *l == LogLevel::Log)
    .map(|(_, m)| m.as_str())
    .collect::<Vec<_>>()
    .join("\n")
}

#[tokio::test]
async fn promise_resolve() {
  let (log, log_fn) = capture_log();
  let engine = FluxEngine::builder().logger(log_fn).build();
  engine
    .eval_source("Promise.resolve('resolved').then(v => console.log(v))")
    .await;

  let log = log.lock().unwrap();
  assert_eq!(log_output(&log), "resolved");
}

#[tokio::test]
async fn promise_then_chain() {
  let (log, log_fn) = capture_log();
  let engine = FluxEngine::builder().logger(log_fn).build();
  engine
    .eval_source(
      r#"
            Promise.resolve(1)
                .then(v => v + 1)
                .then(v => v * 3)
                .then(v => console.log(v))
            "#,
    )
    .await;

  let log = log.lock().unwrap();
  assert_eq!(log_output(&log), "6");
}

#[tokio::test]
async fn promise_catch() {
  let (log, log_fn) = capture_log();
  let engine = FluxEngine::builder().logger(log_fn).build();
  engine
    .eval_source(
      r#"
            Promise.reject(new Error('boom'))
                .catch(e => console.log(e.message))
            "#,
    )
    .await;

  let log = log.lock().unwrap();
  assert_eq!(log_output(&log), "boom");
}

#[tokio::test]
async fn promise_all() {
  let (log, log_fn) = capture_log();
  let engine = FluxEngine::builder().logger(log_fn).build();
  engine
    .eval_source(
      r#"
            Promise.all([
                Promise.resolve('a'),
                Promise.resolve('b'),
                Promise.resolve('c'),
            ]).then(v => console.log(v.join(',')))
            "#,
    )
    .await;

  let log = log.lock().unwrap();
  assert_eq!(log_output(&log), "a,b,c");
}

#[tokio::test]
async fn async_function() {
  let (log, log_fn) = capture_log();
  let engine = FluxEngine::builder().logger(log_fn).build();
  engine
    .eval_source(
      r#"
            (async () => {
                let a = await Promise.resolve('hello');
                let b = await Promise.resolve(' world');
                console.log(a + b);
            })()
            "#,
    )
    .await;

  let log = log.lock().unwrap();
  assert_eq!(log_output(&log), "hello world");
}
