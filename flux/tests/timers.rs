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
    log.iter()
        .filter(|(l, _)| *l == LogLevel::Log)
        .map(|(_, m)| m.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_error(log: &[(LogLevel, String)]) -> bool {
    log.iter().any(|(l, _)| *l == LogLevel::Error)
}

#[tokio::test]
async fn clear_timeout_on_unknown_id_throws() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine.eval_source("clearTimeout(999)").await;

    let log = log.lock().unwrap();
    assert!(has_error(&log), "expected error for unknown id");
}

#[tokio::test]
async fn clear_timeout_on_not_yet_fired_cancels() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let id = setTimeout(() => {}, 100000);
            clearTimeout(id);
            console.log('cancelled');
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert!(!has_error(&log), "unexpected error");
    assert_eq!(log_output(&log), "cancelled");
}

#[tokio::test]
async fn clear_timeout_on_unknown_id_caught() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            try { clearTimeout(999); console.log('no error') } catch (e) { console.log('caught: ' + e.message) }
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    let output = log_output(&log);
    assert!(
        output.starts_with("caught:"),
        "expected caught error, got: {output}"
    );
}

#[tokio::test]
async fn set_timeout_returns_numeric_id() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let id = setTimeout(() => {}, 1);
            console.log(typeof id);
            clearTimeout(id);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "number");
}

#[tokio::test]
async fn set_interval_returns_numeric_id() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let id = setInterval(() => {}, 1);
            console.log(typeof id);
            clearInterval(id);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "number");
}

#[tokio::test]
async fn queue_microtask_runs_before_timers() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let order = [];
            setTimeout(() => order.push('timeout'), 0);
            queueMicrotask(() => order.push('microtask'));
            setTimeout(() => console.log(order.join(',')), 50);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "microtask,timeout");
}

#[tokio::test]
async fn set_timeout_fires() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source("setTimeout(() => console.log('fired'), 10);")
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "fired");
}

#[tokio::test]
async fn set_timeout_chained() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            setTimeout(() => {
                setTimeout(() => console.log("chained"), 10);
            }, 10);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "chained");
}

#[tokio::test]
async fn promise_with_timer() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let p = new Promise(resolve => setTimeout(() => resolve("ok"), 10));
            p.then(v => console.log(v));
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "ok");
}

#[tokio::test]
async fn multiple_concurrent_timers() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let results = [];
            setTimeout(() => results.push("a"), 10);
            setTimeout(() => results.push("b"), 20);
            setTimeout(() => {
                results.push("c");
                console.log(results.join(","));
            }, 30);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "a,b,c");
}

#[tokio::test]
async fn microtask_after_timer() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            setTimeout(() => {
                Promise.resolve().then(() => console.log("microtask"));
            }, 10);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "microtask");
}

#[tokio::test]
async fn deep_promise_chain_after_timer() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            setTimeout(() => {
                Promise.resolve("a")
                    .then(v => v + ",b")
                    .then(v => v + ",c")
                    .then(v => v + ",d")
                    .then(v => console.log(v));
            }, 10);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "a,b,c,d");
}

#[tokio::test]
async fn queue_microtask_after_timer() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            setTimeout(() => {
                queueMicrotask(() => {
                    queueMicrotask(() => {
                        console.log("nested microtask");
                    });
                });
            }, 10);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "nested microtask");
}

#[tokio::test]
async fn microtask_triggers_state_update() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            let state = "initial";
            setTimeout(() => {
                Promise.resolve().then(() => { state = "updated"; });
                setTimeout(() => console.log(state), 50);
            }, 10);
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "updated");
}

#[tokio::test]
async fn async_await_after_timer() {
    let (log, log_fn) = capture_log();
    let engine = FluxEngine::builder().logger(log_fn).build();
    engine
        .eval_source(
            r#"
            async function work() {
                let result = await new Promise(resolve =>
                    setTimeout(() => resolve("step1"), 10)
                );
                result = await Promise.resolve(result + ",step2");
                result = await Promise.resolve(result + ",step3");
                console.log(result);
            }
            work();
            "#,
        )
        .await;

    let log = log.lock().unwrap();
    assert_eq!(log_output(&log), "step1,step2,step3");
}
