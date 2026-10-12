//! JS bindings for microphone capture: a thin marshaling layer over
//! alloy::microphone. Opening is the permission request (SDL semantics, as
//! for the camera): `open()` returns a promise that a task of the engine
//! settles, polling the one session on its own cadence until it leaves
//! Pending (the Android RECORD_AUDIO prompt; at once everywhere else),
//! holding the engine as work in flight meanwhile. No frame is involved: a
//! microphone demands none, so the answer wakes JS without one.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use alloy::microphone::MicrophoneStatus;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object, Persistent, TypedArray};

use crate::plugins::marshal::OptArg;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

/// The settle functions of an `open()` promise, by session, until the
/// open-wait task settles it.
struct PendingOpen {
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

struct Inner {
  gui: Rc<super::Gui>,
  pending: RefCell<HashMap<u64, PendingOpen>>,
  /// The sessions this engine opened and has not closed: what its end
  /// releases (the capture stream).
  sessions: RefCell<HashSet<u64>>,
}

impl Drop for Inner {
  // Engine teardown: the app's handles died with it, so nothing else will
  // ever close its sessions.
  fn drop(&mut self) {
    for session in self.sessions.get_mut().drain() {
      self.gui.alloy.close_microphone(session);
    }
  }
}

#[derive(Clone, JsLifetime)]
struct MicrophonePluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the microphone plugin state in userdata. Runs at engine init,
/// before any module import; the `flux:microphone` module surface reads it.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx
    .store_userdata(MicrophonePluginState(Rc::new(Inner {
      gui: super::gui(ctx),
      pending: RefCell::new(HashMap::new()),
      sessions: RefCell::new(HashSet::new()),
    })))
    .expect("store microphone state");
}

fn state(ctx: &Ctx<'_>) -> Rc<Inner> {
  ctx.userdata::<MicrophonePluginState>().expect("microphone state").0.clone()
}

/// The `flux:microphone` module. `open` resolves to a bound session object
/// (`{ id, sampleRate, read, close }`): the methods are bound, and the id is
/// the key a runtime module takes a reader of its own by
/// (`alloy::Context::microphone_tap`).
pub struct MicrophoneModule;

impl ModuleDef for MicrophoneModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("listMicrophones")?;
    decl.declare("open")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("listMicrophones", Function::new(ctx.clone(), list_impl)?)?;
    exports.export("open", Function::new(ctx.clone(), open_impl)?)?;
    Ok(())
  }
}

fn list_impl(ctx: Ctx<'_>) -> rquickjs::Result<Array<'_>> {
  let arr = Array::new(ctx.clone())?;
  for (i, mic) in state(&ctx).gui.alloy.list_microphones().iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("id", mic.id)?;
    obj.set("name", mic.name.as_str())?;
    arr.set(i, obj)?;
  }
  Ok(arr)
}

fn open_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  let mut device: Option<u32> = None;
  let mut sample_rate: Option<u32> = None;
  if let Some(opts) = options.0 {
    device = opts.get("microphone")?;
    sample_rate = opts.get("sampleRate")?;
  }
  let sample_rate = sample_rate.unwrap_or(16000);

  let state = state(&ctx);
  // A device that cannot be opened rejects the promise rather than
  // throwing: the failure is environmental, and a sync throw here unwinds
  // through the caller's reactive computation, which no .catch can
  // intercept (the camera's reasoning).
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let session = match state.gui.alloy.open_microphone(device, sample_rate) {
    Ok(session) => session,
    Err(e) => {
      let error = Exception::from_message(ctx.clone(), &format!("openMicrophone: {e}"))?;
      reject.call::<_, ()>((error,))?;
      return Ok(promise);
    }
  };
  state.sessions.borrow_mut().insert(session);
  state
    .pending
    .borrow_mut()
    .insert(session, PendingOpen { resolve: Persistent::save(&ctx, resolve), reject: Persistent::save(&ctx, reject) });
  // The open is in flight until the device answers, and it answers to
  // polling only: the task pumps the session on its own cadence, then
  // settles the promise itself, with no frame between.
  let hold = crate::pending::PendingOps::of(&ctx).in_flight("microphone open");
  let (task_ctx, task_state) = (ctx.clone(), state.clone());
  ctx.spawn(async move {
    let status = loop {
      match task_state.gui.alloy.pump_microphone(session) {
        Some(MicrophoneStatus::Pending) => {
          tokio::time::sleep(std::time::Duration::from_millis(super::OPEN_POLL_MS)).await
        }
        other => break other,
      }
    };
    settle_open(&task_ctx, &task_state, session, status);
    drop(hold);
  });
  Ok(promise)
}

/// Settle the open() promise of `session` with the status it left Pending
/// on: Ready resolves with the session object, Denied or Failed close the
/// session (JS gets nothing to close) and reject with the reason.
fn settle_open(ctx: &Ctx<'_>, state: &Inner, session: u64, status: Option<MicrophoneStatus>) {
  let Some(entry) = state.pending.borrow_mut().remove(&session) else {
    return;
  };
  let settled = match status {
    Some(MicrophoneStatus::Ready { sample_rate }) => session_object(ctx.clone(), session, sample_rate)
      .and_then(|obj| entry.resolve.restore(ctx)?.call::<_, ()>((obj,))),
    Some(MicrophoneStatus::Denied) => {
      close_impl(ctx.clone(), session);
      reject_with(ctx, entry.reject, "microphone permission denied")
    }
    Some(MicrophoneStatus::Failed(message)) => {
      close_impl(ctx.clone(), session);
      reject_with(ctx, entry.reject, &format!("openMicrophone: {message}"))
    }
    Some(MicrophoneStatus::Pending) | None => reject_with(ctx, entry.reject, "microphone closed"),
  };
  if let Err(e) = settled {
    log::warn!("[microphone] open settle failed: {e}");
  }
}

fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, msg: &str) -> rquickjs::Result<()> {
  let error = Exception::from_message(ctx.clone(), msg)?;
  reject.restore(ctx)?.call::<_, ()>((error,))
}

/// The session object JS holds: `{ id, sampleRate, read, close }`.
fn session_object(ctx: Ctx<'_>, session: u64, sample_rate: u32) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("id", session)?;
  obj.set("sampleRate", sample_rate)?;
  // read()/close() are bound to this session so the raw handle stays in Rust.
  // read returns an invariant TypedArray<'js>, so its closure needs the HRTB
  // coercion (see flux/CLAUDE.md); close returns () and does not.
  let read_fn = Function::new(ctx.clone(), read_builder(move |ctx| read_impl(ctx, session)))?;
  obj.set("read", read_fn)?;
  let close_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_impl(ctx, session))?;
  obj.set("close", close_fn)?;
  Ok(obj)
}

// Coerces a capturing closure to the `for<'js>` HRTB rquickjs needs to return an
// invariant `TypedArray<'js>`; a capturing closure will not infer it on its own.
fn read_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<TypedArray<'js, f32>>,
{
  f
}

/// Drain the mono f32 samples captured since the last read (the session's
/// own reader on its ring; a reader further behind than the window loses the
/// oldest samples, which alloy logs).
fn read_impl(ctx: Ctx<'_>, session: u64) -> rquickjs::Result<TypedArray<'_, f32>> {
  let samples = state(&ctx).gui.alloy.read_microphone(session).map_err(|e| throw_str(&ctx, &format!("read: {e}")))?;
  TypedArray::new(ctx.clone(), samples)
}

fn close_impl(ctx: Ctx<'_>, session: u64) {
  let state = state(&ctx);
  state.gui.alloy.close_microphone(session);
  state.sessions.borrow_mut().remove(&session);
}
