#![cfg(feature = "compile")]

mod common;

use common::{run_source, LogSink};
use flux::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use flux::{compile_source, on_shutdown, CtxLogger, FluxEngine, LogLevel};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

// Engine-level public API: custom plugins, injected userdata, shutdown hooks,
// precompiled bytecode, top-level error reporting, the stack-size limit, the
// interrupt flag, and the memory limit.
// (ExecHandle is exercised in events.rs.)

#[derive(Clone, JsLifetime)]
struct Identity(#[qjs(skip_trace)] String);

fn identity_plugin(ctx: Ctx<'_>) {
  ctx.logger().log("plugin init");
  ctx.store_userdata(Identity("flux".into())).expect("store userdata");
  let whoami = Function::new(
    ctx.clone(),
    MutFn::from(|ctx: Ctx<'_>| -> String { ctx.userdata::<Identity>().expect("identity userdata").0.clone() }),
  )
  .expect("create whoami function");
  ctx.globals().set("whoami", whoami).expect("set whoami");
}

#[tokio::test]
async fn custom_plugin_stores_userdata_and_global() {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).plugin(identity_plugin).build();
  engine.eval_source(r#"console.log("hello " + whoami())"#).await;
  // The plugin ran (logged "plugin init") and exposed a global that read the
  // userdata it stored.
  assert_eq!(sink.captured().lines_at(LogLevel::Log), vec!["plugin init", "hello flux"]);
}

fn shutdown_plugin(ctx: Ctx<'_>) {
  on_shutdown(&ctx, |logger| logger.log("shutdown ran"));
}

#[tokio::test]
async fn shutdown_hook_runs_after_event_loop() {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).plugin(shutdown_plugin).build();
  engine.eval_source(r#"console.log("during")"#).await;
  // The hook runs after the loop ends, with a working logger, so it lands last.
  assert_eq!(sink.captured().lines_at(LogLevel::Log), vec!["during", "shutdown ran"]);
}

#[tokio::test]
async fn eval_precompiled_bytecode() {
  let bytecode = compile_source(r#"console.log("from bytecode")"#, "main");
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  engine.eval(bytecode).await;
  assert_eq!(sink.captured().log(), "from bytecode");
}

#[tokio::test]
async fn top_level_throw_is_reported() {
  let out = run_source(r#"throw new Error("top level boom")"#).await;
  assert!(out.has_error(), "expected a top-level throw to be reported as an error");
}

#[tokio::test]
async fn stack_size_limit_catches_overflow() {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).stack_size(256 * 1024).build();
  engine
    .eval_source(
      r#"
            console.log("before");
            function r(n) { return r(n + 1); }
            try { r(0); } catch (e) { console.log("overflow caught"); }
            "#,
    )
    .await;
  // Normal code runs under the limit; unbounded recursion overflows and throws
  // a catchable error rather than crashing the process.
  assert_eq!(sink.captured().log(), "before\noverflow caught");
}

#[tokio::test]
async fn interrupt_flag_set_at_start_spares_startup_and_stops_entry() {
  // A flag already set when the engine starts (an isolate killed while it
  // starts): the engine's own startup still completes, and the entry is
  // interrupted at its first check, so a busy loop ends and is reported.
  let sink = LogSink::new();
  let flag = Arc::new(AtomicBool::new(true));
  let engine = FluxEngine::builder().logger(sink.logger()).interrupt_flag(flag).build();
  engine.eval_source("for (;;) {}").await;
  let captured = sink.captured();
  let errors = captured.lines_at(LogLevel::Error);
  assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
  assert!(errors[0].starts_with("Error: interrupted"), "expected the interruption, got {errors:?}");
}

// A heap limit far below what the engine's own startup allocates, so any
// allocation after startup fails at once.
const TINY_MEMORY_LIMIT: usize = 64 * 1024;

#[tokio::test]
async fn memory_limit_below_baseline_spares_startup_and_fails_entry() {
  // The limit takes effect once startup is done: the engine starts, and
  // loading the entry fails and is reported. So far below the baseline
  // QuickJS has no room to build its "out of memory" error and throws null
  // instead, so the report names the module load, not the cause.
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).memory_limit(TINY_MEMORY_LIMIT).build();
  engine.eval_source("let a = [1, 2, 3]; console.log(a.length)").await;
  let captured = sink.captured();
  let errors = captured.lines_at(LogLevel::Error);
  assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
  assert!(errors[0].starts_with("module error: "), "expected the failed module load, got {errors:?}");
  assert!(captured.lines_at(LogLevel::Log).is_empty(), "the entry ran: {:?}", captured.lines_at(LogLevel::Log));
}
