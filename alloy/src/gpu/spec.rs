//! The command-channel shapes: everything a create carries from the UI thread
//! to the raster thread in one owned value, serving both the public Context
//! API and the RasterCmd payloads.

use super::vocab::{ParamValue, PipelineDesc};
use crate::texture::SamplerState;

/// Everything `create_shader_target` needs to build one target over a render
/// pipeline: the per-target half of the split (output size, uniform values,
/// sampler inputs, the concrete vertex buffer, draw count, clear, sampling).
/// The draw-state half lives on the pipeline (`gpu::PipelineDesc`). Owned,
/// so the one struct serves both the public API and the raster channel.
pub struct TargetSpec {
  pub width: u32,
  pub height: u32,
  pub params: Vec<(String, ParamValue)>,
  pub textures: Vec<(String, u64)>,
  /// Registry id of the interleaved vertex buffer the pipeline's attributes
  /// describe; 0 = attributeless rendering via gl_VertexID.
  pub buffer: u64,
  /// Number of vertices to draw; negative derives it from buffer size /
  /// vertex stride.
  pub draw_count: i32,
  pub clear_color: [f32; 4],
  /// How the target's output is sampled everywhere (shader inputs, display).
  pub sampler: SamplerState,
  /// Render mode. False (the default): the dirty flush renders the target
  /// whenever its inputs change, which requires the pass to be a pure
  /// function of them. True: the flush never renders it - only an explicit
  /// `render_target` does, in call order - which is what makes
  /// non-idempotent passes (accumulation, feedback) legal. Creation and
  /// resize clear a manual target instead of rendering it.
  pub manual: bool,
  /// Color load op. False (the default, loadOp "clear"): every render clears
  /// to `clear_color` first. True (loadOp "load"): the draw lands over the
  /// previous contents - single-target accumulation. Requires `manual`
  /// (Context rejects the combination otherwise): on a flush-rendered target
  /// the output would depend on how often the flush ran. Depth is per-render
  /// scratch and always clears. Creation and resize still clear, so a load
  /// target starts (and restarts) at `clear_color`.
  pub load: bool,
}

/// Everything `create_pipeline_texture` (the fused convenience) needs:
/// sources to compile, the draw state they run with, and the target to
/// render into - the same two halves the split API takes separately.
pub struct PipelineSpec {
  pub vertex_src: String,
  pub fragment_src: String,
  pub pipeline: PipelineDesc,
  pub target: TargetSpec,
}

/// The window shader declaration: a linked program drawn over the window's
/// finished frame as the last step before present. The frame resolves into a
/// runtime-owned layer texture the program samples as `uniform sampler2D
/// uSource` (top-left origin, like every sampled texture); `iResolution` is
/// the window in physical pixels. Drawn attributeless as triangles at
/// `vertex_count` vertices (3 = the covering triangle).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowShader {
  /// Registered program handle (see `link_shader_program`).
  pub program: u64,
  /// Float uniforms filled by name.
  pub params: Vec<(String, ParamValue)>,
  /// Extra sampler2D inputs: uniform name -> texture registry id.
  pub textures: Vec<(String, u64)>,
  pub vertex_count: i32,
  /// Retain a second layer holding the last resolved frame, exposed to the
  /// program as `uniform sampler2D uPrevious` (one-frame history: motion
  /// echo, frame differencing). Costs one extra window-sized texture while
  /// declared. A fresh history layer samples opaque black until the second
  /// shaded frame.
  pub previous: bool,
}
