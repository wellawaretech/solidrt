//! Thread bootstrap: spawns sol-raster (sole owner of the GL context and the renderer
//! contexts, see raster/) and sol-ui (JS, layout, paint; zero GL) and wires
//! the command channel between them.

use crate::backend::{FrameOutput, GlBinding};
use crate::raster::{RasterCmd, RasterState};
use crate::Context;
use glow::HasContext;
use std::cell::Cell;
use std::sync::atomic::AtomicU64;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

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

// The raster thread's command sender, kept so the thread can be stopped (see
// stop_raster). One per process, like the GL context it owns; a run that
// spawns a new one replaces a thread that was never stopped.
static RASTER_THREAD: Mutex<Option<crate::raster::RasterSender>> = Mutex::new(None);

thread_local! {
  // Set on sol-raster itself, read by stop_raster: a stop asked for from the
  // raster thread (its panic hook) must not wait on that thread.
  static IS_RASTER_THREAD: Cell<bool> = const { Cell::new(false) };
}

// How long a stop waits for the raster thread to answer. Normally it is
// inside one frame: a vsync swap or a frame's encode, at most a refresh
// period or two. The bound keeps a hung driver from holding the exit open,
// and with the embedder's quit hook fits Android's close budget (see
// lattice's QUIT_HOOK_DEADLINE).
const RASTER_STOP_DEADLINE: Duration = Duration::from_millis(250);

/// Stop the raster thread: the frame it is in finishes, the commands queued
/// behind it execute, and it parks for the rest of the process with its GL
/// context still current. This call returns once it has parked; nothing is
/// in a GL call after that, which is what a window drop or a process exit
/// needs: either unloads the GL driver, and a GL call still running on it
/// crashes.
///
/// The thread parks instead of ending, and keeps the context current
/// instead of releasing it, on purpose (okf/done/exit-races-raster-thread.md):
/// NVIDIA's Wayland EGL faults in the context's destruction, by
/// eglDestroyContext or by its own exit handler, once the context was
/// released from the thread it was current on (measured; a thread exit
/// releases it too, through the driver's thread-exit destructors). A context
/// current on a parked thread is destroyed by nobody: SDL's destroy defers
/// it, the exit handlers pass it by, and the process end reclaims it.
///
/// A stop before the thread exists, or after one, does nothing; called on
/// the raster thread itself it returns at once, since that thread is the
/// only GL user and nothing races it. A thread that does not answer within
/// RASTER_STOP_DEADLINE is left running, with a warning.
pub fn stop_raster() {
  if IS_RASTER_THREAD.with(Cell::get) {
    return;
  }
  // The lock is held through the stop, so a second stopper waits for the
  // first and then finds the slot empty.
  let mut slot = RASTER_THREAD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(raster) = slot.take() else {
    return;
  };
  let (reply_tx, reply_rx) = mpsc::channel();
  if raster.send(RasterCmd::Stop { reply: reply_tx }).is_err() {
    // The thread already ended on its own (its channel is closed).
    return;
  }
  // Disconnected: the thread ended without answering (a panic); it is not
  // in GL either way.
  if let Err(mpsc::RecvTimeoutError::Timeout) = reply_rx.recv_timeout(RASTER_STOP_DEADLINE) {
    log::warn!("[alloy] the raster thread did not stop within {RASTER_STOP_DEADLINE:?}; going on without it");
  }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_context(
  binding: Box<dyn GlBinding>,
  surface_size: Arc<AtomicU64>,
  closure: impl FnOnce(Arc<Context>) + Send + 'static,
  tx: mpsc::Sender<FrameOutput>,
  wake: Option<Arc<dyn Fn() + Send + Sync>>,
  sink: crate::raster::FrameSink,
  stats: Arc<crate::raster::RasterStats>,
) -> crate::raster::RasterSender {
  let (raster_tx, raster_rx) = mpsc::channel::<RasterCmd>();
  let raster_tx = crate::raster::RasterSender::new(raster_tx, stats.clone());
  // The platform loop's clone, for surface-liveness rebinds (liveness.rs):
  // same ordered channel and queue-depth bookkeeping as the Context's half.
  let main_tx = raster_tx.clone();
  let raster_stats = stats.clone();
  // The Context's clone: a pushed video frame wakes the loop like a present.
  let context_wake = wake.clone();

  // The raster thread: sole owner of the process's single GL context and
  // renderer for the engine's lifetime. The renderer's GLES contract
  // requires exactly this: one context, used only on the thread it was
  // created on. Everything GL arrives over the command channel (raster.rs).
  let spawn_raster = std::thread::Builder::new().name("sol-raster".into()).spawn(move || {
    IS_RASTER_THREAD.with(|flag| flag.set(true));
    // Display priority so background processes cannot preempt a frame
    // mid-flight; see ore::threads::raise.
    ore::threads::raise(ore::threads::Tier::Deadline);
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
    crate::gl::probe_msrtt(&gl, &*binding);
    let renderer = crate::gl::create_renderer(&*binding);
    unsafe {
      let vendor = gl.get_parameter_string(glow::VENDOR);
      let renderer_name = gl.get_parameter_string(glow::RENDERER);
      let version = gl.get_parameter_string(glow::VERSION);
      let limits = crate::gl::query_limits(&gl);
      log::info!("[alloy] GPU ready: {vendor} | {renderer_name} | {version}");
      crate::set_gpu_info(crate::GpuInfo { vendor, renderer: renderer_name, version, limits });
    }

    let state = RasterState::new(gl, renderer, binding, surface_size, sink, raster_stats, tx, wake);
    // Map the window now rather than at the first frame, so a UI thread that
    // never submits one is still visible on Wayland (see prime_window).
    state.prime_window();
    state.run(raster_rx);
  });
  // Detached: the thread never ends (see stop_raster), so there is nothing
  // to join.
  spawn_raster.expect("failed to spawn raster thread");
  *RASTER_THREAD.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(raster_tx.clone());

  // The UI thread: QuickJS, layout, hit-testing, DisplayList building. No GL
  // at all; the Context it gets marshals GPU work over the command channel.
  let spawn_ui = std::thread::Builder::new().name("sol-ui".into()).stack_size(UI_THREAD_STACK_SIZE).spawn(move || {
    // Same display-priority rationale as the raster thread, one tier lower
    // (the raster thread owns the present deadline).
    ore::threads::raise(ore::threads::Tier::Interactive);
    closure(Arc::new(Context::new(raster_tx, stats, context_wake)));
    // The embedder's producer returned: the run loop winds the process
    // down once it notices, so say so here, where the cause lives.
    log::info!("[alloy] UI thread finished: the embedder's producer returned");
  });
  spawn_ui.expect("failed to spawn UI thread");
  main_tx
}
