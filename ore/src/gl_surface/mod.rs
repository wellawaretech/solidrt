//! The GL side of the window: where the GL libraries come from, and the
//! GL surface the drawing thread presents through.
//!
//! The library path: the full path of a GL library SDL loads, for a host
//! that carries its own (a single-file packed executable that extracts
//! ANGLE into its cache), so SDL loads exactly that file instead of
//! opening a bare library name. Main thread, before the window opens.
//!
//! The surface (`GlSurface`): a `Send` handle onto the window's GL context
//! for the one thread that draws (make current, swap, proc address), with
//! the macOS swap pacer inside its swap (`display_link`) and the probe of
//! the GPU device behind the EGL display (`DeviceProbe`). The context
//! itself is created, destroyed and recreated on the main thread by the
//! window (`crate::window::Window`), which owns it.

mod device_loss;
#[cfg(target_os = "macos")]
mod display_link;
mod surface;

pub use device_loss::DeviceProbe;
pub(crate) use surface::create_context;
pub use surface::GlSurface;

/// A GL library SDL loads by path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlLibrary {
  /// The EGL library (SDL_HINT_EGL_LIBRARY).
  Egl,
  /// The OpenGL ES library (SDL_HINT_OPENGL_LIBRARY).
  Gles,
}

/// Name the full path SDL loads `library` from, so it loads exactly that
/// file instead of dlopen-ing a bare name. Call it before the window is
/// created: SDL reads the hint when it loads the GL library for the window.
pub fn set_library_path(library: GlLibrary, path: &std::path::Path) {
  let hint = match library {
    GlLibrary::Egl => "SDL_EGL_LIBRARY",
    GlLibrary::Gles => "SDL_OPENGL_LIBRARY",
  };
  sdl3::hint::set(hint, &path.to_string_lossy());
}
