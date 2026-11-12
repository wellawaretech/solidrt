//! The window surface's EGL partial-update facilities, for partial repaint
//! (okf/done/partial-repaint.md stage 2): the back buffer's age
//! (EGL_EXT_buffer_age) and the frame's damage region
//! (EGL_KHR_partial_update). The age says which earlier frame's pixels the
//! back buffer still holds, so a frame may redraw only the union of the
//! damage since then and present the rest preserved. The damage region
//! tells the driver where the frame renders, so a tiled GPU leaves the
//! tiles outside it alone: what makes a multisampled window's patch pay
//! (`WindowRoute::PatchLayer`).
//!
//! Both go straight to libEGL against the calling thread's CURRENT display
//! and surface - the raster thread has the window context bound - so SDL's
//! swap path stays untouched (buffer age and the damage region are valid
//! with plain eglSwapBuffers; swap-with-damage is only a compositor hint,
//! stage 3). Every failure path degrades to "unknown age" or "no region",
//! which callers treat as a full-frame redraw.

use crate::egl_headless::{load_egl, Egl};
use khronos_egl as egl;

// EGL_EXT_buffer_age surface attribute; not in khronos-egl's constants.
// EGL_KHR_partial_update names the same value EGL_BUFFER_AGE_KHR, so the
// query is valid under either extension.
const BUFFER_AGE_EXT: egl::Int = 0x313D;

/// The kill switch for patching a multisampled window: `off` draws every
/// frame of it whole, as before the patch layer existed. Read once, when
/// the window's EGL facilities are probed.
const WINDOW_PATCH_ENV: &str = "SOLIDRT_WINDOW_PATCH";

// eglSetDamageRegionKHR, as EGL_KHR_partial_update declares it:
// `EGLBoolean eglSetDamageRegionKHR(EGLDisplay dpy, EGLSurface surface,
// EGLint *rects, EGLint n_rects)`, an EGLAPIENTRY (the "system" ABI).
// EGLDisplay and EGLSurface are opaque pointers, EGLint an i32, EGLBoolean
// a u32: khronos-egl's raw types are those. Each rectangle is x, y, width,
// height in surface pixels, origin at the bottom-left. This module's two
// unsafe blocks are the cast to this type and the call through it.
type SetDamageRegion =
  unsafe extern "system" fn(egl::EGLDisplay, egl::EGLSurface, *mut egl::Int, egl::Int) -> egl::Boolean;

/// Why a multisampled window's frames cannot be patched, or None when they
/// can: `switch` is the kill switch's value (`SOLIDRT_WINDOW_PATCH`, None
/// when unset), `partial_update` whether the display advertises
/// EGL_KHR_partial_update. A value other than `off` is ignored, with a
/// warning.
pub(crate) fn window_patch_refusal(switch: Option<&str>, partial_update: bool) -> Option<&'static str> {
  match switch.map(str::trim) {
    Some("off") => return Some("switched off (SOLIDRT_WINDOW_PATCH=off)"),
    Some(other) => log::warn!("[alloy] {WINDOW_PATCH_ENV}={other}: expected off; ignored"),
    None => {}
  }
  (!partial_update).then_some("no EGL_KHR_partial_update")
}

pub(crate) struct BufferAge {
  egl: Egl,
  // The damage region call, or why a multisampled window is not patched
  // (the kill switch, the extension missing, the symbol not resolved).
  set_damage_region: Result<SetDamageRegion, &'static str>,
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
    log::info!("[alloy] EGL display extensions: {extensions}");
    let partial_update = extensions.contains("EGL_KHR_partial_update");
    // EGL_KHR_partial_update defines EGL_BUFFER_AGE_KHR (same enum as the
    // EXT attribute), so either extension makes the age query valid.
    if !extensions.contains("EGL_EXT_buffer_age") && !partial_update {
      return Err("neither EGL_EXT_buffer_age nor EGL_KHR_partial_update advertised".to_string());
    }
    let switch = std::env::var(WINDOW_PATCH_ENV).ok();
    let set_damage_region = match window_patch_refusal(switch.as_deref(), partial_update) {
      Some(reason) => Err(reason),
      None => match instance.get_proc_address("eglSetDamageRegionKHR") {
        // SAFETY: the pointer is the driver's eglSetDamageRegionKHR,
        // resolved by its name under the advertised extension, so its
        // real signature is the one `SetDamageRegion` spells out (see the
        // type); a function pointer cast between those types changes no
        // representation.
        Some(f) => Ok(unsafe { std::mem::transmute::<extern "system" fn(), SetDamageRegion>(f) }),
        None => Err("eglSetDamageRegionKHR not resolved"),
      },
    };
    Ok(BufferAge { egl: instance, set_damage_region })
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

  /// Whether a multisampled window's frames may be patched: Err says why
  /// not (the kill switch, or the damage region is not offered).
  pub(crate) fn window_patches(&self) -> Result<(), &'static str> {
    self.set_damage_region.map(|_| ())
  }

  /// Declare the frame's damage: the one rectangle `(x, y, width, height)`
  /// in surface pixels, bottom-left origin. Valid once per frame, after
  /// the age query and before the frame draws; the driver then keeps the
  /// aged buffer's pixels outside it, and nothing may render there. Err
  /// says why it was not taken (the call is not there, no current surface,
  /// or the EGL error the driver refused it with), and the frame then
  /// draws whole.
  pub(crate) fn set_damage_region(&self, rect: (i32, i32, i32, i32)) -> Result<(), String> {
    let call = self.set_damage_region.map_err(str::to_string)?;
    let (Some(display), Some(surface)) = (self.egl.get_current_display(), self.egl.get_current_surface(egl::DRAW))
    else {
      return Err("no current EGL surface".to_string());
    };
    let mut rects = [rect.0, rect.1, rect.2, rect.3];
    // SAFETY: `call` is eglSetDamageRegionKHR (see `new`), given the
    // current display and draw surface, and `rects` holds the one
    // rectangle's four EGLints that `n_rects` = 1 promises; EGL reads them
    // during the call only.
    if unsafe { call(display.as_ptr(), surface.as_ptr(), rects.as_mut_ptr(), 1) } == egl::TRUE {
      return Ok(());
    }
    Err(match self.egl.get_error() {
      Some(e) => format!("EGL error {:#x} ({e:?})", e.native()),
      None => "refused with no EGL error".to_string(),
    })
  }
}
