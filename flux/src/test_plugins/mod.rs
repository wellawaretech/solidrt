// The `flux:test` module: `test`, `expect` and `run`, what `srt test` runs a
// test file with (okf/plans/test-harness.md). A fourth plugin layer beside
// standards/forge/alloy: it is no web standard and marshals neither forge
// nor alloy, but flux's own facilities. Behind the `test` feature, which
// only the `flux` binary turns on, so no shipping runtime carries it.
//
// The surface is plain JS (test.js), embedded here and evaluated once per
// context; this file exports what that source returns. The source is a
// function of the natives below, the pieces that need the engine: the seed
// of `Math.random`.

use rquickjs::context::EvalOptions;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, Object, Value};

use crate::standards_plugins::random;

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

/// Restart `Math.random` at the start of `seed`'s sequence (see
/// `random::seed_random`). The JS half hands over a non-negative integer;
/// anything else is refused here too, since a cast would make up a seed.
fn seed_random(ctx: Ctx<'_>, seed: f64) -> rquickjs::Result<()> {
  if !seed.is_finite() || seed < 0.0 || seed.fract() != 0.0 {
    return Err(Exception::throw_message(&ctx, "seedRandom: the seed must be a non-negative integer"));
  }
  random::seed_random(&ctx, seed as u64)
}
