mod gl;
pub mod sdl_utils;

mod app;
pub mod audio;
mod backend;
pub mod barcode;
pub mod camera;
mod context;
mod egl_headless;
mod event;
mod gamepad;
mod gpu;
mod input;
mod keymap;
mod liveness;
mod logging;
pub mod microphone;
mod mode;
mod playback;
mod present;
mod raster;
pub use raster::RasterCounters;
pub mod rendertree;
pub mod resample;
mod script;
mod texture;
mod threads;
mod vsync;
pub mod yuv;

#[cfg(test)]
mod tests;

pub use impellers;
pub use sdl3;

pub use app::{setup, App};
pub use backend::DisplayContext;
pub use context::{CaptureDone, CaptureInfo, Context, StatsOverlay};
pub use gpu::{
  parse_blend, parse_cull, AttrFormat, BlendMode, CullMode, DepthState, DrawBounds, DrawRange, DrawSpec, DrawUpdate,
  GpuLimits, IndexFormat, ParamValue, PipelineDesc, NodeShader, PipelineSpec, ShaderStage, TargetSpec, Topology,
  UniformKind, UniformSlot, UniformTable, WindowShader,
};
pub use event::{
  AlloyCommand, AlloyEvent, GamepadState, Modifiers, PointerType, TextCapitalization, TextInputOptions, TextInputType,
};
pub use vsync::FramePacing;
pub use input::InputState;
pub use keymap::w3c_code_for_key;
pub use logging::install_logger;
pub use mode::Mode;
pub use playback::PlaybackConfig;
pub use present::PresentClock;
pub use script::{ScriptEvent, ScriptPlayer, ScriptedAction};
pub use texture::{GpuTexture, SamplerState, TextureEntry, TextureFormat, TextureRegistry};
pub use yuv::{YuvLayout, YuvMatrix, YuvRange};

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

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

// Hardware keyboard presence reported by the platform. SDL's Android backend
// never registers keyboards (SDL_HasKeyboard is permanently false there), so
// the platform reports the Configuration fact via JNI, same route as the
// keyboard inset. Elsewhere this stays false and SDL's own device list is
// authoritative; readers OR the two.
static HARDWARE_KEYBOARD: AtomicBool = AtomicBool::new(false);

/// Set whether a hardware keyboard is attached. Called from the platform
/// (Android JNI); thread-safe.
pub fn set_hardware_keyboard(present: bool) {
  HARDWARE_KEYBOARD.store(present, Ordering::Relaxed);
}

/// Whether the platform reported an attached hardware keyboard (false where
/// SDL's own device list covers it; see [`set_hardware_keyboard`]).
pub fn hardware_keyboard() -> bool {
  HARDWARE_KEYBOARD.load(Ordering::Relaxed)
}
