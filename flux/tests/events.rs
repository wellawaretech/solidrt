#![cfg(feature = "compile")]

mod common;

use common::{Captured, LogSink};
use flux::rquickjs::Value;
use flux::{emit_event, FluxEngine, LogLevel};
use std::sync::Arc;

/// Run `code` on a background engine thread, then emit `events` on `channel`
/// from the main thread through the engine's exec handle, each after its given
/// delay in milliseconds. Returns the captured log once the engine finishes.
///
/// Each event's data is a JSON string that is parsed into a real JS value
/// before emitting, mirroring how the host emits structured event objects
/// (an `Object`, not a bare string).
fn run_with_events(code: &str, channel: &str, events: Vec<(&str, u64)>) -> Captured {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let handle = engine.exec_handle();

  let code = code.to_string();
  let channel = channel.to_string();
  let rt = Arc::new(tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime"));
  let rt2 = rt.clone();
  let engine_thread = std::thread::spawn(move || {
    rt2.block_on(engine.eval_source(&code));
  });

  std::thread::sleep(std::time::Duration::from_millis(100));

  for (data, delay_ms) in events {
    if delay_ms > 0 {
      std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
    let event = channel.clone();
    let payload = data.to_string();
    handle.exec(move |ctx| {
      let value = ctx.json_parse(payload).unwrap_or_else(|_| Value::new_undefined(ctx.clone()));
      emit_event(&ctx, &event, value);
    });
  }

  engine_thread.join().expect("engine thread panicked");
  sink.captured()
}

#[test]
fn emit_triggers_listener() {
  let out = run_with_events(
    r#"
        let unsub = Flux.on("test", (data) => {
            console.log("received:" + data.value);
            unsub();
        });
        "#,
    "test",
    vec![(r#"{"value":"hello"}"#, 0)],
  );
  assert_eq!(out.log(), "received:hello");
}

#[test]
fn event_delivery_with_set_interval() {
  let out = run_with_events(
    r#"
        let count = 0;
        let intervalId = setInterval(() => {}, 100);

        let unsub = Flux.on("render", () => {
            count++;
            console.log("render:" + count);
            if (count >= 3) {
                unsub();
                clearInterval(intervalId);
            }
        });
        "#,
    "render",
    vec![("{}", 50), ("{}", 50), ("{}", 50)],
  );
  // Each of the three emits must fire the listener exactly once, in order, and
  // unsub must stop it at 3 (no render:4).
  assert_eq!(out.lines_at(LogLevel::Log), vec!["render:1", "render:2", "render:3"]);
}

#[test]
fn microtask_registered_listener_with_set_interval() {
  let out = run_with_events(
    r#"
        let count = 0;
        let intervalId = setInterval(() => {}, 100);
        let unsub;

        // Register the event listener inside a microtask, like Solid.js onSettled does
        queueMicrotask(() => {
            unsub = Flux.on("render", () => {
                count++;
                console.log("render:" + count);
                if (count >= 3) {
                    unsub();
                    clearInterval(intervalId);
                }
            });
        });
        "#,
    "render",
    vec![("{}", 50), ("{}", 50), ("{}", 50)],
  );
  assert_eq!(out.lines_at(LogLevel::Log), vec!["render:1", "render:2", "render:3"]);
}