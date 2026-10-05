use std::cell::RefCell;
use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Persistent, Promise};

use crate::input_plan::{self, Injected};
use crate::settle::Cap;
use crate::stepped::{Stepper, WindowReady};

// The `sol:test` module: the engine verbs of an app test, which
// `@solidrt/test` builds the test surface on. Thin FFI over the
// stepper (test_host.rs). In the dev client only, and its verbs work only
// in an engine a test host built.

/// Where a capture queued ahead of a `painted()` frame lands its outcome
/// during that frame's paint.
type CaptureSlot = Rc<RefCell<Option<Result<alloy::CaptureInfo, String>>>>;

/// The frame a test asked for and is waiting on: what settles its promise
/// once the frame verb has run the frame, and the hold that keeps the
/// engine alive until then (the frame's signal travels through the
/// runner's loop, outside the engine).
struct Waiting {
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
  _hold: flux::Hold,
  /// A `painted()` frame: the promise fulfills with the capture's pixels
  /// (or rejects with its error) instead of with nothing.
  capture: Option<CaptureSlot>,
}

#[derive(Clone, Default, JsLifetime)]
struct PendingFrame(#[qjs(skip_trace)] Rc<RefCell<Option<Waiting>>>);

fn stepper(ctx: &Ctx<'_>, verb: &str) -> flux::rquickjs::Result<Stepper> {
  match ctx.userdata::<Stepper>() {
    Some(stepper) => Ok((*stepper).clone()),
    None => Err(Exception::throw_message(
      ctx,
      &format!("sol:test {verb}: an app test is run by a test host; use sol test <file>"),
    )),
  }
}

/// The stepper and the frame slot, the slot free: a test runs one frame at
/// a time.
fn free_frame(ctx: &Ctx<'_>, verb: &str) -> flux::rquickjs::Result<(Stepper, PendingFrame)> {
  let stepper = stepper(ctx, verb)?;
  let pending = ctx.userdata::<PendingFrame>().expect("pending frame installed").clone();
  if pending.0.borrow().is_some() {
    return Err(Exception::throw_message(
      ctx,
      &format!("sol:test {verb}: the previous frame has not finished; await it first"),
    ));
  }
  Ok((stepper, pending))
}

/// Run one frame; its promise settles once it has run (`frame_done`).
fn run_frame<'js>(
  ctx: &Ctx<'js>,
  stepper: Stepper,
  pending: PendingFrame,
  capture: Option<CaptureSlot>,
) -> flux::rquickjs::Result<Promise<'js>> {
  let (promise, resolve, reject) = ctx.promise()?;
  *pending.0.borrow_mut() = Some(Waiting {
    resolve: Persistent::save(ctx, resolve),
    reject: Persistent::save(ctx, reject),
    _hold: flux::hold_engine(ctx, "frame"),
    capture,
  });
  stepper.step();
  Ok(promise)
}

/// `frame()`: run one frame; the promise fulfills once it has run.
fn frame<'js>(ctx: Ctx<'js>) -> flux::rquickjs::Result<Promise<'js>> {
  let (stepper, pending) = free_frame(&ctx, "frame()")?;
  run_frame(&ctx, stepper, pending, None)
}

/// `painted(node)`: run one frame with a capture of `node` queued ahead
/// of it, so the frame's own paint services the capture; the promise
/// fulfills with the pixels once the frame has run - what that frame drew
/// of the node, written and painted in one frame, where `capture()`
/// paints the tree afresh after the fact. The capture is queued only once
/// the frame slot is known to be free, so a refused call leaves nothing
/// behind for a later paint to service.
fn painted<'js>(ctx: Ctx<'js>, node: u64) -> flux::rquickjs::Result<Promise<'js>> {
  let (stepper, pending) = free_frame(&ctx, "painted()")?;
  let Some(alloy) = flux::gui::alloy_context(&ctx) else {
    return Err(Exception::throw_message(&ctx, "sol:test painted: no window in this test; mount the app first"));
  };
  let outcome: CaptureSlot = Rc::new(RefCell::new(None));
  let slot = outcome.clone();
  alloy.request_capture(node, Box::new(move |result| *slot.borrow_mut() = Some(result)));
  // The frame must paint for the capture to be serviced, whatever the app
  // demands: the capture first, then the request (captureSnapshot's order).
  flux::gui::request_frame(&ctx);
  run_frame(&ctx, stepper, pending, Some(outcome))
}

/// The frame verb ran a frame: settle the promise of the test that asked
/// for it. No-op when nobody did (the module was never imported).
pub(crate) fn frame_done(ctx: &Ctx<'_>) {
  let Some(pending) = ctx.userdata::<PendingFrame>() else {
    return;
  };
  let Some(waiting) = pending.0.borrow_mut().take() else {
    return;
  };
  let Waiting { resolve, reject, _hold, capture } = waiting;
  let settled = match capture {
    None => resolve.restore(ctx).and_then(|resolve| resolve.call::<_, ()>(())),
    Some(outcome) => match outcome.borrow_mut().take() {
      Some(Ok(info)) => pixels_object(ctx, info)
        .and_then(|image| resolve.restore(ctx).and_then(|resolve| resolve.call::<_, ()>((image,)))),
      Some(Err(e)) => reject_with(ctx, reject, &format!("sol:test painted: {e}")),
      None => reject_with(ctx, reject, "sol:test painted: the frame's paint did not reach the node"),
    },
  };
  if let Err(e) = settled {
    flux::report_uncaught(ctx, e, "sol:test frame()");
  }
}

/// Reject a frame's promise with an Error carrying `message`.
fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, message: &str) -> flux::rquickjs::Result<()> {
  let error = Exception::from_message(ctx.clone(), message)?;
  reject.restore(ctx)?.call::<_, ()>((error,))
}

/// A capture's pixels as the `{ width, height, data }` object `capture()`
/// and `painted()` hand a test.
fn pixels_object<'js>(ctx: &Ctx<'js>, info: alloy::CaptureInfo) -> flux::rquickjs::Result<Object<'js>> {
  let image = Object::new(ctx.clone())?;
  image.set("width", info.width)?;
  image.set("height", info.height)?;
  image.set("data", flux::rquickjs::TypedArray::new(ctx.clone(), info.pixels)?)?;
  Ok(image)
}

/// `windowReady()`: fulfills once the window's size has reached the
/// engine, at once when it already has. No frame runs for it and no time
/// passes: what a mount waits on before its first frame is there to read.
fn window_ready_promise<'js>(ctx: Ctx<'js>) -> flux::rquickjs::Result<Promise<'js>> {
  stepper(&ctx, "windowReady()")?;
  let state = ctx.userdata::<WindowReady>().expect("window ready installed").clone();
  let (promise, resolve, _reject) = ctx.promise()?;
  if state.is_ready() {
    resolve.call::<_, ()>(())?;
  } else {
    state.wait_promise(Persistent::save(&ctx, resolve), flux::hold_engine(&ctx, "window"));
  }
  Ok(promise)
}

/// The input plan a test is sending, step by step (see `input_plan`).
#[derive(Clone, Default, JsLifetime)]
struct InputPlan(#[qjs(skip_trace)] Rc<RefCell<Vec<Option<Injected>>>>);

/// `inputPlan(events)`: expand `events` (JSON text, the `/input` event
/// shape) into steps and return what passes before each: `[ms, frames]`
/// per step. The steps are then sent with `inputStep`, in order.
fn plan_input(ctx: Ctx<'_>, events: String) -> flux::rquickjs::Result<Vec<Vec<f64>>> {
  let stepper = stepper(&ctx, "inputPlan()")?;
  let throw = |message: String| Exception::throw_message(&ctx, &format!("sol:test input: {message}"));
  let events: serde_json::Value = serde_json::from_str(&events).map_err(|e| throw(e.to_string()))?;
  let steps = input_plan::plan(Some(&events), stepper.frame_ms()).map_err(throw)?;
  let waits = steps.iter().map(|step| vec![step.wait.ms as f64, step.wait.frames as f64]).collect();
  let plan = ctx.userdata::<InputPlan>().expect("input plan installed").clone();
  *plan.0.borrow_mut() = steps.into_iter().map(|step| Some(step.inject)).collect();
  Ok(waits)
}

/// The synthetic pads of this engine: alloy's pad table without devices,
/// which a test drives with the same commands the control API sends the
/// interactive loop. Per engine, so a test starts with no pad seated.
#[derive(Clone, JsLifetime)]
struct Pads(#[qjs(skip_trace)] Rc<RefCell<alloy::Gamepads>>);

/// `inputStep(index)`: send one step of the current plan.
fn step_input(ctx: Ctx<'_>, index: usize) -> flux::rquickjs::Result<()> {
  let stepper = stepper(&ctx, "inputStep()")?;
  let plan = ctx.userdata::<InputPlan>().expect("input plan installed").clone();
  let step = plan.0.borrow_mut().get_mut(index).and_then(Option::take);
  let Some(step) = step else {
    return Err(Exception::throw_message(&ctx, "sol:test inputStep(index): no such step left in the plan"));
  };
  match step {
    Injected::Event(event) => stepper.inject(event),
    Injected::Gamepad(command) => {
      let pads = ctx.userdata::<Pads>().expect("pads installed").clone();
      let mut pads = pads.0.borrow_mut();
      pads.apply(command).map_err(|e| Exception::throw_message(&ctx, &format!("sol:test input: gamepad: {e}")))?;
      // What the interactive loop does after a command: the snapshot, and
      // the back edge a synthetic "back" press is.
      if let Some(snapshot) = pads.take_snapshot_if_dirty() {
        stepper.send(snapshot);
      }
      if pads.take_back_edge() {
        stepper.send(alloy::AlloyEvent::Back);
      }
    }
  }
  Ok(())
}

/// `link(link)`: deliver a link the way an OS-routed one arrives (the raw
/// string, through the UI loop, on the `link` event); whether anything
/// listens for one.
fn link(ctx: Ctx<'_>, link: String) -> flux::rquickjs::Result<bool> {
  let stepper = stepper(&ctx, "link()")?;
  let listening = flux::has_listeners(&ctx, "link");
  stepper.send(alloy::AlloyEvent::Link { link });
  Ok(listening)
}

/// `debug(name, args)`: call a debug command the app registered
/// (`registerDebug` of `sol:dev`), `args` and the result as JSON text
/// (null for none).
fn debug(ctx: Ctx<'_>, name: String, args: Option<String>) -> flux::rquickjs::Result<String> {
  stepper(&ctx, "debug()")?;
  let throw = |message: String| Exception::throw_message(&ctx, &format!("sol:test debug: {message}"));
  let args = match args {
    Some(text) => Some(serde_json::from_str(&text).map_err(|e| throw(format!("args: {e}")))?),
    None => None,
  };
  crate::plugins::dev::call_debug(&ctx, &name, args).map(|value| value.to_string()).map_err(throw)
}

/// `capture(node)`: the pixels the node's subtree paints, as `{ width,
/// height, data }` (RGBA8, premultiplied, rows top to bottom, at the
/// display scale). Drawn now, with the tree as it is: no frame runs and no
/// app time passes, so a running transition is read where it stands.
fn capture<'js>(ctx: Ctx<'js>, node: u64) -> flux::rquickjs::Result<Object<'js>> {
  stepper(&ctx, "capture()")?;
  let throw = |message: &str| Exception::throw_message(&ctx, &format!("sol:test capture: {message}"));
  let Some(alloy) = flux::gui::alloy_context(&ctx) else {
    return Err(throw("no window in this test; mount the app first"));
  };
  let outcome = Rc::new(RefCell::new(None));
  let slot = outcome.clone();
  alloy.request_capture(node, Box::new(move |result| *slot.borrow_mut() = Some(result)));
  // The capture is serviced by a paint and delivered when it ends: ask for
  // one and run it here.
  flux::gui::request_frame(&ctx);
  crate::plugins::draw::render_now(&ctx);
  let info = match outcome.borrow_mut().take() {
    Some(Ok(info)) => info,
    Some(Err(e)) => return Err(throw(&e)),
    None => return Err(throw("the paint did not reach the node")),
  };
  pixels_object(&ctx, info)
}

/// `settle(maxMs)`: run frames until the app is at rest (settle.rs):
/// nothing in flight, no timer due, no frame demanded. Rejects once `maxMs`
/// of app time have passed without that, saying what is left.
fn settle<'js>(ctx: Ctx<'js>, max_ms: f64) -> flux::rquickjs::Result<Promise<'js>> {
  let stepper = stepper(&ctx, "settle()")?;
  if !max_ms.is_finite() || max_ms < 0.0 {
    return Err(Exception::throw_message(
      &ctx,
      "sol:test settle(maxMs): the cap must be a non-negative number of milliseconds",
    ));
  }
  let (promise, resolve, reject) = ctx.promise()?;
  let task_ctx = ctx.clone();
  ctx.spawn(async move {
    let now_ms = || stepper.time_ms();
    let step = || stepper.step();
    let outcome = crate::settle::settle(&task_ctx, Cap::AppTime { max_ms, now_ms: &now_ms }, Some(&step)).await;
    let settled = match outcome {
      Ok(()) => resolve.call::<_, ()>(()),
      Err(left) => {
        let message =
          format!("settle: the app did not come to rest within {max_ms} ms of app time: {}", left.describe());
        Exception::from_message(task_ctx.clone(), &message).and_then(|error| reject.call::<_, ()>((error,)))
      }
    };
    if let Err(e) = settled {
      flux::report_uncaught(&task_ctx, e, "sol:test settle()");
    }
  });
  Ok(promise)
}

/// `frameRate()`: the frames per second this engine steps at.
fn frame_rate(ctx: Ctx<'_>) -> flux::rquickjs::Result<u32> {
  Ok(stepper(&ctx, "frameRate()")?.fps())
}

/// `setFrameRate(fps)`: another rate, before the first frame.
fn set_frame_rate(ctx: Ctx<'_>, fps: f64) -> flux::rquickjs::Result<()> {
  let stepper = stepper(&ctx, "setFrameRate()")?;
  if !fps.is_finite() || fps < 1.0 || fps.fract() != 0.0 || fps > u32::MAX as f64 {
    return Err(Exception::throw_message(
      &ctx,
      "sol:test setFrameRate(fps): the frame rate must be a positive integer",
    ));
  }
  stepper.set_fps(fps as u32).map_err(|e| Exception::throw_message(&ctx, &format!("sol:test setFrameRate(fps): {e}")))
}

/// `time()`: app time as of the last frame, in ms.
fn time(ctx: Ctx<'_>) -> flux::rquickjs::Result<f64> {
  Ok(stepper(&ctx, "time()")?.time_ms())
}

pub struct SolTestModule;

impl ModuleDef for SolTestModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("frame")?;
    decl.declare("frameRate")?;
    decl.declare("setFrameRate")?;
    decl.declare("time")?;
    decl.declare("windowReady")?;
    decl.declare("inputPlan")?;
    decl.declare("inputStep")?;
    decl.declare("settle")?;
    decl.declare("link")?;
    decl.declare("debug")?;
    decl.declare("capture")?;
    decl.declare("painted")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    if ctx.userdata::<PendingFrame>().is_none() {
      ctx.store_userdata(PendingFrame::default()).expect("store pending frame");
    }
    exports.export("frame", Function::new(ctx.clone(), frame)?)?;
    exports.export("frameRate", Function::new(ctx.clone(), frame_rate)?)?;
    exports.export("setFrameRate", Function::new(ctx.clone(), set_frame_rate)?)?;
    exports.export("time", Function::new(ctx.clone(), time)?)?;
    if ctx.userdata::<InputPlan>().is_none() {
      ctx.store_userdata(InputPlan::default()).expect("store input plan");
    }
    exports.export("windowReady", Function::new(ctx.clone(), window_ready_promise)?)?;
    exports.export("inputPlan", Function::new(ctx.clone(), plan_input)?)?;
    exports.export("inputStep", Function::new(ctx.clone(), step_input)?)?;
    exports.export("settle", Function::new(ctx.clone(), settle)?)?;
    if ctx.userdata::<Pads>().is_none() {
      ctx.store_userdata(Pads(Rc::new(RefCell::new(alloy::Gamepads::synthetic())))).expect("store pads");
    }
    exports.export("link", Function::new(ctx.clone(), link)?)?;
    exports.export("debug", Function::new(ctx.clone(), debug)?)?;
    exports.export("capture", Function::new(ctx.clone(), capture)?)?;
    exports.export("painted", Function::new(ctx.clone(), painted)?)?;
    Ok(())
  }
}
