//! A custom runtime: the stock program plus one module, the shape a cargo
//! project over lattice takes (docs/runtime, "Native code"). `flux:whoami`
//! exports `whoami()` and `frames()`, how many frame ticks the module has
//! seen through the frame protocol (`frame::on_advance`).
//!
//!   cargo run -p lattice --example custom_runtime --features go,test -- <app.js>
//!
//! Staged as a project's runtime (`"solidrt": { "runtime": ... }` in its
//! package.json), `sol run` and `sol test` run that project on it.

use std::cell::Cell;
use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Function};
use lattice::Modules;

#[derive(Clone)]
struct WhoamiModule;

impl ModuleDef for WhoamiModule {
  fn declare(decl: &Declarations<'_>) -> flux::rquickjs::Result<()> {
    decl.declare("whoami")?;
    decl.declare("frames")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    let frames = Rc::new(Cell::new(0u64));
    let counted = frames.clone();
    flux::gui::frame::on_advance(ctx, "whoami", move |_ctx, _now_ms| {
      counted.set(counted.get() + 1);
      false
    });
    exports.export("whoami", Function::new(ctx.clone(), || "World".to_string())?)?;
    exports.export("frames", Function::new(ctx.clone(), move || frames.get() as f64)?)?;
    Ok(())
  }
}

fn main() {
  lattice::main(Modules::new().add("flux:whoami", WhoamiModule));
}
