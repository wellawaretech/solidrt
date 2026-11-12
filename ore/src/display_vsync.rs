//! The display's vsync as a source: one armed callback per wait, answered
//! with the instant of the vsync it fired at. alloy's frame pacing builds
//! its request/answer protocol on this (alloy/src/vsync.rs); ore only arms
//! and waits.
//!
//! The backend is Android's Choreographer, the only one today. iOS would be
//! a second one here (CADisplayLink, through SDL_SetiOSAnimationCallback).
//! Every other platform has none: `available` is false there and `open`
//! answers None.
//! If SDL ships its own choreographer API (libsdl-org/SDL#15013, milestone
//! 3.8.0), reimplement the Android backend on that and drop ndk / ndk-sys
//! where nothing else uses them; callers only speak `open` / `wait_vsync`.

use std::marker::PhantomData;
use std::time::Instant;

/// Whether this platform has a display vsync source (Android's
/// Choreographer).
pub fn available() -> bool {
  cfg!(target_os = "android")
}

/// A vsync source bound to the thread that opened it; not `Send`, since
/// the choreographer is per thread and dispatches only on that thread.
pub struct Source {
  #[cfg(target_os = "android")]
  inner: android::Choreographer,
  _not_send: PhantomData<*const ()>,
}

impl Source {
  /// Open the source on this thread, which it binds (the choreographer is
  /// per thread and needs the thread to own a looper). None on a platform
  /// without a source, and, warned once, on a device without a
  /// choreographer.
  pub fn open() -> Option<Source> {
    #[cfg(target_os = "android")]
    {
      android::Choreographer::open().map(|inner| Source { inner, _not_send: PhantomData })
    }
    #[cfg(not(target_os = "android"))]
    {
      None
    }
  }

  /// Arm one frame callback and block until it fires: the display's vsync
  /// instant (the frame time on the monotonic clock, as an Instant), or
  /// None when the looper broke and no callback can be dispatched (the
  /// caller answers without a vsync).
  pub fn wait_vsync(&self) -> Option<Instant> {
    #[cfg(target_os = "android")]
    {
      self.inner.wait_vsync()
    }
    #[cfg(not(target_os = "android"))]
    {
      None
    }
  }
}

#[cfg(target_os = "android")]
mod android {
  use std::cell::Cell;
  use std::sync::Once;
  use std::time::{Duration, Instant};

  const NANOS_PER_SECOND: i64 = 1_000_000_000;

  // The vsync instant of the callback that fired, None until it does.
  type Fired = Cell<Option<Instant>>;

  pub(super) struct Choreographer {
    looper: ndk::looper::ThreadLooper,
    choreographer: *mut ndk_sys::AChoreographer,
    // Boxed so the address handed to the callback stays put; leaked on
    // drop while a callback is still armed (see Drop).
    fired: Box<Fired>,
    // A callback is posted and has not fired yet (a broken looper left it).
    armed: Cell<bool>,
  }

  impl Choreographer {
    pub(super) fn open() -> Option<Choreographer> {
      // The choreographer instance is per-thread and requires that thread
      // to own a looper; callbacks are dispatched from this thread's poll
      // calls.
      let looper = ndk::looper::ThreadLooper::prepare();
      // SAFETY: takes no arguments; this thread owns a looper (prepared
      // above), and the call returns null where no choreographer exists.
      let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
      if choreographer.is_null() {
        static WARNED: Once = Once::new();
        WARNED.call_once(|| log::warn!("[ore] No choreographer on this device; vsync pacing disabled"));
        return None;
      }
      Some(Choreographer { looper, choreographer, fired: Box::new(Cell::new(None)), armed: Cell::new(false) })
    }

    pub(super) fn wait_vsync(&self) -> Option<Instant> {
      self.fired.set(None);
      self.armed.set(true);
      // SAFETY: the choreographer is this thread's (non-null, checked at
      // open); `fired` is boxed, so its address lives as long as this
      // value, and Drop leaks it while a callback is still armed. The
      // callback runs only inside this thread's poll_once below, so the
      // Cell is never touched from two threads.
      unsafe {
        ndk_sys::AChoreographer_postFrameCallback(
          self.choreographer,
          Some(frame_callback),
          &*self.fired as *const Fired as *mut core::ffi::c_void,
        );
      }
      while self.fired.get().is_none() {
        if self.looper.poll_once().is_err() {
          // A broken looper cannot dispatch the callback; answer now
          // instead of stalling frame production (= present-return pacing).
          log::warn!("[ore] Vsync looper poll failed; answering without vsync");
          return None;
        }
      }
      self.armed.set(false);
      self.fired.get()
    }
  }

  impl Drop for Choreographer {
    fn drop(&mut self) {
      // A callback still armed would write into `fired` on a later poll of
      // this thread's looper; leave it a live allocation to write into.
      if self.armed.get() {
        std::mem::forget(std::mem::replace(&mut self.fired, Box::new(Cell::new(None))));
      }
    }
  }

  // Runs on the vsync thread, inside looper.poll_once(). The frame time is
  // CLOCK_MONOTONIC nanoseconds; it becomes an Instant by subtracting its
  // age (read on the same clock, here, before any pacing delay adds to it).
  // It is the reference the refresh count is taken from: on the vsync grid
  // rather than delay-plus-wake later. Note the value is the app's wake-up
  // time, a constant Display.getAppVsyncOffsetNanos() off a true vsync; a
  // constant phase offset does not change a refresh COUNT, so it is not
  // corrected here. A consumer needing the absolute vsync phase takes it
  // from the video plane's sampler (VideoPlaneView.java), which applies the
  // correction at its source.
  unsafe extern "C" fn frame_callback(frame_time_nanos: core::ffi::c_long, data: *mut core::ffi::c_void) {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: the out-pointer points to a live local for the whole call.
    let age_ns = if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } == 0 {
      (now.tv_sec as i64 * NANOS_PER_SECOND + now.tv_nsec as i64 - frame_time_nanos as i64).max(0)
    } else {
      0
    };
    let vsync = Instant::now() - Duration::from_nanos(age_ns as u64);
    // SAFETY: `data` is the boxed `Fired` of the Choreographer that posted
    // this callback, alive (or leaked) for as long as the callback is armed.
    unsafe { (*(data as *const Fired)).set(Some(vsync)) };
  }
}
