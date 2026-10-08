// The AbortSignal-on-calls contract of flux:isolate: a signal among a call's
// arguments is consumed as the call's signal (never sent), abort stops the
// waiting without touching the export, a pre-aborted signal sends and spawns
// nothing, and a signal on a stream call is inert. The wider call/stream
// protocol is exercised by flux/examples/isolate.js.
#![cfg(feature = "compile")]

mod common;

use common::{Captured, LogSink};
use flux::{FluxEngine, ModuleCode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const WORKER: &str = r#"
console.log("worker loaded")
let finished = []
export function echoCount(...args) { return args.length }
export async function slow(ms, tag) {
  await new Promise(r => setTimeout(r, ms))
  finished.push(tag)
  return tag
}
export function finishedTags() { return finished }
export async function* counting() {
  try {
    for (let i = 1; ; i++) yield i
  } finally {
    finished.push("gen-ended")
  }
}
"#;

/// Run `code` on an engine that resolves isolate "worker" to WORKER, and
/// return the captured log plus how often the resolver ran (a resolver that
/// never ran means no child was spawned).
async fn run_with_worker(code: &str) -> (Captured, u64) {
  let sink = LogSink::new();
  let spawns = Arc::new(AtomicU64::new(0));
  let counter = spawns.clone();
  let engine = FluxEngine::builder()
    .logger(sink.logger())
    .isolate_resolver(move |id| {
      counter.fetch_add(1, Ordering::Relaxed);
      match id {
        "worker" => Ok(ModuleCode::Source(WORKER.to_string())),
        _ => Err(format!("unknown isolate '{id}'")),
      }
    })
    .build();
  engine.eval_source(code).await;
  (sink.captured(), spawns.load(Ordering::Relaxed))
}

#[tokio::test]
async fn signal_is_consumed_not_sent() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    console.log("count:", await w.echoCount(1, new AbortController().signal, 2))
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("count: 2"), "log: {}", out.log());
}

#[tokio::test]
async fn abort_rejects_with_the_reason_and_the_child_survives() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    let c = new AbortController()
    let reason = new Error("moved on")
    setTimeout(() => c.abort(reason), 10)
    try {
      await w.slow(5000, "abandoned", c.signal)
      console.log("unexpected resolve")
    } catch (e) {
      console.log("aborted:", e === reason)
    }
    console.log("alive:", await w.echoCount() === 0)
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("aborted: true"), "log: {}", out.log());
  assert!(out.log().contains("alive: true"), "log: {}", out.log());
  assert!(!out.log().contains("unexpected resolve"), "log: {}", out.log());
}

#[tokio::test]
async fn the_export_runs_on_and_its_reply_is_dropped() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    let c = new AbortController()
    setTimeout(() => c.abort(), 10)
    try {
      await w.slow(60, "ran-on", c.signal)
    } catch (e) {
      console.log("default reason:", e instanceof Error && e.name === "AbortError")
    }
    await new Promise(r => setTimeout(r, 200))
    console.log("ran on:", (await w.finishedTags()).includes("ran-on"))
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("default reason: true"), "log: {}", out.log());
  assert!(out.log().contains("ran on: true"), "log: {}", out.log());
}

#[tokio::test]
async fn more_than_one_signal_throws() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    try {
      await w.echoCount(new AbortController().signal, new AbortController().signal)
    } catch (e) {
      console.log("two signals:", e instanceof TypeError, e.message)
    }
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("two signals: true a call takes at most one AbortSignal"), "log: {}", out.log());
}

#[tokio::test]
async fn a_pre_aborted_signal_rejects_without_spawning() {
  let (out, spawns) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    try {
      await w.slow(1000, "never", AbortSignal.abort("early"))
    } catch (e) {
      console.log("pre-aborted:", e === "early")
    }
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("pre-aborted: true"), "log: {}", out.log());
  assert_eq!(spawns, 0, "the resolver ran: a child was spawned for a dead call");
  assert!(!out.log().contains("worker loaded"), "log: {}", out.log());
}

#[tokio::test]
async fn abort_ends_a_stream_like_return() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    let c = new AbortController()
    let items = []
    for await (let x of w.counting(c.signal)) {
      items.push(x)
      if (items.length === 2) c.abort()
    }
    console.log("items:", JSON.stringify(items), "finally ran:", (await w.finishedTags()).includes("gen-ended"))
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("items: [1,2] finally ran: true"), "log: {}", out.log());
}

#[tokio::test]
async fn abort_before_the_answer_ends_a_queued_reader() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("worker")
    let c = new AbortController()
    let p = w.counting(c.signal)
    let step = p.next()
    c.abort()
    let r = await step
    console.log("early done:", r.done === true)
    w.terminate()
    "#,
  )
  .await;
  assert!(out.log().contains("early done: true"), "log: {}", out.log());
  // The call's promise rejects with the reason but is marked observed: an
  // iterating caller never awaits it, so no unhandled rejection may surface.
  assert!(!out.has_error(), "errors: {}", out.errors());
}

// A context seeded by its host (flux:test, for a test) seeds the isolates it
// spawns, each with a seed of its own derived from the parent's: what a
// child draws from Math.random is the same on every run, differs from child
// to child, and takes nothing out of the parent's sequence.
const DRAWER: &str = r#"
export function draw() { return Math.random() }
"#;

/// Run `code` on an engine whose Math.random is seeded with `seed` and that
/// resolves isolate "drawer" to DRAWER; the captured log.
async fn run_seeded(seed: u64, code: &str) -> Captured {
  let sink = LogSink::new();
  let engine = FluxEngine::builder()
    .logger(sink.logger())
    .plugin(move |ctx| flux::seed_random(&ctx, seed).expect("seed Math.random"))
    .isolate_resolver(|id| match id {
      "drawer" => Ok(ModuleCode::Source(DRAWER.to_string())),
      _ => Err(format!("unknown isolate '{id}'")),
    })
    .build();
  engine.eval_source(code).await;
  sink.captured()
}

#[tokio::test]
async fn a_seeded_context_seeds_its_isolates() {
  const SPAWN_TWO: &str = r#"
    import { isolate } from "flux:isolate"
    let a = isolate("drawer")
    let b = isolate("drawer")
    let drawn = [await a.draw(), await a.draw(), await b.draw(), Math.random()]
    a.terminate()
    b.terminate()
    console.log(drawn.join())
    "#;
  let first = run_seeded(1, SPAWN_TWO).await.log();
  let again = run_seeded(1, SPAWN_TWO).await.log();
  let other = run_seeded(2, SPAWN_TWO).await.log();
  let values: Vec<&str> = first.split(',').collect();
  assert_eq!(values.len(), 4, "log: {first}");
  assert_eq!(first, again, "the same seed drew differently on a second run");
  assert_ne!(first, other, "another seed drew the same");
  assert_ne!(values[0], values[2], "two isolates drew the same sequence");
  // The parent's first value is the first of seed 1's own sequence
  // (src/tests/random.rs pins it): spawning took nothing from it.
  assert_eq!(values[3], "0.29404672187536485");
}

// A child that cannot start: the failure is the call's rejection (and a
// stream's first error, and what `exited` settles with), never a throw from
// the call expression, so a caller that maps rejections sees it.

#[tokio::test]
async fn an_unknown_module_rejects_the_call() {
  let (out, spawns) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("nope")
    let call = w.anything()
    console.log("promise:", call instanceof Promise)
    await call.then(() => console.log("unexpected resolve"), e => console.log("rejected:", e instanceof Error, e.message))
    try {
      await w.again()
    } catch (e) {
      console.log("again:", e.message)
    }
    "#,
  )
  .await;
  assert!(out.log().contains("promise: true"), "log: {}", out.log());
  assert!(out.log().contains("rejected: true unknown isolate 'nope'"), "log: {}", out.log());
  assert!(out.log().contains("again: unknown isolate 'nope'"), "log: {}", out.log());
  assert_eq!(spawns, 2, "a failed start is remembered: the resolver was not asked again");
  assert!(out.errors().is_empty(), "errors: {}", out.errors());
}

#[tokio::test]
async fn an_unknown_module_ends_a_stream_with_the_error() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("nope")
    try {
      for await (let item of w.counting()) console.log("unexpected item", item)
    } catch (e) {
      console.log("stream:", e instanceof Error, e.message)
    }
    "#,
  )
  .await;
  assert!(out.log().contains("stream: true unknown isolate 'nope'"), "log: {}", out.log());
  assert!(!out.log().contains("unexpected item"), "log: {}", out.log());
  assert!(out.errors().is_empty(), "the call's own rejection went unobserved: {}", out.errors());
}

#[tokio::test]
async fn exited_settles_with_the_start_failure() {
  let (out, _) = run_with_worker(
    r#"
    import { isolate } from "flux:isolate"
    let w = isolate("nope")
    console.log("exited:", await w.exited)
    "#,
  )
  .await;
  assert!(out.log().contains("exited: unknown isolate 'nope'"), "log: {}", out.log());
  assert!(out.errors().is_empty(), "errors: {}", out.errors());
}

#[tokio::test]
async fn a_runtime_without_a_resolver_rejects() {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  engine
    .eval_source(
      r#"
      import { isolate } from "flux:isolate"
      try {
        await isolate("worker").echoCount()
      } catch (e) {
        console.log("no resolver:", e instanceof Error, e.message)
      }
      "#,
    )
    .await;
  let out = sink.captured();
  assert!(out.log().contains("no resolver: true this runtime cannot spawn isolates"), "log: {}", out.log());
}
