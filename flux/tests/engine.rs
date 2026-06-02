#![cfg(feature = "compile")]

mod common;

use common::{run_source, LogSink};
use flux::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use flux::{compile_source, on_shutdown, CtxLogger, FluxEngine, LogLevel};

// Engine-level public API: custom plugins, injected userdata, shutdown hooks,
// precompiled bytecode, top-level error reporting, and the stack-size limit.
// (ExecHandle is exercised in events.rs; an injected Clock in time.rs.)

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
