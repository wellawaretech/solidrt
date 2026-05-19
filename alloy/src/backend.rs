use impellers::{DisplayList, ISize};
use std::sync::{mpsc, Arc};

use crate::gl;
use crate::Context;

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
    tx: mpsc::Sender<DisplayList>,
  ) {
    match self {
      DisplayContext::Gl { ui_context, .. } => gl::run_context(ui_context, closure, tx),
      DisplayContext::Vulkan { .. } => unimplemented!("Vulkan backend not yet implemented"),
      DisplayContext::Metal { .. } => unimplemented!("Metal backend not yet implemented"),
    }
  }
}

#[allow(dead_code)]
pub trait RenderSurface {
  fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>>;
  fn present(&mut self);
  fn resize(&mut self, size: ISize);
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