mod common;

use common::LogSink;
use flux::availability::{set_override, snapshot};
use flux::rquickjs::{Ctx, Function, Value};
use flux::FluxEngine;
use ore::availability::{Availability, Facility};

// The availability as an embedder shapes it: a plugin's override is what
// `Flux.availability` reads and what the sticky `availability` event
// carries, the event emitted once after the plugin chain with every
// facility present.

/// An engine whose plugin reports link unavailable, with `stickyAvailability()`
/// returning the cached payload of the sticky `availability` event (bare flux
/// has no `on`).
fn engine(sink: &LogSink) -> FluxEngine {
  FluxEngine::builder()
    .logger(sink.logger())
    .plugin(|ctx| set_override(&ctx, Facility::Link, Availability::Unavailable))
    .plugin(|ctx: Ctx<'_>| {
      let sticky = Function::new(ctx.clone(), sticky_availability).expect("create stickyAvailability");
      ctx.globals().set("stickyAvailability", sticky).expect("set stickyAvailability");
    })
    .build()
}

/// The cached payload of the sticky `availability` event.
fn sticky_availability<'js>(ctx: Ctx<'js>) -> Option<Value<'js>> {
  flux::sticky_cached(&ctx, "availability")
}

#[tokio::test]
async fn an_override_reaches_the_read_and_the_sticky_payload() {
  let sink = LogSink::new();
  engine(&sink)
    .eval_source(
      r#"
      console.log(Flux.availability("link"))
      const payload = stickyAvailability()
      console.log(payload.link)
      console.log(Object.keys(payload).join(","))
      "#,
    )
    .await;
  let out = sink.captured();
  let lines = out.lines_at(flux::LogLevel::Log);
  assert_eq!(lines[..2], ["unavailable", "unavailable"], "log: {lines:?}");
  let every: Vec<&str> = Facility::ALL.iter().map(|facility| facility.name()).collect();
  assert_eq!(lines[2], every.join(","), "log: {lines:?}");
}

#[tokio::test]
async fn the_snapshot_applies_the_override() {
  let sink = LogSink::new();
  let snap = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let read = snap.clone();
  FluxEngine::builder()
    .logger(sink.logger())
    .plugin(|ctx| set_override(&ctx, Facility::Camera, Availability::Available))
    .plugin(move |ctx| *read.lock().expect("snapshot lock poisoned") = snapshot(&ctx))
    .build()
    .eval_source("")
    .await;
  let snap = snap.lock().expect("snapshot lock poisoned").clone();
  assert_eq!(snap.len(), Facility::ALL.len());
  assert!(snap.contains(&("camera", "available")), "snapshot: {snap:?}");
}
