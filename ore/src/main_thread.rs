//! The main-thread bridge: the one way to run a main-thread-only call
//! (`SDL_OpenURL`, the clipboard, and later the dialogs) from any
//! thread.
//!
//! It is `SDL_RunOnMainThread`. On the main thread, or while SDL has no
//! event loop (headless flux never starts one, and runs its JS on its main
//! thread), the call runs at once. From another thread it is queued for
//! the main thread's event pump, and SDL wakes a blocked wait for it: the
//! video driver's wakeup with a shown window, a lifecycle wake on Android,
//! the wait's 1 ms poll with no window. A queued call returns at once; the
//! outcome travels through what the callback captures.
//!
//! A call still queued when SDL quits never runs: what it captured is
//! leaked, not dropped, so a responder in it never answers.

use std::ffi::c_void;

use sdl3::sys::init::SDL_RunOnMainThread;

/// Run `f` on the main thread, exactly once: with `Ok(())` on the main
/// thread, or with `Err` and SDL's reason at once on the calling thread
/// when SDL could not queue it. `f` must not panic: it runs under SDL's
/// C frames, where a panic aborts.
pub fn run<F: FnOnce(Result<(), String>) + Send + 'static>(f: F) {
  let userdata = Box::into_raw(Box::new(f)).cast::<c_void>();
  // SAFETY: `trampoline::<F>` matches the callback signature and takes
  // back the box `userdata` owns. SDL calls it at most once per call, and
  // only when it returns true (at once, or queued).
  if unsafe { SDL_RunOnMainThread(Some(trampoline::<F>), userdata, false) } {
    return;
  }
  let error = crate::sdl_error();
  // SAFETY: SDL refused the call, so the trampoline never runs and the
  // box is still ours to take back.
  let f = unsafe { Box::from_raw(userdata.cast::<F>()) };
  f(Err(error));
}

unsafe extern "C" fn trampoline<F: FnOnce(Result<(), String>)>(userdata: *mut c_void) {
  // SAFETY: `userdata` is the box `run` leaked for this call, and SDL
  // runs each call once.
  let f = unsafe { Box::from_raw(userdata.cast::<F>()) };
  f(Ok(()));
}
