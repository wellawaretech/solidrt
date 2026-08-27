use std::sync::atomic::AtomicU64;
use std::sync::{mpsc, Arc};

use crate::gl;
use crate::Context;

/// What the raster thread reports to the main loop after finishing a frame.
/// The raster thread owns the process's single GL context and has already
/// drawn (and, in interactive mode, presented) by the time this arrives; the
/// main loop only does frame bookkeeping (fps, FrameRendered events) and
/// playback encoding.
pub enum FrameOutput {
  /// Interactive: the frame is on screen.
  Presented,
  /// Playback: the frame was drawn to the hidden window's backbuffer and read
  /// back. RGBA8, bottom-up rows, at the fixed capture size.
  Captured(Vec<u8>),
}

/// Physical framebuffer size packed for atomic hand-off from the main thread
/// (which receives resize events) to the raster thread (which wraps FBO 0 at
/// this size).
pub fn pack_size(width: u32, height: u32) -> u64 {
  ((width as u64) << 32) | height as u64
}

pub fn unpack_size(packed: u64) -> (u32, u32) {
  ((packed >> 32) as u32, (packed & 0xffff_ffff) as u32)
}

/// The raster thread's handle onto the process's single GL context: how it
/// binds it, presents through it, and resolves GL entry points. Interactive
/// runs bind through SDL (window + GLContext); headless playback on a stack
/// whose SDL offscreen driver cannot go headless binds a bare EGL pbuffer
/// (see egl_headless.rs). Every method runs on the raster thread.
pub(crate) trait GlBinding: Send {
  /// Make the context current on the calling thread. Also used to rebind
  /// after the surface underneath changed, so it must re-execute the bind
  /// even when the same pair looks current already; false with the detail
  /// in `error()`.
  fn bind(&self) -> bool;
  /// Present the backbuffer; false with the detail in `error()`.
  fn swap(&self) -> bool;
  /// Assert the swap interval on the current binding; a surface that never
  /// presents reports true and does nothing.
  fn set_swap_interval(&self) -> bool;
  fn proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void;
  fn error(&self) -> String;
}

/// The display context behind the raster thread. GL (through ANGLE where the
/// platform has no native GL) is the single backend by design - see
/// okf/research/graphics-backend-strategy.md.
pub enum DisplayContext {
  Gl {
    /// The SDL window handle the raster thread presents to. Raw because the
    /// sdl3 Window type is neither Send nor clonable; the Window itself lives
    /// in App on the main thread for the whole run.
    window_raw: *mut sdl3::sys::video::SDL_Window,
    gl_context: sdl3::video::GLContext,
    /// See `pack_size`.
    surface_size: Arc<AtomicU64>,
  },
  /// Headless playback: an EGL pbuffer context created without SDL's video
  /// subsystem. Shared with the raster thread, which binds and draws to it;
  /// FBO 0 is the pbuffer, so capture reads back exactly as from a window.
  EglPbuffer { egl: Arc<crate::egl_headless::HeadlessEgl>, surface_size: Arc<AtomicU64> },
}

impl DisplayContext {
  pub fn new_opengl(window: &sdl3::video::Window) -> Result<Self, Box<dyn std::error::Error>> {
    gl::setup_opengl_platform(window)
  }

  pub fn new_egl_pbuffer(width: u32, height: u32) -> Result<Self, String> {
    let egl = crate::egl_headless::HeadlessEgl::new(width, height)?;
    Ok(DisplayContext::EglPbuffer {
      egl: Arc::new(egl),
      surface_size: Arc::new(AtomicU64::new(pack_size(width, height))),
    })
  }

  /// The main thread's handle for publishing the physical framebuffer size on
  /// resize.
  pub fn surface_size_handle(&self) -> Arc<AtomicU64> {
    match self {
      DisplayContext::Gl { surface_size, .. } | DisplayContext::EglPbuffer { surface_size, .. } => surface_size.clone(),
    }
  }

  pub(crate) fn run_context(
    &self,
    closure: impl FnOnce(Arc<Context>) + Send + 'static,
    tx: mpsc::Sender<FrameOutput>,
    wake: Option<Box<dyn Fn() + Send + Sync>>,
    capture_frames: bool,
    stats: Arc<crate::raster::RasterStats>,
  ) -> crate::raster::RasterSender {
    let (binding, surface_size): (Box<dyn GlBinding>, _) = match self {
      DisplayContext::Gl { window_raw, gl_context, surface_size } => {
        (Box::new(gl::SdlGlBinding::new(*window_raw, gl_context)), surface_size.clone())
      }
      DisplayContext::EglPbuffer { egl, surface_size } => {
        (Box::new(crate::egl_headless::HeadlessEglBinding(egl.clone())), surface_size.clone())
      }
    };
    crate::threads::run_context(binding, surface_size, closure, tx, wake, capture_frames, stats)
  }
}
