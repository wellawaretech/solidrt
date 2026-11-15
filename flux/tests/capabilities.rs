mod common;

use std::sync::{Arc, Mutex};

use common::LogSink;
use flux::rquickjs::{Ctx, Function};
use flux::{add_capability, exclude_capability, listed_capabilities, FluxEngine};

// The capability list as an embedder shapes it: plugins list and exclude
// names, `Flux.capabilities` is defined from that once the plugin chain is
// done, and a change after that reaches neither the registry nor JS.

/// A name an embedder's own module lists, beside the built-ins.
const MODULE_CAPABILITY: &str = "runtime:acme/thing";
/// A base capability every flux build lists, which the test excludes.
const EXCLUDED: &str = "sqlite";

/// An engine whose plugins exclude `EXCLUDED`, list `MODULE_CAPABILITY`,
/// then read the listed names back into `listed`; `addLate(name)` lists a
/// name from evaluated code, after the freeze.
fn engine(sink: &LogSink, listed: Arc<Mutex<Vec<String>>>) -> FluxEngine {
  FluxEngine::builder()
    .logger(sink.logger())
    .plugin(|ctx| exclude_capability(&ctx, EXCLUDED))
    .plugin(|ctx| add_capability(&ctx, MODULE_CAPABILITY))
    .plugin(move |ctx| *listed.lock().expect("listed lock poisoned") = listed_capabilities(&ctx))
    .plugin(|ctx: Ctx<'_>| {
      let add_late =
        Function::new(ctx.clone(), |ctx: Ctx<'_>, name: String| add_capability(&ctx, &name)).expect("create addLate");
      ctx.globals().set("addLate", add_late).expect("set addLate");
    })
    .build()
}

#[tokio::test]
async fn plugins_list_and_exclude_names() {
  let sink = LogSink::new();
  let listed = Arc::new(Mutex::new(Vec::new()));
  engine(&sink, listed.clone())
    .eval_source(
      r#"
      console.log(Flux.capabilities.includes("sqlite"))
      console.log(Flux.capabilities.includes("fs"))
      console.log(Flux.capabilities.includes("runtime:acme/thing"))
      console.log(Flux.capabilities.join(","))
      "#,
    )
    .await;
  let out = sink.captured();
  let lines = out.lines_at(flux::LogLevel::Log);
  assert_eq!(lines[..3], ["false", "true", "true"], "log: {lines:?}");
  // What a plugin reads after the chain's changes is what JS sees.
  let listed = listed.lock().expect("listed lock poisoned").clone();
  assert!(!listed.iter().any(|name| name == EXCLUDED), "listed: {listed:?}");
  assert!(listed.iter().any(|name| name == MODULE_CAPABILITY), "listed: {listed:?}");
  assert_eq!(lines[3], listed.join(","));
}

#[tokio::test]
async fn a_name_added_after_the_freeze_changes_nothing() {
  let sink = LogSink::new();
  let engine = engine(&sink, Arc::new(Mutex::new(Vec::new())));
  engine
    .eval_source(
      r#"
      const before = JSON.stringify(Flux.capabilities)
      addLate("late")
      console.log(Flux.capabilities.includes("late"))
      console.log(JSON.stringify(Flux.capabilities) === before)
      "#,
    )
    .await;
  assert_eq!(sink.captured().log(), "false\ntrue");
}

#[tokio::test]
async fn flux_capabilities_is_not_writable() {
  let sink = LogSink::new();
  let engine = engine(&sink, Arc::new(Mutex::new(Vec::new())));
  engine
    .eval_source(
      r#"
      try {
        Flux.capabilities = []
        console.log("assigned")
      } catch (e) {
        console.log(e instanceof TypeError)
      }
      console.log(Flux.capabilities.includes("fs"))
      "#,
    )
    .await;
  assert_eq!(sink.captured().log(), "true\ntrue");
}
