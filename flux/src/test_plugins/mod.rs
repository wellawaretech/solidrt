// The `flux:test` module: `test`, `expect` and `run`, what `srt test` runs a
// test file with (okf/plans/test-harness.md). A fourth plugin layer beside
// standards/forge/alloy: it is no web standard and marshals neither forge
// nor alloy, but flux's own facilities. Behind the `test` feature, which
// only the `flux` binary turns on, so no shipping runtime carries it.
//
// The surface is plain JS (test.js), embedded here and evaluated once per
// context; this file exports what that source returns. The source is a
// function of the natives below, the pieces that need the engine: the
// stepped clock over flux's virtual timers, a timer that stays on the wall
// clock while a test steps time, and the seed of `Math.random`.

use rquickjs::context::EvalOptions;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, Object, Promise, Value};

use crate::pending::PendingOps;
use crate::standards_plugins::{random, time};

/// The module name, and the file name stack frames inside the harness cite
/// ("at toBe (flux:test:210:13)"), which is how a reporter tells harness
/// frames from the test's own.
pub const MODULE_NAME: &str = "flux:test";

const SOURCE: &str = include_str!("test.js");

const EXPORTS: &[&str] = &["test", "expect", "run"];

pub struct TestModule;

impl ModuleDef for TestModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    for name in EXPORTS {
      decl.declare(*name)?;
    }
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let native = Object::new(ctx.clone())?;
    native.set("installClock", Function::new(ctx.clone(), install_clock)?)?;
    native.set("uninstallClock", Function::new(ctx.clone(), uninstall_clock)?)?;
    native.set("clockNow", Function::new(ctx.clone(), clock_now)?)?;
    native.set("nextDeadline", Function::new(ctx.clone(), next_deadline)?)?;
    native.set("advanceTo", Function::new(ctx.clone(), advance_to)?)?;
    native.set("turn", Function::new(ctx.clone(), turn)?)?;
    native.set("wallTimeout", Function::new(ctx.clone(), wall_timeout)?)?;
    native.set("seedRandom", Function::new(ctx.clone(), seed_random)?)?;

    let mut options = EvalOptions::default();
    options.filename = Some(MODULE_NAME.to_string());
    let build: Function = ctx.eval_with_options(SOURCE, options)?;
    let api: Object = build.call((native,))?;
    for name in EXPORTS {
      exports.export(*name, api.get::<_, Value>(*name)?)?;
    }
    Ok(())
  }
}

/// Put the context's timers on a virtual timeline that starts at 0.
fn install_clock(ctx: Ctx<'_>) {
  time::install_virtual_time(&ctx, 0.0);
}

/// Hand the timers back to the wall clock; what still waited is dropped.
fn uninstall_clock(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  if time::uninstall_virtual_time(&ctx) {
    Ok(())
  } else {
    Err(Exception::throw_message(&ctx, "clock: cannot end a stepped test from inside a timer callback"))
  }
}

/// The virtual time, or undefined outside a stepped test.
fn clock_now(ctx: Ctx<'_>) -> Option<f64> {
  time::virtual_now(&ctx)
}

/// The earliest waiting timer's deadline, or undefined when none waits.
fn next_deadline(ctx: Ctx<'_>) -> Option<f64> {
  time::next_virtual_deadline(&ctx)
}

/// Move the virtual time to `now_ms` and fire what is due: one task-queue
/// turn (see `advance_virtual_time`).
fn advance_to(ctx: Ctx<'_>, now_ms: f64) {
  time::advance_virtual_time(&ctx, now_ms);
}

/// A promise that resolves once the job queue has run dry: the boundary
/// between two task-queue turns, so that everything the timers of one
/// turn set in motion (promise chains of any depth) has run before the
/// clock fires the next. No JS primitive gives that: a resolved promise
/// yields one microtask, and `setTimeout` is virtual while time is stepped.
fn turn<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Promise<'js>> {
  let (promise, resolve, _reject) = Promise::new(&ctx)?;
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  pending.hold();
  let task_ctx = ctx.clone();
  ctx.spawn(async move {
    while task_ctx.execute_pending_job() {}
    let _ = resolve.call::<_, ()>(());
    pending.release();
  });
  Ok(promise)
}

/// `setTimeout` on the wall clock whatever the context's timeline; the id
/// is one `clearTimeout` takes. A negative or non-finite delay is 0.
fn wall_timeout<'js>(ctx: Ctx<'js>, callback: Function<'js>, ms: f64) -> u32 {
  let ms = if ms.is_finite() { ms.max(0.0) as u64 } else { 0 };
  time::set_wall_timeout(&ctx, callback, ms)
}

/// Restart `Math.random` at the start of `seed`'s sequence (see
/// `random::seed_random`). The JS half hands over a non-negative integer;
/// anything else is refused here too, since a cast would make up a seed.
fn seed_random(ctx: Ctx<'_>, seed: f64) -> rquickjs::Result<()> {
  if !seed.is_finite() || seed < 0.0 || seed.fract() != 0.0 {
    return Err(Exception::throw_message(&ctx, "seedRandom: the seed must be a non-negative integer"));
  }
  random::seed_random(&ctx, seed as u64)
}
