//! The GPU protocol layer: the plain-data vocabulary shared by the UI
//! thread's Context mirrors and the raster thread's executors. Draw-state
//! vocabulary and validators (`vocab`), the command/introspection shapes
//! (`spec`, `resources`), device ceilings (`limits`), buffer write leases
//! (`lease`), instance ordering (`order`), and the texture/sampling
//! vocabulary plus the UI-side registry (`texture`).
//!
//! No GL here, by design: nothing in this module takes a `glow::Context`,
//! so the "srt-ui has zero GL" thread contract is visible in the module
//! graph. The GL executors these shapes drive - programs, passes, targets,
//! buffers, the sampler cache - live in `gl/`, raster-thread-only.

mod lease;
mod limits;
mod order;
pub(crate) mod resources;
pub(crate) mod spec;
pub(crate) mod texture;
pub(crate) mod vocab;

pub use lease::WriteLeases;
pub use limits::GpuLimits;
pub use order::{gather_ordered, gather_permuted, order_permutation, InstanceOrder, OrderKey, OrderScratch};
pub use resources::{
  GpuBufferInfo, GpuPipelineInfo, GpuProgramInfo, GpuRenderPipelineInfo, GpuResources, GpuTextureInfo,
  GpuWindowShaderInfo, GpuRegionInfo,
};
pub use spec::{DepthStorage, DrawSpec, NodeShader, PipelineSpec, TargetSpec, WindowShader};
pub use texture::{
  SamplerFilter, SamplerOptions, SamplerOverride, SamplerState, TextureEntry, TextureFormat, TextureRegistry,
};
#[cfg(test)]
pub use vocab::merge_bindings;
pub use vocab::{
  blend_name, cull_name, instance_strides, parse_blend, parse_cull, resolve_draw_range, validate_draw_range,
  validate_instance_slots, validate_order, validate_param_if_declared, validate_params, validate_texture_bindings,
  vertex_stride, AttrFormat, AttributeTable, BlendMode, BufferIds, BufferUpdate, CullMode, DepthState, DrawBounds,
  DrawRange, DrawUpdate, IndexFormat, ParamValue, PipelineDesc, ShaderStage, TextureBinding, Topology, UniformKind,
  UniformSlot, UniformTable, MAX_INSTANCE_SLOTS,
};
