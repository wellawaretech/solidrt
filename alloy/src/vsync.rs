// Vsync source for frame pacing: a platform backend that arms one display
// vsync callback per request() and answers each with one signal, so the main
// loop can defer frame signals to the display's clock. One-shot re-arm keeps
// the demand-driven contract: no request, no armed callback, no wakeups while
// idle. start() returns None on platforms without a backend; the main loop
// then keeps present-return pacing.
//
// Backends: Android (AChoreographer, below). iOS would slot in here via
// SDL_SetiOSAnimationCallback (CADisplayLink) when iOS support lands. This
// module is the only ndk / ndk-sys consumer in the tree; if SDL ships its own
// choreographer API (libsdl-org/SDL#15013, milestone 3.8.0), reimplement the
// Android backend on that and drop both deps - callers only speak
// request()/try_take().

use std::sync::mpsc;
use std::time::Duration;

pub struct VsyncSource {
  req_tx: mpsc::Sender<Duration>,
  signal_rx: mpsc::Receiver<()>,
}

impl VsyncSource {
  /// Start the platform vsync backend, or None if this platform has none.
  /// `wake` runs on the backend thread after each signal is queued, so a main
  /// loop blocked on the SDL event queue notices (same pattern as the raster
  /// thread's post-present wake).
  #[allow(unused_variables)]
  pub fn start(wake: impl Fn() + Send + 'static) -> Option<VsyncSource> {
    #[cfg(target_os = "android")]
    {
      let (req_tx, req_rx) = mpsc::channel::<Duration>();
      let (signal_tx, signal_rx) = mpsc::channel::<()>();
      std::thread::Builder::new()
        .name("srt-vsync".into())
        .spawn(move || android::run(req_rx, signal_tx, wake))
        .expect("Failed to spawn vsync thread");
      Some(VsyncSource { req_tx, signal_rx })
    }
    #[cfg(not(target_os = "android"))]
    None
  }

  /// Arm one frame callback; the matching signal arrives via `try_take` after
  /// `wake`, `delay` after the vsync fires. The delay exists because the
  /// platform delivers the vsync's input batch on its own thread in parallel
  /// with the callback: a signal emitted at the vsync itself races that input
  /// and loses often (measured: 37-44 frames drawn per 60 moves), wasting the
  /// frame on not-yet-dirty state. Requests sent while one is being served
  /// coalesce into it.
  pub fn request(&self, delay: Duration) {
    self.req_tx.send(delay).ok();
  }

  /// Take one queued vsync signal, if any.
  pub fn try_take(&self) -> bool {
    self.signal_rx.try_recv().is_ok()
  }
}

#[cfg(target_os = "android")]
mod android {
  use std::cell::Cell;
  use std::sync::mpsc;

  pub fn run(req_rx: mpsc::Receiver<std::time::Duration>, signal_tx: mpsc::Sender<()>, wake: impl Fn()) {
    // The choreographer instance is per-thread and requires that thread to
    // own a looper; callbacks are dispatched from this thread's poll calls.
    let looper = ndk::looper::ThreadLooper::prepare();
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
      log::warn!("[alloy] no choreographer on this device; vsync pacing disabled");
    }
    let fired = Cell::new(false);
    while let Ok(mut delay) = req_rx.recv() {
      // A burst of requests collapses into one callback: the consumer flushes
      // everything pending on each signal.
      while let Ok(d) = req_rx.try_recv() {
        delay = d;
      }
      if !choreographer.is_null() {
        fired.set(false);
        unsafe {
          ndk_sys::AChoreographer_postFrameCallback(
            choreographer,
            Some(frame_callback),
            &fired as *const Cell<bool> as *mut core::ffi::c_void,
          );
        }
        while !fired.get() {
          if looper.poll_once().is_err() {
            // A broken looper cannot dispatch the callback; answer now
            // instead of stalling frame production (= present-return pacing).
            log::warn!("[alloy] vsync looper poll failed; answering without vsync");
            break;
          }
        }
        if fired.get() && !delay.is_zero() {
          std::thread::sleep(delay);
        }
      }
      if signal_tx.send(()).is_err() {
        return;
      }
      wake();
    }
  }

  // Runs on the vsync thread, inside looper.poll_once(). The frame timestamp
  // is deliberately unused for now: the frame signal is emitted (and timed)
  // sub-millisecond later on the main loop, and the paced clock models time
  // as frame counts, not wall-clock samples.
  unsafe extern "C" fn frame_callback(_frame_time_nanos: core::ffi::c_long, data: *mut core::ffi::c_void) {
    (*(data as *const Cell<bool>)).set(true);
  }
}
