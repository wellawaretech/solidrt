// The alloy-backed GUI marshalling layer: rquickjs bindings for the render tree
// and capture devices, kept here (behind the `gui` feature) rather than in
// lattice so any flux + alloy app can use them and a second engine re-binds
// them symmetrically. Generic GUI capabilities only; app-specific features
// (e.g. speech, dev tooling) keep their core + binding in lattice.

pub mod audio;
pub mod camera;
pub mod events;
pub mod frame;
pub mod input;
pub mod microphone;
pub(crate) mod properties;
pub mod raf;
pub mod gpu;
pub mod spatial;
pub mod tree;
pub mod value;
#[cfg(feature = "video")]
pub mod video;

// The read half of the property adapter, for inspection surfaces (the dev
// connection's tree query); the write half stays crate-internal behind
// setProperty.
pub use properties::{read_jsx, ReadValue};

use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use rquickjs::{Array, Ctx, JsLifetime, Object};

use alloy::rendertree::{PlatformContext, RenderTree};
use alloy::AlloyCommand;

use crate::engine::FluxEngineBuilder;

/// The host instances every gui plugin shares, stored once by `install`
/// before any plugin runs: the alloy context (rendering, textures, capture,
/// spatial, media) and the platform handle (frame requests, window facts).
/// A plugin with data of its own holds this by pointer in its state (`gui`);
/// one without reads it through `gui(&ctx)`.
pub(crate) struct Gui {
  pub(crate) alloy: Arc<alloy::Context>,
  pub(crate) platform: Arc<PlatformContext>,
}

#[derive(Clone, JsLifetime)]
struct GuiState(#[qjs(skip_trace)] Rc<Gui>);

pub(crate) fn gui(ctx: &Ctx<'_>) -> Rc<Gui> {
  ctx.userdata::<GuiState>().expect("gui state userdata").0.clone()
}

/// None before the GUI is installed (the per-frame hooks are no-ops then).
pub(crate) fn try_gui(ctx: &Ctx<'_>) -> Option<Rc<Gui>> {
  ctx.userdata::<GuiState>().map(|g| g.0.clone())
}

/// The alloy context for the runner's out-of-frame queries (a dev-server
/// snapshot, the GPU inventory, a texture or buffer read), which run on the
/// JS thread with a `Ctx` in hand. None before the GUI is installed.
pub fn alloy_context(ctx: &Ctx<'_>) -> Option<Arc<alloy::Context>> {
  try_gui(ctx).map(|g| g.alloy.clone())
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
/// their registration order, and per frame it drives them through `frame`
/// (`advance`, `deliver`, `draw`) rather than the plugins' own hooks or the
/// tree itself.
pub fn install(builder: FluxEngineBuilder, host: GuiHost) -> FluxEngineBuilder {
  let GuiHost { platform, alloy, render_tree, alloy_cmd_tx } = host;
  // navigator.clipboard is a web-standard surface (standards_plugins) that
  // marshals alloy commands, so it installs here at the gui seam.
  let clipboard_cmd_tx = alloy_cmd_tx.clone();
  // The render tree and the capture/render devices are all `flux:*` modules
  // (registered below); only the web-standard rAF stays a global. The shared
  // host state goes in first; the plugins with data of their own store it
  // in userdata before any import, and the module surfaces read it in their
  // `evaluate`.
  let builder = builder
    .plugin(move |ctx| {
      ctx.store_userdata(GuiState(Rc::new(Gui { alloy, platform }))).expect("store gui state");
    })
    .plugin(move |ctx| tree::store_state(&ctx, render_tree, alloy_cmd_tx))
    .plugin(|ctx| input::store_state(&ctx))
    .plugin(|ctx| raf::init(&ctx))
    .plugin(|ctx| gpu::store_state(&ctx))
    .plugin(|ctx| camera::store_state(&ctx))
    .plugin(move |ctx| crate::standards_plugins::clipboard::init_clipboard(&ctx, clipboard_cmd_tx))
    .plugin(register_capabilities)
    .module_override("flux:rendertree", tree::RenderTreeModule)
    .module_override("flux:camera", camera::CameraModule)
    .module_override("flux:microphone", microphone::MicrophoneModule)
    .module_override("flux:audio", audio::AudioModule)
    .module_override("flux:gpu", gpu::GpuModule)
    .module_override("flux:spatial", spatial::SpatialModule);
  #[cfg(feature = "video")]
  let builder = builder.plugin(|ctx| video::store_state(&ctx)).module_override("flux:video", video::VideoModule);
  builder
}

/// Capability names the gui feature adds on top of `BASE_CAPABILITIES`.
#[cfg(feature = "video")]
pub const GUI_CAPABILITIES: &[&str] = &["camera", "microphone", "audio", "gpu", "spatial", "video"];
#[cfg(not(feature = "video"))]
pub const GUI_CAPABILITIES: &[&str] = &["camera", "microphone", "audio", "gpu", "spatial"];

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
