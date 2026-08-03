//! The command-channel shapes: everything a create carries from the UI thread
//! to the raster thread in one owned value, serving both the public Context
//! API and the RasterCmd payloads.

use super::vocab::{DrawRange, ParamValue, PipelineDesc};
use crate::texture::SamplerState;

/// Everything `create_shader_target` needs to build one target over a render
/// pipeline: the per-target half of the split (output size, uniform values,
/// sampler inputs, the concrete vertex buffer, draw range, clear, sampling).
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
  /// Which vertices to draw and how many instances (see `DrawRange`). A
  /// negative `vertex_count` here means "the rest of the buffer"; Context
  /// resolves it (`resolve_draw_range`) before the spec crosses to the
  /// raster thread.
  pub draw: DrawRange,
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
  /// Free-form debug name for the target's texture (WebGPU's label),
  /// surfaced in the resource inventory and raster-side messages.
  pub label: Option<String>,
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

/// A shader declared on a snapshot repaint boundary: one fullscreen pass of a
/// linked program over the boundary's rasterized subtree, composited in its
/// place (see `rendertree::composite::snapshot_node`). The rasterization
/// binds as `uniform sampler2D uSource` (top-left origin, like every sampled
/// texture); `iResolution` is the boundary in physical pixels. The program
/// contract matches shader targets, not the window pass: offscreen in,
/// offscreen out.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeShader {
  /// Registered program handle (see `link_shader_program`).
  pub program: u64,
  /// Float uniforms filled by name.
  pub params: Vec<(String, ParamValue)>,
  /// Extra sampler2D inputs: uniform name -> texture registry id.
  pub textures: Vec<(String, u64)>,
  /// Transparent margin in logical px on every side of the layout box, for
  /// the effect to write into (glow, shadow, bleeding blur). Composite-side
  /// geometry only: it grows the rasterized canvas and the composited quad,
  /// while the subtree's own paint stays clipped to the layout box; the pass
  /// itself never reads it (declare an app uniform to know the margin in the
  /// program).
  pub outset: f32,
  /// Retain the prior rasterization of the subtree, exposed to the program
  /// as `uniform sampler2D uPrevious`. Source history, not output history:
  /// it rotates when the content actually re-rasterizes, not per frame - so
  /// for a static subtree with animated params uPrevious equals uSource,
  /// and on a content change it holds exactly the old look (transition
  /// material: cross-dissolve old into new). Feedback/accumulation is not
  /// this; that stays with manual targets. Costs one extra canvas-sized
  /// texture while declared; transparent until the first rotation, and
  /// reset to transparent by a canvas resize.
  pub previous: bool,
}
