use std::cell::RefCell;
use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Exception, Function, JsLifetime, Persistent, Promise};

use crate::test_host::Stepper;

// The `srt:test` module: the engine verbs of an app test, which
// `@solidrt/core/test` builds the test surface on. Thin FFI over the
// stepper (test_host.rs). In the dev client only, and its verbs work only
// in an engine a test host built.

/// The frame a test asked for and is waiting on: the resolver of its
/// promise, called once the frame verb has run the frame, and the hold
/// that keeps the engine alive until then (the frame's signal travels
/// through the runner's loop, outside the engine).
#[derive(Clone, Default, JsLifetime)]
struct PendingFrame(#[qjs(skip_trace)] Rc<RefCell<Option<(Persistent<Function<'static>>, flux::EngineHold)>>>);

fn stepper(ctx: &Ctx<'_>, verb: &str) -> flux::rquickjs::Result<Stepper> {
  match ctx.userdata::<Stepper>() {
    Some(stepper) => Ok((*stepper).clone()),
    None => Err(Exception::throw_message(
      ctx,
      &format!("srt:test {verb}: an app test is run by a test host; use srt test <file>"),
    )),
  }
}

/// `frame()`: run one frame; the promise fulfills once it has run.
fn frame<'js>(ctx: Ctx<'js>) -> flux::rquickjs::Result<Promise<'js>> {
  let stepper = stepper(&ctx, "frame()")?;
  let pending = ctx.userdata::<PendingFrame>().expect("pending frame installed").clone();
  if pending.0.borrow().is_some() {
    return Err(Exception::throw_message(
      &ctx,
      "srt:test frame(): the previous frame has not finished; await it first",
    ));
  }
  let (promise, resolve, _reject) = ctx.promise()?;
  *pending.0.borrow_mut() = Some((Persistent::save(&ctx, resolve), flux::hold_engine(&ctx)));
  stepper.step();
  Ok(promise)
}

/// The frame verb ran a frame: settle the promise of the test that asked
/// for it. No-op when nobody did (the module was never imported).
pub(crate) fn frame_done(ctx: &Ctx<'_>) {
  let Some(pending) = ctx.userdata::<PendingFrame>() else {
    return;
  };
  let Some((resolve, _hold)) = pending.0.borrow_mut().take() else {
    return;
  };
  let settled = resolve.restore(ctx).and_then(|resolve| resolve.call::<_, ()>(()));
  if let Err(e) = settled {
    flux::report_uncaught(ctx, e, "srt:test frame()");
  }
}

/// `frameRate()`: the frames per second this engine steps at.
fn frame_rate(ctx: Ctx<'_>) -> flux::rquickjs::Result<u32> {
  Ok(stepper(&ctx, "frameRate()")?.rate())
}

/// `setFrameRate(fps)`: another rate, before the first frame.
fn set_frame_rate(ctx: Ctx<'_>, fps: f64) -> flux::rquickjs::Result<()> {
  let stepper = stepper(&ctx, "setFrameRate()")?;
  if !fps.is_finite() || fps < 1.0 || fps.fract() != 0.0 || fps > u32::MAX as f64 {
    return Err(Exception::throw_message(
      &ctx,
      "srt:test setFrameRate(fps): the frame rate must be a positive integer",
    ));
  }
  stepper.set_rate(fps as u32).map_err(|e| Exception::throw_message(&ctx, &format!("srt:test setFrameRate(fps): {e}")))
}

/// `time()`: app time as of the last frame, in ms.
fn time(ctx: Ctx<'_>) -> flux::rquickjs::Result<f64> {
  Ok(stepper(&ctx, "time()")?.time_ms())
}

pub struct SrtTestModule;

impl ModuleDef for SrtTestModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("frame")?;
    decl.declare("frameRate")?;
    decl.declare("setFrameRate")?;
    decl.declare("time")?;
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
    Ok(())
  }
}
