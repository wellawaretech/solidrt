use rquickjs::function::MutFn;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, JsLifetime};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Notify;

use super::events::{add_listener, clear_listeners, emit_event, has_listeners, remove_listener};
use crate::logger::CtxLogger;
use forge::tty::{is_terminal, open_lines, write as write_stdout, Input};

// flux:tty - the terminal attached to this process. Node's tty/readline
// vocabulary with the semantics a dev tool needs:
//
//   import { isTTY, on, write } from "flux:tty"
//   isTTY            // stdin is a terminal
//   on("line", cb)   // one cooked line per delivery, newline stripped
//   on("close", cb)  // stdin reached end of file
//   write(text)      // stdout as is, no newline appended (a prompt)
//
// The same event-bus shape as flux:process on(): a listener holds the engine
// loop alive, the last unsubscribe wakes the watcher so it stops and the loop
// can go idle. The line source (a process-wide reader thread) lives in
// `forge::tty`; this layer owns the watcher task and the bus. After EOF no
// line can come, so the watcher drops every tty listener itself: a listener
// left registered would hold the loop for an event that never fires.
//
// The bus is shared with every other event surface in the context, so the
// names on it are prefixed; the JS names stay "line" and "close".
const LINE: &str = "tty:line";
const CLOSE: &str = "tty:close";

fn bus_name(event: &str) -> Option<&'static str> {
  match event {
    "line" => Some(LINE),
    "close" => Some(CLOSE),
    _ => None,
  }
}

// Per-context reader state: the channel end parked between watcher runs (a
// watcher takes it while running), the wakeup that stops the running watcher,
// and whether stdin has ended.
#[derive(Clone, JsLifetime, Default)]
struct TtyState(#[qjs(skip_trace)] Rc<Inner>);

#[derive(Default)]
struct Inner {
  lines: RefCell<Option<UnboundedReceiver<Input>>>,
  stop: RefCell<Option<Arc<Notify>>>,
  eof: Cell<bool>,
}

pub struct TtyModule;

impl ModuleDef for TtyModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("isTTY")?;
    decl.declare("on")?;
    decl.declare("once")?;
    decl.declare("write")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let _ = ctx.store_userdata(TtyState::default());
    exports.export("isTTY", is_terminal())?;
    exports.export("on", Function::new(ctx.clone(), on_impl)?)?;
    exports.export("once", Function::new(ctx.clone(), once_impl)?)?;
    exports.export("write", Function::new(ctx.clone(), write_impl)?)?;
    Ok(())
  }
}

fn on_impl<'js>(ctx: Ctx<'js>, event: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  subscribe(ctx, event, callback, false)
}

fn once_impl<'js>(ctx: Ctx<'js>, event: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  subscribe(ctx, event, callback, true)
}

fn write_impl(ctx: Ctx<'_>, text: String) -> rquickjs::Result<()> {
  write_stdout(&text).map_err(|e| Exception::throw_message(&ctx, &format!("[flux:tty] stdout write failed: {e}")))
}

// Register on the bus and hand JS an unsubscribe that captures only the bus
// name and the listener id (never a JS value, see flux:process). Removing the
// last tty listener stops the watcher so the engine can go idle.
fn subscribe<'js>(
  ctx: Ctx<'js>,
  event: String,
  callback: Function<'js>,
  once: bool,
) -> rquickjs::Result<Function<'js>> {
  let Some(name) = bus_name(&event) else {
    return Err(Exception::throw_type(&ctx, &format!("Unknown tty event: {event} (expected \"line\" or \"close\")")));
  };
  let state = ctx.userdata::<TtyState>().expect("tty state userdata").clone();
  if state.0.eof.get() {
    // Nothing can fire after close; a listener would only hold the loop.
    return Function::new(ctx, MutFn::from(|| {}));
  }
  ensure_watcher(&ctx, &state);
  let id = add_listener(&ctx, name.to_string(), callback, once);
  Function::new(
    ctx,
    MutFn::from(move |ctx: Ctx<'_>| {
      if remove_listener(&ctx, name, id) && !has_listeners(&ctx, LINE) && !has_listeners(&ctx, CLOSE) {
        stop_watcher(&ctx);
      }
    }),
  )
}

fn stop_watcher(ctx: &Ctx<'_>) {
  let state = ctx.userdata::<TtyState>().expect("tty state userdata");
  let stop = state.0.stop.borrow().clone();
  if let Some(stop) = stop {
    stop.notify_one();
  }
}

// Start the watcher task unless one runs: take the parked channel end (or
// open stdin on the first run), then deliver lines to the bus until EOF, the
// last unsubscribe, or the stop wakeup. stdin is read once per process, so a
// second engine (an isolate) gets no reader and logs why.
fn ensure_watcher(ctx: &Ctx<'_>, state: &TtyState) {
  if state.0.stop.borrow().is_some() {
    return;
  }
  let parked = state.0.lines.borrow_mut().take();
  let Some(mut rx) = parked.or_else(open_lines) else {
    ctx.logger().error("[flux:tty] stdin is already read by another engine in this process");
    return;
  };
  let stop = Arc::new(Notify::new());
  *state.0.stop.borrow_mut() = Some(stop.clone());

  let state = state.clone();
  let ctx_cb = ctx.clone();
  ctx.spawn(async move {
    loop {
      let input = tokio::select! {
        got = rx.recv() => got,
        _ = stop.notified() => {
          // The last unsubscribe woke us; a listener added since (off()
          // then on() in one tick) keeps the watcher, which is the one
          // ensure_watcher saw as running.
          if has_listeners(&ctx_cb, LINE) || has_listeners(&ctx_cb, CLOSE) {
            continue;
          }
          None
        }
      };
      match input {
        Some(Input::Line(text)) => {
          emit_event(&ctx_cb, LINE, text);
          // Fired once() listeners are pruned by the bus; with none left,
          // stop so the engine can go idle.
          if !has_listeners(&ctx_cb, LINE) && !has_listeners(&ctx_cb, CLOSE) {
            break;
          }
        }
        Some(Input::Eof) => {
          state.0.eof.set(true);
          emit_event(&ctx_cb, CLOSE, ());
          clear_listeners(&ctx_cb, LINE);
          clear_listeners(&ctx_cb, CLOSE);
          break;
        }
        None => break,
      }
    }
    if !state.0.eof.get() {
      *state.0.lines.borrow_mut() = Some(rx);
    }
    *state.0.stop.borrow_mut() = None;
  });
}
