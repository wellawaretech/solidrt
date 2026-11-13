pub mod color;
mod gl;

mod app;
pub mod audio;
mod backend;
pub mod barcode;
mod cadence;
pub mod camera;
pub mod clock;
mod context;
mod egl_headless;
mod event;
mod frame_timestamps;
mod gpu;
mod input;
mod liveness;
mod logging;
pub mod microphone;
mod mode;
pub mod motion;
mod present;
mod raster;
pub use raster::{DamageRect, HeadlessPatch, PresentDamage, RasterCounters, TargetCounters};
pub mod renderer;
pub mod rendertree;
pub mod resample;
mod script;
pub mod spatial;
mod stepped;
mod threads;
mod vsync;
mod window_size;
pub mod yuv;

#[cfg(test)]
mod tests;

pub use app::{setup, App};
pub use backend::DisplayContext;
pub use cadence::CadenceHold;
pub use context::{CaptureDone, CaptureInfo, Context, Overlay, WindowTap, YuvFrameSink};
pub use event::{AlloyCommand, AlloyEvent, Arrival, EventSender};
pub use gl::TapRect;
pub use gpu::{
  check_cube_faces, mip_levels, mip_size, DrawListFacts, GpuBufferLayoutInfo, GpuResources, LostContent, SamplerFilter,
  SamplerOptions, SamplerOverride, SamplerState, SamplerUpdate, SamplerWrap, TextureBox, TextureEntry, TextureFormat,
  TextureRect, TextureRegistry, TextureShape, CUBE_FACES, MIN_ANISOTROPY,
};
pub use gpu::{
  parse_blend, parse_cull, AttrFormat, BlendMode, BufferIds, BufferLayout, BufferUpdate, CellOptions, CellSize,
  CullMode, DepthState, DepthStorage, DrawBounds, DrawRange, DrawSpec, DrawUpdate, GpuLimits, IndexFormat,
  InstanceOrder, KeyColumn, KeyOptions, NodeShader, OrderCounters, OrderKey, OrderOptions, OrderView, PackedFormat,
  PackedKey, ParamValue, PipelineDesc, PipelineSpec, RecordExtent, RunTable, Selection, ShaderStage, StepMode,
  TargetSpec, TextureBinding, Topology, UniformKind, UniformSlot, UniformTable, VertexAttr, WindowShader,
  CELL_BOX_FLOATS, INDEX_NONE, MAX_BUFFERS, RUN_RAMP_SAMPLES,
};
pub use gpu::{CoverageMode, CoveragePolicy, GlyphGradient, GlyphGroup, GlyphQuad, GradientKind, RAMP_WIDTH};
pub use gpu::{PxRect, DAMAGE_EDGE_PX, MAX_DAMAGE_SLOTS};
pub use input::InputState;
pub use logging::install_logger;
pub use mode::Mode;
pub use present::{CountTally, Counted, RefreshCounter, RefreshCounting, SignalLedger, SignalRecord};
pub use raster::TapSize;
pub use script::{ScriptEvent, ScriptPlayer, ScriptedAction};
pub use stepped::SteppedConfig;
pub use vsync::FramePacing;
pub use yuv::{YuvLayout, YuvMatrix, YuvRange};

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
// looking at: the SDL video driver (set at setup from the window) and the GL strings
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
// loop at startup and on every rate ore reports, so an out-of-loop reader (the
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

// Whether the window is backgrounded. Set at push time of
// WILL_ENTER_BACKGROUND by the lifecycle callback (app.rs), the last moment the
// platform still listens, and cleared by the raster thread when it runs the
// return-to-visible rebind (liveness.rs decides that; the raster thread's
// RebindWindowSurface arm clears). While set, the raster thread does no
// window work: Android has destroyed the surface, so a present fails with
// EGL_BAD_SURFACE, which is not a context loss, and iOS kills an app that
// touches the GPU in the background. The case is a raster-bound app, whose
// queue still holds frames when the surface goes away: without the flag they
// present into the dead surface, the rebind-and-retry fails the same way, and
// two failures in a row is the context-loss exit (seen live: a 23 fps app
// died 0.4 s after the home press). Desktop never sets it: the event does not
// fire there. A flag rather than a raster command because a command would
// queue behind the very frames that must be dropped.
static WINDOW_BACKGROUNDED: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_window_backgrounded(backgrounded: bool) {
  WINDOW_BACKGROUNDED.store(backgrounded, Ordering::Relaxed);
}

pub(crate) fn window_backgrounded() -> bool {
  WINDOW_BACKGROUNDED.load(Ordering::Relaxed)
}
