//! Context bootstrap: SDL GL attribute configuration, context creation on the
//! main thread, the interactive GlBinding, and loading the glow context and
//! the renderer on the raster thread.

use crate::backend::GlBinding;
use crate::gpu::GpuLimits;
use crate::renderer::prelude::*;
use crate::renderer::Renderer;
use crate::DisplayContext;
use glow::HasContext;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
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
  /// The display link the swap waits on (display_link.rs): ANGLE-Metal's
  /// swap never blocks, so the wait for the display's tick happens here.
  /// Created at the first swap, on the raster thread; None when the machine
  /// has no link.
  #[cfg(target_os = "macos")]
  pacer: std::cell::OnceCell<Option<crate::display_link::SwapPacer>>,
}

impl SdlGlBinding {
  pub(crate) fn new(window: *mut sdl3::sys::video::SDL_Window, gl_context: &sdl3::video::GLContext) -> Self {
    SdlGlBinding {
      window: SendablePtr(window as *mut std::ffi::c_void),
      context: SendablePtr(unsafe { gl_context.raw() as *mut std::ffi::c_void }),
      #[cfg(target_os = "macos")]
      pacer: std::cell::OnceCell::new(),
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
    #[cfg(target_os = "macos")]
    if let Some(pacer) = self.pacer.get_or_init(crate::display_link::SwapPacer::start) {
      pacer.wait_tick();
    }
    crate::sdl_utils::gl_swap_window_checked(self.window())
  }

  fn set_swap_interval(&self) -> bool {
    unsafe { sdl3::sys::video::SDL_GL_SetSwapInterval(crate::sdl_utils::WINDOW_SWAP_INTERVAL) }
  }

  fn release(&self) -> bool {
    unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(self.window(), std::ptr::null_mut()) }
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

// EGL_EXT_create_context_robustness: the context attribute that asks for
// reset notification, and the strategy that loses the context on a reset.
// Without it the reset status reads no-error forever (the robustness spec
// requires it), and a context-level loss on ANGLE is invisible: the swap
// keeps succeeding (measured, okf/done/gpu-context-loss.md). SDL's EGL
// backend never asks for it; its attribute callback appends these to the
// eglCreateContext attributes.
const EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT: sdl3::sys::video::SDL_EGLint = 0x3138;
const EGL_LOSE_CONTEXT_ON_RESET_EXT: sdl3::sys::video::SDL_EGLint = 0x31BF;
const EGL_NONE: sdl3::sys::video::SDL_EGLint = 0x3038;

// Whether EGL granted the reset-notification attribute: 0 = not asked yet,
// 1 = granted (the callback stays installed for every later context), 2 =
// refused (the callback was removed; contexts are created without it).
static RESET_NOTIFICATION: AtomicU8 = AtomicU8::new(RESET_NOTIFICATION_UNTRIED);
const RESET_NOTIFICATION_UNTRIED: u8 = 0;
const RESET_NOTIFICATION_GRANTED: u8 = 1;
const RESET_NOTIFICATION_REFUSED: u8 = 2;

/// SDL's EGL context-attribute hook: the array (SDL frees it) is appended
/// to the eglCreateContext attributes.
unsafe extern "C" fn reset_notification_attribs(
  _userdata: *mut std::ffi::c_void,
  _display: sdl3::sys::video::SDL_EGLDisplay,
  _config: sdl3::sys::video::SDL_EGLConfig,
) -> *mut sdl3::sys::video::SDL_EGLint {
  let attribs = [EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT, EGL_LOSE_CONTEXT_ON_RESET_EXT, EGL_NONE];
  let out = sdl3::sys::stdinc::SDL_malloc(std::mem::size_of_val(&attribs)) as *mut sdl3::sys::video::SDL_EGLint;
  if !out.is_null() {
    std::ptr::copy_nonoverlapping(attribs.as_ptr(), out, attribs.len());
  }
  out
}

fn set_reset_notification_callback(install: bool) {
  let callback = if install { Some(reset_notification_attribs as _) } else { None };
  unsafe { sdl3::sys::video::SDL_EGL_SetAttributeCallbacks(None, None, callback, std::ptr::null_mut()) };
}

/// The window's GL context, with reset notification where EGL grants it:
/// the first creation asks, and a refusal (an EGL without
/// EGL_EXT_create_context_robustness fails the create with the attribute
/// present) is remembered, so every later context - the context-loss
/// recovery's included - is created the way the first one succeeded. The
/// context is current on the calling thread afterwards, as SDL leaves it.
pub(crate) fn create_window_gl_context(window: &sdl3::video::Window) -> Result<sdl3::video::GLContext, String> {
  if RESET_NOTIFICATION.load(Ordering::Relaxed) == RESET_NOTIFICATION_UNTRIED {
    set_reset_notification_callback(true);
    match window.gl_create_context() {
      Ok(context) => {
        RESET_NOTIFICATION.store(RESET_NOTIFICATION_GRANTED, Ordering::Relaxed);
        return Ok(context);
      }
      Err(e) => {
        log::info!("[alloy] GL context without reset notification: EGL refused the attribute ({e})");
        set_reset_notification_callback(false);
        RESET_NOTIFICATION.store(RESET_NOTIFICATION_REFUSED, Ordering::Relaxed);
      }
    }
  }
  window.gl_create_context().map_err(|e| format!("Failed to create GL context: {e}"))
}

/// Load GLES bindings through the binding's proc-address. Must be called on
/// the raster thread with the GL context current; the returned context drives
/// all of alloy's GL work (texture allocation/upload, offscreen FBOs,
/// readback) and shares the underlying GL objects with the renderer, which
/// draws on the same context.
pub(crate) fn create_gl_context(binding: &dyn GlBinding) -> glow::Context {
  unsafe {
    glow::Context::from_loader_function(|name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      binding.proc_address(&cname)
    })
  }
}

/// Create the renderer over the context current on the raster thread; it
/// lives as long as the context (see raster/recover.rs for the order on a
/// context loss).
pub(crate) fn create_renderer(binding: &dyn GlBinding) -> Renderer {
  unsafe {
    Renderer::new(&mut |name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      binding.proc_address(&cname) as *mut _
    })
  }
  .expect("Failed to create the renderer")
}

/// Keep ANGLE's libraries loaded for the process life. SDL loads them for
/// the first GL window and frees them when the last one goes, and a
/// context-loss recovery on these platforms recreates the window
/// (okf/done/gpu-context-loss.md): the unload tears down ANGLE's D3D and
/// DXCore state and the reload makes it anew, after which the D3D runtime
/// fast-fails on a corrupted internal list on the first frame (measured on
/// Windows: FAST_FAIL_CORRUPT_LIST_ENTRY in dxcore.dll under the raster
/// thread). With a reference of our own the free never unmaps them, and the
/// recovery is what ANGLE designed for: eglTerminate and eglInitialize on
/// one loaded library. Resolved by name, which finds the modules SDL will
/// load (beside the executable, or already loaded by the packed runner's
/// provisioning); GLES before EGL, since EGL imports it and a name lookup
/// must find it loaded. A library that is not found is SDL's problem to
/// report at window creation, not this one's.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn pin_gl_libraries() {
  #[cfg(target_os = "windows")]
  const LIBRARIES: [&str; 2] = ["libGLESv2.dll", "libEGL.dll"];
  #[cfg(target_os = "macos")]
  const LIBRARIES: [&str; 2] = ["libGLESv2.dylib", "libEGL.dylib"];
  for name in LIBRARIES {
    // Safety: loading ANGLE's libraries runs their initializers, which SDL
    // runs the same way a moment later.
    match unsafe { libloading::Library::new(name) } {
      Ok(library) => std::mem::forget(library),
      Err(e) => log::debug!("[alloy] GL library {name} not pinned: {e}"),
    }
  }
}

// Whether `configure_opengl` asked for a multisampled window backbuffer and
// nothing withdrew it since. Kept here rather than read back from SDL: after
// a failed window creation no GL driver is loaded and SDL_GL_GetAttribute
// fails, while SDL_GL_SetAttribute works at any time.
static WINDOW_MULTISAMPLING_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Withdraw the request for a multisampled window backbuffer (see
/// `configure_opengl`); true when one was in force, false when there was
/// nothing to withdraw and a failed window creation has another cause.
pub(crate) fn drop_window_multisampling(video: &sdl3::VideoSubsystem) -> bool {
  if !WINDOW_MULTISAMPLING_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
    return false;
  }
  let gl_attr = video.gl_attr();
  gl_attr.set_multisample_buffers(0);
  gl_attr.set_multisample_samples(0);
  true
}

/// Must be called before window creation so SDL selects ANGLE (EGL) on macOS.
pub(crate) fn configure_opengl(video: &sdl3::VideoSubsystem) {
  #[cfg(any(target_os = "windows", target_os = "macos"))]
  pin_gl_libraries();
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

  // Alpha bits on Android, so the window buffer carries the transparency
  // the frame draw already writes (it clears to transparent black): with
  // SDL's SurfaceView flagged translucent (SolidRTActivity), uncovered
  // pixels then show the video plane beneath the UI
  // (okf/done/android-video-punch-through.md). Without alpha bits the
  // buffer is RGBX and the hole never opens. Android only: nowhere else is
  // there a plane to show through to, and a desktop window's alpha means
  // something else to its compositor.
  #[cfg(target_os = "android")]
  gl_attr.set_alpha_size(8);

  // On Android the window backbuffer itself is multisampled: the tiled GPU
  // resolves in-tile at swap, so plain window frames draw straight into
  // FBO 0 with no rig pass and no resolve copy - the only MSAA
  // configuration this class of GPU runs at full rate
  // (okf/backlog/android-surface-swap-latency.md).
  #[cfg(target_os = "android")]
  {
    gl_attr.set_multisample_buffers(1);
    gl_attr.set_multisample_samples(super::rig::MSAA_SAMPLES as u8);
    WINDOW_MULTISAMPLING_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
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
  let gl_context = create_window_gl_context(window)?;
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

/// Query the device ceilings from the live context (raster thread, the one
/// place GL exists). Values are clamped up to the ES 3.0 floors: a context
/// this engine could create guarantees them, and a garbage glGet must not
/// turn every create into an error.
pub(crate) fn query_limits(gl: &glow::Context) -> GpuLimits {
  let floor = GpuLimits::FLOOR;
  unsafe {
    let tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
    let cube = gl.get_parameter_i32(glow::MAX_CUBE_MAP_TEXTURE_SIZE);
    let tex_3d = gl.get_parameter_i32(glow::MAX_3D_TEXTURE_SIZE);
    let layers = gl.get_parameter_i32(glow::MAX_ARRAY_TEXTURE_LAYERS);
    let rb = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE);
    let units = gl.get_parameter_i32(glow::MAX_TEXTURE_IMAGE_UNITS);
    let attribs = gl.get_parameter_i32(glow::MAX_VERTEX_ATTRIBS);
    let vertex_vectors = gl.get_parameter_i32(glow::MAX_VERTEX_UNIFORM_VECTORS);
    // An extension, never core at any GL level, but present on practically
    // every ES 3.0 device and on ANGLE over D3D11/Metal; absence is a fact
    // to report, not an error. A desktop core profile may list only the
    // GL 4.6 spelling (ARB), same enums. The parameter is a float in the
    // spec.
    let ext = gl.supported_extensions();
    let anisotropy =
      if ext.contains("GL_EXT_texture_filter_anisotropic") || ext.contains("GL_ARB_texture_filter_anisotropic") {
        gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT) as i32
      } else {
        1
      };
    // Half-float render targets are likewise an extension at every GLES
    // level (the float one implies the half one); a fact for the mip gate
    // on rgba16f, never an error.
    let half_float_renderable =
      ext.contains("GL_EXT_color_buffer_half_float") || ext.contains("GL_EXT_color_buffer_float");
    // BC7 uploads: the desktop block format, an extension at every GLES
    // level (ANGLE over D3D11 and Metal, Mesa) and absent on mobile GPUs;
    // a fact for the upload-format gate, never an error. ETC2 needs no
    // query: GLES 3.0 core.
    let bc7_textures =
      ext.contains("GL_EXT_texture_compression_bptc") || ext.contains("GL_ARB_texture_compression_bptc");
    GpuLimits {
      max_texture_size: tex.min(rb).max(floor.max_texture_size as i32) as u32,
      max_cube_map_size: cube.max(floor.max_cube_map_size as i32) as u32,
      max_3d_texture_size: tex_3d.max(floor.max_3d_texture_size as i32) as u32,
      max_array_texture_layers: layers.max(floor.max_array_texture_layers as i32) as u32,
      max_texture_units: units.max(floor.max_texture_units as i32) as u32,
      max_vertex_attribs: attribs.max(floor.max_vertex_attribs as i32) as u32,
      max_anisotropy: anisotropy.max(floor.max_anisotropy as i32) as u32,
      max_vertex_uniform_vectors: vertex_vectors.max(floor.max_vertex_uniform_vectors as i32) as u32,
      half_float_renderable,
      bc7_textures,
    }
  }
}
