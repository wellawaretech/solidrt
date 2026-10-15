//! The context's reset status and the test hook that forces a loss
//! (okf/done/gpu-context-loss.md). Both are extension entry points glow has
//! no binding for, resolved through the binding's proc-address like glow's
//! own: `glGetGraphicsResetStatus` (GL_KHR_robustness, core at ES 3.2) says
//! whether the context was reset, and `glLoseContextCHROMIUM`
//! (GL_CHROMIUM_lose_context, ANGLE's test hook) marks it lost on demand.
//!
//! The status reads no-error forever on a context created without reset
//! notification (the robustness spec requires it); the window's context is
//! created with `EGL_LOSE_CONTEXT_ON_RESET_EXT` where EGL grants it (see
//! `context::create_window_gl_context`), measured on ANGLE/D3D11: a
//! context-level loss keeps swapping and only the status reports it, a
//! device removal fails the swap with `EGL_CONTEXT_LOST`.

use std::ffi::c_void;

use crate::backend::GlBinding;

/// GL_KHR_robustness reset statuses; 0 (GL_NO_ERROR) is "not reset".
const GUILTY_CONTEXT_RESET: u32 = 0x8253;
const INNOCENT_CONTEXT_RESET: u32 = 0x8254;
const UNKNOWN_CONTEXT_RESET: u32 = 0x8255;

type ResetStatusFn = unsafe extern "system" fn() -> u32;
type LoseContextFn = unsafe extern "system" fn(u32, u32);

/// What `glGetGraphicsResetStatus` answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResetStatus {
  /// The context was not reset (or the query is unavailable).
  None,
  /// Reset by this context's own work.
  Guilty,
  /// Reset by someone else's work.
  Innocent,
  /// Reset for a reason the driver does not attribute.
  Unknown,
}

impl ResetStatus {
  pub(crate) fn is_reset(self) -> bool {
    self != ResetStatus::None
  }

  pub(crate) fn name(self) -> &'static str {
    match self {
      ResetStatus::None => "no reset",
      ResetStatus::Guilty => "guilty context reset",
      ResetStatus::Innocent => "innocent context reset",
      ResetStatus::Unknown => "unknown context reset",
    }
  }

  pub(crate) fn from_gl(status: u32) -> ResetStatus {
    match status {
      0 => ResetStatus::None,
      GUILTY_CONTEXT_RESET => ResetStatus::Guilty,
      INNOCENT_CONTEXT_RESET => ResetStatus::Innocent,
      UNKNOWN_CONTEXT_RESET => ResetStatus::Unknown,
      _ => ResetStatus::Unknown,
    }
  }
}

/// The two entry points as the current context resolves them; loaded once
/// per context on the raster thread (`load`).
pub(crate) struct Robustness {
  reset_status: Option<ResetStatusFn>,
  lose_context: Option<LoseContextFn>,
}

impl Robustness {
  pub(crate) fn load(binding: &dyn GlBinding) -> Self {
    let reset_status = [c"glGetGraphicsResetStatusKHR", c"glGetGraphicsResetStatusEXT", c"glGetGraphicsResetStatus"]
      .into_iter()
      .map(|name| binding.proc_address(name))
      .find(|p| !p.is_null())
      // Safety: the loader resolved this name on the current context, and
      // the KHR, EXT and core forms share one signature.
      .map(|p| unsafe { std::mem::transmute::<*const c_void, ResetStatusFn>(p) });
    let lose = binding.proc_address(c"glLoseContextCHROMIUM");
    // Safety: same, for GL_CHROMIUM_lose_context's one signature.
    let lose_context = (!lose.is_null()).then(|| unsafe { std::mem::transmute::<*const c_void, LoseContextFn>(lose) });
    Robustness { reset_status, lose_context }
  }

  /// Whether the context was reset. `ResetStatus::None` where the query is
  /// unavailable: a loss then only shows as a failed present.
  pub(crate) fn reset_status(&self) -> ResetStatus {
    match self.reset_status {
      // Safety: a GL call on the raster thread with its context current.
      Some(f) => ResetStatus::from_gl(unsafe { f() }),
      None => ResetStatus::None,
    }
  }

  /// Mark the context lost as if its own work had reset the device; false
  /// where the hook is absent (every driver but ANGLE).
  pub(crate) fn lose_context(&self) -> bool {
    match self.lose_context {
      Some(f) => {
        // Safety: a GL call on the raster thread with its context current.
        unsafe { f(GUILTY_CONTEXT_RESET, GUILTY_CONTEXT_RESET) };
        true
      }
      None => false,
    }
  }
}
