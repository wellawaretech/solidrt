use crate::backend::FrameOutput;
use crate::raster::{RasterCmd, RasterState};
use crate::{Context, DisplayContext, GpuTexture};
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
  // In-tile MSAA storage for the window path (EXT_multisampled_render_to_
  // texture); see draw_and_resolve.
  ext: Option<ExtStorage>,
  // Fullscreen 1:1 copy program consuming ext.color into the destination;
  // lazily compiled on the first in-tile resolve.
  copy: Option<crate::gpu::ShaderProgram>,
  // Depth-stencil for the single-sample path, where the resolve texture
  // attaches directly as color.
  ss_depth: Option<SizedRenderbuffer>,
  // Latched on the first incomplete multisampled framebuffer: a driver may
  // advertise MAX_SAMPLES yet reject this config, and one rejection means
  // every later attempt fails too, so stay single-sample for the process.
  msaa_unavailable: bool,
  // Same latch policy for the in-tile path: one rejection and the process
  // stays on the explicit resolve.
  ext_unavailable: bool,
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

/// Storage for the in-tile MSAA path: a single-sample color texture the
/// driver resolves into at tile writeback, and a depth-stencil whose samples
/// exist only in tile memory (RenderbufferStorageMultisampleEXT backs it
/// single-sample). The sample-count multiples of memory the explicit path
/// stores and re-reads never exist here.
struct ExtStorage {
  color: glow::NativeTexture,
  depth_stencil: glow::NativeRenderbuffer,
  size: (i32, i32),
  samples: i32,
}

// The in-tile resolve copy pass (see draw_and_resolve): a 1:1 sample of the
// window rect out of the aligned ext.color allocation. iResolution is the
// window size, textureSize the allocation, so the ratio rescales vUV to the
// content corner; at pixel centers the mapping is exact.
const EXT_RESOLVE_COPY_SRC: &str = r"uniform sampler2D uSource;
void main() {
  fragColor = texture(uSource, vUV * iResolution / vec2(textureSize(uSource, 0)));
}
";

/// GL_EXT_multisampled_render_to_texture entry points, loaded once from the
/// current context. On tiled GPUs this extension multisamples entirely inside
/// tile memory and writes out only the resolved image, so MSAA stops costing
/// sample-count multiples of DDR bandwidth - the difference between ~80 ms
/// and a few ms per 1080p window frame on the 2017 MediaTek TV
/// (okf/backlog/android-surface-swap-latency.md). None when the extension is
/// not advertised; desktop GL lacks it and its bandwidth does not miss it.
/// Call on the raster thread with the GL context current.
struct MsrttFns {
  framebuffer_texture_2d_multisample:
    unsafe extern "C" fn(target: u32, attachment: u32, textarget: u32, texture: u32, level: i32, samples: i32),
  renderbuffer_storage_multisample:
    unsafe extern "C" fn(target: u32, samples: i32, internalformat: u32, width: i32, height: i32),
}

fn msrtt() -> Option<&'static MsrttFns> {
  static FNS: std::sync::OnceLock<Option<MsrttFns>> = std::sync::OnceLock::new();
  FNS
    .get_or_init(|| unsafe {
      if !sdl3::sys::video::SDL_GL_ExtensionSupported(c"GL_EXT_multisampled_render_to_texture".as_ptr()) {
        return None;
      }
      let ftm = sdl3::sys::video::SDL_GL_GetProcAddress(c"glFramebufferTexture2DMultisampleEXT".as_ptr())?;
      let rsm = sdl3::sys::video::SDL_GL_GetProcAddress(c"glRenderbufferStorageMultisampleEXT".as_ptr())?;
      log::info!("[alloy] window MSAA uses EXT_multisampled_render_to_texture (in-tile resolve)");
      Some(MsrttFns {
        framebuffer_texture_2d_multisample: std::mem::transmute(ftm),
        renderbuffer_storage_multisample: std::mem::transmute(rsm),
      })
    })
    .as_ref()
}

impl OffscreenRig {
  pub fn new() -> Self {
    Self {
      draw_fbo: None,
      resolve_fbo: None,
      msaa: None,
      ext: None,
      copy: None,
      ss_depth: None,
      msaa_unavailable: false,
      ext_unavailable: false,
    }
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

  /// Grow the in-tile MSAA pair (see ExtStorage) to cover `alloc`, same
  /// grow-only sizing rules as ensure_msaa. Leaves the texture and
  /// renderbuffer bindings dirty; the caller restores them.
  fn ensure_ext(&mut self, gl: &glow::Context, fns: &MsrttFns, alloc: (i32, i32), samples: i32) -> Result<(), String> {
    let (cur_w, cur_h, cur_samples) =
      self.ext.as_ref().map(|m| (m.size.0, m.size.1, m.samples)).unwrap_or((0, 0, samples));
    let size = (cur_w.max(alloc.0), cur_h.max(alloc.1));
    if self.ext.is_some() && size == (cur_w, cur_h) && cur_samples == samples {
      return Ok(());
    }
    if let Some(old) = self.ext.take() {
      unsafe {
        gl.delete_texture(old.color);
        gl.delete_renderbuffer(old.depth_stencil);
      }
    }
    unsafe {
      let color = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
      gl.bind_texture(glow::TEXTURE_2D, Some(color));
      gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        size.0,
        size.1,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
      );
      // Sampled 1:1 by the resolve copy pass; NEAREST is exact there.
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
      let depth_stencil = match gl.create_renderbuffer() {
        Ok(rb) => rb,
        Err(e) => {
          gl.delete_texture(color);
          return Err(format!("glGenRenderbuffers failed: {e}"));
        }
      };
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_stencil));
      (fns.renderbuffer_storage_multisample)(glow::RENDERBUFFER, samples, glow::DEPTH24_STENCIL8, size.0, size.1);
      self.ext = Some(ExtStorage { color, depth_stencil, size, samples });
    }
    Ok(())
  }

  /// Latch the in-tile path off for the process and free its storage; the
  /// window path falls back to the explicit resolve.
  fn latch_ext_unavailable(&mut self, gl: &glow::Context) {
    self.ext_unavailable = true;
    if let Some(old) = self.ext.take() {
      unsafe {
        gl.delete_texture(old.color);
        gl.delete_renderbuffer(old.depth_stencil);
      }
    }
    if let Some(copy) = self.copy.take() {
      copy.delete(gl);
    }
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
///
/// Storage is exactly `size`: no tile-alignment padding. A historical 64px
/// round-up (for suspected Android tile-boundary corruption) was removed -
/// the corruption's real cause was cross-context completion, a bug class the
/// single-context raster thread ended, and the exactly-window-sized shader
/// layer had long rendered through this same rig on Android without padding.
pub fn render_display_list_to_texture(
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

/// FBO 0's multisample count, queried once per process. Positive when the
/// window backbuffer itself is multisampled (Android requests this, see
/// configure_opengl); the driver then resolves in-tile at swap and plain
/// window frames skip the rig entirely.
fn window_samples(gl: &glow::Context) -> i32 {
  static N: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
  *N.get_or_init(|| unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    let sample_buffers = gl.get_parameter_i32(glow::SAMPLE_BUFFERS);
    let samples = if sample_buffers > 0 { gl.get_parameter_i32(glow::SAMPLES) } else { 0 };
    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    if samples >= 2 {
      log::info!("[alloy] window backbuffer is {samples}x multisampled (in-tile resolve at swap)");
    } else {
      log::info!("[alloy] window backbuffer is single-sample");
    }
    samples
  })
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
pub fn render_display_list_to_layer(
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
    if window_samples(gl) >= 2 {
      // glReadPixels cannot read a multisampled framebuffer: resolve the
      // window rect into a temporary single-sample FBO first.
      let prev_draw = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
      let prev_rbo = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
      if let (Ok(rbo), Ok(fbo)) = (gl.create_renderbuffer(), gl.create_framebuffer()) {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, width, height);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rbo));
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
        gl.read_pixels(
          0,
          0,
          width,
          height,
          glow::RGBA,
          glow::UNSIGNED_BYTE,
          glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        gl.delete_framebuffer(fbo);
        gl.delete_renderbuffer(rbo);
      }
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_framebuffer(prev_draw));
      gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
    } else {
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
      gl.read_pixels(
        0,
        0,
        width,
        height,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut pixels)),
      );
    }
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

#[allow(clippy::too_many_arguments)]
pub fn run_context(
  window: *mut sdl3::sys::video::SDL_Window,
  gl_context: &sdl3::video::GLContext,
  surface_size: Arc<AtomicU64>,
  closure: impl FnOnce(Arc<Context>) + Send + 'static,
  tx: mpsc::Sender<FrameOutput>,
  wake: Option<Box<dyn Fn() + Send + Sync>>,
  capture_frames: bool,
  stats: Arc<crate::raster::RasterStats>,
) -> crate::raster::RasterSender {
  let window_ptr = SendablePtr(window as *mut std::ffi::c_void);
  let context_ptr = SendablePtr(unsafe { gl_context.raw() as *mut std::ffi::c_void });
  let (raster_tx, raster_rx) = mpsc::channel::<RasterCmd>();
  let raster_tx = crate::raster::RasterSender::new(raster_tx, stats.clone());
  // The platform loop's clone, for surface-liveness rebinds (liveness.rs):
  // same ordered channel and queue-depth bookkeeping as the Context's half.
  let main_tx = raster_tx.clone();
  let raster_stats = stats.clone();

  // The raster thread: sole owner of the process's single GL context and
  // Impeller context for the engine's lifetime. Impeller's GLES contract
  // requires exactly this: one context, used only on the thread it was
  // created on. Everything GL arrives over the command channel (raster.rs).
  let spawn_raster = std::thread::Builder::new().name("srt-raster".into()).spawn(move || {
    // Display priority so background processes cannot preempt a frame
    // mid-flight; see sdl_utils::frame_thread_priority.
    crate::sdl_utils::frame_thread_priority(true);
    let window = window_ptr.get() as *mut sdl3::sys::video::SDL_Window;
    let current =
      unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(window, context_ptr.get() as sdl3::sys::video::SDL_GLContext) };
    assert!(current, "SDL_GL_MakeCurrent failed on raster thread: {}", crate::sdl_utils::sdl_error());
    // The swap interval belongs to the current-context binding, so it must be
    // set on this thread, not where the context was created. Blocking this
    // thread in the vsync wait is the point: the UI thread stays free to
    // build the next frame and dispatch input. Playback never swaps, so the
    // setting is inert there.
    if !unsafe { sdl3::sys::video::SDL_GL_SetSwapInterval(crate::sdl_utils::WINDOW_SWAP_INTERVAL) } {
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

    let state = RasterState::new(
      gl,
      impeller_ctx,
      window,
      surface_size,
      capture_frames,
      raster_stats,
      tx,
      wake,
    );
    // Map the window now rather than at the first frame, so a UI thread that
    // never submits one is still visible on Wayland (see prime_window).
    state.prime_window();
    state.run(raster_rx);
  });
  spawn_raster.expect("failed to spawn raster thread");

  // The UI thread: QuickJS, layout, hit-testing, DisplayList building. No GL
  // at all; the Context it gets marshals GPU work over the command channel.
  let spawn_ui = std::thread::Builder::new().name("srt-ui".into()).stack_size(UI_THREAD_STACK_SIZE).spawn(move || {
    // Same display-priority rationale as the raster thread, one tier lower
    // (the raster thread owns the present deadline).
    crate::sdl_utils::frame_thread_priority(false);
    closure(Arc::new(Context::new(raster_tx, stats)));
  });
  spawn_ui.expect("failed to spawn UI thread");
  main_tx
}

/// Must be called before window creation so SDL selects ANGLE (EGL) on macOS.
pub(crate) fn configure_opengl(video: &sdl3::VideoSubsystem) {
  sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");
  let gl_attr = video.gl_attr();
  gl_attr.set_context_profile(sdl3::video::GLProfile::GLES);
  gl_attr.set_context_version(3, 0);
  gl_attr.set_stencil_size(8);

  // 8-bit color, explicitly: SDL's defaults request only 3/3/2 and its EGL
  // config scorer picks the closest match, which on Android Mali drivers
  // lands on RGB565 window buffers (observed as format=4 on the 2017
  // MediaTek TV's SurfaceFlinger layer dump). HWUI never presents 565;
  // 565 also banded visibly. Desktop GL configs are 8-bit anyway.
  gl_attr.set_red_size(8);
  gl_attr.set_green_size(8);
  gl_attr.set_blue_size(8);

  // On Android the window backbuffer itself is multisampled: the tiled GPU
  // resolves in-tile at swap, so plain window frames draw straight into
  // FBO 0 with no rig pass and no resolve copy - the only MSAA
  // configuration this class of GPU runs at full rate
  // (okf/backlog/android-surface-swap-latency.md).
  #[cfg(target_os = "android")]
  {
    gl_attr.set_multisample_buffers(1);
    gl_attr.set_multisample_samples(MSAA_SAMPLES as u8);
  }

  // On desktop the window is deliberately single-sample: every frame
  // rasterizes into the multisampled offscreen rig and resolves into FBO 0
  // (see render_display_list_to_window), so a multisampled backbuffer would only
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
