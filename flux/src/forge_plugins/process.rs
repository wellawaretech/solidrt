use rquickjs::function::MutFn;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Function, JsLifetime, Object};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Notify;

use super::events::{add_listener, emit_event, has_listeners, remove_listener};
use crate::logger::CtxLogger;
use forge::process::{alive, arch, env_vars, exec_path, home_dir, kill, pid, platform, rss, SignalStream};

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
fn memory_usage(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx)?;
  obj.set("rss", rss() as f64)?;
  Ok(obj)
}

// flux:process also exposes the user's home directory, the running
// executable, a portable kill and a liveness probe:
//
//   import { homedir, execPath, kill, alive } from "flux:process"
//   homedir()  // "/home/me", or null when the environment names none
//   execPath   // "/opt/solidrt/solidrt-go", or null when the OS cannot say
//   kill(pid)  // true when the process was terminated (SIGKILL / TerminateProcess)
//   alive(pid) // true while a process with that id exists
//
// homedir names the one path a tool needs first (the machine-wide ~/.solidrt
// state) without spelling the HOME/USERPROFILE split. execPath is what a dev
// tool spawns to get another instance of the runtime it runs in (a value,
// snapshotted at evaluation like pid: the executable does not move). In a
// packed app it is the app itself, so spawning it launches that app, not a
// bare runtime. kill takes a pid only - no signal argument - matching the portable
// contract of Child.kill in flux:subprocess. alive is the "signal 0" idiom
// with its own name: a registry reader asks it before trusting a record.
fn homedir_impl() -> Option<String> {
  home_dir()
}

fn kill_impl(pid: u32) -> bool {
  kill(pid)
}

fn alive_impl(pid: u32) -> bool {
  alive(pid)
}

// flux:process also exposes the environment:
//
//   import { env } from "flux:process"
//   env.SRT_HOME  // the value, or undefined
//
// A plain object snapshotted when the module is evaluated (Node's process.env
// is live and writable; a dev tool reads its environment once at startup, so
// the snapshot is the whole contract).
fn env_object<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
  let env = Object::new(ctx.clone())?;
  for (name, value) in env_vars() {
    env.set(name.as_str(), value.as_str())?;
  }
  Ok(env)
}

// Signals that already have an OS watcher installed for this context, so
// repeated on()/once() calls do not spawn duplicate watchers, each with the
// wakeup that tells the watcher to stop. A watcher removes its own entry when
// it stops, so a later subscribe reinstalls it.
#[derive(Clone, rquickjs::JsLifetime, Default)]
struct InstalledSignals(#[qjs(skip_trace)] Rc<RefCell<HashMap<String, Arc<Notify>>>>);

pub struct ProcessModule;

impl ModuleDef for ProcessModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("on")?;
    decl.declare("once")?;
    decl.declare("argv")?;
    decl.declare("platform")?;
    decl.declare("arch")?;
    decl.declare("memoryUsage")?;
    decl.declare("pid")?;
    decl.declare("homedir")?;
    decl.declare("execPath")?;
    decl.declare("kill")?;
    decl.declare("alive")?;
    decl.declare("env")?;
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
    exports.export("pid", pid())?;
    exports.export("homedir", Function::new(ctx.clone(), homedir_impl)?)?;
    exports.export("execPath", exec_path())?;
    exports.export("kill", Function::new(ctx.clone(), kill_impl)?)?;
    exports.export("alive", Function::new(ctx.clone(), alive_impl)?)?;
    exports.export("env", env_object(ctx)?)?;
    Ok(())
  }
}

fn on_impl<'js>(ctx: Ctx<'js>, signal: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  subscribe(ctx, signal, callback, false)
}

fn once_impl<'js>(ctx: Ctx<'js>, signal: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  subscribe(ctx, signal, callback, true)
}

// Register on the bus and hand JS an unsubscribe that captures only the
// signal name and the listener id (never a JS value: one captured in a native
// closure still alive at teardown is never released, and the runtime asserts
// on it). Removing the last listener wakes the watcher so it stops and an
// engine whose only remaining work was the watcher can go idle; without that
// wakeup the watcher would only notice on the signal's next delivery.
fn subscribe<'js>(
  ctx: Ctx<'js>,
  signal: String,
  callback: Function<'js>,
  once: bool,
) -> rquickjs::Result<Function<'js>> {
  ensure_watcher(&ctx, &signal);
  let id = add_listener(&ctx, signal.clone(), callback, once);
  Function::new(
    ctx,
    MutFn::from(move |ctx: Ctx<'_>| {
      if remove_listener(&ctx, &signal, id) {
        stop_watcher(&ctx, &signal);
      }
    }),
  )
}

// Wake the watcher for `signal` so it stops; a no-op when none is installed.
fn stop_watcher(ctx: &Ctx<'_>, signal: &str) {
  let installed = ctx.userdata::<InstalledSignals>().expect("installed signals userdata");
  let notify = installed.0.borrow().get(signal).cloned();
  if let Some(notify) = notify {
    notify.notify_one();
  }
}

// Installs a once-per-context OS watcher for `signal` that emits the signal name
// through the event bus on each delivery, stopping when the signal has no more
// listeners. The OS source (and the unix vs non-unix split) lives in
// `forge::process::SignalStream`; an unopenable signal (unknown name, unsupported
// on this platform, install failure) logs and installs nothing.
fn ensure_watcher(ctx: &Ctx<'_>, signal: &str) {
  let installed = ctx.userdata::<InstalledSignals>().expect("installed signals userdata");
  if installed.0.borrow().contains_key(signal) {
    return;
  }
  let mut stream = match SignalStream::open(signal) {
    Ok(stream) => stream,
    Err(msg) => {
      ctx.logger().error(&format!("[flux:process] {msg}"));
      return;
    }
  };
  let stop = Arc::new(Notify::new());
  installed.0.borrow_mut().insert(signal.to_string(), stop.clone());

  let name = signal.to_string();
  let ctx_cb = ctx.clone();
  ctx.spawn(async move {
    loop {
      // Deliveries and the last unsubscribe both end the wait; only the
      // former dispatches.
      let delivered = tokio::select! {
        got = stream.recv() => got,
        _ = stop.notified() => false,
      };
      if !delivered {
        break;
      }
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
