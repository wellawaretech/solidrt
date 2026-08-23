//! The capture and readback path: offscreen rasterization of display lists
//! into adopted textures (snapshot boundaries, node captures), independent of
//! the frame loop in the parent module.

use std::num::NonZeroU32;
use std::sync::atomic::Ordering;

use glow::HasContext;
use impellers::{DisplayList, ISize, Texture};

use super::RasterState;
use crate::gl;
use crate::gpu::{NodeShader, PassInput};
use crate::texture::SamplerState;

impl RasterState {
  /// Rasterize a display list into a new adopted texture of the given pixel
  /// size, ready for sampling.
  pub(super) fn rasterize(&mut self, dl: &DisplayList, width: u32, height: u32, aa: bool) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let flipped = flip_for_fbo(dl, height)?;
    gl::render_display_list_to_texture(&self.gl, &mut self.impeller_ctx, &mut self.offscreen_rig, &flipped, size, aa)
  }

  /// Re-rasterize a display list into an existing adopted texture whose
  /// backing is exactly `width` x `height` (the UI thread only reuses at an
  /// exact match).
  pub(super) fn rasterize_into(
    &mut self,
    dl: &DisplayList,
    texture: &Texture,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<(), String> {
    let size = ISize::new(width as i64, height as i64);
    let flipped = flip_for_fbo(dl, height)?;
    gl::render_display_list_into_texture(
      &self.gl,
      &mut self.impeller_ctx,
      &mut self.offscreen_rig,
      &flipped,
      texture,
      size,
      aa,
    )
  }

  /// Rasterize a shaded snapshot boundary and run its node shader pass in
  /// one trip (see `RasterCmd::RasterizeDlShaded`). Some(texture) reuses
  /// storage in place; the UI side only passes handles whose dimensions
  /// match exactly. With `shader.previous`, `history` binds as `uPrevious`
  /// and a missing history is created transparent (the first shaded
  /// rasterization, or a canvas resize); the UI side owns the source/history
  /// role rotation across calls.
  pub(super) fn rasterize_shaded(
    &mut self,
    dl: &DisplayList,
    width: u32,
    height: u32,
    aa: bool,
    shader: &NodeShader,
    source: Option<Texture>,
    output: Option<Texture>,
    history: Option<Texture>,
  ) -> Result<(Texture, Texture, Option<Texture>), String> {
    let size = ISize::new(width as i64, height as i64);
    let flipped = flip_for_fbo(dl, height)?;
    let source = match source {
      Some(texture) => {
        gl::render_display_list_into_texture(
          &self.gl,
          &mut self.impeller_ctx,
          &mut self.offscreen_rig,
          &flipped,
          &texture,
          size,
          aa,
        )?;
        texture
      }
      None => gl::render_display_list_to_texture(
        &self.gl,
        &mut self.impeller_ctx,
        &mut self.offscreen_rig,
        &flipped,
        size,
        aa,
      )?,
    };
    let history = match (shader.previous, history) {
      (false, _) => None,
      (true, Some(h)) => Some(h),
      (true, None) => Some(self.create_history_texture(width, height)?),
    };
    let output = self.node_shader_pass(shader, &source, output, history.as_ref(), width, height)?;
    Ok((source, output, history))
  }

  /// An adopted, transparent, exactly-sized texture for a fresh `uPrevious`
  /// history: it is sampled before anything ever renders into it, and a
  /// snapshot's empty regions are transparent, so its defined start is too.
  fn create_history_texture(&mut self, width: u32, height: u32) -> Result<Texture, String> {
    let (tex, fbo) = crate::gpu::create_layer_target(&self.gl, width, height, [0.0; 4])?;
    unsafe { self.gl.delete_framebuffer(fbo) };
    match unsafe { self.impeller_ctx.adopt_opengl_texture(width, height, 1, tex.0.get() as u64) } {
      Some(adopted) => Ok(adopted),
      None => {
        unsafe { self.gl.delete_texture(tex) };
        Err("failed to adopt history texture".to_string())
      }
    }
  }

  /// One node shader pass: `shader.program` over `source` into `output`
  /// (allocated, adopted and returned when None - Impeller then owns the GL
  /// name, like every snapshot texture). The boundary's rasterization binds
  /// as `uSource` through a runtime sampler (linear/clamp), never its
  /// texture-object state: Impeller rewrites object state on textures it
  /// touches, so on this shared context only a bound sampler object is a
  /// reliable contract. Extra declared inputs resolve through the registry
  /// by id with their declared sampling, a missing id dropping to unbound
  /// (samples black), the same contract as shader targets.
  pub(super) fn node_shader_pass(
    &mut self,
    shader: &NodeShader,
    source: &Texture,
    output: Option<Texture>,
    history: Option<&Texture>,
    width: u32,
    height: u32,
  ) -> Result<Texture, String> {
    let program = self
      .programs
      .get(&shader.program)
      .ok_or_else(|| format!("program {} not found", shader.program))?
      .clone();
    let source_name = gl_name(source)?;

    // Reused output: wrap the existing name in a scratch framebuffer for the
    // one draw. Fresh output: an exact-size layer target (texture + FBO).
    let (out_name, fbo) = match &output {
      Some(texture) => {
        let name = gl_name(texture)?;
        (name, pass_fbo(&self.gl, name)?)
      }
      None => crate::gpu::create_layer_target(&self.gl, width, height, [0.0; 4])?,
    };

    let mut textures: Vec<PassInput> =
      vec![("uSource".to_string(), source_name, Some(self.samplers.get(SamplerState::default())))];
    if let Some(history) = history {
      textures.push(("uPrevious".to_string(), gl_name(history)?, Some(self.samplers.get(SamplerState::default()))));
    }
    for b in &shader.textures {
      match self.textures.get(&b.id) {
        Some(gpu) => {
          textures.push((b.name.clone(), gpu.gl_texture, Some(self.samplers.get(gpu.sampler.overridden(&b.sampler)))))
        }
        None => log::warn!("[alloy] node shader input '{}': texture {} not found", b.name, b.id),
      }
    }

    let start = std::time::Instant::now();
    self.pass_timer.begin(&self.gl);
    crate::gpu::render_program_to_fbo(&self.gl, &program, Some(fbo), width, height, &shader.params, &textures);
    self.pass_timer.end(&self.gl, crate::gpu::Timed::Pass { target: 0 });
    self.stats.passes.fetch_add(1, Ordering::Relaxed);
    self.stats.pass_issue_micros.fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);

    unsafe { self.gl.delete_framebuffer(fbo) };
    match output {
      Some(texture) => Ok(texture),
      None => match unsafe { self.impeller_ctx.adopt_opengl_texture(width, height, 1, out_name.0.get() as u64) } {
        Some(adopted) => Ok(adopted),
        None => {
          unsafe { self.gl.delete_texture(out_name) };
          Err("failed to adopt node shader output".to_string())
        }
      },
    }
  }
}

/// The GL name behind an adopted Impeller texture, as a glow handle.
fn gl_name(texture: &Texture) -> Result<glow::NativeTexture, String> {
  NonZeroU32::new(texture.get_opengl_handle() as u32)
    .map(glow::NativeTexture)
    .ok_or_else(|| "texture has no GL handle".to_string())
}

/// Wrap `tex` in a scratch framebuffer for one pass; the caller deletes it.
/// Restores the framebuffer binding it touches.
fn pass_fbo(gl: &glow::Context, tex: glow::NativeTexture) -> Result<glow::Framebuffer, String> {
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let fbo = gl.create_framebuffer().map_err(|e| format!("glGenFramebuffers failed: {e}"))?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    gl.bind_framebuffer(glow::FRAMEBUFFER, NonZeroU32::new(prev_fbo as u32).map(glow::NativeFramebuffer));
    if status != glow::FRAMEBUFFER_COMPLETE {
      gl.delete_framebuffer(fbo);
      return Err(format!("node shader framebuffer incomplete: {status:#x}"));
    }
    Ok(fbo)
  }
}

/// A wrapped FBO is treated like a window backbuffer, which GL stores
/// bottom-up; pre-flip the content so the texture ends up upright.
pub(super) fn flip_for_fbo(dl: &DisplayList, height: u32) -> Result<DisplayList, String> {
  let mut flipped = impellers::DisplayListBuilder::new(None);
  flipped.translate(0.0, height as f32);
  flipped.scale(1.0, -1.0);
  flipped.draw_display_list(dl, 1.0);
  flipped.build().ok_or_else(|| "failed to build flipped display list".to_string())
}
