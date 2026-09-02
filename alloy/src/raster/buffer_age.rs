//! EGL_EXT_buffer_age on the window surface, for partial repaint
//! (okf/done/partial-repaint.md stage 2). The age of the back buffer says
//! which earlier frame's pixels it still holds, so a frame may redraw only
//! the union of the damage since then and present the rest preserved.
//!
//! The query goes straight to libEGL against the calling thread's CURRENT
//! display and surface - the raster thread has the window context bound -
//! so SDL's swap path stays untouched (buffer age is valid with plain
//! eglSwapBuffers; swap-with-damage is only a compositor hint, stage 3).
//! Every failure path degrades to "unknown age", which callers treat as a
//! full-frame redraw.

use crate::egl_headless::{load_egl, Egl};
use khronos_egl as egl;

// EGL_EXT_buffer_age surface attribute; not in khronos-egl's constants.
const BUFFER_AGE_EXT: egl::Int = 0x313D;

pub(crate) struct BufferAge {
  egl: Egl,
}

impl BufferAge {
  /// Ready to query, or a reason it never will be (no libEGL, the context
  /// is not on EGL, the extension is missing). Call on the raster thread
  /// with the window context current.
  pub(crate) fn new() -> Result<Self, String> {
    let instance = load_egl()?;
    let Some(display) = instance.get_current_display() else {
      return Err("no current EGL display (not an EGL context)".to_string());
    };
    let extensions = instance
      .query_string(Some(display), egl::EXTENSIONS)
      .map(|s| s.to_string_lossy().into_owned())
      .unwrap_or_default();
    if !extensions.contains("EGL_EXT_buffer_age") {
      return Err("EGL_EXT_buffer_age not advertised".to_string());
    }
    Ok(BufferAge { egl: instance })
  }

  /// Age of the current back buffer: its content is the frame presented
  /// `age` swaps ago. 0 means unknown/undefined content (a fresh or resized
  /// buffer, or a failed query) - redraw everything.
  pub(crate) fn age(&self) -> i32 {
    let (Some(display), Some(surface)) = (self.egl.get_current_display(), self.egl.get_current_surface(egl::DRAW))
    else {
      return 0;
    };
    self.egl.query_surface(display, surface, BUFFER_AGE_EXT).unwrap_or(0)
  }
}
