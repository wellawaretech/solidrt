#![cfg(feature = "compile")]

use qjsrt::{JsEngine, LogLevel};
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

fn messages_at(log: &[(LogLevel, String)], level: LogLevel) -> Vec<&str> {
    log.iter()
        .filter(|(l, _)| *l == level)
        .map(|(_, m)| m.as_str())
        .collect()
}

#[tokio::test]
async fn console_log_prints_to_stdout() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine.eval_source("console.log('hello')").await;

    let log = log.lock().unwrap();
    let info = messages_at(&log, LogLevel::Log);
    assert_eq!(info, vec!["hello"]);
    assert!(messages_at(&log, LogLevel::Error).is_empty());
}

#[tokio::test]
async fn console_warn_prints_to_stderr() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine.eval_source("console.warn('warning')").await;

    let log = log.lock().unwrap();
    assert!(messages_at(&log, LogLevel::Log).is_empty());
    assert_eq!(messages_at(&log, LogLevel::Warn), vec!["warning"]);
}

#[tokio::test]
async fn console_error_prints_to_stderr() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine.eval_source("console.error('oops')").await;

    let log = log.lock().unwrap();
    assert!(messages_at(&log, LogLevel::Log).is_empty());
    assert_eq!(messages_at(&log, LogLevel::Error), vec!["oops"]);
}

#[tokio::test]
async fn console_log_multiple_args() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine.eval_source("console.log('a', 'b', 'c')").await;

    let log = log.lock().unwrap();
    assert_eq!(messages_at(&log, LogLevel::Log), vec!["a b c"]);
}

#[tokio::test]
async fn console_log_mixed_types() {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder().logger(log_fn).build();
    engine
        .eval_source("console.log('count:', 42, true, null)")
        .await;

    let log = log.lock().unwrap();
    assert_eq!(messages_at(&log, LogLevel::Log), vec!["count: 42 true null"]);
}
