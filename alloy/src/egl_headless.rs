//! Headless GL context without SDL's video subsystem: an EGL pbuffer on the
//! default display, for playback on a stack whose SDL offscreen driver cannot
//! go headless (ANGLE lacks EGL_EXT_device_enumeration, which that driver
//! requires; see okf/backlog/playback-headless-angle.md). Playback draws to
//! FBO 0 and reads it back, so a pbuffer keeps every downstream path (window
//! draw, MSAA resolve, capture readback) unchanged. libEGL is loaded at
//! runtime by name; the crate compiles on every target and only ever runs
//! after the offscreen driver failed.

use crate::backend::GlBinding;
use khronos_egl as egl;
use std::sync::Arc;

pub(crate) type Egl = egl::DynamicInstance<egl::EGL1_4>;

/// The pbuffer context and everything it hangs off. Handles are process-wide
/// EGL objects with no thread affinity of their own; the raster thread is the
/// only one that binds them (Send + Sync below covers the Arc hand-off).
pub struct HeadlessEgl {
  egl: Egl,
  display: egl::Display,
  surface: egl::Surface,
  context: egl::Context,
  // ANGLE resolves core GLES entry points through eglGetProcAddress
  // (EGL_KHR_get_all_proc_addresses); a stack that does not gets them from
  // the client library directly.
  gles: Option<libloading::Library>,
}

unsafe impl Send for HeadlessEgl {}
unsafe impl Sync for HeadlessEgl {}

#[cfg(target_os = "windows")]
const EGL_LIB: &[&str] = &["libEGL.dll"];
#[cfg(target_os = "windows")]
const GLES_LIB: &[&str] = &["libGLESv2.dll"];
#[cfg(target_os = "macos")]
const EGL_LIB: &[&str] = &["libEGL.dylib"];
#[cfg(target_os = "macos")]
const GLES_LIB: &[&str] = &["libGLESv2.dylib"];
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const EGL_LIB: &[&str] = &["libEGL.so.1", "libEGL.so"];
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const GLES_LIB: &[&str] = &["libGLESv2.so.2", "libGLESv2.so"];

// The candidate paths for a library: next to the executable first (where the
// packaged ANGLE lives), then the loader's own search.
fn library_candidates(names: &[&str]) -> Vec<std::path::PathBuf> {
  let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
  let mut out = Vec::new();
  for name in names {
    if let Some(dir) = &exe_dir {
      let local = dir.join(name);
      if local.exists() {
        out.push(local);
      }
    }
    out.push(std::path::PathBuf::from(name));
  }
  out
}

// Also used by raster::buffer_age: the same library SDL's EGL path loaded
// (system libEGL, or the packaged ANGLE next to the executable), so
// current-display/surface queries see SDL's per-thread binding.
pub(crate) fn load_egl() -> Result<Egl, String> {
  let mut last = String::new();
  for path in library_candidates(EGL_LIB) {
    match unsafe { Egl::load_required_from_filename(&path) } {
      Ok(egl) => return Ok(egl),
      Err(e) => last = format!("{}: {e}", path.display()),
    }
  }
  Err(format!("libEGL not loadable ({last})"))
}

fn load_gles() -> Option<libloading::Library> {
  library_candidates(GLES_LIB).into_iter().find_map(|path| unsafe { libloading::Library::new(&path) }.ok())
}

fn egl_err(what: &str, e: egl::Error) -> String {
  format!("{what}: {e}")
}

impl HeadlessEgl {
  /// Create the display, an ES 3.0 context, and a `width` x `height` pbuffer
  /// matching the interactive window's config (RGBA8, stencil 8). The context
  /// is left unbound: the raster thread binds it.
  pub fn new(width: u32, height: u32) -> Result<Self, String> {
    let egl = load_egl()?;
    let display = unsafe { egl.get_display(egl::DEFAULT_DISPLAY) }.ok_or("no default EGL display")?;
    let (major, minor) = egl.initialize(display).map_err(|e| egl_err("eglInitialize", e))?;
    let build = || -> Result<(egl::Surface, egl::Context), String> {
      egl.bind_api(egl::OPENGL_ES_API).map_err(|e| egl_err("eglBindAPI", e))?;
      let attribs = [
        egl::SURFACE_TYPE,
        egl::PBUFFER_BIT,
        egl::RENDERABLE_TYPE,
        egl::OPENGL_ES3_BIT,
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::DEPTH_SIZE,
        16,
        egl::STENCIL_SIZE,
        8,
        egl::NONE,
      ];
      let count = egl.matching_config_count(display, &attribs).map_err(|e| egl_err("eglChooseConfig", e))?;
      let mut configs = Vec::with_capacity(count.max(1));
      egl.choose_config(display, &attribs, &mut configs).map_err(|e| egl_err("eglChooseConfig", e))?;
      let config = *configs.first().ok_or("no EGL config offers an ES 3.0 RGBA8/stencil pbuffer")?;
      let surface = egl
        .create_pbuffer_surface(display, config, &[egl::WIDTH, width as i32, egl::HEIGHT, height as i32, egl::NONE])
        .map_err(|e| egl_err("eglCreatePbufferSurface", e))?;
      let context = match egl.create_context(display, config, None, &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE]) {
        Ok(context) => context,
        Err(e) => {
          egl.destroy_surface(display, surface).ok();
          return Err(egl_err("eglCreateContext", e));
        }
      };
      Ok((surface, context))
    };
    let (surface, context) = match build() {
      Ok(pair) => pair,
      Err(e) => {
        egl.terminate(display).ok();
        return Err(e);
      }
    };
    log::info!("[alloy] headless EGL {major}.{minor} pbuffer context ({width}x{height})");
    Ok(HeadlessEgl { egl, display, surface, context, gles: load_gles() })
  }
}

impl Drop for HeadlessEgl {
  fn drop(&mut self) {
    self.egl.make_current(self.display, None, None, None).ok();
    self.egl.destroy_context(self.display, self.context).ok();
    self.egl.destroy_surface(self.display, self.surface).ok();
    self.egl.terminate(self.display).ok();
  }
}

/// The raster thread's handle onto a HeadlessEgl (see backend::GlBinding).
pub(crate) struct HeadlessEglBinding(pub Arc<HeadlessEgl>);

impl GlBinding for HeadlessEglBinding {
  fn bind(&self) -> bool {
    let h = &self.0;
    h.egl.make_current(h.display, Some(h.surface), Some(h.surface), Some(h.context)).is_ok()
  }

  /// A pbuffer has nothing to present to; capture reads FBO 0 directly.
  fn swap(&self) -> bool {
    true
  }

  fn set_swap_interval(&self) -> bool {
    true
  }

  fn proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
    let h = &self.0;
    let Ok(name_str) = name.to_str() else { return std::ptr::null() };
    if let Some(f) = h.egl.get_proc_address(name_str) {
      return f as *const std::ffi::c_void;
    }
    match &h.gles {
      Some(lib) => unsafe { lib.get::<*const std::ffi::c_void>(name.to_bytes_with_nul()) }
        .map(|sym| *sym)
        .unwrap_or(std::ptr::null()),
      None => std::ptr::null(),
    }
  }

  fn error(&self) -> String {
    self.0.egl.get_error().map(|e| e.to_string()).unwrap_or_else(|| "no EGL error".into())
  }
}
