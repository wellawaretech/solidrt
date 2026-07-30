// The alloy-backed GUI marshalling layer: rquickjs bindings for the render tree
// and capture devices, kept here (behind the `gui` feature) rather than in
// lattice so any flux + alloy app can use them and a second engine re-binds
// them symmetrically. Generic GUI capabilities only; app-specific features
// (e.g. speech, dev tooling) keep their core + binding in lattice.

pub mod audio;
pub mod camera;
pub mod events;
pub mod input;
pub mod microphone;
mod properties;
pub mod raf;
pub mod gpu;
pub mod tree;
pub mod value;

use std::sync::mpsc::Sender;
use std::sync::Arc;

use rquickjs::{Array, Ctx, JsLifetime, Object};

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
/// field per plugin cluster as they move in - but only for clusters that need
/// host instances (pointer input, for example, is self-contained and has none).
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
/// their registration order. The tree plugin stores the shared render tree in
/// userdata, which the runner's draw bridge (`srt:render`) reads to draw it.
pub fn install(builder: FluxEngineBuilder, host: GuiHost) -> FluxEngineBuilder {
  let GuiHost { platform, alloy, render_tree, alloy_cmd_tx } = host;
  let tree_platform = platform.clone();
  let raf_platform = platform.clone();
  let gpu_platform = platform;
  let tree_atx = AlloyContext(alloy.clone());
  let gpu_atx = AlloyContext(alloy.clone());
  let camera_atx = AlloyContext(alloy.clone());
  let microphone_atx = AlloyContext(alloy.clone());
  // Stored as standalone userdata (below) so the runner can reach the alloy
  // context off the JS thread's `Ctx` - e.g. to service a dev-server snapshot
  // query - the way it reaches `SharedRenderTree` for a tree query.
  let query_atx = AlloyContext(alloy.clone());
  let audio_atx = AlloyContext(alloy);
  // The render tree and the capture/render devices are all `flux:*` modules
  // (registered below); only the web-standard rAF stays a global. The plugins
  // store each module's host state in userdata before any import; the module
  // surfaces read it in their `evaluate`.
  builder
    .plugin(move |ctx| tree::store_state(&ctx, render_tree, alloy_cmd_tx, tree_platform, tree_atx))
    .plugin(move |ctx| {
      ctx.store_userdata(query_atx).expect("store alloy context userdata");
    })
    .plugin(|ctx| input::store_state(&ctx))
    .plugin(move |ctx| raf::init(&ctx, raf_platform))
    .plugin(move |ctx| gpu::store_state(&ctx, gpu_atx, gpu_platform))
    .plugin(move |ctx| camera::store_state(&ctx, camera_atx))
    .plugin(move |ctx| microphone::store_state(&ctx, microphone_atx))
    .plugin(move |ctx| audio::store_state(&ctx, audio_atx))
    .plugin(register_capabilities)
    .module_override("flux:rendertree", tree::RenderTreeModule)
    .module_override("flux:camera", camera::CameraModule)
    .module_override("flux:microphone", microphone::MicrophoneModule)
    .module_override("flux:audio", audio::AudioModule)
    .module_override("flux:gpu", gpu::GpuModule)
}

/// Capability names the gui feature adds on top of `BASE_CAPABILITIES`.
pub const GUI_CAPABILITIES: &[&str] = &["camera", "microphone", "audio", "gpu"];

/// Append the gui capability names to `Flux.capabilities` so availability checks
/// are uniform with the other modules (`Flux.capabilities.includes("camera")`).
/// Runs as a plugin (after `Flux` is created) and only on a gui build, since
/// `install` is the gui-feature seam.
fn register_capabilities(ctx: Ctx<'_>) {
  let Ok(flux) = ctx.globals().get::<_, Object>("Flux") else {
    return;
  };
  let Ok(caps) = flux.get::<_, Array>("capabilities") else {
    return;
  };
  for name in GUI_CAPABILITIES {
    let _ = caps.set(caps.len(), *name);
  }
}
