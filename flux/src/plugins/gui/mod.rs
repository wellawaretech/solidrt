// The alloy-backed GUI marshalling layer: rquickjs bindings for the render tree
// and capture devices, kept here (behind the `gui` feature) rather than in
// lattice so any flux + alloy app can use them and a second engine re-binds
// them symmetrically. Generic GUI capabilities only; app-specific features
// (e.g. speech, dev tooling) keep their core + binding in lattice.

pub mod camera;
pub mod microphone;
mod properties;
pub mod raf;
pub mod texture;
pub mod tree;
pub mod value;

use std::sync::mpsc::Sender;
use std::sync::Arc;

use rquickjs::JsLifetime;

use alloy::rendertree::{PlatformContext, RenderTree};
use alloy::AlloyCommand;

use crate::engine::FluxEngineBuilder;

/// JsLifetime wrapper around the shared alloy context, so the gui plugins can
/// hold it in JS userdata. Derefs to `alloy::Context` for the rendering /
/// texture / capture methods the bindings forward to.
#[derive(Clone, JsLifetime)]
pub struct AlloyContext(#[qjs(skip_trace)] pub Arc<alloy::Context>);

impl std::ops::Deref for AlloyContext {
  type Target = alloy::Context;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

/// The host instances the GUI bindings need, owned by the runner (lattice) and
/// lent to flux at engine-build time. flux owns which plugins exist and the
/// order they register in; the runner only supplies the instances. Grows a
/// field per plugin cluster as they move in (render tree, input/engine state,
/// ...).
pub struct GuiHost {
  pub platform: Arc<PlatformContext>,
  pub alloy: Arc<alloy::Context>,
  /// The (freshly created) render tree this engine drives. Moved into the tree
  /// plugin; recreated by the runner on every reload.
  pub render_tree: RenderTree,
  /// Channel to the render thread for tree-driven alloy commands (text input).
  pub alloy_cmd_tx: Sender<AlloyCommand>,
}

/// Register the GUI plugin set onto the engine builder. The single seam the
/// runner calls: it must not need to know individual plugin init functions or
/// their registration order. The tree plugin registers first: it creates the
/// `ffi` global the draw bridge (in the runner) attaches `renderFrame` to.
pub fn install(builder: FluxEngineBuilder, host: GuiHost) -> FluxEngineBuilder {
  let GuiHost { platform, alloy, render_tree, alloy_cmd_tx } = host;
  let tree_platform = platform.clone();
  let texture_platform = platform.clone();
  let tree_atx = AlloyContext(alloy.clone());
  let texture_atx = AlloyContext(alloy.clone());
  let camera_atx = AlloyContext(alloy.clone());
  let microphone_atx = AlloyContext(alloy);
  builder
    .plugin(move |ctx| tree::init(&ctx, render_tree, alloy_cmd_tx, tree_platform, tree_atx))
    .plugin(move |ctx| raf::init(&ctx, platform))
    .plugin(move |ctx| texture::init(ctx, texture_atx, texture_platform))
    .plugin(move |ctx| camera::init(ctx, camera_atx))
    .plugin(move |ctx| microphone::init(ctx, microphone_atx))
}