//! Render targets: `ShaderTexture` (an FBO-backed target texture rendered as
//! a fullscreen fragment pass or as an ordered list of mesh draws) and the
//! retained layer target. This is where per-target state lives - the draw
//! entries with their VAOs, buffers, params and bindings, the target-owned
//! depth storage, and the clear color.

use glow::HasContext;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;

use super::buffer::{release_buffer, GpuBuffer};
use super::pass::{run_pass, PassDraw, PassInput, ResolvedDraw};
use super::program::{release_pipeline, release_program, RenderPipeline, ShaderProgram};
use super::resources::GpuDrawInfo;
use super::vocab::{blend_name, AttrFormat, DrawRange, ParamValue, PipelineDesc};
use super::{prev_buffer, prev_framebuffer, prev_texture, prev_vertex_array};

/// One draw of a mesh target's ordered list: the pipeline it draws with
/// (which owns the draw state), plus everything bound to THIS entry - the
/// VAO built against its concrete vertex buffer, that buffer's registry id,
/// the draw range, uniform values, and sampler inputs. Entries are addressed
/// by a UI-allocated id that stays stable across add/remove (never an index).
pub(super) struct DrawEntry {
  pub(super) id: u64,
  pub(super) pipeline: Rc<RenderPipeline>,
  /// Registry id of the shared pipeline this entry draws with; None for the
  /// fused create path, whose pipeline is anonymous and dies with the target.
  pipeline_id: Option<u64>,
  pub(super) vao: glow::VertexArray,
  /// The interleaved vertex buffer this entry's VAO reads, held by Rc like
  /// the pipeline so destroying the registry entry in either order is safe;
  /// None when the pipeline is attributeless.
  buffer: Option<Rc<GpuBuffer>>,
  /// Registry id of that buffer (buffer writes re-render targets through
  /// this, see `reads_buffer`). 0 when the pipeline is attributeless.
  buffer_id: u64,
  /// Resolved and bounds-checked UI-side (see `resolve_draw_range`) before
  /// it ever reaches this field.
  pub(super) draw: DrawRange,
  /// This entry's current uniform values, folded in by the params merge (its
  /// only writer) and re-applied at every render - entries sharing a program
  /// overwrite each other's uniforms per pass, so re-application is
  /// mandatory, not redundancy.
  pub(super) params: Vec<(String, ParamValue)>,
  /// sampler2D uniform name -> source texture id. Resolved to a live GL
  /// texture at each render by the owner (which holds the texture registry),
  /// so an input whose contents or registry entry changed is picked up
  /// automatically.
  pub(super) bindings: Vec<(String, u64)>,
}

/// The mesh half of a target: the ordered draw list sharing this target's
/// color (and optional depth) storage, rendered as one pass - clear once,
/// then entries in list order.
pub(super) struct MeshState {
  /// The ordered draw list. The single-draw creates hold exactly one entry
  /// (id 0, `fixed`); draw targets start empty and mutate via
  /// `add_entry`/`remove_entry`.
  pub(super) entries: Vec<DrawEntry>,
  /// Present when the target owns depth storage: explicit on a draw target
  /// (`create_draw_target`'s depth option), derived from the pipeline on the
  /// single-draw creates. The renderbuffer stays private to the FBO (never
  /// adopted into Impeller).
  depth: Option<glow::Renderbuffer>,
  pub(super) clear_color: [f32; 4],
  /// Color load op (see `TargetSpec::load`): true = draw over the previous
  /// contents instead of clearing. Only ever true on manual targets.
  pub(super) load: bool,
  /// The single-draw creates: the entry set is fixed at creation. The
  /// per-target verbs address entry 0; add/remove are rejected (gated
  /// UI-side, backstopped here).
  fixed: bool,
}

/// Which kind of pass renders this target.
pub(super) enum TargetKind {
  /// A fullscreen fragment pass: one program with target-level params and
  /// bindings. No clear, depth, or draw list - the covering triangle writes
  /// every pixel.
  Fragment { program: Rc<ShaderProgram>, params: Vec<(String, ParamValue)>, bindings: Vec<(String, u64)> },
  /// A vertex+fragment mesh target: clear + the ordered draw list.
  Mesh(MeshState),
}

/// An FBO-backed RGBA8 target texture rendered by shader passes: either a
/// fullscreen fragment pass or an ordered list of mesh draws. The target's GL
/// name is also adopted into Impeller (and held in the TextureRegistry); this
/// struct keeps the FBO and draw state so the same texture can be re-rendered
/// with new params. Like GpuTexture it never deletes the target name:
/// Impeller owns it once adopted, and deleting here would double-free.
pub struct ShaderTexture {
  kind: TargetKind,
  fbo: glow::Framebuffer,
  target: glow::Texture,
  width: u32,
  height: u32,
  /// Declared sampling for this target's output (how OTHER passes and the
  /// display draw sample it; the target's own inputs carry their own states).
  /// Survives resize; set via `with_sampler` after construction.
  sampler: crate::texture::SamplerState,
  /// Manual render mode (see `TargetSpec::manual`): the dirty flush never
  /// renders this target, only an explicit RenderTarget command does. Set via
  /// `with_manual` after construction.
  manual: bool,
  /// Cumulative passes rendered into this target and their wall time in
  /// microseconds, recorded by the owner around each render (raster-thread
  /// occupancy, not GPU-side duration; see raster::RasterStats). Survives
  /// resize, dies with the target. Cell because renders take &self.
  passes: Cell<u64>,
  pass_micros: Cell<u64>,
}

/// Target texture + FBO shared by both shader kinds. Returns (target, fbo)
/// with the FBO left bound so the caller can attach more (depth) before the
/// completeness check; the caller restores the previous FBO binding.
fn create_target(gl: &glow::Context, width: u32, height: u32) -> Result<(glow::Texture, glow::Framebuffer), String> {
  unsafe {
    let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);

    let target = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(target));
    gl.tex_image_2d(
      glow::TEXTURE_2D,
      0,
      glow::RGBA8 as i32,
      width as i32,
      height as i32,
      0,
      glow::RGBA,
      glow::UNSIGNED_BYTE,
      glow::PixelUnpackData::Slice(None),
    );
    // No mips exist: the default MIN_FILTER references mipmaps, which would
    // make the texture sampling-incomplete (reads as black) when Impeller
    // samples it.
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));

    let fbo = match gl.create_framebuffer() {
      Ok(fbo) => fbo,
      Err(e) => {
        gl.delete_texture(target);
        return Err(format!("glGenFramebuffers failed: {e}"));
      }
    };
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);
    Ok((target, fbo))
  }
}

/// Attach a fresh DEPTH_COMPONENT24 renderbuffer to the currently bound FBO,
/// restoring the renderbuffer binding it touches.
unsafe fn attach_depth(gl: &glow::Context, width: u32, height: u32) -> Result<glow::Renderbuffer, String> {
  let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
  let rb = gl.create_renderbuffer().map_err(|e| format!("glGenRenderbuffers failed: {e}"))?;
  gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
  gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width as i32, height as i32);
  gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
  gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
  Ok(rb)
}

/// Record a pipeline's interleaved vertex layout against a concrete buffer in
/// a fresh VAO. Attribute locations are looked up by name, so an attribute
/// the shader does not use is skipped - its bytes still occupy the stride.
/// Restores the VAO and array-buffer bindings it touches.
fn build_vao(
  gl: &glow::Context,
  program: &ShaderProgram,
  attributes: &[(String, AttrFormat)],
  buffer: Option<&Rc<GpuBuffer>>,
) -> Result<glow::VertexArray, String> {
  unsafe {
    let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
    let prev_ab = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
    let vao = gl.create_vertex_array().map_err(|e| format!("glGenVertexArrays failed: {e}"))?;
    gl.bind_vertex_array(Some(vao));
    if let Some(buffer) = buffer {
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer.vbo));
      let stride = super::vocab::vertex_stride(attributes);
      let mut offset = 0i32;
      for (name, fmt) in attributes {
        // None means the shader does not (actively) use the attribute; that
        // is fine, the bytes are simply skipped over via the stride.
        if let Some(loc) = gl.get_attrib_location(program.program, name) {
          gl.enable_vertex_attrib_array(loc);
          gl.vertex_attrib_pointer_f32(loc, fmt.components(), glow::FLOAT, false, stride, offset);
        }
        offset += fmt.components() * 4;
      }
    }
    gl.bind_vertex_array(prev_vertex_array(prev_vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev_ab));
    Ok(vao)
  }
}

/// Fold a params update into a record by name (new names append, existing
/// names overwrite): the merge rule shared by target-level and per-entry
/// params writes.
fn merge_record(record: &mut Vec<(String, ParamValue)>, params: &[(String, ParamValue)]) {
  for (name, value) in params {
    match record.iter_mut().find(|(n, _)| n == name) {
      Some(entry) => entry.1 = value.clone(),
      None => record.push((name.clone(), value.clone())),
    }
  }
}

/// Create a retained layer target: an exactly-sized RGBA8 texture + FBO
/// (the window-shader layer, a boundary shader's output or history). Exact
/// on purpose - shaders sample it with 0..1 coordinates, so padding would
/// leak into the sampling contract. Completeness-checked here (unlike shader
/// targets, nothing later would catch it); restores the FBO binding it
/// touches. The new layer starts cleared to `clear`: a history layer
/// (`uPrevious`) is sampled before anything renders into it, and undefined
/// storage must not reach a program - the window path clears opaque black
/// (its frames are opaque), boundary layers clear transparent (a snapshot's
/// empty regions are).
pub fn create_layer_target(
  gl: &glow::Context,
  width: u32,
  height: u32,
  clear: [f32; 4],
) -> Result<(glow::Texture, glow::Framebuffer), String> {
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let result = create_target(gl, width, height).and_then(|(target, fbo)| {
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err(format!("layer framebuffer incomplete: {status:#x}"));
      }
      // The FBO is still bound from create_target. Scissor, color mask, and
      // clear color are Impeller-cached state on this shared context: force
      // a full clear and put all three back.
      let scissor = gl.is_enabled(glow::SCISSOR_TEST);
      let mut prev_mask = [0i32; 4];
      gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut prev_mask);
      let mut prev_clear = [0f32; 4];
      gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);
      gl.disable(glow::SCISSOR_TEST);
      gl.color_mask(true, true, true, true);
      gl.clear_color(clear[0], clear[1], clear[2], clear[3]);
      gl.clear(glow::COLOR_BUFFER_BIT);
      gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
      gl.color_mask(prev_mask[0] != 0, prev_mask[1] != 0, prev_mask[2] != 0, prev_mask[3] != 0);
      if scissor {
        gl.enable(glow::SCISSOR_TEST);
      }
      Ok((target, fbo))
    });
    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    result
  }
}

impl ShaderTexture {
  pub fn new(
    gl: &glow::Context,
    width: u32,
    height: u32,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
  ) -> Result<Self, String> {
    let program = Rc::new(ShaderProgram::new_fragment(gl, fragment_src)?);
    Self::from_fragment_program(gl, program, width, height, sampler_bindings).map_err(|(program, e)| {
      release_program(gl, program);
      e
    })
  }

  /// A fullscreen fragment target over an already-compiled program. On error
  /// the program Rc is handed back so the caller decides its fate.
  pub fn from_fragment_program(
    gl: &glow::Context,
    program: Rc<ShaderProgram>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<(String, u64)>,
  ) -> Result<Self, (Rc<ShaderProgram>, String)> {
    if program.is_pipeline() {
      return Err((program, "program is a pipeline; the target needs a render pipeline".to_string()));
    }
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => return Err((program, e)),
      };
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err((program, format!("shader framebuffer incomplete: {status:#x}")));
      }

      Ok(ShaderTexture {
        kind: TargetKind::Fragment { program, params: Vec::new(), bindings: sampler_bindings },
        fbo,
        target,
        width,
        height,
        sampler: crate::texture::SamplerState::default(),
        manual: false,
        passes: Cell::new(0),
        pass_micros: Cell::new(0),
      })
    }
  }

  /// The fused create path: compile a vertex+fragment pair, wrap it in an
  /// anonymous pipeline, and build a one-entry target over it in one step.
  #[allow(clippy::too_many_arguments)]
  pub fn new_pipeline(
    gl: &glow::Context,
    width: u32,
    height: u32,
    vertex_src: &str,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
    desc: PipelineDesc,
    buffer: Option<Rc<GpuBuffer>>,
    buffer_id: u64,
    draw: DrawRange,
    clear_color: [f32; 4],
  ) -> Result<Self, String> {
    let program = Rc::new(ShaderProgram::new_pipeline(gl, vertex_src, fragment_src)?);
    let pipeline = match RenderPipeline::new(program, None, desc) {
      Ok(p) => Rc::new(p),
      Err((program, e)) => {
        release_program(gl, program);
        return Err(e);
      }
    };
    Self::from_pipeline(gl, pipeline, None, width, height, sampler_bindings, buffer, buffer_id, draw, clear_color)
      .map_err(|(pipeline, e)| {
        release_pipeline(gl, pipeline);
        e
      })
  }

  /// A fixed single-entry target over a render pipeline: the pipeline's
  /// vertex layout is bound to this target's concrete buffer in a fresh VAO,
  /// and the pipeline's depth state gives the target its private depth
  /// storage. On error the pipeline Rc is handed back so the caller decides
  /// its fate (a fused create releases it, a shared pipeline stays
  /// registered).
  #[allow(clippy::too_many_arguments)]
  pub fn from_pipeline(
    gl: &glow::Context,
    pipeline: Rc<RenderPipeline>,
    pipeline_id: Option<u64>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<(String, u64)>,
    buffer: Option<Rc<GpuBuffer>>,
    buffer_id: u64,
    draw: DrawRange,
    clear_color: [f32; 4],
  ) -> Result<Self, (Rc<RenderPipeline>, String)> {
    if !pipeline.desc.attributes.is_empty() && buffer.is_none() {
      return Err((pipeline, "pipeline declares attributes but no vertex buffer".to_string()));
    }
    let depth = pipeline.desc.depth.is_some();

    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => {
          gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
          return Err((pipeline, e));
        }
      };

      // FBO is still bound; attach the private depth renderbuffer when asked.
      let depth_rb = if depth {
        match attach_depth(gl, width, height) {
          Ok(rb) => Some(rb),
          Err(e) => {
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
            gl.delete_framebuffer(fbo);
            gl.delete_texture(target);
            return Err((pipeline, e));
          }
        }
      } else {
        None
      };

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      if status != glow::FRAMEBUFFER_COMPLETE {
        if let Some(rb) = depth_rb {
          gl.delete_renderbuffer(rb);
        }
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err((pipeline, format!("pipeline framebuffer incomplete: {status:#x}")));
      }

      let vao = match build_vao(gl, &pipeline.program, &pipeline.desc.attributes, buffer.as_ref()) {
        Ok(vao) => vao,
        Err(e) => {
          if let Some(rb) = depth_rb {
            gl.delete_renderbuffer(rb);
          }
          gl.delete_framebuffer(fbo);
          gl.delete_texture(target);
          return Err((pipeline, e));
        }
      };

      let entry = DrawEntry {
        id: 0,
        pipeline,
        pipeline_id,
        vao,
        buffer,
        buffer_id,
        draw,
        params: Vec::new(),
        bindings: sampler_bindings,
      };
      Ok(ShaderTexture {
        kind: TargetKind::Mesh(MeshState {
          entries: vec![entry],
          depth: depth_rb,
          clear_color,
          load: false,
          fixed: true,
        }),
        fbo,
        target,
        width,
        height,
        sampler: crate::texture::SamplerState::default(),
        manual: false,
        passes: Cell::new(0),
        pass_micros: Cell::new(0),
      })
    }
  }

  /// A mesh target with an empty, mutable draw list (`create_draw_target`):
  /// color storage plus optional target-owned depth storage, rendered as
  /// clear + entries in list order. Entries arrive via `add_entry`; with none
  /// the render is the clear alone.
  pub fn new_draw_target(
    gl: &glow::Context,
    width: u32,
    height: u32,
    depth: bool,
    clear_color: [f32; 4],
  ) -> Result<Self, String> {
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => {
          gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
          return Err(e);
        }
      };
      let depth_rb = if depth {
        match attach_depth(gl, width, height) {
          Ok(rb) => Some(rb),
          Err(e) => {
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
            gl.delete_framebuffer(fbo);
            gl.delete_texture(target);
            return Err(e);
          }
        }
      } else {
        None
      };
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      if status != glow::FRAMEBUFFER_COMPLETE {
        if let Some(rb) = depth_rb {
          gl.delete_renderbuffer(rb);
        }
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err(format!("draw target framebuffer incomplete: {status:#x}"));
      }
      Ok(ShaderTexture {
        kind: TargetKind::Mesh(MeshState {
          entries: Vec::new(),
          depth: depth_rb,
          clear_color,
          load: false,
          fixed: false,
        }),
        fbo,
        target,
        width,
        height,
        sampler: crate::texture::SamplerState::default(),
        manual: false,
        passes: Cell::new(0),
        pass_micros: Cell::new(0),
      })
    }
  }

  /// Set the declared sampling for this target's output (builder-style, right
  /// after construction).
  pub fn with_sampler(mut self, sampler: crate::texture::SamplerState) -> Self {
    self.sampler = sampler;
    self
  }

  /// Set the render mode (builder-style, right after construction); see the
  /// `manual` field.
  pub fn with_manual(mut self, manual: bool) -> Self {
    self.manual = manual;
    self
  }

  /// Set the color load op (builder-style, right after construction); see
  /// `TargetSpec::load`. A no-op on fragment targets, which have no mesh
  /// state (and cannot be manual anyway).
  pub fn with_load(mut self, load: bool) -> Self {
    if let TargetKind::Mesh(mesh) = &mut self.kind {
      mesh.load = load;
    }
    self
  }

  fn mesh(&self) -> Option<&MeshState> {
    match &self.kind {
      TargetKind::Mesh(mesh) => Some(mesh),
      TargetKind::Fragment { .. } => None,
    }
  }

  fn entry0(&self) -> Option<&DrawEntry> {
    self.mesh().and_then(|m| m.entries.first())
  }

  /// Whether the target draws over its previous contents (loadOp "load").
  pub fn load(&self) -> bool {
    self.mesh().is_some_and(|m| m.load)
  }

  pub fn sampler(&self) -> crate::texture::SamplerState {
    self.sampler
  }

  /// Whether the target renders only on an explicit RenderTarget command.
  pub fn manual(&self) -> bool {
    self.manual
  }

  pub fn gl_texture(&self) -> glow::Texture {
    self.target
  }

  /// Registry id of the shared program behind the first entry's pipeline;
  /// None for fragment targets and for the fused create path, whose program
  /// is anonymous.
  pub fn program_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.pipeline.program_id)
  }

  /// Registry id of the shared pipeline the first entry draws with; None
  /// for fragment targets and the fused create path.
  pub fn pipeline_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.pipeline_id)
  }

  /// Registry id of the vertex buffer the first entry draws from, if any.
  pub fn buffer_id(&self) -> Option<u64> {
    self.entry0().map(|e| e.buffer_id).filter(|id| *id != 0)
  }

  /// Whether this is a mesh target (vs a fullscreen fragment pass).
  pub fn is_pipeline(&self) -> bool {
    self.mesh().is_some()
  }

  /// Whether this is a draw target: a mesh target whose entry list mutates
  /// via add/remove (vs the fixed single-entry creates).
  pub fn is_draw_list(&self) -> bool {
    self.mesh().is_some_and(|m| !m.fixed)
  }

  /// Whether any draw entry fetches from vertex buffer `id`: buffer writes
  /// re-render the targets this returns true for.
  pub fn reads_buffer(&self, id: u64) -> bool {
    self.mesh().is_some_and(|m| m.entries.iter().any(|e| e.buffer_id == id))
  }

  /// The draw range of the first entry; None on a fragment-only shader.
  pub fn draw_range(&self) -> Option<DrawRange> {
    self.entry0().map(|e| e.draw)
  }

  /// The first entry's topology as the string `Topology::parse` accepts;
  /// None on a fragment-only shader.
  pub fn topology_name(&self) -> Option<&'static str> {
    self.entry0().map(|e| e.pipeline.desc.topology.name())
  }

  /// The first entry's declared interleaved attribute layout; empty for
  /// fragment-only shaders and attributeless pipelines.
  pub fn attributes(&self) -> &[(String, AttrFormat)] {
    self.entry0().map(|e| e.pipeline.desc.attributes.as_slice()).unwrap_or(&[])
  }

  /// Whether the target owns depth storage.
  pub fn has_depth(&self) -> bool {
    self.mesh().is_some_and(|m| m.depth.is_some())
  }

  /// Whether the first entry's draw writes depth; None on a fragment-only
  /// shader.
  pub fn depth_write(&self) -> Option<bool> {
    self.entry0().map(|e| e.pipeline.desc.depth.map_or(true, |d| d.write))
  }

  /// The first entry's blend mode as the string `parse_blend` accepts; None
  /// on a fragment-only shader.
  pub fn blend_name(&self) -> Option<&'static str> {
    self.entry0().map(|e| blend_name(e.pipeline.desc.blend))
  }

  /// Set the first entry's draw range (resolved and validated UI-side, see
  /// `Context::set_draw`): the single-draw targets' setDraw. Errors on a
  /// fragment-only shader (its fullscreen triangle is fixed).
  pub fn set_draw(&mut self, range: DrawRange) -> Result<(), String> {
    match &mut self.kind {
      TargetKind::Fragment { .. } => Err("not a pipeline texture".to_string()),
      TargetKind::Mesh(mesh) => match mesh.entries.first_mut() {
        Some(entry) => {
          entry.draw = range;
          Ok(())
        }
        None => Err("target has no draw entries".to_string()),
      },
    }
  }

  /// Append a draw entry to a draw target's list (see `DrawEntry`; validated
  /// UI-side, backstopped here). The entry draws last in list order.
  #[allow(clippy::too_many_arguments)]
  pub fn add_entry(
    &mut self,
    gl: &glow::Context,
    id: u64,
    pipeline: Rc<RenderPipeline>,
    pipeline_id: Option<u64>,
    buffer: Option<Rc<GpuBuffer>>,
    buffer_id: u64,
    draw: DrawRange,
    params: Vec<(String, ParamValue)>,
    bindings: Vec<(String, u64)>,
  ) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    if pipeline.desc.depth.is_some() && mesh.depth.is_none() {
      return Err("pipeline tests depth but the target has no depth buffer (create the draw target with depth: true)".to_string());
    }
    if !pipeline.desc.attributes.is_empty() && buffer.is_none() {
      return Err("pipeline declares attributes but no vertex buffer".to_string());
    }
    let vao = build_vao(gl, &pipeline.program, &pipeline.desc.attributes, buffer.as_ref())?;
    mesh.entries.push(DrawEntry { id, pipeline, pipeline_id, vao, buffer, buffer_id, draw, params, bindings });
    Ok(())
  }

  /// Remove a draw entry by id, releasing its VAO and its uses of the
  /// pipeline and buffer (deleted only when nothing else holds them).
  pub fn remove_entry(&mut self, gl: &glow::Context, id: u64) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    let pos = mesh.entries.iter().position(|e| e.id == id).ok_or_else(|| format!("draw {id} not found"))?;
    let entry = mesh.entries.remove(pos);
    unsafe { gl.delete_vertex_array(entry.vao) };
    release_pipeline(gl, entry.pipeline);
    if let Some(buffer) = entry.buffer {
      release_buffer(gl, buffer);
    }
    Ok(())
  }

  fn entry_mut(&mut self, id: u64) -> Result<&mut DrawEntry, String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    mesh.entries.iter_mut().find(|e| e.id == id).ok_or_else(|| format!("draw {id} not found"))
  }

  /// Fold a params update into one entry's record by name (validated UI-side
  /// against the entry's program).
  pub fn merge_entry_params(&mut self, id: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    merge_record(&mut self.entry_mut(id)?.params, params);
    Ok(())
  }

  /// Set one entry's draw range (resolved and validated UI-side).
  pub fn set_entry_draw(&mut self, id: u64, range: DrawRange) -> Result<(), String> {
    self.entry_mut(id)?.draw = range;
    Ok(())
  }

  /// Rebind one entry's sampler2D inputs by uniform name; bindings not named
  /// keep their current source. Names are validated against the entry's
  /// program before anything changes.
  pub fn set_entry_bindings(&mut self, id: u64, updates: &[(String, u64)]) -> Result<(), String> {
    let entry = self.entry_mut(id)?;
    for (name, _) in updates {
      if !entry.pipeline.program.uniforms.contains_key(name) {
        return Err(format!("no active uniform named '{name}'"));
      }
    }
    for (name, src_id) in updates {
      match entry.bindings.iter_mut().find(|(n, _)| n == name) {
        Some(binding) => binding.1 = *src_id,
        None => entry.bindings.push((name.clone(), *src_id)),
      }
    }
    Ok(())
  }

  /// A copy of the first pass's current uniform values (fragment, or entry
  /// 0), for the flat resource introspection fields.
  pub fn last_params(&self) -> Vec<(String, ParamValue)> {
    match &self.kind {
      TargetKind::Fragment { params, .. } => params.clone(),
      TargetKind::Mesh(mesh) => mesh.entries.first().map(|e| e.params.clone()).unwrap_or_default(),
    }
  }

  /// Record one executed pass into this target (see the `passes` field).
  pub fn record_pass(&self, micros: u64) {
    self.passes.set(self.passes.get() + 1);
    self.pass_micros.set(self.pass_micros.get() + micros);
  }

  /// (cumulative passes, cumulative microseconds) rendered into this target,
  /// for resource introspection.
  pub fn pass_stats(&self) -> (u64, u64) {
    (self.passes.get(), self.pass_micros.get())
  }

  /// Recreate the render target at a new size, keeping the compiled programs,
  /// FBO, entries, and draw state; the caller re-renders afterwards. The old
  /// target texture is NOT deleted here: Impeller owns its GL name via the
  /// adopted Texture handle (see register_shader_target), which dies with the
  /// UI side's last reference once the registry entry is replaced. On error
  /// the old target is left attached and the shader stays usable at its
  /// previous size.
  pub fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) -> Result<(), String> {
    unsafe {
      let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);

      let target = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
      gl.bind_texture(glow::TEXTURE_2D, Some(target));
      gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        width as i32,
        height as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
      );
      // Same sampling state as create_target: no mips exist, so the default
      // MIN_FILTER would make the texture sampling-incomplete (reads black).
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
      gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));

      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);

      // The target's depth buffer must match the color target's size or the
      // FBO goes incomplete.
      let depth_rb = self.mesh().and_then(|m| m.depth);
      if let Some(rb) = depth_rb {
        let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width as i32, height as i32);
        gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
      }

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        // Roll back to the old target (and depth storage) so the shader keeps
        // rendering at its previous size.
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.target), 0);
        if let Some(rb) = depth_rb {
          let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
          gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
          gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, self.width as i32, self.height as i32);
          gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
        gl.delete_texture(target);
        return Err(format!("shader framebuffer incomplete after resize: {status:#x}"));
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

      self.target = target;
      self.width = width;
      self.height = height;
      Ok(())
    }
  }

  /// Release GL resources owned by this target (FBO, depth renderbuffer, and
  /// every entry's VAO), and drop its uses of pipelines, programs, and vertex
  /// buffers - which delete the underlying GL objects only when nothing else
  /// (a registry, another target) still holds them. The target texture is NOT
  /// deleted here: Impeller owns it via the adopted Texture handle in the
  /// TextureRegistry, and that handle is responsible for deletion.
  pub fn destroy(self, gl: &glow::Context) {
    match self.kind {
      TargetKind::Fragment { program, .. } => release_program(gl, program),
      TargetKind::Mesh(mesh) => {
        for entry in mesh.entries {
          unsafe { gl.delete_vertex_array(entry.vao) };
          release_pipeline(gl, entry.pipeline);
          if let Some(buffer) = entry.buffer {
            release_buffer(gl, buffer);
          }
        }
        if let Some(rb) = mesh.depth {
          unsafe { gl.delete_renderbuffer(rb) };
        }
      }
    }
    unsafe { gl.delete_framebuffer(self.fbo) };
  }

  /// The active uniforms of this target's program (fragment, or entry 0 -
  /// the two fused creates are single-entry by construction), for the create
  /// replies that seed the UI-side validation mirror.
  pub fn uniform_table(&self) -> super::vocab::UniformTable {
    match &self.kind {
      TargetKind::Fragment { program, .. } => program.uniform_table(),
      TargetKind::Mesh(mesh) => {
        mesh.entries.first().map(|e| e.pipeline.program.uniform_table()).unwrap_or_default()
      }
    }
  }

  /// The sampler2D inputs of the first pass (fragment, or entry 0), as
  /// (uniform name, source texture id): the flat introspection view and the
  /// create-time validation input.
  pub fn sampler_bindings(&self) -> &[(String, u64)] {
    match &self.kind {
      TargetKind::Fragment { bindings, .. } => bindings,
      TargetKind::Mesh(mesh) => mesh.entries.first().map(|e| e.bindings.as_slice()).unwrap_or(&[]),
    }
  }

  /// Every source texture id any pass of this target samples: the fragment
  /// bindings, or the union over all draw entries. What the flush graph and
  /// the propagation walk read as this target's incoming edges.
  pub fn binding_sources(&self) -> Vec<u64> {
    match &self.kind {
      TargetKind::Fragment { bindings, .. } => bindings.iter().map(|(_, id)| *id).collect(),
      TargetKind::Mesh(mesh) => {
        mesh.entries.iter().flat_map(|e| e.bindings.iter().map(|(_, id)| *id)).collect()
      }
    }
  }

  /// Per-entry introspection for the resource inventory's `draws` list;
  /// empty for fragment targets.
  pub fn draw_infos(&self) -> Vec<GpuDrawInfo> {
    self
      .mesh()
      .map(|m| {
        m.entries
          .iter()
          .map(|e| GpuDrawInfo {
            id: e.id,
            pipeline_id: e.pipeline_id,
            buffer_id: (e.buffer_id != 0).then_some(e.buffer_id),
            topology: e.pipeline.desc.topology.name(),
            blend: blend_name(e.pipeline.desc.blend),
            depth_write: e.pipeline.desc.depth.map_or(true, |d| d.write),
            first_vertex: e.draw.first_vertex,
            vertex_count: e.draw.vertex_count,
            instance_count: e.draw.instance_count,
            params: e.params.clone(),
            textures: e.bindings.clone(),
          })
          .collect()
      })
      .unwrap_or_default()
  }

  /// Rebind the first pass's sampler2D inputs by uniform name (fragment, or
  /// entry 0: the single-draw update path); bindings not named keep their
  /// current source, and a name without an existing binding is added (a
  /// declared sampler left unbound at creation). Every name is validated
  /// against the program's active uniforms before anything changes, so a
  /// failed call leaves all bindings intact. The caller re-renders afterwards.
  pub fn set_sampler_bindings(&mut self, updates: &[(String, u64)]) -> Result<(), String> {
    {
      let uniforms = match &self.kind {
        TargetKind::Fragment { program, .. } => &program.uniforms,
        TargetKind::Mesh(mesh) => match mesh.entries.first() {
          Some(e) => &e.pipeline.program.uniforms,
          None => return Err("target has no draw entries".to_string()),
        },
      };
      for (name, _) in updates {
        if !uniforms.contains_key(name) {
          return Err(format!("no active uniform named '{name}'"));
        }
      }
    }
    let bindings = match &mut self.kind {
      TargetKind::Fragment { bindings, .. } => bindings,
      TargetKind::Mesh(mesh) => {
        &mut mesh.entries.first_mut().expect("entry checked above").bindings
      }
    };
    for (name, src_id) in updates {
      match bindings.iter_mut().find(|(n, _)| n == name) {
        Some(binding) => binding.1 = *src_id,
        None => bindings.push((name.clone(), *src_id)),
      }
    }
    Ok(())
  }

  /// Fold a params update into the first pass's record by name (fragment, or
  /// entry 0: the single-draw update path). Uniforms are program state in GL,
  /// so rendering once with the merged record is equivalent to rendering
  /// after each partial params list; the owner defers that render to its
  /// dirty flush.
  pub fn merge_params(&mut self, params: &[(String, ParamValue)]) {
    match &mut self.kind {
      TargetKind::Fragment { params: record, .. } => merge_record(record, params),
      TargetKind::Mesh(mesh) => {
        if let Some(entry) = mesh.entries.first_mut() {
          merge_record(&mut entry.params, params);
        }
      }
    }
  }

  /// Render the target's pass into its texture: the fullscreen fragment
  /// draw, or clear + the ordered entry list. `resolve` maps a binding list
  /// to live GL textures + sampler objects (the owner holds the registries).
  /// See `run_pass` for the GL state contract; Context::submit's per-frame
  /// fence orders the work ahead of the render thread sampling the target
  /// from its shared GL context, so no glFinish is needed here.
  pub fn render(&self, gl: &glow::Context, resolve: &dyn Fn(&[(String, u64)]) -> Vec<PassInput>) {
    match &self.kind {
      TargetKind::Fragment { program, params, bindings } => {
        let inputs = resolve(bindings);
        let draw = PassDraw::Fullscreen { program, params, textures: &inputs, vertex_count: 3, clear: None };
        run_pass(gl, Some(self.fbo), self.width, self.height, draw);
      }
      TargetKind::Mesh(mesh) => {
        let draws: Vec<ResolvedDraw> = mesh
          .entries
          .iter()
          .map(|e| ResolvedDraw {
            program: &e.pipeline.program,
            desc: &e.pipeline.desc,
            vao: e.vao,
            range: e.draw,
            params: &e.params,
            inputs: resolve(&e.bindings),
          })
          .collect();
        let draw = PassDraw::Draws {
          clear: (!mesh.load).then_some(mesh.clear_color),
          depth: mesh.depth.is_some(),
          draws: &draws,
        };
        run_pass(gl, Some(self.fbo), self.width, self.height, draw);
      }
    }
  }

  /// Draw the resolved inputs over this target's full contents via `program`
  /// (the shared copy program), no clear - the covering triangle writes every
  /// pixel: the copyTexture write. A sampling draw, never a blit (see
  /// `gl::draw_and_resolve` for why blits are not an option on this stack).
  pub fn overwrite_with(&self, gl: &glow::Context, program: &ShaderProgram, textures: &[PassInput]) {
    super::pass::render_program_to_fbo(gl, program, Some(self.fbo), self.width, self.height, &[], textures);
  }

  /// Clear the target to its clear color (and its depth buffer, when
  /// attached) without running any program: the defined initial contents of a
  /// manual target, whose pass may be non-idempotent and therefore must not
  /// run outside an explicit render. Creation and resize would otherwise
  /// leave undefined storage. Scissor, color/depth masks, clear values and
  /// the FBO binding are Impeller-cached state on this shared context: force,
  /// clear, and put everything back (same contract as `run_pass`).
  pub fn clear(&self, gl: &glow::Context) {
    let [r, g, b, a] = self.mesh().map(|m| m.clear_color).unwrap_or([0.0; 4]);
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let scissor = gl.is_enabled(glow::SCISSOR_TEST);
      let mut prev_mask = [0i32; 4];
      gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut prev_mask);
      let mut prev_clear = [0f32; 4];
      gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);

      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
      gl.disable(glow::SCISSOR_TEST);
      gl.color_mask(true, true, true, true);
      gl.clear_color(r, g, b, a);
      if self.mesh().is_some_and(|m| m.depth.is_some()) {
        let prev_depth_mask = gl.get_parameter_i32(glow::DEPTH_WRITEMASK) != 0;
        let prev_clear_depth = gl.get_parameter_f32(glow::DEPTH_CLEAR_VALUE);
        gl.depth_mask(true);
        // Always the far plane; Impeller's clip passes leave 0.0 behind (see
        // run_pass).
        gl.clear_depth_f32(1.0);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        gl.depth_mask(prev_depth_mask);
        gl.clear_depth_f32(prev_clear_depth);
      } else {
        gl.clear(glow::COLOR_BUFFER_BIT);
      }

      gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
      gl.color_mask(prev_mask[0] != 0, prev_mask[1] != 0, prev_mask[2] != 0, prev_mask[3] != 0);
      if scissor {
        gl.enable(glow::SCISSOR_TEST);
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    }
  }
}
