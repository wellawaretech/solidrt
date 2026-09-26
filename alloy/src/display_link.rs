//! macOS swap pacing on a CVDisplayLink. ANGLE over Metal returns from
//! eglSwapBuffers at once whatever the swap interval (measured 2026-09-25 on
//! an M1 at 60 Hz: 1575 presents/s, present 0.0 ms, window in front or
//! display asleep alike), and SDL's own display-link pacing lives on its
//! native CGL path, which the GLES driver alloy forces (gl::configure_opengl)
//! never takes. This is that pacing reproduced one layer up, the way
//! SDL_cocoaopengl.m does it: the link's callback counts ticks and signals a
//! condvar, and the window swap waits for the next tick before it swaps. The
//! swap then blocks once per refresh like Mesa's and D3D11's, and macOS is
//! an ordinary SwapPaced desktop above it (okf/design/frame-timing.md,
//! "Frame signal, macOS").
//!
//! Only the tick count matters here, never the callback's phase: the link
//! calls back mid-period, about one and a half periods before the vsync it
//! prepares for (measured: output vsync 24.7 ms ahead at a 16.67 ms period),
//! which is what made the request/answer vsync chain of vsync.rs (built for
//! the Choreographer, whose callback is the vsync) the wrong fit.
//!
//! CVDisplayLink is deprecated by Apple (macOS 15) for CADisplayLink on
//! NSView/NSWindow/NSScreen and the binding carries that deprecation; it
//! still runs (macOS 26 measured) and SDL paces on it. When it goes, the
//! replacement is CADisplayLink through the same objc2 family, internal to
//! this module.
#![allow(deprecated)]

use std::ptr::NonNull;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use objc2_core_foundation::CFRetained;
use objc2_core_video::{kCVReturnSuccess, CVDisplayLink, CVOptionFlags, CVReturn, CVTimeStamp};

// Upper bound on one wait for a tick: with the display asleep the callbacks
// stop, and the swap goes ahead at this cadence instead of stalling the
// raster thread (and every frame behind it) until the display wakes.
const TICK_WAIT_TIMEOUT: Duration = Duration::from_millis(100);

// The callback's hand-off to the swapping thread: ticks counted, one per
// refresh while the link runs.
struct Ticks {
  count: Mutex<u64>,
  tick: Condvar,
}

/// One running display link, owned by the raster thread's window binding and
/// created there at the first swap. It runs for the binding's life, idle
/// frames included, as SDL's does: the callback is an increment and a notify
/// on CoreVideo's thread, and stopping it from a raster thread that has no
/// idle hook would cost more than it saves.
pub(crate) struct SwapPacer {
  link: CFRetained<CVDisplayLink>,
  ticks: Arc<Ticks>,
}

// The link is a CoreFoundation object driven from CoreVideo's own thread;
// this handle only starts, stops and waits on it, and lives on the raster
// thread inside the (Send) window binding.
unsafe impl Send for SwapPacer {}

impl SwapPacer {
  /// The link over the active displays, running. None (logged) when the
  /// machine has none or CoreVideo refuses; the swap then paces nothing and
  /// the release floor holds the refresh rate (vsync.rs).
  pub(crate) fn start() -> Option<SwapPacer> {
    let mut raw: *mut CVDisplayLink = std::ptr::null_mut();
    let status = unsafe { CVDisplayLink::create_with_active_cg_displays(NonNull::from(&mut raw)) };
    let Some(ptr) = NonNull::new(raw).filter(|_| status == kCVReturnSuccess) else {
      log::warn!("[alloy] no display link on this machine ({status}); the swap paces nothing");
      return None;
    };
    let link = unsafe { CFRetained::from_raw(ptr) };
    let ticks = Arc::new(Ticks { count: Mutex::new(0), tick: Condvar::new() });
    // The callback's reference to the ticks is a leaked Arc: the link lives
    // for the process, so nothing ever reconstructs it.
    let user = Arc::into_raw(ticks.clone()) as *mut core::ffi::c_void;
    let status = unsafe { link.set_output_callback(Some(output), user) };
    if status != kCVReturnSuccess {
      log::warn!("[alloy] display link callback rejected ({status}); the swap paces nothing");
      return None;
    }
    let status = link.start();
    if status != kCVReturnSuccess {
      log::warn!("[alloy] display link start failed ({status}); the swap paces nothing");
      return None;
    }
    log::info!("[alloy] swap paced on the display link");
    Some(SwapPacer { link, ticks })
  }

  /// Block until the next tick after this call - any tick, the swap interval
  /// being 1 ("always wait here so we know we just hit a swap interval", as
  /// SDL puts it) - or until TICK_WAIT_TIMEOUT passes without one.
  pub(crate) fn wait_tick(&self) {
    let guard = self.ticks.count.lock().expect("display link tick mutex poisoned");
    let seen = *guard;
    let _ = self
      .ticks
      .tick
      .wait_timeout_while(guard, TICK_WAIT_TIMEOUT, |count| *count == seen)
      .expect("display link tick mutex poisoned");
  }
}

impl Drop for SwapPacer {
  fn drop(&mut self) {
    self.link.stop();
  }
}

// Runs on CoreVideo's display-link thread, once per refresh while the link
// runs.
unsafe extern "C-unwind" fn output(
  _link: NonNull<CVDisplayLink>,
  _now: NonNull<CVTimeStamp>,
  _output_time: NonNull<CVTimeStamp>,
  _flags_in: CVOptionFlags,
  _flags_out: NonNull<CVOptionFlags>,
  user: *mut core::ffi::c_void,
) -> CVReturn {
  let ticks = unsafe { &*(user as *const Ticks) };
  *ticks.count.lock().expect("display link tick mutex poisoned") += 1;
  ticks.tick.notify_all();
  kCVReturnSuccess
}
