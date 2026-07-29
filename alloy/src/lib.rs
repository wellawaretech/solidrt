mod gl;
pub mod sdl_utils;

mod app;
pub mod audio;
mod backend;
pub mod barcode;
pub mod camera;
mod context;
mod event;
mod gamepad;
mod keymap;
mod logging;
pub mod microphone;
mod mode;
mod playback;
mod raster;
pub mod rendertree;
mod script;
mod shader;
mod texture;
mod vsync;

#[cfg(test)]
mod tests;

pub use impellers;
pub use sdl3;

pub use app::{setup, App};
pub use backend::{Backend, DisplayContext};
pub use context::{CaptureDone, CaptureInfo, Context, PipelineSpec, TargetSpec, WindowShader};
pub use shader::{parse_blend, AttrFormat, BlendMode, DepthState, ParamValue, PipelineDesc, ShaderStage, Topology};
pub use event::{AlloyCommand, AlloyEvent, GamepadState, Modifiers, PointerType};
pub use logging::install_logger;
pub use mode::Mode;
pub use playback::PlaybackConfig;
pub use script::{ScriptEvent, ScriptPlayer, ScriptedAction};
pub use texture::{GpuTexture, SamplerState, TextureEntry, TextureRegistry};

use std::sync::atomic::{AtomicI32, Ordering};

// Soft-keyboard (IME) inset height in raw pixels. On Android the platform
// reports it via JNI (the cdylib re-exports a symbol that calls the setter);
// elsewhere it stays 0. The event loop reads it each iteration and emits a
// KeyboardVisibility change when it moves.
static KEYBOARD_INSET_PX: AtomicI32 = AtomicI32::new(0);

/// Set the current soft-keyboard inset in raw pixels. Called from the platform
/// (Android JNI); thread-safe.
pub fn set_keyboard_inset_px(px: i32) {
  KEYBOARD_INSET_PX.store(px.max(0), Ordering::Relaxed);
}

/// Current soft-keyboard inset in raw pixels (0 when hidden or unsupported).
pub fn keyboard_inset_px() -> i32 {
  KEYBOARD_INSET_PX.load(Ordering::Relaxed)
}
