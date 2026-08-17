//! Context bootstrap: SDL GL attribute configuration, context creation on the
//! main thread, the interactive GlBinding, and loading the glow and Impeller
//! contexts on the raster thread.

use crate::backend::GlBinding;
use crate::{DisplayContext, GpuTexture};
use impellers::{Context as ImpellerContext, ISize, Texture};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

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

/// The interactive binding: SDL's window + GLContext pair (see GlBinding).
pub(crate) struct SdlGlBinding {
  window: SendablePtr,
  context: SendablePtr,
}

impl SdlGlBinding {
  pub(crate) fn new(window: *mut sdl3::sys::video::SDL_Window, gl_context: &sdl3::video::GLContext) -> Self {
    SdlGlBinding {
      window: SendablePtr(window as *mut std::ffi::c_void),
      context: SendablePtr(unsafe { gl_context.raw() as *mut std::ffi::c_void }),
    }
  }

  fn window(&self) -> *mut sdl3::sys::video::SDL_Window {
    self.window.get() as *mut sdl3::sys::video::SDL_Window
  }
}

impl GlBinding for SdlGlBinding {
  /// The unbind step is load-bearing for the rebind case: SDL_GL_MakeCurrent
  /// short-circuits when its per-thread bookkeeping says this (window,
  /// context) pair is already current (SDL_video.c), so a same-pair call never
  /// reaches eglMakeCurrent - and re-executing eglMakeCurrent against the
  /// window's new EGL surface (recreated across an Android background/resume)
  /// is the whole point. SDL's own android_egl_context_restore does the same
  /// dance.
  fn bind(&self) -> bool {
    let context = self.context.get() as sdl3::sys::video::SDL_GLContext;
    unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(self.window(), std::ptr::null_mut()) };
    unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(self.window(), context) }
  }

  fn swap(&self) -> bool {
    crate::sdl_utils::gl_swap_window_checked(self.window())
  }

  fn set_swap_interval(&self) -> bool {
    unsafe { sdl3::sys::video::SDL_GL_SetSwapInterval(crate::sdl_utils::WINDOW_SWAP_INTERVAL) }
  }

  fn proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
    unsafe { sdl3::sys::video::SDL_GL_GetProcAddress(name.as_ptr()) }
      .map(|f| f as *const std::ffi::c_void)
      .unwrap_or(std::ptr::null())
  }

  fn error(&self) -> String {
    crate::sdl_utils::sdl_error()
  }
}

/// Load GLES bindings through the binding's proc-address. Must be called on
/// the raster thread with the GL context current; the returned context drives
/// all of alloy's GL work (texture allocation/upload, offscreen FBOs,
/// readback) and shares the underlying GL objects with Impeller, which
/// renders on the same context.
pub(crate) fn create_gl_context(binding: &dyn GlBinding) -> glow::Context {
  unsafe {
    glow::Context::from_loader_function(|name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      binding.proc_address(&cname)
    })
  }
}

pub(crate) fn create_impeller_context(binding: &dyn GlBinding) -> ImpellerContext {
  unsafe {
    ImpellerContext::new_opengl_es(|name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      binding.proc_address(&cname) as *mut _
    })
  }
  .expect("Failed to create Impeller context")
}

/// Adopt a GpuTexture's GL name into Impeller (zero-copy). Adoption transfers
/// ownership of the GL name to Impeller, which deletes it when its Texture is
/// dropped; GpuTexture therefore never deletes the name itself.
pub(crate) fn adopt_texture(gpu_texture: &GpuTexture, impeller_ctx: &ImpellerContext, size: ISize) -> Option<Texture> {
  let (width, height) = (size.width as u32, size.height as u32);
  let gl_handle = gpu_texture.gl_texture.0.get() as u64;

  unsafe { impeller_ctx.adopt_opengl_texture(width, height, 1, gl_handle) }
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
    gl_attr.set_multisample_samples(super::rig::MSAA_SAMPLES as u8);
  }

  // On desktop the window is deliberately single-sample: every frame
  // rasterizes into the multisampled offscreen rig and resolves into FBO 0
  // (see render_display_list_to_window), so a multisampled backbuffer would only
  // duplicate that storage. This also removes the old dependency on the
  // driver exposing a multisampled EGL config at all (the Android emulator
  // does not, which used to force a retry-without-MSAA window path).
}

pub(crate) fn setup_opengl_platform(
  window: &sdl3::video::Window,
) -> Result<DisplayContext, Box<dyn std::error::Error>> {
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
