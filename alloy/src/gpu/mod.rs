//! The GPU protocol layer: the plain-data vocabulary shared by the UI
//! thread's Context mirrors and the raster thread's executors. Draw-state
//! vocabulary and validators (`vocab`), the command/introspection shapes
//! (`spec`, `resources`), device ceilings (`limits`), buffer write leases
//! (`lease`), instance ordering (`order`), and the texture/sampling
//! vocabulary plus the UI-side registry (`texture`).
//!
//! No GL here, by design: nothing in this module takes a `glow::Context`,
//! so the "sol-ui has zero GL" thread contract is visible in the module
//! graph. The GL executors these shapes drive - programs, passes, targets,
//! buffers, the sampler cache - live in `gl/`, raster-thread-only.

mod glyphs;
mod lease;
mod limits;
mod order;
pub(crate) mod resources;
pub(crate) mod spec;
pub(crate) mod texture;
pub(crate) mod vocab;

pub use glyphs::{
  CoverageMode, CoveragePolicy, GlyphGradient, GlyphGroup, GlyphQuad, GradientKind, GradientTile, RAMP_WIDTH,
};
pub use lease::WriteLeases;
pub use limits::GpuLimits;
pub use order::{
  gather_ordered, gather_permuted, materialize_indices, order_permutation, InstanceOrder, OrderKey, OrderScratch,
  INDEX_NONE,
};
pub use resources::{
  GpuBufferInfo, GpuBufferLayoutInfo, GpuPipelineInfo, GpuProgramInfo, GpuRegionInfo, GpuRenderPipelineInfo,
  GpuResources, GpuTextureInfo, GpuWindowShaderInfo,
};
pub use spec::{DepthStorage, DrawSpec, NodeShader, PipelineSpec, TargetSpec, WindowShader};
pub use texture::{
  check_cube_faces, mip_levels, mip_size, SamplerFilter, SamplerOptions, SamplerOverride, SamplerState, SamplerWrap,
  TextureEntry, TextureFormat, TextureRect, TextureRegistry, TextureShape, CUBE_FACES, MIN_ANISOTROPY,
};
#[cfg(test)]
pub use vocab::merge_bindings;
pub use vocab::{
  blend_name, buffer_strides, cull_name, parse_blend, parse_cull, resolve_draw_range, validate_binding_shapes,
  validate_buffers, validate_draw_range, validate_order, validate_param_if_declared, validate_params,
  validate_texture_bindings, AttrFormat, AttributeTable, BlendMode, BoundTexture, BufferBound, BufferIds, BufferLayout,
  BufferStride, BufferUpdate, CullMode, DepthState, DrawBounds, DrawRange, DrawUpdate, IndexFormat, ParamValue,
  PipelineDesc, ShaderStage, StepMode, TextureBinding, Topology, UniformKind, UniformSlot, UniformTable, VertexAttr,
  MAX_BUFFERS,
};
