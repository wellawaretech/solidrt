//! Introspection DTOs: a point-in-time inventory of the GPU bookkeeping, for
//! resource introspection (the dev server's gpu query). Plain data only, so
//! consumers stay free of GL types.

use super::vocab::{ParamValue, TextureBinding};

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
  /// Pixel format name: "rgba8" or "r8" for uploads, "rgba8" for a target's
  /// color, "depth24" for a draw target's depth texture id.
  pub format: &'static str,
  /// The create's debug label, when one was given.
  pub label: Option<String>,
}

pub struct GpuBufferInfo {
  pub id: u64,
  pub byte_length: usize,
  /// The create's debug label, when one was given.
  pub label: Option<String>,
}

pub struct GpuProgramInfo {
  pub id: u64,
  /// The create's debug label, when one was given.
  pub label: Option<String>,
}

/// A registered render pipeline: a program paired with the draw state its
/// targets share.
pub struct GpuRenderPipelineInfo {
  pub id: u64,
  pub program_id: u64,
  /// The create's debug label, when one was given.
  pub label: Option<String>,
  pub topology: &'static str,
  /// "none", "add", "multiply" or "alpha".
  pub blend: &'static str,
  /// "none", "back", or "front".
  pub cull: &'static str,
  pub depth: bool,
  pub depth_write: bool,
  /// (name, format string) of the declared interleaved vertex layout.
  pub attributes: Vec<(String, String)>,
  /// (name, format string, buffer slot) of the declared per-instance
  /// layout; empty when the pipeline declares none.
  pub instance_attributes: Vec<(String, String, u32)>,
}

/// One entry of a draw target's ordered list, as reported in
/// `GpuPipelineInfo::draws`.
pub struct GpuDrawInfo {
  /// The entry's stable draw id (target-scoped, UI-allocated).
  pub id: u64,
  /// The registered pipeline this entry draws with.
  pub pipeline_id: Option<u64>,
  pub buffer_id: Option<u64>,
  /// The entry's index buffer; present = it draws indexed, and the range
  /// fields below count indices.
  pub index_buffer_id: Option<u64>,
  /// "uint16" or "uint32", present with `index_buffer_id`.
  pub index_format: Option<&'static str>,
  /// The entry's per-instance buffers in slot order; empty when its
  /// pipeline declares no instance attributes.
  pub instance_buffer_ids: Vec<u64>,
  pub topology: &'static str,
  /// "none", "add", "multiply" or "alpha".
  pub blend: &'static str,
  /// "none", "back", or "front".
  pub cull: &'static str,
  /// Whether this entry's draw writes depth.
  pub depth_write: bool,
  pub first_vertex: i32,
  pub vertex_count: i32,
  pub instance_count: i32,
  /// The float uniforms applied on the entry's most recent render.
  pub params: Vec<(String, ParamValue)>,
  /// sampler2D uniform name -> source texture id.
  pub textures: Vec<TextureBinding>,
}

pub struct GpuPipelineInfo {
  /// The registry id its output texture is sampleable under.
  pub texture_id: u64,
  /// The create's debug label, when one was given (held by the target's
  /// texture entry, same id).
  pub label: Option<String>,
  /// "pipeline" (vertex+fragment over a buffer), "fragment" (fullscreen
  /// pass), or "draws" (a draw target: ordered entry list, see `draws`).
  pub kind: &'static str,
  /// The shared program behind this target's pipeline; None when it was
  /// created through the fused path and owns its program alone.
  pub program_id: Option<u64>,
  /// The registered pipeline this target was created from; None for fragment
  /// targets and the fused path.
  pub pipeline_id: Option<u64>,
  pub buffer_id: Option<u64>,
  /// The first entry's index buffer; present = it draws indexed, and the
  /// range fields below count indices.
  pub index_buffer_id: Option<u64>,
  /// "uint16" or "uint32", present with `index_buffer_id`.
  pub index_format: Option<&'static str>,
  /// The first entry's per-instance buffers in slot order; empty when its
  /// pipeline declares no instance attributes.
  pub instance_buffer_ids: Vec<u64>,
  pub topology: Option<&'static str>,
  /// The vertex count of the target's draw range; None on a fragment-only
  /// target, like the two range fields below.
  pub draw_count: Option<i32>,
  /// First vertex of the draw range (0 = the buffer's start).
  pub first_vertex: Option<i32>,
  /// Instances the range is drawn as (1 = the plain non-instanced draw, 0 =
  /// draws nothing).
  pub instance_count: Option<i32>,
  pub depth: bool,
  /// Effective multisample count (1 = single-sample).
  pub samples: u32,
  /// Whether the draw writes depth; None on a fragment-only target.
  pub depth_write: Option<bool>,
  /// "none", "add", "multiply" or "alpha"; None on a fragment-only target.
  pub blend: Option<&'static str>,
  /// "none", "back", or "front"; None on a fragment-only target.
  pub cull: Option<&'static str>,
  /// (name, format string) of the declared interleaved vertex layout.
  pub attributes: Vec<(String, String)>,
  /// (name, format string, buffer slot) of the declared per-instance
  /// layout; empty when the pipeline declares none.
  pub instance_attributes: Vec<(String, String, u32)>,
  /// sampler2D uniform name -> source texture id; for a draw target (kind
  /// "draws"), its shared (target-level) bindings - per-entry bindings live
  /// in `draws`.
  pub textures: Vec<TextureBinding>,
  /// The float uniforms applied on the most recent render; for a draw target
  /// (kind "draws"), its shared (target-level) params - per-entry params live
  /// in `draws`.
  pub params: Vec<(String, ParamValue)>,
  /// A draw target's ordered entry list (kind "draws"); empty for the fixed
  /// kinds, whose one pass lives in the flat fields above.
  pub draws: Vec<GpuDrawInfo>,
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
  pub pass_issue_micros: u64,
  /// Cumulative GPU-side execution time of those passes, in microseconds
  /// (timer queries; 0 when the context has none).
  pub pass_exec_micros: u64,
}
