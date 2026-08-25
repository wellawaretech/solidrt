//! Thread bootstrap: spawns srt-raster (sole owner of the GL and Impeller
//! contexts, see raster/) and srt-ui (JS, layout, paint; zero GL) and wires
//! the command channel between them.

use crate::backend::{FrameOutput, GlBinding};
use crate::raster::{RasterCmd, RasterState};
use crate::Context;
use glow::HasContext;
use std::sync::atomic::AtomicU64;
use std::sync::{mpsc, Arc};

// Native stack for the UI/JS thread. Large so deep JS recursion behaves the
// same everywhere (the SDL main thread's stack is irrelevant: the engine runs
// here, not there). This is virtual address space, committed only as it is
// used; it is the hard ceiling under which QuickJS's own (smaller, tunable)
// soft limit sits.
//
// 32-bit targets get a much smaller reservation: a 32-bit process does not
// have enough address space left to reserve a contiguous 1GB stack once its
// libraries are loaded, and pthread_create simply fails (observed on a 32-bit
// Android device). 64MB is still far beyond plausible JS recursion depth.
#[cfg(target_pointer_width = "64")]
const UI_THREAD_STACK_SIZE: usize = 1024 * 1024 * 1024;
#[cfg(target_pointer_width = "32")]
const UI_THREAD_STACK_SIZE: usize = 64 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_context(
  binding: Box<dyn GlBinding>,
  surface_size: Arc<AtomicU64>,
  closure: impl FnOnce(Arc<Context>) + Send + 'static,
  tx: mpsc::Sender<FrameOutput>,
  wake: Option<Box<dyn Fn() + Send + Sync>>,
  capture_frames: bool,
  stats: Arc<crate::raster::RasterStats>,
) -> crate::raster::RasterSender {
  let (raster_tx, raster_rx) = mpsc::channel::<RasterCmd>();
  let raster_tx = crate::raster::RasterSender::new(raster_tx, stats.clone());
  // The platform loop's clone, for surface-liveness rebinds (liveness.rs):
  // same ordered channel and queue-depth bookkeeping as the Context's half.
  let main_tx = raster_tx.clone();
  let raster_stats = stats.clone();

  // The raster thread: sole owner of the process's single GL context and
  // Impeller context for the engine's lifetime. Impeller's GLES contract
  // requires exactly this: one context, used only on the thread it was
  // created on. Everything GL arrives over the command channel (raster.rs).
  let spawn_raster = std::thread::Builder::new().name("srt-raster".into()).spawn(move || {
    // Display priority so background processes cannot preempt a frame
    // mid-flight; see sdl_utils::frame_thread_priority.
    crate::sdl_utils::frame_thread_priority(true);
    assert!(binding.bind(), "GL make-current failed on raster thread: {}", binding.error());
    // The swap interval belongs to the current-context binding, so it must be
    // set on this thread, not where the context was created. Blocking this
    // thread in the vsync wait is the point: the UI thread stays free to
    // build the next frame and dispatch input. Playback never swaps, so the
    // setting is inert there.
    if !binding.set_swap_interval() {
      log::warn!("[alloy] set swap interval failed: {}", binding.error());
    }

    let gl = crate::gl::create_gl_context(&*binding);
    let impeller_ctx = crate::gl::create_impeller_context(&*binding);
    unsafe {
      let vendor = gl.get_parameter_string(glow::VENDOR);
      let renderer = gl.get_parameter_string(glow::RENDERER);
      let version = gl.get_parameter_string(glow::VERSION);
      log::info!("[alloy] GPU ready: {vendor} | {renderer} | {version}");
      crate::set_gpu_info(crate::GpuInfo { vendor, renderer, version });
    }

    let state = RasterState::new(gl, impeller_ctx, binding, surface_size, capture_frames, raster_stats, tx, wake);
    // Map the window now rather than at the first frame, so a UI thread that
    // never submits one is still visible on Wayland (see prime_window).
    state.prime_window();
    state.run(raster_rx);
  });
  spawn_raster.expect("failed to spawn raster thread");

  // The UI thread: QuickJS, layout, hit-testing, DisplayList building. No GL
  // at all; the Context it gets marshals GPU work over the command channel.
  let spawn_ui = std::thread::Builder::new().name("srt-ui".into()).stack_size(UI_THREAD_STACK_SIZE).spawn(move || {
    // Same display-priority rationale as the raster thread, one tier lower
    // (the raster thread owns the present deadline).
    crate::sdl_utils::frame_thread_priority(false);
    closure(Arc::new(Context::new(raster_tx, stats)));
  });
  spawn_ui.expect("failed to spawn UI thread");
  main_tx
}
