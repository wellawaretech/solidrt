use impellers::{DisplayList, ISize};
use std::sync::{mpsc, Arc};

use crate::gl;
use crate::Context;

/// A GPU fence (sync object) the UI thread creates after a frame's GPU work and
/// hands to the render thread. The render thread waits on it (GPU-side, no CPU
/// stall) before sampling that work, replacing a blocking glFinish on the UI
/// thread. The handle is valid across the shared GL context group; Send because
/// the producing thread passes it to the render thread.
pub struct GpuFence(pub glow::Fence);
unsafe impl Send for GpuFence {}

/// One composited frame on its way from the UI thread to the render thread: the
/// display list to draw, plus an optional fence ordering all the UI thread's GPU
/// work for that frame (shader renders, texture uploads, offscreen draws) ahead
/// of the render thread sampling it.
pub struct Frame {
  pub dl: DisplayList,
  pub fence: Option<GpuFence>,
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
    window_opaque: *const std::ffi::c_void,
    main_context: sdl3::video::GLContext,
    ui_context: sdl3::video::GLContext,
  },
  Vulkan {},
  Metal {},
}

#[allow(dead_code)]
impl DisplayContext {
  pub fn new_opengl(
    video: &sdl3::VideoSubsystem,
    window: &sdl3::video::Window,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    gl::setup_opengl_platform(video, window)
  }

  pub fn backend(&self) -> Backend {
    match self {
      DisplayContext::Gl { .. } => Backend::Gl,
      DisplayContext::Vulkan { .. } => Backend::Vulkan,
      DisplayContext::Metal { .. } => Backend::Metal,
    }
  }

  pub fn run_context(
    &self,
    closure: impl FnOnce(Arc<Context>) + Send + 'static,
    tx: mpsc::Sender<Frame>,
    wake: Option<Box<dyn Fn() + Send + Sync>>,
  ) {
    match self {
      DisplayContext::Gl { ui_context, .. } => gl::run_context(ui_context, closure, tx, wake),
      DisplayContext::Vulkan { .. } => unimplemented!("Vulkan backend not yet implemented"),
      DisplayContext::Metal { .. } => unimplemented!("Metal backend not yet implemented"),
    }
  }
}

#[allow(dead_code)]
pub trait RenderSurface {
  fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>>;
  fn present(&mut self, window: &sdl3::video::Window);
  fn resize(&mut self, size: ISize);
  /// Order this surface's subsequent GPU work after the producer's fenced work.
  /// When `wait`, the surface's later draw commands wait on the fence (GPU-side,
  /// no CPU stall) so it samples completed textures; either way the sync object
  /// is released. No-op default for backends that do not use GL fences.
  fn consume_fence(&self, _fence: Option<GpuFence>, _wait: bool) {}
}

pub fn create_render_surface(
  platform: &DisplayContext,
  size: ISize,
) -> Result<Box<dyn RenderSurface>, Box<dyn std::error::Error>> {
  match platform {
    DisplayContext::Gl { window_opaque, .. } => {
      let window = unsafe { &*(*window_opaque as *const sdl3::video::Window) };
      gl::GlSurface::create(window, size).map(|s| Box::new(s) as Box<dyn RenderSurface>)
    }
    DisplayContext::Vulkan { .. } => Err("Vulkan backend not yet implemented".into()),
    DisplayContext::Metal { .. } => Err("Metal backend not yet implemented".into()),
  }
}
