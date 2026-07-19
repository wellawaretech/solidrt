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

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Backend {
  Gl,
  Vulkan,
  Metal,
}

#[allow(dead_code)]
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
  Vulkan {},
  Metal {},
}

#[allow(dead_code)]
impl DisplayContext {
  pub fn new_opengl(window: &sdl3::video::Window) -> Result<Self, Box<dyn std::error::Error>> {
    gl::setup_opengl_platform(window)
  }

  pub fn backend(&self) -> Backend {
    match self {
      DisplayContext::Gl { .. } => Backend::Gl,
      DisplayContext::Vulkan { .. } => Backend::Vulkan,
      DisplayContext::Metal { .. } => Backend::Metal,
    }
  }

  /// The main thread's handle for publishing the physical framebuffer size on
  /// resize.
  pub fn surface_size_handle(&self) -> Arc<AtomicU64> {
    match self {
      DisplayContext::Gl { surface_size, .. } => surface_size.clone(),
      DisplayContext::Vulkan { .. } => unimplemented!("Vulkan backend not yet implemented"),
      DisplayContext::Metal { .. } => unimplemented!("Metal backend not yet implemented"),
    }
  }

  pub fn run_context(
    &self,
    closure: impl FnOnce(Arc<Context>) + Send + 'static,
    tx: mpsc::Sender<FrameOutput>,
    wake: Option<Box<dyn Fn() + Send + Sync>>,
    capture_frames: bool,
  ) {
    match self {
      DisplayContext::Gl { window_raw, gl_context, surface_size } => {
        gl::run_context(*window_raw, gl_context, surface_size.clone(), closure, tx, wake, capture_frames)
      }
      DisplayContext::Vulkan { .. } => unimplemented!("Vulkan backend not yet implemented"),
      DisplayContext::Metal { .. } => unimplemented!("Metal backend not yet implemented"),
    }
  }
}
