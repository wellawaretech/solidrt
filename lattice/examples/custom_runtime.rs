//! A custom runtime: the stock program plus one module, the shape a cargo
//! project over lattice takes (docs/runtime, "Native code"). `runtime:whoami`
//! exports `whoami()`, `frames()` (how many frame ticks the module has seen
//! through the frame protocol, `frame::on_advance`), and the two shapes a
//! module's bindings take with flux's toolkit: `checksum(bytes)` reads a JS
//! buffer in place, `digest(bytes)` is the async binding, a promise over work
//! on another thread with the engine held until it settles.
//!
//!   cargo run -p lattice --example custom_runtime --features go,test -- <app.js>
//!
//! Staged as a project's runtime (`"solidrt": { "runtime": ... }` in its
//! package.json), `sol run` and `sol test` run that project on it.

use std::cell::Cell;
use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Function, TypedArray};
use lattice::Modules;

#[derive(Clone)]
struct WhoamiModule;

impl ModuleDef for WhoamiModule {
  fn declare(decl: &Declarations<'_>) -> flux::rquickjs::Result<()> {
    decl.declare("whoami")?;
    decl.declare("frames")?;
    decl.declare("checksum")?;
    decl.declare("digest")?;
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
    exports.export("checksum", Function::new(ctx.clone(), checksum)?)?;
    exports.export("digest", Function::new(ctx.clone(), digest)?)?;
    Ok(())
  }
}

/// `checksum(bytes: Uint8Array): number`, the sum of the bytes. A binding
/// that takes the context and arguments is a named generic function: a
/// closure's `Ctx<'_>` and value parameters do not unify. The slice is the
/// engine's memory, read here and never kept.
fn checksum<'js>(ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> flux::rquickjs::Result<f64> {
  let data = flux::bytes_of(&ctx, &bytes, "checksum")?;
  Ok(data.iter().map(|&b| b as u64).sum::<u64>() as f64)
}

/// `digest(bytes: Uint8Array): Promise<number>`, the same sum on a thread of
/// its own: the bytes are copied out (the buffer may change once JS runs
/// again), the work is a plain future, and `with_in_flight` makes it the
/// promise, holds the engine as work in flight until it settles, and turns
/// an `Err(String)` into a rejection with that message.
fn digest<'js>(ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> flux::rquickjs::Result<impl flux::rquickjs::IntoJs<'js>> {
  let data = flux::bytes_of(&ctx, &bytes, "digest")?.to_vec();
  Ok(flux::with_in_flight(&ctx, "digest", async move {
    let (send, recv) = flux::tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
      let _ = send.send(data.iter().map(|&b| b as u64).sum::<u64>());
    });
    recv.await.map(|sum| sum as f64).map_err(|_| "digest: the worker ended early".to_string())
  }))
}

fn main() {
  lattice::main(lattice::Program {
    modules: Modules::new().add("runtime:whoami", WhoamiModule),
    // The screens from the checkout's stock program; a custom runtime takes
    // them from the @solidrt/cli package in its node_modules instead.
    bsod: include_str!("../../runtime/resources/bsod/bsod.sol.js"),
    #[cfg(feature = "go")]
    player: include_str!("../../runtime/resources/player/index.sol.js"),
  });
}
