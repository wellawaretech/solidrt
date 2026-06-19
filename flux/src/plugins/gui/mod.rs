// The alloy-backed GUI marshalling layer: rquickjs bindings for the render tree
// and capture devices, kept here (behind the `gui` feature) rather than in
// lattice so any flux + alloy app can use them and a second engine re-binds
// them symmetrically. Generic GUI capabilities only; app-specific features
// (e.g. speech, dev tooling) keep their core + binding in lattice.

pub mod raf;

use std::sync::Arc;

use alloy::rendertree::PlatformContext;

use crate::engine::FluxEngineBuilder;

/// The host instances the GUI bindings need, owned by the runner (lattice) and
/// lent to flux at engine-build time. flux owns which plugins exist and the
/// order they register in; the runner only supplies the instances. Grows a
/// field per plugin cluster as they move in (render tree, alloy context,
/// input/engine state, ...).
pub struct GuiHost {
  pub platform: Arc<PlatformContext>,
}

/// Register the GUI plugin set onto the engine builder. The single seam the
/// runner calls: it must not need to know individual plugin init functions or
/// their registration order.
pub fn install(builder: FluxEngineBuilder, host: GuiHost) -> FluxEngineBuilder {
  let GuiHost { platform } = host;
  builder.plugin(move |ctx| raf::init(&ctx, platform))
}