//! The GL surface: a `Send` handle onto the window's GL context for the
//! thread that draws, and the context's creation on the main thread (with
//! reset notification where EGL grants it).
//!
//! Thread rule: `create_context` runs on the main thread (SDL creates and
//! destroys GL contexts there, `crate::window::Window` owns them); every
//! `GlSurface` method runs on the one thread that draws, which makes the
//! context current there first.

use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicU8, Ordering};

use sdl3::sys::video::{
  SDL_EGLConfig, SDL_EGLDisplay, SDL_EGL_SetAttributeCallbacks, SDL_EGLint, SDL_GLContext, SDL_GL_GetProcAddress,
  SDL_GL_MakeCurrent, SDL_GL_SetSwapInterval, SDL_GL_SwapWindow, SDL_Window,
};

use super::DeviceProbe;

/// The swap interval for window presents: 1 everywhere. The blocking swap
/// is the frame pacer on desktop and the stock EGL sync path on Android;
/// async mode (0) was measured no better on the 2017 MediaTek TV during the
/// swap-latency investigation (okf/backlog/android-surface-swap-latency.md).
const WINDOW_SWAP_INTERVAL: i32 = 1;

/// A Send handle onto a window's GL context for the thread that draws:
/// make-current, swap, proc address. Non-owning: the Window owns the
/// context and the window, and outlives every use (it lives on the main
/// thread for the run; a recreate hands out a new surface after the old
/// one was released and dropped).
pub struct GlSurface {
  window: *mut SDL_Window,
  context: SDL_GLContext,
  /// The display link the swap waits on (display_link.rs): ANGLE-Metal's
  /// swap never blocks, so the wait for the display's tick happens here.
  /// Created at the first swap, on the drawing thread; None when the
  /// machine has no link.
  #[cfg(target_os = "macos")]
  pacer: std::cell::OnceCell<Option<super::display_link::SwapPacer>>,
}

// SAFETY: the two handles are plain pointers SDL hands to any thread for
// the GL calls below; SDL's GL calls act on the calling thread's current
// context, and the context is current on at most one thread at a time (the
// drawing thread binds it after the main thread released it). The Window
// that owns both outlives the surface (see the type doc). The macOS pacer
// is Send itself.
unsafe impl Send for GlSurface {}

impl GlSurface {
  pub(crate) fn new(window: *mut SDL_Window, context: SDL_GLContext) -> GlSurface {
    GlSurface {
      window,
      context,
      #[cfg(target_os = "macos")]
      pacer: std::cell::OnceCell::new(),
    }
  }

  /// Make the context current on the calling thread; false with the detail
  /// in `error()`.
  ///
  /// The unbind step is load-bearing for the rebind case: SDL_GL_MakeCurrent
  /// short-circuits when its per-thread bookkeeping says this (window,
  /// context) pair is already current (SDL_video.c), so a same-pair call never
  /// reaches eglMakeCurrent - and re-executing eglMakeCurrent against the
  /// window's new EGL surface (recreated across an Android background/resume)
  /// is the whole point. SDL's own android_egl_context_restore does the same
  /// dance.
  pub fn make_current(&self) -> bool {
    // SAFETY: the window and context are alive (the Window owns them and
    // outlives this surface); a null context unbinds the calling thread.
    unsafe {
      SDL_GL_MakeCurrent(self.window, std::ptr::null_mut());
      SDL_GL_MakeCurrent(self.window, self.context)
    }
  }

  /// Make no context current on the calling thread: the step before the
  /// main thread destroys the context (a context-loss recovery), since a
  /// context current on another thread cannot be destroyed. False with the
  /// detail in `error()`.
  pub fn release(&self) -> bool {
    // SAFETY: the window is alive (see make_current); a null context
    // unbinds the calling thread.
    unsafe { SDL_GL_MakeCurrent(self.window, std::ptr::null_mut()) }
  }

  /// Present the backbuffer: waits the display link's tick on macOS, then
  /// SDL_GL_SwapWindow, checked. The sdl3 crate's `Window::gl_swap_window`
  /// discards SDL_GL_SwapWindow's result, but a present can fail
  /// permanently (EGL context lost / D3D device removed under ANGLE).
  /// False on failure, with the detail in `error()`. Call only from the
  /// thread the context is current on.
  pub fn swap(&self) -> bool {
    #[cfg(target_os = "macos")]
    if let Some(pacer) = self.pacer.get_or_init(super::display_link::SwapPacer::start) {
      pacer.wait_tick();
    }
    // SAFETY: the window is alive (see make_current), and its context is
    // current on this thread.
    unsafe { SDL_GL_SwapWindow(self.window) }
  }

  /// Assert WINDOW_SWAP_INTERVAL on the current binding; false with the
  /// detail in `error()`.
  pub fn set_swap_interval(&self) -> bool {
    // SAFETY: takes a plain integer and acts on the calling thread's
    // current context.
    unsafe { SDL_GL_SetSwapInterval(WINDOW_SWAP_INTERVAL) }
  }

  /// The GL entry point `name`, null when the GL library has none.
  pub fn proc_address(&self, name: &CStr) -> *const c_void {
    // SAFETY: `name` is a valid C string that outlives the call.
    unsafe { SDL_GL_GetProcAddress(name.as_ptr()) }.map(|f| f as *const c_void).unwrap_or(std::ptr::null())
  }

  /// SDL's last error on this thread.
  pub fn error(&self) -> String {
    crate::sdl_error()
  }

  /// The device behind the current EGL display, resolved now with the
  /// context current (device_loss.rs).
  pub fn device_probe(&self) -> DeviceProbe {
    DeviceProbe::resolve()
  }
}

// EGL_EXT_create_context_robustness: the context attribute that asks for
// reset notification, and the strategy that loses the context on a reset.
// Without it the reset status reads no-error forever (the robustness spec
// requires it), and a context-level loss on ANGLE is invisible: the swap
// keeps succeeding (measured, okf/done/gpu-context-loss.md). SDL's EGL
// backend never asks for it; its attribute callback appends these to the
// eglCreateContext attributes.
const EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT: SDL_EGLint = 0x3138;
const EGL_LOSE_CONTEXT_ON_RESET_EXT: SDL_EGLint = 0x31BF;
const EGL_NONE: SDL_EGLint = 0x3038;

// Whether EGL granted the reset-notification attribute: 0 = not asked yet,
// 1 = granted (the callback stays installed for every later context), 2 =
// refused (the callback was removed; contexts are created without it).
static RESET_NOTIFICATION: AtomicU8 = AtomicU8::new(RESET_NOTIFICATION_UNTRIED);
const RESET_NOTIFICATION_UNTRIED: u8 = 0;
const RESET_NOTIFICATION_GRANTED: u8 = 1;
const RESET_NOTIFICATION_REFUSED: u8 = 2;

/// SDL's EGL context-attribute hook: the array (SDL frees it) is appended
/// to the eglCreateContext attributes. Must not panic: it runs under SDL's
/// C frames.
unsafe extern "C" fn reset_notification_attribs(
  _userdata: *mut c_void,
  _display: SDL_EGLDisplay,
  _config: SDL_EGLConfig,
) -> *mut SDL_EGLint {
  let attribs = [EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT, EGL_LOSE_CONTEXT_ON_RESET_EXT, EGL_NONE];
  // SAFETY: SDL_malloc returns null or a block of the asked size, which
  // SDL frees with SDL_free after reading the attributes; the copy writes
  // exactly that many bytes into it.
  unsafe {
    let out = sdl3::sys::stdinc::SDL_malloc(std::mem::size_of_val(&attribs)) as *mut SDL_EGLint;
    if !out.is_null() {
      std::ptr::copy_nonoverlapping(attribs.as_ptr(), out, attribs.len());
    }
    out
  }
}

fn set_reset_notification_callback(install: bool) {
  let callback = if install { Some(reset_notification_attribs as _) } else { None };
  // SAFETY: the callback matches SDL_EGLIntArrayCallback, needs no
  // userdata, and lives for the process; None removes it.
  unsafe { SDL_EGL_SetAttributeCallbacks(None, None, callback, std::ptr::null_mut()) };
}

/// The window's GL context, with reset notification where EGL grants it:
/// the first creation asks, and a refusal (an EGL without
/// EGL_EXT_create_context_robustness fails the create with the attribute
/// present) is remembered, so every later context - the context-loss
/// recovery's included - is created the way the first one succeeded. The
/// context is current on the calling thread afterwards, as SDL leaves it.
/// Main thread only.
pub(crate) fn create_context(window: &sdl3::video::Window) -> Result<sdl3::video::GLContext, String> {
  if RESET_NOTIFICATION.load(Ordering::Relaxed) == RESET_NOTIFICATION_UNTRIED {
    set_reset_notification_callback(true);
    match window.gl_create_context() {
      Ok(context) => {
        RESET_NOTIFICATION.store(RESET_NOTIFICATION_GRANTED, Ordering::Relaxed);
        return Ok(context);
      }
      Err(e) => {
        log::info!("[ore] GL context without reset notification: EGL refused the attribute ({e})");
        set_reset_notification_callback(false);
        RESET_NOTIFICATION.store(RESET_NOTIFICATION_REFUSED, Ordering::Relaxed);
      }
    }
  }
  window.gl_create_context().map_err(|e| format!("Failed to create GL context: {e}"))
}
