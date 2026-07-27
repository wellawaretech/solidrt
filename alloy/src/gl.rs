use crate::backend::FrameOutput;
use crate::raster::{RasterCmd, RasterState};
use crate::{Backend, Context, DisplayContext, GpuTexture};
use glow::HasContext;
use impellers::{Context as ImpellerContext, DisplayList, ISize, PixelFormat, Texture};
use std::num::NonZeroU32;
use std::sync::atomic::AtomicU64;
use std::sync::{mpsc, Arc};

struct SendablePtr(*mut std::ffi::c_void);
unsafe impl Send for SendablePtr {}
impl SendablePtr {
  // A method rather than direct field access: closures capture precise paths,
  // and capturing the raw-pointer field directly would make the closure !Send;
  // a method call captures the whole Send wrapper.
  fn get(&self) -> *mut std::ffi::c_void {
    self.0
  }
}

/// Load GLES bindings from SDL's GL proc-address. Must be called on the
/// raster thread with the GL context current; the returned context drives all
/// of alloy's GL work (texture allocation/upload, offscreen FBOs, readback)
/// and shares the underlying GL objects with Impeller, which renders on the
/// same context.
pub fn create_gl_context() -> glow::Context {
  unsafe {
    glow::Context::from_loader_function(|name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      sdl3::sys::video::SDL_GL_GetProcAddress(cname.as_ptr())
        .map(|f| f as *const std::ffi::c_void)
        .unwrap_or(std::ptr::null())
    })
  }
}

pub fn create_impeller_context() -> ImpellerContext {
  unsafe {
    ImpellerContext::new_opengl_es(|name| {
      sdl3::sys::video::SDL_GL_GetProcAddress(name.as_ptr() as *const _)
        .map(|f| f as *mut _)
        .unwrap_or(std::ptr::null_mut())
    })
  }
  .expect("Failed to create Impeller context")
}

/// Adopt a GpuTexture's GL name into Impeller (zero-copy). Adoption transfers
/// ownership of the GL name to Impeller, which deletes it when its Texture is
/// dropped; GpuTexture therefore never deletes the name itself.
pub fn adopt_texture(gpu_texture: &GpuTexture, impeller_ctx: &ImpellerContext, size: ISize) -> Option<Texture> {
  let (width, height) = (size.width as u32, size.height as u32);
  let gl_handle = gpu_texture.gl_texture.0.get() as u64;

  unsafe { impeller_ctx.adopt_opengl_texture(width, height, 1, gl_handle) }
}

// Rebind helpers: glGetIntegerv returns a GL name as i32; map 0 (the default
// object) to None and any live name to the typed glow handle.
fn prev_texture(name: i32) -> Option<glow::NativeTexture> {
  NonZeroU32::new(name as u32).map(glow::NativeTexture)
}
fn prev_framebuffer(name: i32) -> Option<glow::NativeFramebuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeFramebuffer)
}
fn prev_renderbuffer(name: i32) -> Option<glow::NativeRenderbuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeRenderbuffer)
}

/// 4x multisampling for every rig rasterization: window frames and
/// repaint-boundary snapshots alike. Impeller's GL backend has no analytic
/// path AA; it relies on the target framebuffer being multisampled. The
/// window itself is created single-sample and gets its anti-aliasing from
/// the rig's resolve (see `render_display_list_to_window`).
const MSAA_SAMPLES: i32 = 4;

/// Retained GL objects for rig rasterization, owned by the raster thread and
/// shared by every rig draw: the window frame itself, snapshot boundaries,
/// and node captures. Only the per-boundary resolve texture persists
/// elsewhere; everything transient lives here: both FBOs, the multisampled
/// color and depth-stencil storage, and the single-sample depth-stencil.
/// Storage grows monotonically to the largest allocation requested and
/// smaller rasters render into a subrect, so the sustained allocate/release
/// cycle a per-call rig would impose on the driver (which ANGLE/D3D11
/// handles poorly, see okf/backlog/angle-cross-context-impeller-textures.md)
/// never happens.
pub struct OffscreenRig {
  draw_fbo: Option<glow::NativeFramebuffer>,
  resolve_fbo: Option<glow::NativeFramebuffer>,
  msaa: Option<MsaaStorage>,
  // Depth-stencil for the single-sample path, where the resolve texture
  // attaches directly as color.
  ss_depth: Option<SizedRenderbuffer>,
  // Latched on the first incomplete multisampled framebuffer: a driver may
  // advertise MAX_SAMPLES yet reject this config, and one rejection means
  // every later attempt fails too, so stay single-sample for the process.
  msaa_unavailable: bool,
}

struct SizedRenderbuffer {
  rbo: glow::NativeRenderbuffer,
  size: (i32, i32),
}

struct MsaaStorage {
  color: glow::NativeRenderbuffer,
  depth_stencil: glow::NativeRenderbuffer,
  size: (i32, i32),
  samples: i32,
}

impl OffscreenRig {
  pub fn new() -> Self {
    Self { draw_fbo: None, resolve_fbo: None, msaa: None, ss_depth: None, msaa_unavailable: false }
  }

  /// Grow the multisampled color + depth-stencil pair to cover `alloc`
  /// (component-wise, never shrinking). Contents are transient per raster, so
  /// regrowth discards them. Leaves the renderbuffer binding dirty; the
  /// caller restores it. `samples == 0` allocates single-sample storage
  /// (renderbuffer_storage_multisample with zero samples is plain storage per
  /// the ES 3.0 spec), used by the window path's no-MSAA fallback.
  fn ensure_msaa(&mut self, gl: &glow::Context, alloc: (i32, i32), samples: i32) -> Result<(), String> {
    let (cur_w, cur_h, cur_samples) =
      self.msaa.as_ref().map(|m| (m.size.0, m.size.1, m.samples)).unwrap_or((0, 0, samples));
    let size = (cur_w.max(alloc.0), cur_h.max(alloc.1));
    if self.msaa.is_some() && size == (cur_w, cur_h) && cur_samples == samples {
      return Ok(());
    }
    if let Some(old) = self.msaa.take() {
      unsafe {
        gl.delete_renderbuffer(old.color);
        gl.delete_renderbuffer(old.depth_stencil);
      }
    }
    unsafe {
      let color = gl.create_renderbuffer().map_err(|e| format!("glGenRenderbuffers failed: {e}"))?;
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
      gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA8, size.0, size.1);
      let depth_stencil = match gl.create_renderbuffer() {
        Ok(rb) => rb,
        Err(e) => {
          gl.delete_renderbuffer(color);
          return Err(format!("glGenRenderbuffers failed: {e}"));
        }
      };
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_stencil));
      gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH24_STENCIL8, size.0, size.1);
      self.msaa = Some(MsaaStorage { color, depth_stencil, size, samples });
    }
    Ok(())
  }

  /// Grow the single-sample depth-stencil to cover `alloc` (component-wise,
  /// never shrinking). Leaves the renderbuffer binding dirty; the caller
  /// restores it.
  fn ensure_ss_depth(&mut self, gl: &glow::Context, alloc: (i32, i32)) -> Result<(), String> {
    let (cur_w, cur_h) = self.ss_depth.as_ref().map(|d| d.size).unwrap_or((0, 0));
    let size = (cur_w.max(alloc.0), cur_h.max(alloc.1));
    if self.ss_depth.is_some() && size == (cur_w, cur_h) {
      return Ok(());
    }
    if let Some(old) = self.ss_depth.take() {
      unsafe { gl.delete_renderbuffer(old.rbo) };
    }
    unsafe {
      let rbo = gl.create_renderbuffer().map_err(|e| format!("glGenRenderbuffers failed: {e}"))?;
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
      gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, size.0, size.1);
      self.ss_depth = Some(SizedRenderbuffer { rbo, size });
    }
    Ok(())
  }

  /// Latch single-sample mode and free the multisampled storage.
  fn latch_msaa_unavailable(&mut self, gl: &glow::Context) {
    self.msaa_unavailable = true;
    if let Some(old) = self.msaa.take() {
      unsafe {
        gl.delete_renderbuffer(old.color);
        gl.delete_renderbuffer(old.depth_stencil);
      }
    }
  }
}

/// glInvalidateFramebuffer is core in ES 3.0 (the platform minimum) but only
/// reached desktop GL at 4.3, so a desktop context below that must skip the
/// hint rather than call an unloaded function.
fn supports_invalidate(gl: &glow::Context) -> bool {
  let v = gl.version();
  if v.is_embedded {
    v.major >= 3
  } else {
    v.major > 4 || (v.major == 4 && v.minor >= 3)
  }
}

/// Rasterize a display list into a new GL texture and adopt it into Impeller,
/// which becomes the single owner of the GL name (adoption transfers handle
/// ownership per the Impeller API; we never glDeleteTextures it ourselves).
/// The calling thread must have the GL context current and `impeller_ctx` must
/// have been created on it. The texture is later sampled on this same context
/// (the process has exactly one), so GL program order covers all ordering and
/// no glFinish or fence is needed.
pub fn render_display_list_to_texture(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
  aa: bool,
) -> Result<Texture, String> {
  let (width, height) = (size.width as i32, size.height as i32);

  // Some Android GPUs require render-target dimensions aligned to a tile
  // boundary; an unaligned offscreen texture/renderbuffer can come back with
  // corrupted (shifted, channel-scrambled) content. Over-allocate storage to
  // the next multiple of 64 and render into the unaligned top-left corner
  // (via the wrap_fbo viewport below), so the content itself stays at
  // (width, height) but lives in aligned backing storage.
  let align_up = |v: i32| (v + 63) & !63;
  let alloc = (align_up(width), align_up(height));
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
/// reusing its storage. `size` must fit the texture's aligned backing
/// allocation (the caller checks this; both sides compute it with the same
/// 64px round-up). The texture's owner is unchanged - Impeller adopted the GL
/// name when the texture was first created and keeps it.
pub fn render_display_list_into_texture(
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
  let align_up = |v: i32| (v + 63) & !63;
  let alloc = (align_up(size.width as i32), align_up(size.height as i32));
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

enum OffscreenDraw {
  /// Rendered (and resolved, under MSAA) into the target texture.
  Done,
  /// The multisampled framebuffer was incomplete; retry single-sample.
  MsaaUnavailable,
  /// A GL object failed to allocate or Impeller failed to draw.
  Failed(String),
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
pub fn render_display_list_to_window(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
) -> Result<(), String> {
  let max_samples = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
  let samples = if rig.msaa_unavailable { 0 } else { MSAA_SAMPLES.min(max_samples) };
  let mut outcome = draw_to_default_framebuffer(gl, impeller_ctx, rig, dl, size, samples);
  if matches!(outcome, OffscreenDraw::MsaaUnavailable) {
    log::warn!("[alloy] window MSAA unavailable; rendering without anti-aliasing");
    rig.latch_msaa_unavailable(gl);
    outcome = draw_to_default_framebuffer(gl, impeller_ctx, rig, dl, size, 0);
  }
  match outcome {
    OffscreenDraw::Done => Ok(()),
    OffscreenDraw::Failed(e) => Err(e),
    OffscreenDraw::MsaaUnavailable => Err("window framebuffer incomplete".to_string()),
  }
}

/// Render `dl` into the rig's renderbuffer storage and blit the window-sized
/// rect into the default framebuffer. `samples >= 2` draws multisampled and
/// the blit is the MSAA resolve; `samples == 0` draws single-sample and the
/// blit is a plain copy. Both cases attach the rig's renderbuffer pair, so
/// one code path covers them. Restores the framebuffer and renderbuffer
/// bindings it touches so Impeller's cached GL state stays valid.
fn draw_to_default_framebuffer(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut OffscreenRig,
  dl: &DisplayList,
  size: ISize,
  samples: i32,
) -> OffscreenDraw {
  let (width, height) = (size.width as i32, size.height as i32);
  // Same aligned backing as the offscreen path (Android tilers corrupt
  // unaligned render targets); content renders into the corner and the blit
  // reads only the window-sized rect.
  let align_up = |v: i32| (v + 63) & !63;
  let alloc = (align_up(width), align_up(height));

  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let prev_rbo = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);

    let ensured = rig.ensure_msaa(gl, alloc, samples);
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

    // Same defined-base rationale as draw_offscreen: rig storage carries the
    // previous raster (or driver-defined garbage when fresh).
    gl.disable(glow::SCISSOR_TEST);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);

    let result = match impeller_ctx.wrap_fbo(draw_fbo.0.get() as u64, PixelFormat::RGBA8888, size) {
      Some(mut surface) => surface.draw_display_list(dl).map_err(|e| format!("frame draw failed: {e}")),
      None => Err("wrap_fbo failed for frame framebuffer".to_string()),
    };

    if result.is_ok() {
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
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

/// Read back an Impeller GL texture's RGBA8 pixels by attaching its handle to
/// a temporary framebuffer and calling glReadPixels. Returns memory-order rows
/// (row 0 first), which is image top-to-bottom for every texture alloy
/// produces.
pub fn read_texture_pixels(gl: &glow::Context, texture: &Texture, size: ISize) -> Result<Vec<u8>, String> {
  let gl_handle = texture.get_opengl_handle();
  let tex =
    glow::NativeTexture(NonZeroU32::new(gl_handle as u32).ok_or_else(|| "texture has no GL handle".to_string())?);
  let (width, height) = (size.width as i32, size.height as i32);

  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);

    let fbo = gl.create_framebuffer().map_err(|e| format!("glGenFramebuffers failed: {e}"))?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);

    let result = if status != glow::FRAMEBUFFER_COMPLETE {
      Err(format!("readback framebuffer incomplete: {status:#x}"))
    } else {
      let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
      gl.read_pixels(
        0,
        0,
        width,
        height,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut pixels)),
      );
      Ok(pixels)
    };

    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    gl.delete_framebuffer(fbo);
    result
  }
}

/// Read back the window backbuffer's RGBA8 pixels (FBO 0, bottom-up rows as GL
/// stores them; the playback encoder flips when writing). Called on the raster
/// thread right after the frame's draw, which glReadPixels implicitly waits on.
pub fn read_fbo0_pixels(gl: &glow::Context, size: ISize) -> Vec<u8> {
  let (width, height) = (size.width as i32, size.height as i32);
  let mut pixels = vec![0u8; (width.max(0) as usize) * (height.max(0) as usize) * 4];
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
    gl.read_pixels(0, 0, width, height, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_framebuffer(prev_fbo));
  }
  pixels
}

// Native stack for the UI/JS thread. Large so deep JS recursion behaves the
// same everywhere (the SDL main thread's stack is irrelevant: the engine runs
// here, not there). This is virtual address space, committed only as it is
// used; it is the hard ceiling under which QuickJS's own (smaller, tunable)
// soft limit sits.
//
// 32-bit targets get a much smaller reservation: a 32-bit process does not
// have enough address space left to reserve a contiguous 1GB stack once its
// libraries are loaded, and pthread_create simply fails (observed on a 32-bit
// Android device). 64MB is still far beyond plausible JS recursion depth.
#[cfg(target_pointer_width = "64")]
const UI_THREAD_STACK_SIZE: usize = 1024 * 1024 * 1024;
#[cfg(target_pointer_width = "32")]
const UI_THREAD_STACK_SIZE: usize = 64 * 1024 * 1024;

pub fn run_context(
  window: *mut sdl3::sys::video::SDL_Window,
  gl_context: &sdl3::video::GLContext,
  surface_size: Arc<AtomicU64>,
  closure: impl FnOnce(Arc<Context>) + Send + 'static,
  tx: mpsc::Sender<FrameOutput>,
  wake: Option<Box<dyn Fn() + Send + Sync>>,
  capture_frames: bool,
) {
  let window_ptr = SendablePtr(window as *mut std::ffi::c_void);
  let context_ptr = SendablePtr(unsafe { gl_context.raw() as *mut std::ffi::c_void });
  let (raster_tx, raster_rx) = mpsc::channel::<RasterCmd>();

  // The raster thread: sole owner of the process's single GL context and
  // Impeller context for the engine's lifetime. Impeller's GLES contract
  // requires exactly this: one context, used only on the thread it was
  // created on. Everything GL arrives over the command channel (raster.rs).
  let spawn_raster = std::thread::Builder::new().name("srt-raster".into()).spawn(move || {
    let window = window_ptr.get() as *mut sdl3::sys::video::SDL_Window;
    let current =
      unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(window, context_ptr.get() as sdl3::sys::video::SDL_GLContext) };
    assert!(current, "SDL_GL_MakeCurrent failed on raster thread: {}", crate::sdl_utils::sdl_error());
    // The swap interval belongs to the current-context binding, so it must be
    // set on this thread, not where the context was created. Blocking this
    // thread in the vsync wait is the point: the UI thread stays free to
    // build the next frame and dispatch input. Playback never swaps, so the
    // setting is inert there.
    if !unsafe { sdl3::sys::video::SDL_GL_SetSwapInterval(1) } {
      log::warn!("[alloy] SDL_GL_SetSwapInterval failed: {}", crate::sdl_utils::sdl_error());
    }

    let gl = create_gl_context();
    let impeller_ctx = create_impeller_context();
    unsafe {
      log::info!(
        "[alloy] GPU ready: {} | {} | {}",
        gl.get_parameter_string(glow::VENDOR),
        gl.get_parameter_string(glow::RENDERER),
        gl.get_parameter_string(glow::VERSION)
      );
    }

    let state = RasterState::new(Backend::Gl, gl, impeller_ctx, window, surface_size, capture_frames, tx, wake);
    state.run(raster_rx);
  });
  spawn_raster.expect("failed to spawn raster thread");

  // The UI thread: QuickJS, layout, hit-testing, DisplayList building. No GL
  // at all; the Context it gets marshals GPU work over the command channel.
  let spawn_ui = std::thread::Builder::new().name("srt-ui".into()).stack_size(UI_THREAD_STACK_SIZE).spawn(move || {
    closure(Arc::new(Context::new(raster_tx)));
  });
  spawn_ui.expect("failed to spawn UI thread");
}

/// Must be called before window creation so SDL selects ANGLE (EGL) on macOS.
pub(crate) fn configure_opengl(video: &sdl3::VideoSubsystem) {
  sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");
  let gl_attr = video.gl_attr();
  gl_attr.set_context_profile(sdl3::video::GLProfile::GLES);
  gl_attr.set_context_version(3, 0);
  gl_attr.set_stencil_size(8);

  // The window is deliberately single-sample: every frame rasterizes into
  // the multisampled offscreen rig and resolves into FBO 0 (see
  // render_display_list_to_window), so a multisampled backbuffer would only
  // duplicate that storage. This also removes the old dependency on the
  // driver exposing a multisampled EGL config at all (the Android emulator
  // does not, which used to force a retry-without-MSAA window path).
}

pub(crate) fn setup_opengl_platform(window: &sdl3::video::Window) -> Result<DisplayContext, Box<dyn std::error::Error>> {
  // The process's single GL context. Creating it makes it current on this
  // (main) thread; release it right away so the UI thread - the only GL user -
  // can bind it (a GL context can be current on at most one thread).
  let gl_context = window.gl_create_context().map_err(|e| format!("Failed to create GL context: {}", e))?;
  if !unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(window.raw(), std::ptr::null_mut()) } {
    return Err(format!("Failed to release GL context on main thread: {}", crate::sdl_utils::sdl_error()).into());
  }

  let (w, h) = window.size_in_pixels();
  Ok(DisplayContext::Gl {
    window_raw: window.raw(),
    gl_context,
    surface_size: Arc::new(AtomicU64::new(crate::backend::pack_size(w, h))),
  })
}
