//! Display-list draw paths through the rig: into a fresh or reused offscreen
//! texture (snapshots, captures), into a retained layer FBO, and into the
//! window's default framebuffer.

use super::rig::{
  msrtt, prev_framebuffer, prev_renderbuffer, prev_texture, supports_invalidate, window_samples, OffscreenDraw,
  OffscreenRig, EXT_RESOLVE_COPY_SRC, MSAA_SAMPLES,
};
use glow::HasContext;
use impellers::{Context as ImpellerContext, DisplayList, ISize, PixelFormat, Texture};
use std::num::NonZeroU32;

/// Rasterize a display list into a new GL texture and adopt it into Impeller,
/// which becomes the single owner of the GL name (adoption transfers handle
/// ownership per the Impeller API; we never glDeleteTextures it ourselves).
/// The calling thread must have the GL context current and `impeller_ctx` must
/// have been created on it. The texture is later sampled on this same context
/// (the process has exactly one), so GL program order covers all ordering and
/// no glFinish or fence is needed.
///
/// Storage is exactly `size`: no tile-alignment padding. A historical 64px
/// round-up (for suspected Android tile-boundary corruption) was removed -
/// the corruption's real cause was cross-context completion, a bug class the
/// single-context raster thread ended, and the exactly-window-sized shader
/// layer had long rendered through this same rig on Android without padding.
pub(crate) fn render_display_list_to_texture(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
  aa: bool,
) -> Result<Texture, String> {
  let (width, height) = (size.width as i32, size.height as i32);
  let alloc = (width, height);
  let (alloc_width, alloc_height) = alloc;

  // The resolve target: a plain single-sample texture that Impeller adopts.
  // draw_offscreen either draws into it directly (single-sample) or resolves a
  // multisampled render into it.
  let tex = unsafe {
    let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
    let tex = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
      glow::TEXTURE_2D,
      0,
      glow::RGBA8 as i32,
      alloc_width,
      alloc_height,
      0,
      glow::RGBA,
      glow::UNSIGNED_BYTE,
      glow::PixelUnpackData::Slice(None),
    );
    // No mips exist: the default MIN_FILTER references mipmaps, which would
    // make the texture sampling-incomplete (reads as black).
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));
    tex
  };

  match draw_offscreen_with_fallback(gl, impeller_ctx, rig, dl, tex, size, alloc, aa) {
    Ok(()) => {
      unsafe { impeller_ctx.adopt_opengl_texture(alloc_width as u32, alloc_height as u32, 1, tex.0.get() as u64) }
        .ok_or_else(|| "failed to adopt offscreen texture".to_string())
    }
    Err(e) => {
      unsafe { gl.delete_texture(tex) };
      Err(e)
    }
  }
}

/// Re-rasterize a display list into an already-adopted offscreen texture,
/// reusing its storage. The texture's backing must be exactly `size` (the
/// caller only reuses at an exact dimension match). The texture's owner is
/// unchanged - Impeller adopted the GL name when the texture was first
/// created and keeps it.
pub(crate) fn render_display_list_into_texture(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  texture: &Texture,
  size: ISize,
  aa: bool,
) -> Result<(), String> {
  let gl_handle = texture.get_opengl_handle();
  let tex =
    glow::NativeTexture(NonZeroU32::new(gl_handle as u32).ok_or_else(|| "texture has no GL handle".to_string())?);
  let alloc = (size.width as i32, size.height as i32);
  draw_offscreen_with_fallback(gl, impeller_ctx, rig, dl, tex, size, alloc, aa)
}

/// Draw `dl` into `tex` at 4x MSAA (or single-sample when the caller opted
/// out of AA), dropping to single-sample for the rest of the process if the
/// driver rejects the multisampled config once (the same latch the window
/// path honours).
fn draw_offscreen_with_fallback(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  tex: glow::NativeTexture,
  size: ISize,
  alloc: (i32, i32),
  aa: bool,
) -> Result<(), String> {
  // Match the window's 4x request, clamped to the driver ceiling.
  let max_samples = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
  let samples = if !aa || rig.msaa_unavailable { 1 } else { MSAA_SAMPLES.min(max_samples) };
  let mut outcome = draw_offscreen(gl, impeller_ctx, rig, dl, tex, size, alloc, samples);
  if matches!(outcome, OffscreenDraw::MsaaUnavailable) {
    log::warn!("[alloy] offscreen MSAA unavailable; rendering snapshots without anti-aliasing");
    rig.latch_msaa_unavailable(gl);
    outcome = draw_offscreen(gl, impeller_ctx, rig, dl, tex, size, alloc, 1);
  }
  match outcome {
    OffscreenDraw::Done => Ok(()),
    OffscreenDraw::Failed(e) => Err(e),
    // The single-sample retry above resolves to Done or Failed, so this arm
    // is unreachable; treat it as a failure defensively.
    OffscreenDraw::MsaaUnavailable => Err("offscreen framebuffer incomplete".to_string()),
  }
}

/// Render `dl` into `tex` via the retained rig with `samples`x multisampling:
/// a count below 2 draws straight into `tex`; >= 2 draws into the rig's
/// multisampled storage and resolves the `alloc` subrect into `tex` with
/// glBlitFramebuffer. `alloc` is the aligned backing size of `tex`; `size` is
/// the logical viewport handed to Impeller. A GL context must be current.
/// Restores the framebuffer and renderbuffer bindings it touches so
/// Impeller's cached GL state stays valid. `tex` is detached from the rig's
/// FBOs before returning: the rig outlives every resolve texture, and a
/// deleted texture left attached to an unbound FBO is a dangling reference.
fn draw_offscreen(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  tex: glow::NativeTexture,
  size: ISize,
  alloc: (i32, i32),
  samples: i32,
) -> OffscreenDraw {
  let (alloc_width, alloc_height) = alloc;
  let use_msaa = samples >= 2;

  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let prev_rbo = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);

    // The depth-stencil attachment is load-bearing either way: Impeller fills
    // non-convex paths with stencil-then-cover and culls clips via depth;
    // without it the cover pass floods the path bounds.
    let ensured = if use_msaa { rig.ensure_msaa(gl, alloc, samples) } else { rig.ensure_ss_depth(gl, alloc) };
    gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
    if let Err(e) = ensured {
      return OffscreenDraw::Failed(e);
    }

    let draw_fbo = match rig.draw_fbo {
      Some(fbo) => fbo,
      None => match gl.create_framebuffer() {
        Ok(fbo) => {
          rig.draw_fbo = Some(fbo);
          fbo
        }
        Err(e) => return OffscreenDraw::Failed(format!("glGenFramebuffers failed: {e}")),
      },
    };

    // Re-attaching an unchanged object to a reused FBO is cheap; attaching
    // unconditionally keeps the msaa/single-sample switch stateless.
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(draw_fbo));
    if use_msaa {
      let msaa = rig.msaa.as_ref().expect("ensure_msaa populated the rig");
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(msaa.color));
      gl.framebuffer_renderbuffer(
        glow::FRAMEBUFFER,
        glow::DEPTH_STENCIL_ATTACHMENT,
        glow::RENDERBUFFER,
        Some(msaa.depth_stencil),
      );
    } else {
      let ds = rig.ss_depth.as_ref().expect("ensure_ss_depth populated the rig");
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(ds.rbo));
    }

    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    if status != glow::FRAMEBUFFER_COMPLETE {
      if !use_msaa {
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, None, 0);
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      // Under MSAA an incomplete config is recoverable by dropping MSAA (the
      // caller latches it); a single-sample incomplete FBO is fatal.
      return if use_msaa {
        OffscreenDraw::MsaaUnavailable
      } else {
        OffscreenDraw::Failed(format!("offscreen framebuffer incomplete: {status:#x}"))
      };
    }

    // A prior Impeller pass may leave the scissor test enabled; the clear and
    // the resolve blit below both honour it.
    gl.disable(glow::SCISSOR_TEST);
    // Fresh storage is driver-defined (on Android's tile-based GPUs, leftover
    // tile data from unrelated content) and reused rig storage carries the
    // previous raster; force a defined transparent base either way.
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);

    // A wrapped-FBO surface is use-and-throw: draw once, then drop it.
    let mut result = match impeller_ctx.wrap_fbo(draw_fbo.0.get() as u64, PixelFormat::RGBA8888, size) {
      Some(mut surface) => surface.draw_display_list(dl).map_err(|e| format!("offscreen draw failed: {e}")),
      None => Err("wrap_fbo failed for offscreen framebuffer".to_string()),
    };

    // Resolve the multisampled color into the single-sample adopt target. The
    // full aligned rect is blitted so content lands identically to the direct
    // single-sample path regardless of where Impeller's viewport placed it.
    if result.is_ok() && use_msaa {
      let resolve_fbo = match rig.resolve_fbo {
        Some(fbo) => Some(fbo),
        None => match gl.create_framebuffer() {
          Ok(fbo) => {
            rig.resolve_fbo = Some(fbo);
            Some(fbo)
          }
          Err(e) => {
            result = Err(format!("glGenFramebuffers failed (resolve): {e}"));
            None
          }
        },
      };
      if let Some(resolve_fbo) = resolve_fbo {
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(resolve_fbo));
        gl.framebuffer_texture_2d(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
        if gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
          gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(draw_fbo));
          gl.blit_framebuffer(
            0,
            0,
            alloc_width,
            alloc_height,
            0,
            0,
            alloc_width,
            alloc_height,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
          );
          // The multisampled contents are dead after the resolve; the
          // invalidate keeps tilers from writing them back to main memory.
          if supports_invalidate(gl) {
            gl.invalidate_framebuffer(
              glow::READ_FRAMEBUFFER,
              &[glow::COLOR_ATTACHMENT0, glow::DEPTH_STENCIL_ATTACHMENT],
            );
          }
        } else {
          result = Err("offscreen resolve framebuffer incomplete".to_string());
        }
        gl.framebuffer_texture_2d(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, None, 0);
      }
    }
    if !use_msaa {
      // Impeller's draw may have rebound framebuffers; reclaim the draw FBO
      // to invalidate its transient depth-stencil and detach the target.
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(draw_fbo));
      if supports_invalidate(gl) {
        gl.invalidate_framebuffer(glow::FRAMEBUFFER, &[glow::DEPTH_STENCIL_ATTACHMENT]);
      }
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, None, 0);
    }

    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    // No glFinish: the adopted texture is sampled later on this same context,
    // so GL program order already sequences the draw before the sampling.

    match result {
      Ok(()) => OffscreenDraw::Done,
      Err(e) => OffscreenDraw::Failed(e),
    }
  }
}

/// Rasterize a display list into the retained rig at the window's physical
/// size and resolve it 1:1 into the default framebuffer (FBO 0). The display
/// list is drawn unflipped: Impeller treats every wrapped FBO as a bottom-up
/// window target, so the rig content is already in window orientation and
/// the straight blit preserves it (only offscreen textures that get sampled
/// pre-flip; see `flip_for_fbo`). A reversed-Y resolve blit would be the
/// alternative, and that is a driver-inconsistent path under multisampling,
/// so it is deliberately never issued. MSAA matches the snapshot path and
/// falls back to single-sample via the same process-wide latch when the
/// driver rejects the multisampled config.
pub(crate) fn render_display_list_to_window(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
) -> Result<(), String> {
  // Multisampled-backbuffer fast path (Android, see configure_opengl): the
  // driver multisamples FBO 0 inside tile memory and resolves at swap, so
  // the frame draws straight into the window - no rig pass, no resolve
  // copy, and MSAA costs almost nothing. The layer variant below still goes
  // via the rig (its target must end up in a sampleable texture).
  if window_samples(gl) >= 2 {
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      gl.bind_framebuffer(glow::FRAMEBUFFER, None);
      // Same defined-base rationale as the rig path: the backbuffer carries
      // driver-defined garbage or a stale frame.
      gl.disable(glow::SCISSOR_TEST);
      gl.clear_color(0.0, 0.0, 0.0, 0.0);
      gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
      let result = match impeller_ctx.wrap_fbo(0, PixelFormat::RGBA8888, size) {
        Some(mut surface) => surface.draw_display_list(dl).map_err(|e| format!("frame draw failed: {e}")),
        None => Err("wrap_fbo failed for window framebuffer".to_string()),
      };
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      return result;
    }
  }
  render_display_list_via_rig(gl, impeller_ctx, rig, dl, size, None)
}

/// Rasterize a display list into the retained rig at `size` and resolve it
/// 1:1 into `layer` (a single-sample FBO of that size, see
/// `gpu::create_layer_target`). Orientation is the caller's choice: the
/// window shader path hands a display list already flipped for sampling (the
/// layer is read as a top-left-origin texture, see `flip_for_fbo`, and the
/// pass's vertex stage flips back to window orientation), while the stats
/// overlay hands its list unflipped so the layer shares FBO 0's bottom-up
/// convention and composites with no flip anywhere. MSAA and the no-MSAA
/// latch behave exactly like the window path.
pub(crate) fn render_display_list_to_layer(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
  layer: glow::NativeFramebuffer,
) -> Result<(), String> {
  render_display_list_via_rig(gl, impeller_ctx, rig, dl, size, Some(layer))
}

fn render_display_list_via_rig(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
  dst: Option<glow::NativeFramebuffer>,
) -> Result<(), String> {
  let max_samples = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
  let samples = if rig.msaa_unavailable { 0 } else { MSAA_SAMPLES.min(max_samples) };
  let mut outcome = draw_and_resolve(gl, impeller_ctx, rig, dl, size, dst, samples);
  if matches!(outcome, OffscreenDraw::MsaaUnavailable) {
    log::warn!("[alloy] window MSAA unavailable; rendering without anti-aliasing");
    rig.latch_msaa_unavailable(gl);
    outcome = draw_and_resolve(gl, impeller_ctx, rig, dl, size, dst, 0);
  }
  match outcome {
    OffscreenDraw::Done => Ok(()),
    OffscreenDraw::Failed(e) => Err(e),
    OffscreenDraw::MsaaUnavailable => Err("window framebuffer incomplete".to_string()),
  }
}

/// Render `dl` into the rig's storage and blit the window-sized rect into
/// `dst` (None = the default framebuffer; Some = the retained window layer).
/// `samples >= 2` draws multisampled - in-tile when the driver has
/// EXT_multisampled_render_to_texture (the blit is then a plain copy),
/// otherwise into the explicit multisampled renderbuffers (the blit is the
/// resolve). `samples == 0` draws single-sample. Restores the framebuffer,
/// renderbuffer, and texture bindings it touches so Impeller's cached GL
/// state stays valid.
fn draw_and_resolve(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
  dst: Option<glow::NativeFramebuffer>,
  samples: i32,
) -> OffscreenDraw {
  let (width, height) = (size.width as i32, size.height as i32);
  // The rig's transient storage is 64px-quantized purely as an allocation
  // granularity: it grows monotonically and coarse steps keep slightly
  // different raster sizes from each forcing a regrow. Content renders into
  // the corner and the blit reads only the window-sized rect, so this never
  // shapes what a consumer sees (resolve targets are exact-size).
  let align_up = |v: i32| (v + 63) & !63;
  let alloc = (align_up(width), align_up(height));

  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let prev_rbo = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);

    let draw_fbo = match rig.draw_fbo {
      Some(fbo) => fbo,
      None => match gl.create_framebuffer() {
        Ok(fbo) => {
          rig.draw_fbo = Some(fbo);
          fbo
        }
        Err(e) => return OffscreenDraw::Failed(format!("glGenFramebuffers failed: {e}")),
      },
    };

    // In-tile MSAA first (EXT_multisampled_render_to_texture, see MsrttFns):
    // the driver multisamples inside tile memory and resolves into the rig's
    // single-sample texture at writeback, so the closing blit is a plain
    // copy. The explicit path below stores and re-reads sample-count
    // multiples of the frame instead, which on bandwidth-starved tiled GPUs
    // dominates the whole frame budget (~80 ms at 1080p on the 2017 MediaTek
    // TV). Any failure latches the path off and falls back.
    let mut ext_attached = false;
    if samples >= 2 && !rig.ext_unavailable {
      if let Some(fns) = msrtt() {
        let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
        let ensured = rig.ensure_ext(gl, fns, alloc, samples);
        gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
        gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));
        match ensured {
          Ok(()) => {
            let ext = rig.ext.as_ref().expect("ensure_ext populated the rig");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(draw_fbo));
            (fns.framebuffer_texture_2d_multisample)(
              glow::FRAMEBUFFER,
              glow::COLOR_ATTACHMENT0,
              glow::TEXTURE_2D,
              ext.color.0.get(),
              0,
              samples,
            );
            gl.framebuffer_renderbuffer(
              glow::FRAMEBUFFER,
              glow::DEPTH_STENCIL_ATTACHMENT,
              glow::RENDERBUFFER,
              Some(ext.depth_stencil),
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
              ext_attached = true;
            } else {
              gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
              log::warn!("[alloy] in-tile MSAA framebuffer incomplete; using explicit resolve");
              rig.latch_ext_unavailable(gl);
            }
          }
          Err(e) => {
            log::warn!("[alloy] in-tile MSAA storage failed ({e}); using explicit resolve");
            rig.latch_ext_unavailable(gl);
          }
        }
      }
    }

    if !ext_attached {
      let ensured = rig.ensure_msaa(gl, alloc, samples);
      gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
      if let Err(e) = ensured {
        return OffscreenDraw::Failed(e);
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(draw_fbo));
      let msaa = rig.msaa.as_ref().expect("ensure_msaa populated the rig");
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(msaa.color));
      gl.framebuffer_renderbuffer(
        glow::FRAMEBUFFER,
        glow::DEPTH_STENCIL_ATTACHMENT,
        glow::RENDERBUFFER,
        Some(msaa.depth_stencil),
      );

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
        return if samples >= 2 {
          OffscreenDraw::MsaaUnavailable
        } else {
          OffscreenDraw::Failed(format!("window framebuffer incomplete: {status:#x}"))
        };
      }
    }

    // Same defined-base rationale as draw_offscreen: rig storage carries the
    // previous raster (or driver-defined garbage when fresh).
    gl.disable(glow::SCISSOR_TEST);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);

    let mut result = match impeller_ctx.wrap_fbo(draw_fbo.0.get() as u64, PixelFormat::RGBA8888, size) {
      Some(mut surface) => surface.draw_display_list(dl).map_err(|e| format!("frame draw failed: {e}")),
      None => Err("wrap_fbo failed for frame framebuffer".to_string()),
    };

    if result.is_ok() && ext_attached {
      // Consume the resolved image the way the extension intends: sample
      // ext.color in a fullscreen draw into `dst`. Blitting it out instead
      // (through a wrapper FBO) is rejected by Adreno with GL_INVALID_OPERATION
      // on frames where Impeller's draw did internal texture maintenance, so
      // the copy is a draw, not a blit; the bandwidth is identical (one
      // full-frame read plus one write). The fragment maps the window rect
      // out of the aligned allocation via textureSize.
      if rig.copy.is_none() {
        match crate::gpu::ShaderProgram::new_fragment(gl, EXT_RESOLVE_COPY_SRC) {
          Ok(program) => rig.copy = Some(program),
          Err(e) => {
            log::warn!("[alloy] in-tile resolve copy program failed ({e}); using explicit resolve");
            rig.latch_ext_unavailable(gl);
            result = Err("in-tile resolve copy program failed".to_string());
          }
        }
      }
      if let Some(program) = &rig.copy {
        let ext_color = rig.ext.as_ref().expect("ext_attached implies ext storage").color;
        crate::gpu::render_program_to_fbo(
          gl,
          program,
          dst,
          width as u32,
          height as u32,
          &[],
          &[("uSource".to_string(), ext_color, None)],
        );
      }
      // Only the depth-stencil samples are dead here; ext.color must survive
      // (it is the resolve target the driver reloads from on rebind).
      if supports_invalidate(gl) {
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(draw_fbo));
        gl.invalidate_framebuffer(glow::READ_FRAMEBUFFER, &[glow::DEPTH_STENCIL_ATTACHMENT]);
      }
    } else if result.is_ok() {
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, dst);
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(draw_fbo));
      gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
      // The rig contents are dead after the resolve; the invalidate keeps
      // tilers from writing them back to main memory.
      if supports_invalidate(gl) {
        gl.invalidate_framebuffer(glow::READ_FRAMEBUFFER, &[glow::COLOR_ATTACHMENT0, glow::DEPTH_STENCIL_ATTACHMENT]);
      }
    }

    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

    match result {
      Ok(()) => OffscreenDraw::Done,
      Err(e) => OffscreenDraw::Failed(e),
    }
  }
}
