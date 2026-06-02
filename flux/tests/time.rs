#![cfg(feature = "compile")]

mod common;

use common::{run_source, LogSink};
use flux::{Clock, FluxEngine};

#[tokio::test]
async fn clear_timeout_on_unknown_id_throws() {
  let out = run_source("clearTimeout(999)").await;
  assert!(out.has_error(), "expected error for unknown id");
}

#[tokio::test]
async fn clear_timeout_on_not_yet_fired_cancels() {
  let out = run_source(
    r#"
            let id = setTimeout(() => {}, 100000);
            clearTimeout(id);
            console.log('cancelled');
            "#,
  )
  .await;
  assert!(!out.has_error(), "unexpected error");
  assert_eq!(out.log(), "cancelled");
}

#[tokio::test]
async fn clear_timeout_on_unknown_id_caught() {
  let out = run_source(
    r#"
            try { clearTimeout(999); console.log('no error') } catch (e) { console.log('caught: ' + e.message) }
            "#,
  )
  .await;
  let output = out.log();
  assert!(output.starts_with("caught:"), "expected caught error, got: {output}");
}

#[tokio::test]
async fn set_timeout_returns_numeric_id() {
  let out = run_source(
    r#"
            let id = setTimeout(() => {}, 1);
            console.log(typeof id);
            clearTimeout(id);
            "#,
  )
  .await;
  assert_eq!(out.log(), "number");
}

#[tokio::test]
async fn set_interval_returns_numeric_id() {
  let out = run_source(
    r#"
            let id = setInterval(() => {}, 1);
            console.log(typeof id);
            clearInterval(id);
            "#,
  )
  .await;
  assert_eq!(out.log(), "number");
}

#[tokio::test]
async fn queue_microtask_runs_before_timers() {
  let out = run_source(
    r#"
            let order = [];
            setTimeout(() => order.push('timeout'), 0);
            queueMicrotask(() => order.push('microtask'));
            setTimeout(() => console.log(order.join(',')), 50);
            "#,
  )
  .await;
  assert_eq!(out.log(), "microtask,timeout");
}

#[tokio::test]
async fn queue_microtask_throw_is_reported() {
  let out = run_source(r#"queueMicrotask(() => { throw new Error("boom"); });"#).await;
  assert!(out.has_error(), "expected throwing microtask to be reported as uncaught");
}

#[tokio::test]
async fn set_timeout_fires() {
  let out = run_source("setTimeout(() => console.log('fired'), 10);").await;
  assert_eq!(out.log(), "fired");
}

#[tokio::test]
async fn set_timeout_chained() {
  let out = run_source(
    r#"
            setTimeout(() => {
                setTimeout(() => console.log("chained"), 10);
            }, 10);
            "#,
  )
  .await;
  assert_eq!(out.log(), "chained");
}

#[tokio::test]
async fn promise_with_timer() {
  let out = run_source(
    r#"
            let p = new Promise(resolve => setTimeout(() => resolve("ok"), 10));
            p.then(v => console.log(v));
            "#,
  )
  .await;
  assert_eq!(out.log(), "ok");
}

#[tokio::test]
async fn multiple_concurrent_timers() {
  let out = run_source(
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
  assert_eq!(out.log(), "a,b,c");
}

#[tokio::test]
async fn microtask_after_timer() {
  let out = run_source(
    r#"
            setTimeout(() => {
                Promise.resolve().then(() => console.log("microtask"));
            }, 10);
            "#,
  )
  .await;
  assert_eq!(out.log(), "microtask");
}

#[tokio::test]
async fn deep_promise_chain_after_timer() {
  let out = run_source(
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
  assert_eq!(out.log(), "a,b,c,d");
}

#[tokio::test]
async fn queue_microtask_after_timer() {
  let out = run_source(
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
  assert_eq!(out.log(), "nested microtask");
}

#[tokio::test]
async fn microtask_triggers_state_update() {
  let out = run_source(
    r#"
            let state = "initial";
            setTimeout(() => {
                Promise.resolve().then(() => { state = "updated"; });
                setTimeout(() => console.log(state), 50);
            }, 10);
            "#,
  )
  .await;
  assert_eq!(out.log(), "updated");
}

#[tokio::test]
async fn async_await_after_timer() {
  let out = run_source(
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
  assert_eq!(out.log(), "step1,step2,step3");
}

// ----- performance.now() / Clock -----

#[tokio::test]
async fn performance_now_returns_number() {
  let out = run_source("console.log(typeof performance.now())").await;
  assert_eq!(out.log(), "number");
}

#[tokio::test]
async fn performance_now_is_monotonic() {
  let out = run_source(
    r#"
            let a = performance.now();
            let b = performance.now();
            console.log(b >= a);
            "#,
  )
  .await;
  assert_eq!(out.log(), "true");
}

#[tokio::test]
async fn performance_now_advances_after_timeout() {
  let out = run_source(
    r#"
            let start = performance.now();
            setTimeout(() => console.log(performance.now() > start), 20);
            "#,
  )
  .await;
  assert_eq!(out.log(), "true");
}

#[tokio::test]
async fn injected_clock_drives_performance_now() {
  // An embedder can inject a Clock via the builder; performance.now() then
  // reports through it instead of the default monotonic origin.
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).userdata(Clock::new(|| 1234.5)).build();
  engine.eval_source("console.log(performance.now())").await;
  assert_eq!(sink.captured().log(), "1234.5");
}
