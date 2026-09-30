// The `flux:test` module: `test` and `expect`, what a test file registers
// and asserts with, and `settle` (okf/plans/test-harness.md). A fourth plugin layer beside
// standards/forge/alloy: it is no web standard and marshals neither forge
// nor alloy, but flux's own facilities. Behind the `test` feature, which
// only the `flux` binary turns on, so no shipping runtime carries it.
//
// The surface is plain JS (test.js), embedded here and evaluated once per
// context; this file exports what that source returns beside the native
// `settle`, and keeps the two functions the host runs a file through
// (host.rs): every test of a file
// gets an engine of its own, so the file is evaluated once to list its
// tests and once more for each of them.

#[cfg(feature = "gui")]
pub mod gui;
pub mod host;

use std::rc::Rc;

use rquickjs::context::EvalOptions;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Persistent, Value};

/// The module name, and the file name stack frames inside the harness cite
/// ("at toBe (flux:test:210:13)"), which is how a reporter tells harness
/// frames from the test's own.
pub const MODULE_NAME: &str = "flux:test";

const SOURCE: &str = include_str!("test.js");

const EXPORTS: &[&str] = &["test", "expect"];

/// `settle()`: a promise that fulfills once nothing the engine started is
/// still in flight and the job queue is dry (`PendingOps::settled`). What
/// stands (a server, an open socket, a timer) is not waited for.
fn settle(ctx: Ctx<'_>) -> Promised<impl std::future::Future<Output = ()>> {
  Promised(crate::pending::settled(&ctx))
}

/// What the host calls in an engine whose file has evaluated: `names()`,
/// the tests it registered, and `runOne(name)`, a promise of the one test's
/// error or null. Context userdata, stored when the module is first
/// imported; a file that never imports flux:test has none and no tests.
#[derive(Clone, JsLifetime)]
pub(crate) struct Registry(#[qjs(skip_trace)] Rc<RegistryInner>);

struct RegistryInner {
  names: Persistent<Function<'static>>,
  run_one: Persistent<Function<'static>>,
}

impl Registry {
  pub(crate) fn names<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<Vec<String>> {
    self.0.names.clone().restore(ctx)?.call(())
  }

  pub(crate) fn run_one<'js>(&self, ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<rquickjs::Promise<'js>> {
    self.0.run_one.clone().restore(ctx)?.call((name,))
  }
}

pub struct TestModule;

impl ModuleDef for TestModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    for name in EXPORTS {
      decl.declare(*name)?;
    }
    decl.declare("settle")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    // Registered tests only run under a host. Without one the file would
    // end cleanly with nothing run, which reads as a pass.
    if ctx.userdata::<host::Hosted>().is_none() {
      return Err(Exception::throw_message(
        ctx,
        "flux:test: a test file is run by a test host; use srt test <file> (or flux --test <file>)",
      ));
    }
    let mut options = EvalOptions::default();
    options.filename = Some(MODULE_NAME.to_string());
    let build: Function = ctx.eval_with_options(SOURCE, options)?;
    let api: Object = build.call(())?;
    for name in EXPORTS {
      exports.export(*name, api.get::<_, Value>(*name)?)?;
    }
    exports.export("settle", Function::new(ctx.clone(), settle)?)?;
    let registry = Registry(Rc::new(RegistryInner {
      names: Persistent::save(ctx, api.get::<_, Function>("names")?),
      run_one: Persistent::save(ctx, api.get::<_, Function>("runOne")?),
    }));
    ctx.store_userdata(registry).expect("store test registry");
    Ok(())
  }
}
