//! The GL side of the crate, raster-thread-only: context bootstrap
//! (context), the retained offscreen rig and its capability probes (rig),
//! the display-list draw paths into textures, layers, and the window (draw),
//! pixel readback (readback), and the executors behind the `gpu` protocol
//! vocabulary - stage compile/link and programs (program), vertex buffers
//! (buffer), render targets (target), pass execution (pass), GL textures and
//! the sampler cache (texture), and pass timing (timing). Every function
//! here runs on the raster thread with the process's single GL context
//! current; the thread contract is that nothing outside this module, raster/
//! and the raster-thread bootstrap in threads.rs touches GL.

mod buffer;
mod context;
mod draw;
mod pass;
mod program;
mod readback;
mod rig;
mod target;
mod texture;
mod timing;

pub(crate) use buffer::{release_buffer, GpuBuffer};
pub(crate) use context::{
  adopt_texture, configure_opengl, create_gl_context, create_impeller_context, query_limits, setup_opengl_platform,
  SdlGlBinding,
};
pub(crate) use pass::{
  composite_program_over_window, render_program_to_fbo, render_program_to_window, PassInput, TILE_CLEAR_FRAGMENT,
};
pub(crate) use program::{
  compile_stage, delete_stage, release_pipeline, release_program, CompiledStage, RenderPipeline, ShaderProgram,
};
#[cfg(test)]
pub(crate) use program::declared_uniform_names;
pub(crate) use target::{create_layer_target, EntryBuffers, ShaderTexture};
pub(crate) use texture::{GpuTexture, SamplerCache};
pub(crate) use timing::{PassTimer, Timed};
pub(crate) use draw::{
  render_display_list_into_texture, render_display_list_to_layer, render_display_list_to_texture,
  render_display_list_to_window,
};
pub(crate) use readback::{read_fbo0_pixels, read_texture_pixels};
pub(crate) use rig::{msrtt, supports_invalidate, window_fast_path, MsrttFns, OffscreenRig};

use std::num::NonZeroU32;

// A previously-read GL binding (glGetIntegerv name) as a glow handle, for
// restoring state after a pass touches it: 0 maps back to "unbound".
fn prev_texture(name: i32) -> Option<glow::NativeTexture> {
  NonZeroU32::new(name as u32).map(glow::NativeTexture)
}
fn prev_framebuffer(name: i32) -> Option<glow::NativeFramebuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeFramebuffer)
}
fn prev_program(name: i32) -> Option<glow::NativeProgram> {
  NonZeroU32::new(name as u32).map(glow::NativeProgram)
}
fn prev_vertex_array(name: i32) -> Option<glow::NativeVertexArray> {
  NonZeroU32::new(name as u32).map(glow::NativeVertexArray)
}
fn prev_buffer(name: i32) -> Option<glow::NativeBuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeBuffer)
}
fn prev_sampler(name: i32) -> Option<glow::NativeSampler> {
  NonZeroU32::new(name as u32).map(glow::NativeSampler)
}
