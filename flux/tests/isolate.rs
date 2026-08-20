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
