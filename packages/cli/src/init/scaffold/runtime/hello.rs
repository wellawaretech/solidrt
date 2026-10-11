//! `runtime:hello`, the starter module: one function, `hello()`. The shapes
//! a binding takes (a buffer read in place, async work on another thread,
//! a frame tick, an options object) are in docs/runtime, "A custom runtime".

use lattice::flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use lattice::flux::rquickjs::{Ctx, Function, Result};

/// Registered per engine; `Modules::add` clones it for each.
#[derive(Clone)]
pub struct HelloModule;

impl ModuleDef for HelloModule {
  fn declare(decl: &Declarations<'_>) -> Result<()> {
    decl.declare("hello")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    exports.export("hello", Function::new(ctx.clone(), || "Hello from the runtime".to_string())?)?;
    Ok(())
  }
}
