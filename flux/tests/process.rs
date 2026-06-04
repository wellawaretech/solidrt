#![cfg(all(unix, feature = "compile"))]

mod common;

use common::{Captured, LogSink};
use flux::{FluxEngine, LogLevel, ProcessArgs};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

// Signals are process-global, so the signal tests in this binary must not run
// concurrently. Each acquires this guard for its whole duration.
fn serial_guard() -> MutexGuard<'static, ()> {
  static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

// Run `code` on a background engine, wait until it logs "ready" (its listeners
// are registered and the OS watcher installed), then deliver `signals` to this
// process, each followed by its delay in ms. The engine is expected to exit on
// its own once its listeners are gone; if it does not within a few seconds that
// is a test failure rather than a hang.
fn run_with_signals(code: &str, signals: &[(i32, u64)]) -> Captured {
  let _guard = serial_guard();
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();

  let code = code.to_string();
  let (done_tx, done_rx) = mpsc::channel();
  let handle = std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
    rt.block_on(engine.eval_source(&code));
    let _ = done_tx.send(());
  });

  wait_for_ready(&sink);

  for (sig, delay_ms) in signals {
    // kill(self) delivers process-wide; tokio's handler picks it up regardless
    // of which thread runs the handler.
    unsafe {
      libc::kill(libc::getpid(), *sig);
    }
    std::thread::sleep(Duration::from_millis(*delay_ms));
  }

  done_rx.recv_timeout(Duration::from_secs(5)).expect("engine did not exit after signals");
  handle.join().expect("engine thread panicked");
  sink.captured()
}

fn wait_for_ready(sink: &LogSink) {
  let deadline = Instant::now() + Duration::from_secs(3);
  while Instant::now() < deadline {
    if sink.captured().lines_at(LogLevel::Log).iter().any(|l| *l == "ready") {
      return;
    }
    std::thread::sleep(Duration::from_millis(10));
  }
  panic!("engine never logged \"ready\" (flux:process import or registration failed?)");
}

#[test]
fn signal_fires_listener_with_name() {
  let out = run_with_signals(
    r#"
        import { once } from "flux:process"
        once("SIGUSR1", (sig) => console.log("got:" + sig))
        console.log("ready")
        "#,
    &[(libc::SIGUSR1, 200)],
  );
  let lines = out.lines_at(LogLevel::Log);
  // The callback fires with the signal name, and the once() teardown lets the
  // engine exit on its own (otherwise run_with_signals would have timed out).
  assert!(lines.contains(&"got:SIGUSR1"), "expected callback to fire with name, got {lines:?}");
}

#[test]
fn on_fires_until_unsubscribe() {
  let out = run_with_signals(
    r#"
        import { on } from "flux:process"
        let n = 0
        let unsub
        unsub = on("SIGUSR1", () => {
            n++
            console.log("sig:" + n)
            if (n >= 3) unsub()
        })
        console.log("ready")
        "#,
    &[(libc::SIGUSR1, 150), (libc::SIGUSR1, 150), (libc::SIGUSR1, 150)],
  );
  // Each of the three deliveries fires the listener once, in order; unsubscribe
  // on the third lets the engine exit.
  assert_eq!(out.lines_at(LogLevel::Log), vec!["ready", "sig:1", "sig:2", "sig:3"]);
}

#[test]
fn once_does_not_refire_after_teardown() {
  // A persistent listener on a second signal keeps the engine alive so we can
  // observe that a second SIGUSR1 does not re-fire the (already torn-down) once.
  let out = run_with_signals(
    r#"
        import { on, once } from "flux:process"
        let stop
        stop = on("SIGUSR2", () => stop())
        once("SIGUSR1", (sig) => console.log("once:" + sig))
        console.log("ready")
        "#,
    &[(libc::SIGUSR1, 200), (libc::SIGUSR1, 200), (libc::SIGUSR2, 200)],
  );
  let onces: Vec<&str> = out.lines_at(LogLevel::Log).into_iter().filter(|l| l.starts_with("once:")).collect();
  assert_eq!(onces, vec!["once:SIGUSR1"], "once must fire exactly once even across repeated signals");
}

#[test]
fn argv_reflects_process_args() {
  let sink = LogSink::new();
  let args = vec!["flux".to_string(), "script.js".to_string(), "hello".to_string(), "--name=foo".to_string()];
  let engine = FluxEngine::builder().logger(sink.logger()).userdata(ProcessArgs(args)).build();
  let code = r#"
        import { argv } from "flux:process"
        console.log(JSON.stringify(argv))
        "#;
  let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("build runtime");
  rt.block_on(engine.eval_source(code));
  assert_eq!(sink.captured().at(LogLevel::Log), r#"["flux","script.js","hello","--name=foo"]"#);
}

#[test]
fn argv_is_empty_when_host_sets_none() {
  let sink = LogSink::new();
  let engine = FluxEngine::builder().logger(sink.logger()).build();
  let code = r#"
        import { argv } from "flux:process"
        console.log(JSON.stringify(argv))
        "#;
  let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("build runtime");
  rt.block_on(engine.eval_source(code));
  assert_eq!(sink.captured().at(LogLevel::Log), "[]");
}