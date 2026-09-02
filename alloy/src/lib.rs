pub mod color;
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
pub mod motion;
mod playback;
mod present;
mod raster;
pub use raster::{DamageRect, PresentDamage, RasterCounters};
pub mod rendertree;
pub mod resample;
mod script;
pub mod spatial;
mod threads;
mod vsync;
pub mod yuv;

#[cfg(test)]
mod tests;

pub use impellers;
pub use sdl3;

pub use app::{setup, App};
pub use backend::DisplayContext;
pub use context::{CaptureDone, CaptureInfo, Context, Overlay};
pub use event::{
  AlloyCommand, AlloyEvent, GamepadState, Modifiers, PointerType, TextCapitalization, TextInputOptions, TextInputType,
};
pub use gpu::{
  parse_blend, parse_cull, AttrFormat, BlendMode, BufferIds, BufferUpdate, CullMode, DepthState, DepthStorage,
  DrawBounds, DrawRange, DrawSpec, DrawUpdate, GpuLimits, IndexFormat, InstanceOrder, NodeShader, OrderKey,
  ParamValue, PipelineDesc, PipelineSpec, ShaderStage, TargetSpec, TextureBinding, Topology, UniformKind,
  UniformSlot, UniformTable, WindowShader, MAX_INSTANCE_SLOTS,
};
pub use gpu::{
  GpuTexture, SamplerFilter, SamplerOptions, SamplerOverride, SamplerState, TextureEntry, TextureFormat, TextureRegistry,
};
pub use input::InputState;
pub use keymap::w3c_code_for_key;
pub use logging::install_logger;
pub use mode::Mode;
pub use playback::PlaybackConfig;
pub use present::PresentClock;
pub use script::{ScriptEvent, ScriptPlayer, ScriptedAction};
pub use vsync::FramePacing;
pub use yuv::{YuvLayout, YuvMatrix, YuvRange};

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::OnceLock;

/// The GPU behind the process's GL context, as GL names it, with the device
/// ceilings it reported (the same GpuLimits every create validates against).
#[derive(Clone, Debug)]
pub struct GpuInfo {
  pub vendor: String,
  pub renderer: String,
  pub version: String,
  pub limits: gpu::GpuLimits,
}

// Platform facts fixed at startup, for a dev tool asking what machine it is
// looking at: the SDL video driver (set by setup_video) and the GL strings
// (set by the raster thread once its context exists). Each is set once and
// read from any thread; a reader that comes first sees None.
static VIDEO_DRIVER: OnceLock<String> = OnceLock::new();
static GPU_INFO: OnceLock<GpuInfo> = OnceLock::new();

pub(crate) fn set_video_driver(name: String) {
  let _ = VIDEO_DRIVER.set(name);
}

/// The SDL video driver in use ("wayland", "x11", "android", ...), once the
/// window exists.
pub fn video_driver() -> Option<&'static str> {
  VIDEO_DRIVER.get().map(String::as_str)
}

pub(crate) fn set_gpu_info(info: GpuInfo) {
  let _ = GPU_INFO.set(info);
}

/// The GPU strings, once the raster thread has its GL context.
pub fn gpu_info() -> Option<&'static GpuInfo> {
  GPU_INFO.get()
}

// The display's nominal refresh rate (f32 bits), published by the event
// loop each time it queries the display mode, so an out-of-loop reader (the
// dev-server client info) can report it; 0 until the window exists.
static REFRESH_RATE: AtomicU32 = AtomicU32::new(0);

pub(crate) fn set_refresh_rate(hz: f32) {
  REFRESH_RATE.store(hz.to_bits(), Ordering::Relaxed);
}

/// The display's nominal refresh rate in Hz as SDL reports the mode, once
/// the window exists.
pub fn refresh_rate() -> Option<f32> {
  let hz = f32::from_bits(REFRESH_RATE.load(Ordering::Relaxed));
  (hz > 0.0).then_some(hz)
}

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
