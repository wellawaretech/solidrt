//! The retained offscreen rig: FBOs and multisampled storage shared by every
//! rig draw, plus the once-per-process capability probes the draw paths
//! consult (EXT_multisampled_render_to_texture, glInvalidateFramebuffer,
//! FBO 0's sample count).

use glow::HasContext;
use std::num::NonZeroU32;

// Rebind helpers: glGetIntegerv returns a GL name as i32; map 0 (the default
// object) to None and any live name to the typed glow handle.
pub(super) fn prev_texture(name: i32) -> Option<glow::NativeTexture> {
  NonZeroU32::new(name as u32).map(glow::NativeTexture)
}
pub(super) fn prev_framebuffer(name: i32) -> Option<glow::NativeFramebuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeFramebuffer)
}
pub(super) fn prev_renderbuffer(name: i32) -> Option<glow::NativeRenderbuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeRenderbuffer)
}

/// 4x multisampling for every rig rasterization: window frames and
/// repaint-boundary snapshots alike. Impeller's GL backend has no analytic
/// path AA; it relies on the target framebuffer being multisampled. The
/// window itself is created single-sample and gets its anti-aliasing from
/// the rig's resolve (see `render_display_list_to_window`).
pub(super) const MSAA_SAMPLES: i32 = 4;

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
pub(crate) struct OffscreenRig {
  pub(super) draw_fbo: Option<glow::NativeFramebuffer>,
  pub(super) resolve_fbo: Option<glow::NativeFramebuffer>,
  pub(super) msaa: Option<MsaaStorage>,
  // In-tile MSAA storage for the window path (EXT_multisampled_render_to_
  // texture); see draw_and_resolve.
  pub(super) ext: Option<ExtStorage>,
  // Fullscreen 1:1 copy program consuming ext.color into the destination;
  // lazily compiled on the first in-tile resolve.
  pub(super) copy: Option<super::ShaderProgram>,
  // Depth-stencil for the single-sample path, where the resolve texture
  // attaches directly as color.
  pub(super) ss_depth: Option<SizedRenderbuffer>,
  // Latched on the first incomplete multisampled framebuffer: a driver may
  // advertise MAX_SAMPLES yet reject this config, and one rejection means
  // every later attempt fails too, so stay single-sample for the process.
  pub(super) msaa_unavailable: bool,
  // Same latch policy for the in-tile path: one rejection and the process
  // stays on the explicit resolve.
  pub(super) ext_unavailable: bool,
}

pub(super) struct SizedRenderbuffer {
  pub(super) rbo: glow::NativeRenderbuffer,
  pub(super) size: (i32, i32),
}

pub(super) struct MsaaStorage {
  pub(super) color: glow::NativeRenderbuffer,
  pub(super) depth_stencil: glow::NativeRenderbuffer,
  pub(super) size: (i32, i32),
  pub(super) samples: i32,
}

/// Storage for the in-tile MSAA path: a single-sample color texture the
/// driver resolves into at tile writeback, and a depth-stencil whose samples
/// exist only in tile memory (RenderbufferStorageMultisampleEXT backs it
/// single-sample). The sample-count multiples of memory the explicit path
/// stores and re-reads never exist here.
pub(super) struct ExtStorage {
  pub(super) color: glow::NativeTexture,
  pub(super) depth_stencil: glow::NativeRenderbuffer,
  pub(super) size: (i32, i32),
  pub(super) samples: i32,
}

// The in-tile resolve copy pass (see draw_and_resolve): a 1:1 sample of the
// window rect out of the aligned ext.color allocation. iResolution is the
// window size, textureSize the allocation, so the ratio rescales vUV to the
// content corner; at pixel centers the mapping is exact.
pub(super) const EXT_RESOLVE_COPY_SRC: &str = r"uniform sampler2D uSource;
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
pub(crate) struct MsrttFns {
  pub(crate) framebuffer_texture_2d_multisample:
    unsafe extern "C" fn(target: u32, attachment: u32, textarget: u32, texture: u32, level: i32, samples: i32),
  pub(crate) renderbuffer_storage_multisample:
    unsafe extern "C" fn(target: u32, samples: i32, internalformat: u32, width: i32, height: i32),
}

pub(crate) fn msrtt() -> Option<&'static MsrttFns> {
  static FNS: std::sync::OnceLock<Option<MsrttFns>> = std::sync::OnceLock::new();
  FNS
    .get_or_init(|| unsafe {
      if !sdl3::sys::video::SDL_GL_ExtensionSupported(c"GL_EXT_multisampled_render_to_texture".as_ptr()) {
        return None;
      }
      let ftm = sdl3::sys::video::SDL_GL_GetProcAddress(c"glFramebufferTexture2DMultisampleEXT".as_ptr())?;
      let rsm = sdl3::sys::video::SDL_GL_GetProcAddress(c"glRenderbufferStorageMultisampleEXT".as_ptr())?;
      log::info!("[alloy] MSAA uses EXT_multisampled_render_to_texture (in-tile resolve)");
      Some(MsrttFns {
        framebuffer_texture_2d_multisample: std::mem::transmute(ftm),
        renderbuffer_storage_multisample: std::mem::transmute(rsm),
      })
    })
    .as_ref()
}

impl OffscreenRig {
  pub(crate) fn new() -> Self {
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
  pub(super) fn ensure_msaa(&mut self, gl: &glow::Context, alloc: (i32, i32), samples: i32) -> Result<(), String> {
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
  pub(super) fn ensure_ss_depth(&mut self, gl: &glow::Context, alloc: (i32, i32)) -> Result<(), String> {
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
  pub(super) fn ensure_ext(
    &mut self,
    gl: &glow::Context,
    fns: &MsrttFns,
    alloc: (i32, i32),
    samples: i32,
  ) -> Result<(), String> {
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
  pub(super) fn latch_ext_unavailable(&mut self, gl: &glow::Context) {
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
  pub(super) fn latch_msaa_unavailable(&mut self, gl: &glow::Context) {
    self.msaa_unavailable = true;
    if let Some(old) = self.msaa.take() {
      unsafe {
        gl.delete_renderbuffer(old.color);
        gl.delete_renderbuffer(old.depth_stencil);
      }
    }
  }
}

pub(super) enum OffscreenDraw {
  /// Rendered (and resolved, under MSAA) into the target texture.
  Done,
  /// The multisampled framebuffer was incomplete; retry single-sample.
  MsaaUnavailable,
  /// A GL object failed to allocate or Impeller failed to draw.
  Failed(String),
}

/// glInvalidateFramebuffer is core in ES 3.0 (the platform minimum) but only
/// reached desktop GL at 4.3, so a desktop context below that must skip the
/// hint rather than call an unloaded function.
pub(crate) fn supports_invalidate(gl: &glow::Context) -> bool {
  let v = gl.version();
  if v.is_embedded {
    v.major >= 3
  } else {
    v.major > 4 || (v.major == 4 && v.minor >= 3)
  }
}

/// True when the window's default framebuffer is multisampled (the Android
/// in-tile fast path): frames then draw straight into FBO 0 and a damage
/// patch cannot apply - Impeller clears the wrapped target, and there is no
/// rig to blit from (see draw::render_display_list_to_window).
pub(crate) fn window_fast_path(gl: &glow::Context) -> bool {
  window_samples(gl) >= 2
}

/// FBO 0's multisample count, queried once per process. Positive when the
/// window backbuffer itself is multisampled (Android requests this, see
/// configure_opengl); the driver then resolves in-tile at swap and plain
/// window frames skip the rig entirely.
pub(super) fn window_samples(gl: &glow::Context) -> i32 {
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
