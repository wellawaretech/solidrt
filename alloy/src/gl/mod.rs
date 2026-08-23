//! GL-side rasterization for the raster thread: context bootstrap (context),
//! the retained offscreen rig and its capability probes (rig), the display-list
//! draw paths into textures, layers, and the window (draw), and pixel readback
//! (readback). Every function here runs on the raster thread with the
//! process's single GL context current.

mod context;
mod draw;
mod readback;
mod rig;

pub(crate) use context::{
  adopt_texture, configure_opengl, create_gl_context, create_impeller_context, setup_opengl_platform, SdlGlBinding,
};
pub(crate) use draw::{
  render_display_list_into_texture, render_display_list_to_layer, render_display_list_to_texture,
  render_display_list_to_window,
};
pub(crate) use readback::{read_fbo0_pixels, read_texture_pixels};
pub(crate) use rig::{msrtt, supports_invalidate, MsrttFns, OffscreenRig};
