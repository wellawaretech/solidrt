use crate::backend::{Frame, GpuFence};
use crate::{Backend, Context, DisplayContext, GpuTexture, RenderSurface};
use glow::HasContext;
use impellers::{Context as ImpellerContext, DisplayList, ISize, PixelFormat, Texture};
use sdl3::video::SwapInterval;
use std::num::NonZeroU32;
use std::sync::{mpsc, Arc};

struct SendablePtr(*mut std::ffi::c_void);
unsafe impl Send for SendablePtr {}

pub fn create_ui_pbuffer(display: *mut std::ffi::c_void, gl_context: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
  const EGL_NONE: i32 = 0x3038;
  const EGL_CONFIG_ID: i32 = 0x3028;
  const EGL_WIDTH: i32 = 0x3057;
  const EGL_HEIGHT: i32 = 0x3056;

  type EglQueryContextFn = extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i32, *mut i32) -> u32;
  type EglChooseConfigFn =
    extern "C" fn(*mut std::ffi::c_void, *const i32, *mut *mut std::ffi::c_void, i32, *mut i32) -> u32;
  type EglCreatePbufferFn =
    extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *const i32) -> *mut std::ffi::c_void;

  unsafe {
    let egl_query_context: EglQueryContextFn =
      std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglQueryContext".as_ptr()).unwrap());
    let egl_choose_config: EglChooseConfigFn =
      std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglChooseConfig".as_ptr()).unwrap());
    let egl_create_pbuffer: EglCreatePbufferFn =
      std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglCreatePbufferSurface".as_ptr()).unwrap());

    let mut config_id: i32 = 0;
    let r = egl_query_context(display, gl_context, EGL_CONFIG_ID, &mut config_id);
    assert!(r != 0, "eglQueryContext(EGL_CONFIG_ID) failed");

    let select = [EGL_CONFIG_ID, config_id, EGL_NONE];
    let mut config: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut num_configs: i32 = 0;
    let r = egl_choose_config(display, select.as_ptr(), &mut config, 1, &mut num_configs);
    assert!(r != 0 && num_configs > 0 && !config.is_null(), "eglChooseConfig failed");

    let pb_attribs = [EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE];
    let pbuffer = egl_create_pbuffer(display, config, pb_attribs.as_ptr());
    assert!(!pbuffer.is_null(), "eglCreatePbufferSurface failed");
    pbuffer
  }
}

pub fn make_current(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, gl_context: *mut std::ffi::c_void) {
  let egl_make_current: extern "C" fn(
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
  ) -> u32 =
    unsafe { std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglMakeCurrent".as_ptr()).unwrap()) };
  let result = egl_make_current(display, surface, surface, gl_context);
  assert!(result != 0, "eglMakeCurrent failed on UI thread");
}

/// Load GLES bindings from SDL's GL proc-address. Must be called on the UI
/// thread with its GL context current; the returned context drives all of
/// alloy's GL work (texture allocation/upload, offscreen FBOs, readback) and
/// shares the underlying GL objects with Impeller, which renders on the same
/// context.
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

/// 4x multisampling for offscreen (repaint-boundary snapshot) rendering, to
/// match the onscreen surface's request in `configure_opengl`. Impeller's GL
/// backend has no analytic path AA; it relies on the target framebuffer being
/// multisampled, exactly as the onscreen surface does.
const MSAA_SAMPLES: i32 = 4;

/// Rasterize a display list into a new GL texture and adopt it into Impeller,
/// which becomes the single owner of the GL name (adoption transfers handle
/// ownership per the Impeller API; we never glDeleteTextures it ourselves).
/// The calling thread must have a GL context current and `impeller_ctx` must
/// have been created on it. Cross-thread ordering (the texture is sampled on a
/// separate shared GL context, where its contents are otherwise undefined) is
/// handled by Context::submit's per-frame fence, so no glFinish is issued here.
pub fn render_display_list_to_texture(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  dl: &DisplayList,
  size: ISize,
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

  // Match the window's 4x request, clamped to the driver ceiling.
  let max_samples = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
  let mut outcome = draw_offscreen(gl, impeller_ctx, dl, tex, size, alloc, MSAA_SAMPLES.min(max_samples));
  if matches!(outcome, OffscreenDraw::MsaaUnavailable) {
    // A driver may advertise MAX_SAMPLES yet reject this multisampled config;
    // retry once single-sample rather than failing the frame (mirrors the
    // window-creation fallback in app::setup / gl::disable_msaa).
    log::warn!("[alloy] offscreen MSAA unavailable; rendering snapshot without anti-aliasing");
    outcome = draw_offscreen(gl, impeller_ctx, dl, tex, size, alloc, 1);
  }

  match outcome {
    OffscreenDraw::Done => {
      unsafe { impeller_ctx.adopt_opengl_texture(alloc_width as u32, alloc_height as u32, 1, tex.0.get() as u64) }
        .ok_or_else(|| "failed to adopt offscreen texture".to_string())
    }
    OffscreenDraw::Failed(e) => {
      unsafe { gl.delete_texture(tex) };
      Err(e)
    }
    OffscreenDraw::MsaaUnavailable => {
      // The single-sample retry above resolves to Done or Failed, so this arm
      // is unreachable; treat it as a failure defensively.
      unsafe { gl.delete_texture(tex) };
      Err("offscreen framebuffer incomplete".to_string())
    }
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

/// Render `dl` into `tex` via an FBO with `samples`x multisampling: a count
/// below 2 draws straight into `tex`; >= 2 draws into a multisampled
/// renderbuffer and resolves it into `tex` with glBlitFramebuffer. `alloc` is
/// the aligned backing size; `size` is the logical viewport handed to Impeller.
/// A GL context must be current. Restores the framebuffer and renderbuffer
/// bindings it touches so Impeller's cached GL state stays valid.
fn draw_offscreen(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
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

    // Impeller fills non-convex paths with stencil-then-cover and culls clips
    // via depth; without this attachment the cover pass floods the path bounds.
    // Multisampled to match the color target when use_msaa.
    let ds_rbo = match gl.create_renderbuffer() {
      Ok(rb) => rb,
      Err(e) => return OffscreenDraw::Failed(format!("glGenRenderbuffers failed: {e}")),
    };
    gl.bind_renderbuffer(glow::RENDERBUFFER, Some(ds_rbo));
    if use_msaa {
      gl.renderbuffer_storage_multisample(
        glow::RENDERBUFFER,
        samples,
        glow::DEPTH24_STENCIL8,
        alloc_width,
        alloc_height,
      );
    } else {
      gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, alloc_width, alloc_height);
    }

    // Color target: a multisampled renderbuffer to resolve from (MSAA), or the
    // adopt-target texture attached directly (single-sample).
    let color_rbo = if use_msaa {
      let rb = match gl.create_renderbuffer() {
        Ok(rb) => rb,
        Err(e) => {
          gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
          gl.delete_renderbuffer(ds_rbo);
          return OffscreenDraw::Failed(format!("glGenRenderbuffers failed: {e}"));
        }
      };
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
      gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA8, alloc_width, alloc_height);
      Some(rb)
    } else {
      None
    };
    gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));

    let draw_fbo = match gl.create_framebuffer() {
      Ok(fbo) => fbo,
      Err(e) => {
        if let Some(rb) = color_rbo {
          gl.delete_renderbuffer(rb);
        }
        gl.delete_renderbuffer(ds_rbo);
        return OffscreenDraw::Failed(format!("glGenFramebuffers failed: {e}"));
      }
    };
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(draw_fbo));
    match color_rbo {
      Some(rb) => gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rb)),
      None => gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0),
    }
    gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(ds_rbo));

    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    if status != glow::FRAMEBUFFER_COMPLETE {
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      gl.delete_framebuffer(draw_fbo);
      if let Some(rb) = color_rbo {
        gl.delete_renderbuffer(rb);
      }
      gl.delete_renderbuffer(ds_rbo);
      // Under MSAA an incomplete config is recoverable by dropping MSAA; a
      // single-sample incomplete FBO is fatal.
      return if use_msaa {
        OffscreenDraw::MsaaUnavailable
      } else {
        OffscreenDraw::Failed(format!("offscreen framebuffer incomplete: {status:#x}"))
      };
    }

    // glTexImage2D(..., null) / renderbuffer storage leave contents
    // driver-defined. Desktop GL tends to hand back zeroed memory, but on
    // Android's tile-based GPUs it can be leftover tile data from unrelated
    // content. Force a defined transparent base regardless of whether
    // Impeller's own surface clear covers an externally-built FBO.
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
      match gl.create_framebuffer() {
        Ok(resolve_fbo) => {
          gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(resolve_fbo));
          gl.framebuffer_texture_2d(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
          if gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
            // A prior Impeller pass may leave the scissor test enabled; blit
            // honours it, so disable it to copy the full rect.
            gl.disable(glow::SCISSOR_TEST);
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
          } else {
            result = Err("offscreen resolve framebuffer incomplete".to_string());
          }
          gl.delete_framebuffer(resolve_fbo);
        }
        Err(e) => result = Err(format!("glGenFramebuffers failed (resolve): {e}")),
      }
    }

    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    gl.delete_framebuffer(draw_fbo);
    if let Some(rb) = color_rbo {
      gl.delete_renderbuffer(rb);
    }
    gl.delete_renderbuffer(ds_rbo);
    // No glFinish here: this offscreen draw is part of the UI thread's frame, so
    // Context::submit's per-frame fence orders it ahead of the render thread
    // sampling the adopted texture (it waits on that fence before compositing).

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

#[allow(dead_code)]
pub struct GlSurface {
  ctx: ImpellerContext,
  surface: impellers::Surface,
  // GLES bindings for the render thread's main context (current on this thread).
  // Used to wait on the UI thread's per-frame fence before Impeller samples its
  // textures; the wait and Impeller's draws share this context, so the GPU
  // orders the wait ahead of compositing.
  gl: glow::Context,
}

impl GlSurface {
  pub fn create(_window: &sdl3::video::Window, size: ISize) -> Result<Self, Box<dyn std::error::Error>> {
    let mut ctx = create_impeller_context();

    let surface = unsafe { ctx.wrap_fbo(0, PixelFormat::RGBA8888, size) }
      .ok_or_else(|| Box::new(std::io::Error::other("Failed to wrap framebuffer")) as Box<dyn std::error::Error>)?;

    let gl = create_gl_context();

    Ok(GlSurface { ctx, surface, gl })
  }
}

impl RenderSurface for GlSurface {
  fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>> {
    self
      .surface
      .draw_display_list(dl)
      .map_err(|_| Box::new(std::io::Error::other("Failed to draw display list")) as Box<dyn std::error::Error>)
  }

  fn present(&mut self, window: &sdl3::video::Window) {
    window.gl_swap_window();
  }

  fn resize(&mut self, size: ISize) {
    self.surface = unsafe { self.ctx.wrap_fbo(0, PixelFormat::RGBA8888, size) }.expect("Failed to resize GL surface");
  }

  fn consume_fence(&self, fence: Option<GpuFence>, wait: bool) {
    if let Some(GpuFence(sync)) = fence {
      unsafe {
        // GPU-side wait: subsequent draws on this context wait for the UI
        // thread's work to complete, without stalling the render thread's CPU.
        if wait {
          self.gl.wait_sync(sync, 0, glow::TIMEOUT_IGNORED);
        }
        self.gl.delete_sync(sync);
      }
    }
  }
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
  ui_context: &sdl3::video::GLContext,
  closure: impl FnOnce(Arc<Context>) + Send + 'static,
  tx: mpsc::Sender<Frame>,
  wake: Option<Box<dyn Fn() + Send + Sync>>,
) {
  let gl_context_ptr = Box::new(SendablePtr(unsafe { ui_context.raw() as *mut std::ffi::c_void }));

  let spawn_result =
    std::thread::Builder::new().name("srt-ui".into()).stack_size(UI_THREAD_STACK_SIZE).spawn(move || {
      let egl_display = unsafe { sdl3::sys::video::SDL_EGL_GetCurrentDisplay() };
      assert!(!egl_display.is_null(), "no EGL display");

      let ui_pbuffer = create_ui_pbuffer(egl_display, gl_context_ptr.0);
      make_current(egl_display, ui_pbuffer, gl_context_ptr.0);

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

      let gpu_ctx = Arc::new(Context::new(Backend::Gl, gl, impeller_ctx, tx, wake));
      closure(gpu_ctx);
    });
  spawn_result.expect("failed to spawn UI thread");
}

/// Must be called before window creation so SDL selects ANGLE (EGL) on macOS.
pub(crate) fn configure_opengl(video: &sdl3::VideoSubsystem) {
  sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");
  let gl_attr = video.gl_attr();
  gl_attr.set_context_profile(sdl3::video::GLProfile::GLES);
  gl_attr.set_context_version(3, 0);
  gl_attr.set_stencil_size(8);

  // Request 4x MSAA for path anti-aliasing. Not all drivers expose a
  // multisampled config; on those, window creation retries without MSAA
  // (see disable_msaa and app::setup).
  gl_attr.set_multisample_buffers(1);
  gl_attr.set_multisample_samples(4);
}

/// Drop the MSAA request so window and GL-context creation can succeed on
/// drivers that expose no multisampled EGL config (notably the Android
/// emulator's GLES translator). Path anti-aliasing is lost; rendering proceeds.
pub(crate) fn disable_msaa(video: &sdl3::VideoSubsystem) {
  let gl_attr = video.gl_attr();
  gl_attr.set_multisample_buffers(0);
  gl_attr.set_multisample_samples(0);
}

pub(crate) fn setup_opengl_platform(
  video: &sdl3::VideoSubsystem,
  window: &sdl3::video::Window,
) -> Result<DisplayContext, Box<dyn std::error::Error>> {
  // Create UI GL context
  let ui_context = window.gl_create_context().map_err(|e| format!("Failed to create UI GL context: {}", e))?;

  // Enable context sharing for main GL context
  let gl_attr = video.gl_attr();
  gl_attr.set_share_with_current_context(true);

  // Create main GL context
  let main_context = window.gl_create_context().map_err(|e| format!("Failed to create main GL context: {}", e))?;

  // Make main context current on the render thread
  window.gl_make_current(&main_context).map_err(|e| format!("Failed to make main GL context current: {}", e))?;

  // Set swap interval (vsync) via FFI
  video.gl_set_swap_interval(SwapInterval::VSync).map_err(|e| format!("Failed to set swap interval: {}", e))?;

  Ok(DisplayContext::Gl { window_opaque: window as *const _ as *const std::ffi::c_void, main_context, ui_context })
}
