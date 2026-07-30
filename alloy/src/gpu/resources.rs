//! Introspection DTOs: a point-in-time inventory of the GPU bookkeeping, for
//! resource introspection (the dev server's gpu query). Plain data only, so
//! consumers stay free of GL types.

use super::vocab::ParamValue;

pub struct GpuResources {
  pub textures: Vec<GpuTextureInfo>,
  pub buffers: Vec<GpuBufferInfo>,
  pub pipelines: Vec<GpuPipelineInfo>,
  pub render_pipelines: Vec<GpuRenderPipelineInfo>,
  pub programs: Vec<GpuProgramInfo>,
  pub window_shader: Option<GpuWindowShaderInfo>,
}

/// The active window shader: its program and the retained layer's pixel size
/// (0x0 until the first shaded frame allocates it).
pub struct GpuWindowShaderInfo {
  pub program_id: u64,
  pub width: u32,
  pub height: u32,
  /// The `uPrevious` history layer is declared AND allocated.
  pub previous: bool,
  /// Shaded frames that skipped the tree raster and ran only the pass over
  /// the retained layer (the clean-tree fast path), cumulative for this
  /// raster thread.
  pub pass_only_frames: u64,
}

pub struct GpuTextureInfo {
  pub id: u64,
  pub width: u32,
  pub height: u32,
  /// A shader or pipeline renders into this texture (vs a sampled upload).
  pub target: bool,
}

pub struct GpuBufferInfo {
  pub id: u64,
  pub byte_length: usize,
}

pub struct GpuProgramInfo {
  pub id: u64,
}

/// A registered render pipeline: a program paired with the draw state its
/// targets share.
pub struct GpuRenderPipelineInfo {
  pub id: u64,
  pub program_id: u64,
  pub topology: &'static str,
  /// "none" or "add".
  pub blend: &'static str,
  pub depth: bool,
  pub depth_write: bool,
  /// (name, format string) of the declared interleaved vertex layout.
  pub attributes: Vec<(String, String)>,
}

pub struct GpuPipelineInfo {
  /// The registry id its output texture is sampleable under.
  pub texture_id: u64,
  /// "pipeline" (vertex+fragment over a buffer) or "fragment" (fullscreen pass).
  pub kind: &'static str,
  /// The shared program behind this target's pipeline; None when it was
  /// created through the fused path and owns its program alone.
  pub program_id: Option<u64>,
  /// The registered pipeline this target was created from; None for fragment
  /// targets and the fused path.
  pub pipeline_id: Option<u64>,
  pub buffer_id: Option<u64>,
  pub topology: Option<&'static str>,
  pub draw_count: Option<i32>,
  pub depth: bool,
  /// Whether the draw writes depth; None on a fragment-only target.
  pub depth_write: Option<bool>,
  /// "none" or "add"; None on a fragment-only target.
  pub blend: Option<&'static str>,
  /// (name, format string) of the declared interleaved vertex layout.
  pub attributes: Vec<(String, String)>,
  /// sampler2D uniform name -> source texture id.
  pub textures: Vec<(String, u64)>,
  /// The float uniforms applied on the most recent render.
  pub params: Vec<(String, ParamValue)>,
  /// Manual render mode: rendered only by an explicit render, never by the
  /// dirty flush (see `TargetSpec::manual`).
  pub manual: bool,
  /// Color load op (see `TargetSpec::load`): true = loadOp "load", the draw
  /// lands over the previous contents instead of a clear.
  pub load: bool,
  /// Cumulative passes rendered into this target.
  pub passes: u64,
  /// Cumulative raster-thread wall time those passes took, in microseconds
  /// (occupancy, not GPU-side duration; see raster::RasterStats).
  pub pass_micros: u64,
}
