//! Render targets: `ShaderTexture` (an FBO-backed target texture rendered by
//! a program, fullscreen-fragment or mesh-pipeline kind) and the retained
//! window-layer target. This is where per-target state lives - the VAO bound
//! to a concrete buffer, depth storage, clear color, last-applied params.

use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::rc::Rc;

use super::pass::{run_pass, PassDraw, PassInput};
use super::program::{release_pipeline, release_program, RenderPipeline, ShaderProgram};
use super::vocab::{blend_name, vertex_stride, AttrFormat, ParamValue, PipelineDesc};
use super::{prev_buffer, prev_framebuffer, prev_texture, prev_vertex_array};

/// The per-target mesh half of a pipeline target: the pipeline it draws with
/// (which owns the draw state), plus everything bound to THIS target - the
/// VAO built against its concrete vertex buffer, that buffer's registry id,
/// the draw count, the private depth storage, and the clear color.
pub(super) struct MeshState {
  pub(super) pipeline: Rc<RenderPipeline>,
  /// Registry id of the shared pipeline this target was created from; None
  /// for the fused create path, whose pipeline is anonymous and dies with the
  /// target.
  pipeline_id: Option<u64>,
  pub(super) vao: glow::VertexArray,
  /// Registry id of the interleaved vertex buffer (Context resolves writes to
  /// re-renders through this). 0 when the pipeline is attributeless.
  buffer_id: u64,
  pub(super) draw_count: Cell<i32>,
  /// Present when the pipeline carries depth state; the renderbuffer stays
  /// private to the FBO (never adopted into Impeller).
  depth: Option<glow::Renderbuffer>,
  pub(super) clear_color: [f32; 4],
  /// Color load op (see `TargetSpec::load`): true = draw over the previous
  /// contents instead of clearing. Only ever true on manual targets.
  pub(super) load: bool,
}

/// An FBO-backed RGBA8 target texture rendered by a ShaderProgram: either a
/// fullscreen fragment pass (`mesh: None`) or a vertex+fragment pipeline
/// drawing an interleaved vertex buffer (`mesh: Some`). The target's GL name
/// is also adopted into Impeller (and held in the TextureRegistry); this
/// struct keeps the program/FBO so the same texture can be re-rendered with
/// new params. Like GpuTexture it never deletes the target name: Impeller
/// owns it once adopted, and deleting here would double-free.
pub struct ShaderTexture {
  /// The program rendering this target: a mesh target's own clone of its
  /// pipeline's program Rc (so render and reflection never branch on kind),
  /// a fragment target's whole identity.
  program: Rc<ShaderProgram>,
  fbo: glow::Framebuffer,
  target: glow::Texture,
  width: u32,
  height: u32,
  /// sampler2D uniform name -> source texture id. Resolved to a live GL texture
  /// at each render by the owner (which holds the texture registry), so an input
  /// whose contents or registry entry changed is picked up automatically.
  sampler_bindings: Vec<(String, u64)>,
  mesh: Option<MeshState>,
  /// The target's current uniform values, folded in by `merge_params` (its
  /// only writer) and read by every render. Held here so a re-render the app
  /// did not ask for directly - a vertex-buffer write, a draw-count change, a
  /// sampled input that changed - needs no params from the caller.
  last_params: RefCell<Vec<(String, ParamValue)>>,
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

/// Create the retained window-layer target: an exactly window-sized RGBA8
/// texture + FBO the frame resolves into while a window shader is active.
/// Exactly window-sized on purpose - the shader samples it with 0..1
/// coordinates, so aligned padding would leak into the sampling contract.
/// Completeness-checked here (unlike shader targets, nothing later would
/// catch it); restores the FBO binding it touches. The new layer starts
/// opaque black (the pass's clear color): a history layer (`uPrevious`) is
/// sampled one frame before anything resolves into it, and undefined storage
/// must not reach a program.
pub fn create_layer_target(
  gl: &glow::Context,
  width: u32,
  height: u32,
) -> Result<(glow::Texture, glow::Framebuffer), String> {
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let result = create_target(gl, width, height).and_then(|(target, fbo)| {
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err(format!("window layer framebuffer incomplete: {status:#x}"));
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
      gl.clear_color(0.0, 0.0, 0.0, 1.0);
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
        program,
        fbo,
        target,
        width,
        height,
        sampler_bindings,
        mesh: None,
        last_params: RefCell::new(Vec::new()),
        sampler: crate::texture::SamplerState::default(),
        manual: false,
        passes: Cell::new(0),
        pass_micros: Cell::new(0),
      })
    }
  }

  /// The fused create path: compile a vertex+fragment pair, wrap it in an
  /// anonymous pipeline, and build a target over it in one step.
  #[allow(clippy::too_many_arguments)]
  pub fn new_pipeline(
    gl: &glow::Context,
    width: u32,
    height: u32,
    vertex_src: &str,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
    desc: PipelineDesc,
    vbo: Option<glow::Buffer>,
    buffer_id: u64,
    draw_count: i32,
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
    Self::from_pipeline(gl, pipeline, None, width, height, sampler_bindings, vbo, buffer_id, draw_count, clear_color)
      .map_err(|(pipeline, e)| {
        release_pipeline(gl, pipeline);
        e
      })
  }

  /// A target over a render pipeline: the pipeline's vertex layout is bound
  /// to this target's concrete buffer in a fresh VAO (attribute locations are
  /// looked up by name, so an attribute the shader does not use is skipped -
  /// its bytes still occupy the stride), and depth state gets a private
  /// renderbuffer. On error the pipeline Rc is handed back so the caller
  /// decides its fate (a fused create releases it, a shared pipeline stays
  /// registered).
  #[allow(clippy::too_many_arguments)]
  pub fn from_pipeline(
    gl: &glow::Context,
    pipeline: Rc<RenderPipeline>,
    pipeline_id: Option<u64>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<(String, u64)>,
    vbo: Option<glow::Buffer>,
    buffer_id: u64,
    draw_count: i32,
    clear_color: [f32; 4],
  ) -> Result<Self, (Rc<RenderPipeline>, String)> {
    if !pipeline.desc.attributes.is_empty() && vbo.is_none() {
      return Err((pipeline, "pipeline declares attributes but no vertex buffer".to_string()));
    }
    let program = pipeline.program.clone();
    let attributes = &pipeline.desc.attributes;
    let depth = pipeline.desc.depth.is_some();

    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
      let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
      let prev_ab = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);

      // Cleanup helper for every early exit below; the pipeline itself
      // travels back in the Err.
      let fail = |gl: &glow::Context,
                  target: Option<glow::Texture>,
                  fbo: Option<glow::Framebuffer>,
                  rb: Option<glow::Renderbuffer>,
                  vao: Option<glow::VertexArray>| {
        if let Some(v) = vao {
          gl.delete_vertex_array(v);
        }
        if let Some(r) = rb {
          gl.delete_renderbuffer(r);
        }
        if let Some(f) = fbo {
          gl.delete_framebuffer(f);
        }
        if let Some(t) = target {
          gl.delete_texture(t);
        }
      };

      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => {
          gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
          return Err((pipeline, e));
        }
      };

      // FBO is still bound; attach a private depth renderbuffer when asked.
      let depth_rb = if depth {
        match gl.create_renderbuffer() {
          Ok(rb) => {
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width as i32, height as i32);
            gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
            Some(rb)
          }
          Err(e) => {
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
            fail(gl, Some(target), Some(fbo), None, None);
            return Err((pipeline, format!("glGenRenderbuffers failed: {e}")));
          }
        }
      } else {
        None
      };

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      if status != glow::FRAMEBUFFER_COMPLETE {
        fail(gl, Some(target), Some(fbo), depth_rb, None);
        return Err((pipeline, format!("pipeline framebuffer incomplete: {status:#x}")));
      }

      // Record the interleaved vertex layout in a VAO. The VAO captures the
      // buffer binding per attribute, so rendering only rebinds the VAO.
      let vao = match gl.create_vertex_array() {
        Ok(vao) => vao,
        Err(e) => {
          fail(gl, Some(target), Some(fbo), depth_rb, None);
          return Err((pipeline, format!("glGenVertexArrays failed: {e}")));
        }
      };
      gl.bind_vertex_array(Some(vao));
      if let Some(vbo) = vbo {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let stride = vertex_stride(attributes);
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

      Ok(ShaderTexture {
        program,
        fbo,
        target,
        width,
        height,
        sampler_bindings,
        mesh: Some(MeshState {
          pipeline,
          pipeline_id,
          vao,
          buffer_id,
          draw_count: Cell::new(draw_count),
          depth: depth_rb,
          clear_color,
          load: false,
        }),
        last_params: RefCell::new(Vec::new()),
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
    if let Some(mesh) = &mut self.mesh {
      mesh.load = load;
    }
    self
  }

  /// Whether the target draws over its previous contents (loadOp "load").
  pub fn load(&self) -> bool {
    self.mesh.as_ref().is_some_and(|m| m.load)
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

  /// Registry id of the shared program behind this target's pipeline; None
  /// for fragment targets and for the fused create path, whose program is
  /// anonymous.
  pub fn program_id(&self) -> Option<u64> {
    self.mesh.as_ref().and_then(|m| m.pipeline.program_id)
  }

  /// Registry id of the shared pipeline this target was created from; None
  /// for fragment targets and the fused create path.
  pub fn pipeline_id(&self) -> Option<u64> {
    self.mesh.as_ref().and_then(|m| m.pipeline_id)
  }

  /// Registry id of the vertex buffer this pipeline draws from, if any.
  pub fn buffer_id(&self) -> Option<u64> {
    self.mesh.as_ref().map(|m| m.buffer_id).filter(|id| *id != 0)
  }

  /// Whether this is a vertex+fragment pipeline (vs a fullscreen fragment pass).
  pub fn is_pipeline(&self) -> bool {
    self.mesh.is_some()
  }

  /// The number of vertices the next render draws; None on a fragment-only
  /// shader.
  pub fn draw_count(&self) -> Option<i32> {
    self.mesh.as_ref().map(|m| m.draw_count.get())
  }

  /// The pipeline's topology as the string `Topology::parse` accepts; None on
  /// a fragment-only shader.
  pub fn topology_name(&self) -> Option<&'static str> {
    self.mesh.as_ref().map(|m| m.pipeline.desc.topology.name())
  }

  /// The declared interleaved attribute layout; empty for fragment-only
  /// shaders and attributeless pipelines.
  pub fn attributes(&self) -> &[(String, AttrFormat)] {
    self.mesh.as_ref().map(|m| m.pipeline.desc.attributes.as_slice()).unwrap_or(&[])
  }

  /// Whether the pipeline renders with a depth buffer attached.
  pub fn has_depth(&self) -> bool {
    self.mesh.as_ref().is_some_and(|m| m.depth.is_some())
  }

  /// Whether the pipeline's draw writes depth; None on a fragment-only shader.
  pub fn depth_write(&self) -> Option<bool> {
    self.mesh.as_ref().map(|m| m.pipeline.desc.depth.map_or(true, |d| d.write))
  }

  /// The pipeline's blend mode as the string `parse_blend` accepts; None on a
  /// fragment-only shader.
  pub fn blend_name(&self) -> Option<&'static str> {
    self.mesh.as_ref().map(|m| blend_name(m.pipeline.desc.blend))
  }

  /// Set the number of vertices the next render draws. Errors on a
  /// fragment-only shader (its fullscreen triangle is fixed).
  pub fn set_draw_count(&self, count: i32) -> Result<(), String> {
    let mesh = self.mesh.as_ref().ok_or_else(|| "not a pipeline texture".to_string())?;
    mesh.draw_count.set(count);
    Ok(())
  }

  /// A copy of the target's current uniform values, for resource
  /// introspection (`render` reads the record directly).
  pub fn last_params(&self) -> Vec<(String, ParamValue)> {
    self.last_params.borrow().clone()
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

  /// Recreate the render target at a new size, keeping the compiled program,
  /// FBO, sampler bindings, and draw state; the caller re-renders afterwards.
  /// The old target texture is NOT deleted here: Impeller owns its GL name via
  /// the adopted Texture handle (see register_shader_target), which dies with
  /// the UI side's last reference once the registry entry is replaced. On
  /// error the old target is left attached and the shader stays usable at its
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

      // A pipeline's private depth buffer must match the color target's size
      // or the FBO goes incomplete.
      let depth_rb = self.mesh.as_ref().and_then(|m| m.depth);
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

  /// Release GL resources owned by this target (FBO, and for pipelines the
  /// VAO and depth renderbuffer), and drop its uses of the pipeline and
  /// program - which delete the underlying GL program only when nothing else
  /// (a registry, another target) still holds them. The target texture is NOT
  /// deleted here: Impeller owns it via the adopted Texture handle in the
  /// TextureRegistry, and that handle is responsible for deletion. The vertex
  /// buffer is owned by the buffer registry, not deleted here either.
  pub fn destroy(self, gl: &glow::Context) {
    unsafe {
      if let Some(mesh) = &self.mesh {
        gl.delete_vertex_array(mesh.vao);
        if let Some(rb) = mesh.depth {
          gl.delete_renderbuffer(rb);
        }
      }
      gl.delete_framebuffer(self.fbo);
    }
    release_program(gl, self.program);
    if let Some(mesh) = self.mesh {
      release_pipeline(gl, mesh.pipeline);
    }
  }

  /// The sampler2D inputs this shader declared, as (uniform name, source texture
  /// id). The owner resolves each id to a live GL texture before rendering.
  pub fn sampler_bindings(&self) -> &[(String, u64)] {
    &self.sampler_bindings
  }

  /// Rebind sampler2D inputs by uniform name; bindings not named keep their
  /// current source, and a name without an existing binding is added (a
  /// declared sampler left unbound at creation). Every name is validated
  /// against the program's active uniforms before anything changes, so a
  /// failed call leaves all bindings intact. The caller re-renders afterwards.
  pub fn set_sampler_bindings(&mut self, updates: &[(String, u64)]) -> Result<(), String> {
    for (name, _) in updates {
      if !self.program.uniforms.contains_key(name) {
        return Err(format!("no active uniform named '{name}'"));
      }
    }
    for (name, src_id) in updates {
      match self.sampler_bindings.iter_mut().find(|(n, _)| n == name) {
        Some(binding) => binding.1 = *src_id,
        None => self.sampler_bindings.push((name.clone(), *src_id)),
      }
    }
    Ok(())
  }

  /// Fold a params update into the current record by name (new names
  /// append, existing names overwrite). Uniforms are program state in GL, so
  /// rendering once with the merged record is equivalent to rendering after
  /// each partial params list; the owner defers that render to its dirty
  /// flush.
  pub fn merge_params(&self, params: &[(String, ParamValue)]) {
    let mut last = self.last_params.borrow_mut();
    for (name, value) in params {
      match last.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = value.clone(),
        None => last.push((name.clone(), value.clone())),
      }
    }
  }

  /// Render the shader into its target texture with its current params (see
  /// `merge_params`, the only writer) and the given resolved sampler inputs
  /// (uniform name -> source GL texture, in the order `sampler_bindings`
  /// declared them). See `run_pass` for the GL state contract;
  /// Context::submit's per-frame fence orders the work ahead of the render
  /// thread sampling the target from its shared GL context, so no glFinish is
  /// needed here.
  pub fn render(&self, gl: &glow::Context, textures: &[PassInput]) {
    let params = self.last_params.borrow();
    let draw = match &self.mesh {
      None => PassDraw::Fullscreen { vertex_count: 3, clear: None },
      Some(mesh) => PassDraw::Mesh(mesh),
    };
    run_pass(gl, &self.program, Some(self.fbo), self.width, self.height, &params, textures, draw);
  }

  /// Draw the resolved inputs over this target's full contents via `program`
  /// (the shared copy program), no clear - the covering triangle writes every
  /// pixel: the copyTexture write. A sampling draw, never a blit (see
  /// `gl::draw_and_resolve` for why blits are not an option on this stack).
  pub fn overwrite_with(&self, gl: &glow::Context, program: &ShaderProgram, textures: &[PassInput]) {
    super::pass::render_program_to_fbo(gl, program, Some(self.fbo), self.width, self.height, textures);
  }

  /// Clear the target to its clear color (and its depth buffer, when
  /// attached) without running the program: the defined initial contents of a
  /// manual target, whose pass may be non-idempotent and therefore must not
  /// run outside an explicit render. Creation and resize would otherwise
  /// leave undefined storage. Scissor, color/depth masks, clear values and
  /// the FBO binding are Impeller-cached state on this shared context: force,
  /// clear, and put everything back (same contract as `run_pass`).
  pub fn clear(&self, gl: &glow::Context) {
    let [r, g, b, a] = self.mesh.as_ref().map(|m| m.clear_color).unwrap_or([0.0; 4]);
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
      if self.mesh.as_ref().is_some_and(|m| m.depth.is_some()) {
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
