//! The GPU shading layer behind the raster thread: draw-state vocabulary
//! (`vocab`), stage compile/link and programs (`program`), vertex buffers
//! (`buffer`), render targets (`target`), pass execution (`pass`), textures
//! and sampling (`texture`: registry, sampler vocabulary, sampler cache),
//! and the plain-data command/introspection shapes (`spec`, `resources`).
//! All GL use stays on the raster thread; see raster.rs.

mod buffer;
mod lease;
mod limits;
mod order;
mod pass;
pub(crate) mod program;
mod resources;
mod spec;
mod target;
pub(crate) mod texture;
mod timing;
mod vocab;

pub use buffer::{release_buffer, GpuBuffer};
pub use lease::WriteLeases;
pub use limits::GpuLimits;
pub use order::{gather_ordered, gather_permuted, order_permutation, InstanceOrder, OrderKey, OrderScratch};
pub use pass::{composite_program_over_window, render_program_to_fbo, render_program_to_window, PassInput, TILE_CLEAR_FRAGMENT};
pub use program::{
  compile_stage, delete_stage, release_pipeline, release_program, CompiledStage, RenderPipeline, ShaderProgram,
};
pub use resources::{
  GpuBufferInfo, GpuPipelineInfo, GpuProgramInfo, GpuRenderPipelineInfo, GpuResources, GpuTextureInfo,
  GpuWindowShaderInfo, GpuRegionInfo,
};
pub use spec::{DepthStorage, DrawSpec, NodeShader, PipelineSpec, TargetSpec, WindowShader};
pub use target::{create_layer_target, EntryBuffers, ShaderTexture};
pub use texture::{
  generate_mipmap, GpuTexture, SamplerCache, SamplerFilter, SamplerOptions, SamplerOverride, SamplerState, TextureEntry,
  TextureFormat,
  TextureRegistry,
};
pub use timing::{PassTimer, Timed};
#[cfg(test)]
pub use vocab::merge_bindings;
pub use vocab::{
  blend_name, cull_name, instance_strides, parse_blend, parse_cull, resolve_draw_range, validate_draw_range,
  validate_instance_slots, validate_order, validate_param_if_declared, validate_params, validate_texture_bindings,
  vertex_stride, AttrFormat, AttributeTable, BlendMode, BufferIds, BufferUpdate, CullMode, DepthState, DrawBounds,
  DrawRange, DrawUpdate, IndexFormat, ParamValue, PipelineDesc, ShaderStage, TextureBinding, Topology, UniformKind,
  UniformSlot, UniformTable, MAX_INSTANCE_SLOTS,
};

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
