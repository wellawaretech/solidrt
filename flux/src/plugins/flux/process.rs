use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Function, JsLifetime};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::events::register_listener;

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
// it would keep the engine from ever going idle.

// flux:process also exposes the process argument vector:
//
//   import { argv } from "flux:process"
//
// The host sets it with FluxEngine::builder().userdata(ProcessArgs(...)); when
// unset, argv is an empty array. Node/Bun parity: argv[0] is the executable,
// argv[1] the script path, and the rest are the user-supplied arguments.
#[derive(Clone, JsLifetime, Default)]
pub struct ProcessArgs(#[qjs(skip_trace)] pub Vec<String>);

// flux:process also exposes the host OS and CPU architecture:
//
//   import { platform, arch } from "flux:process"

/// The host OS ("darwin", "win32", "linux", "android", ...).
pub fn platform() -> &'static str {
  match std::env::consts::OS {
    "macos" => "darwin",
    "windows" => "win32",
    other => other,
  }
}

/// The CPU architecture ("x64", "arm64", ...).
pub fn arch() -> &'static str {
  match std::env::consts::ARCH {
    "x86_64" => "x64",
    "aarch64" => "arm64",
    "x86" => "ia32",
    other => other,
  }
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
    Ok(())
  }
}

fn on_impl<'js>(ctx: Ctx<'js>, signal: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  ensure_watcher(&ctx, &signal);
  register_listener(&ctx, signal, callback, false)
}

fn once_impl<'js>(ctx: Ctx<'js>, signal: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  ensure_watcher(&ctx, &signal);
  register_listener(&ctx, signal, callback, true)
}

const KNOWN_SIGNALS: &[&str] = &["SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT", "SIGUSR1", "SIGUSR2"];

// Installs a once-per-context OS watcher for `signal` that emits the signal name
// through the event bus on each delivery, stopping when the signal has no more
// listeners. Unknown signal names install no watcher (their listeners simply
// never fire).
fn ensure_watcher(ctx: &Ctx<'_>, signal: &str) {
  use crate::logger::CtxLogger;

  if !KNOWN_SIGNALS.contains(&signal) {
    ctx.logger().error(&format!("[flux:process] unrecognized signal: {signal}"));
    return;
  }
  let installed = ctx.userdata::<InstalledSignals>().expect("installed signals userdata");
  if installed.0.borrow().contains(signal) {
    return;
  }
  install_watcher(ctx, signal, &installed);
}

#[cfg(unix)]
fn install_watcher(ctx: &Ctx<'_>, signal: &str, installed: &InstalledSignals) {
  use super::events::{emit_event, has_listeners};
  use crate::logger::CtxLogger;
  use tokio::signal::unix::SignalKind;

  let kind = match signal {
    "SIGINT" => SignalKind::interrupt(),
    "SIGTERM" => SignalKind::terminate(),
    "SIGHUP" => SignalKind::hangup(),
    "SIGQUIT" => SignalKind::quit(),
    "SIGUSR1" => SignalKind::user_defined1(),
    "SIGUSR2" => SignalKind::user_defined2(),
    _ => unreachable!(),
  };
  let mut stream = match tokio::signal::unix::signal(kind) {
    Ok(stream) => stream,
    Err(e) => {
      ctx.logger().error(&format!("[flux:process] failed to install {signal} handler: {e}"));
      return;
    }
  };
  installed.0.borrow_mut().insert(signal.to_string());

  let name = signal.to_string();
  let ctx_cb = ctx.clone();
  ctx.spawn(async move {
    while stream.recv().await.is_some() {
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

// On non-Unix platforms only SIGINT (Ctrl+C) is supported via tokio's ctrl_c().
#[cfg(not(unix))]
fn install_watcher(ctx: &Ctx<'_>, signal: &str, installed: &InstalledSignals) {
  use super::events::{emit_event, has_listeners};
  use crate::logger::CtxLogger;

  if signal != "SIGINT" {
    ctx.logger().error(&format!("[flux:process] unsupported signal on this platform: {signal}"));
    return;
  }
  installed.0.borrow_mut().insert(signal.to_string());

  let ctx_cb = ctx.clone();
  ctx.spawn(async move {
    loop {
      if let Err(e) = tokio::signal::ctrl_c().await {
        ctx_cb.logger().error(&format!("[flux:process] failed to install SIGINT handler: {e}"));
        break;
      }
      emit_event(&ctx_cb, "SIGINT", "SIGINT".to_string());
      if !has_listeners(&ctx_cb, "SIGINT") {
        break;
      }
    }
    if let Some(installed) = ctx_cb.userdata::<InstalledSignals>() {
      installed.0.borrow_mut().remove("SIGINT");
    }
  });
}
