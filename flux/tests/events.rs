#![cfg(feature = "compile")]

mod common;

use common::{Captured, LogSink};
use flux::rquickjs::{Ctx, Function, Value};
use flux::{emit_event, register_listener, FluxEngine, LogLevel};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

// How long a test waits for the script to register its first listener. The
// engine starts on a thread and the entry evaluates after plugin init, so this
// covers a slow debug startup on a loaded runner without hiding a broken `on`.
const LISTENER_TIMEOUT: Duration = Duration::from_secs(10);
// How long a test waits for the engine to finish after the last emit. The
// scripts unsubscribe on their last event, so an engine still alive past this
// has lost an event; the test fails instead of hanging the job.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

// flux ships no JS event surface (consumers build their own on top of
// register_listener). The tests install a minimal `on(event, cb)` global the
// same way a real consumer would, then exercise the bus through it. Every `on`
// call signals `ready`, so the test can emit only once a listener exists: an
// exec closure queued before the engine is up runs before the entry module
// (the pre-eval drain in `FluxEngine::run`) and would reach no listener.
fn install_on<'js>(ctx: Ctx<'js>, ready: mpsc::Sender<()>) {
  let on_impl = move |event: String, callback: Function<'js>| -> flux::rquickjs::Result<Function<'js>> {
    let ctx = callback.ctx().clone();
    let unsub = register_listener(&ctx, event, callback, false);
    ready.send(()).expect("test dropped the ready receiver");
    unsub
  };
  let on = Function::new(ctx.clone(), on_impl).expect("create on");
  ctx.globals().set("on", on).expect("set on");
}

/// Run `code` on a background engine thread, then emit `events` on `channel`
/// from the main thread through the engine's exec handle, each after its given
/// delay in milliseconds. The first emit waits for the script to register a
/// listener. Returns the captured log once the engine finishes.
///
/// Each event's data is a JSON string that is parsed into a real JS value
/// before emitting, mirroring how the host emits structured event objects
/// (an `Object`, not a bare string).
fn run_with_events(code: &str, channel: &str, events: Vec<(&str, u64)>) -> Captured {
  let sink = LogSink::new();
  let (ready_tx, ready_rx) = mpsc::channel();
  let engine = FluxEngine::builder().logger(sink.logger()).plugin(move |ctx| install_on(ctx, ready_tx)).build();
  let handle = engine.exec_handle();

  let code = code.to_string();
  let channel = channel.to_string();
  let rt = Arc::new(tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime"));
  let rt2 = rt.clone();
  let (done_tx, done_rx) = mpsc::channel();
  let engine_thread = std::thread::spawn(move || {
    rt2.block_on(engine.eval_source(&code));
    done_tx.send(()).expect("test dropped the done receiver");
  });

  ready_rx.recv_timeout(LISTENER_TIMEOUT).expect("script registered no listener in time");

  for (data, delay_ms) in events {
    if delay_ms > 0 {
      std::thread::sleep(Duration::from_millis(delay_ms));
    }
    let event = channel.clone();
    let payload = data.to_string();
    handle.exec(move |ctx| {
      let value = ctx.json_parse(payload).unwrap_or_else(|_| Value::new_undefined(ctx.clone()));
      emit_event(&ctx, &event, value);
    });
  }

  done_rx.recv_timeout(ENGINE_TIMEOUT).expect("engine still running after the last emit: an event was lost");
  engine_thread.join().expect("engine thread panicked");
  sink.captured()
}

#[test]
fn emit_triggers_listener() {
  let out = run_with_events(
    r#"
        let unsub = on("test", (data) => {
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

        let unsub = on("render", () => {
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
            unsub = on("render", () => {
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
