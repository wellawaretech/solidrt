use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, BigInt, Ctx, Function, JsLifetime, Object};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::events::{emit_event, has_listeners, register_listener};
use crate::logger::CtxLogger;
use forge::process::{arch, hrtime_nanos, platform, rss, SignalStream};

// flux:process - process-level events. The first such surface flux owns on top
// of its own event bus (register_listener + emit_event), separate from the UI
// event surface a host like lattice provides. Currently just OS signals:
//
//   import { on } from "flux:process"
//   on("SIGINT", (sig) => { ... })
//
// A signal listener registered through on/once holds the engine loop alive (via
// the bus keep-alive) so the process waits for the signal; the callback
// receives the signal name. The OS watcher behind a signal is a long-lived
// spawned task, so it is torn down once the signal's last listener is gone
// (a fired once(), or the final unsubscribe followed by a delivery) - otherwise
// it would keep the engine from ever going idle. The OS-specific signal source
// lives in `forge::process::SignalStream`; this layer owns the spawn + bus.

// flux:process also exposes the process argument vector:
//
//   import { argv } from "flux:process"
//
// The host sets it with FluxEngine::builder().userdata(ProcessArgs(...)); when
// unset, argv is an empty array. The contract is app arguments only: no
// executable path, no script slot - argv[0] is the first argument passed to
// the app. Deliberately simpler than Node/Bun's two leading entries, which
// would force filler values in hosts without a script path (packed binaries,
// lattice apps).
#[derive(Clone, JsLifetime, Default)]
pub struct ProcessArgs(#[qjs(skip_trace)] pub Vec<String>);

// flux:process also exposes the host OS and CPU architecture (platform/arch) and
// current-process memory usage (Node/Bun parity):
//
//   import { platform, arch, memoryUsage } from "flux:process"
//   memoryUsage() // { rss }  - resident set size in bytes
//
// Node returns { rss, heapTotal, heapUsed, external, arrayBuffers }; we expose
// rss for now (the headline figure). A companion cpuUsage() is deferred until we
// settle on a portable user/system CPU-time split (getrusage / GetProcessTimes).
//
// And the high-resolution wall clock, Node's shape (the legacy [sec, ns] tuple
// form is not offered):
//
//   import { hrtime } from "flux:process"
//   let t0 = hrtime.bigint()   // monotonic nanoseconds, bigint
//
// This is REAL elapsed time. In a GUI host performance.now() is the paced app
// timeline (frame-stepped, freezable), which reads as 0 ms across any
// synchronous work; hrtime is what to time such work with.
fn memory_usage(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx)?;
  obj.set("rss", rss() as f64)?;
  Ok(obj)
}

// Signals that already have an OS watcher installed for this context, so
// repeated on()/once() calls do not spawn duplicate watchers. A watcher removes
// its own entry when it stops, so a later subscribe reinstalls it.
#[derive(Clone, rquickjs::JsLifetime, Default)]
struct InstalledSignals(#[qjs(skip_trace)] Rc<RefCell<HashSet<String>>>);

pub struct ProcessModule;

impl ModuleDef for ProcessModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("on")?;
    decl.declare("once")?;
    decl.declare("argv")?;
    decl.declare("platform")?;
    decl.declare("arch")?;
    decl.declare("memoryUsage")?;
    decl.declare("hrtime")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let _ = ctx.store_userdata(InstalledSignals::default());
    exports.export("on", Function::new(ctx.clone(), on_impl)?)?;
    exports.export("once", Function::new(ctx.clone(), once_impl)?)?;

    let argv = Array::new(ctx.clone())?;
    if let Some(args) = ctx.userdata::<ProcessArgs>() {
      for (i, arg) in args.0.iter().enumerate() {
        argv.set(i, arg.as_str())?;
      }
    }
    exports.export("argv", argv)?;
    exports.export("platform", platform())?;
    exports.export("arch", arch())?;
    exports.export("memoryUsage", Function::new(ctx.clone(), memory_usage)?)?;
    let hrtime = Object::new(ctx.clone())?;
    hrtime.set("bigint", Function::new(ctx.clone(), hrtime_bigint)?)?;
    exports.export("hrtime", hrtime)?;
    Ok(())
  }
}

fn hrtime_bigint(ctx: Ctx<'_>) -> rquickjs::Result<BigInt<'_>> {
  // u64 nanoseconds cover ~584 years of uptime.
  BigInt::from_u64(ctx, hrtime_nanos() as u64)
}

fn on_impl<'js>(ctx: Ctx<'js>, signal: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  ensure_watcher(&ctx, &signal);
  register_listener(&ctx, signal, callback, false)
}

fn once_impl<'js>(ctx: Ctx<'js>, signal: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  ensure_watcher(&ctx, &signal);
  register_listener(&ctx, signal, callback, true)
}

// Installs a once-per-context OS watcher for `signal` that emits the signal name
// through the event bus on each delivery, stopping when the signal has no more
// listeners. The OS source (and the unix vs non-unix split) lives in
// `forge::process::SignalStream`; an unopenable signal (unknown name, unsupported
// on this platform, install failure) logs and installs nothing.
fn ensure_watcher(ctx: &Ctx<'_>, signal: &str) {
  let installed = ctx.userdata::<InstalledSignals>().expect("installed signals userdata");
  if installed.0.borrow().contains(signal) {
    return;
  }
  let mut stream = match SignalStream::open(signal) {
    Ok(stream) => stream,
    Err(msg) => {
      ctx.logger().error(&format!("[flux:process] {msg}"));
      return;
    }
  };
  installed.0.borrow_mut().insert(signal.to_string());

  let name = signal.to_string();
  let ctx_cb = ctx.clone();
  ctx.spawn(async move {
    while stream.recv().await {
      emit_event(&ctx_cb, &name, name.clone());
      // A fired once() listener is pruned by the bus; if no listeners remain,
      // stop watching so the engine can go idle.
      if !has_listeners(&ctx_cb, &name) {
        break;
      }
    }
    if let Some(installed) = ctx_cb.userdata::<InstalledSignals>() {
      installed.0.borrow_mut().remove(&name);
    }
  });
}
