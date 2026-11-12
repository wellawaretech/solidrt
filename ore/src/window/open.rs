//! Opening the window: the GL configuration that must precede a GL window,
//! the window build with its multisampling retry, and the headless chain
//! (offscreen, then dummy without GL, then a hidden window on the real
//! display). Main thread only.

use super::{Backing, Display, Window, WindowConfig};

/// Open the window `config` asks for (see `Display`). A GL window comes
/// back with its context created and current on this thread;
/// `Window::create_gl_surface` releases it for the drawing thread.
/// Once the window exists, the dialogs' availability is probed for it: a
/// hidden or offscreen surface is one no dialog could attach to.
pub(crate) fn open(sdl: &sdl3::Sdl, config: WindowConfig<'_>) -> Result<Window, String> {
  let window = match config.display {
    Display::Interactive => open_gl(sdl, &config, true),
    Display::Hidden => open_gl(sdl, &config, false),
    Display::Offscreen => open_offscreen(sdl, &config),
  }?;
  crate::dialog::set_available(config.display == Display::Interactive && crate::dialog::platform_available());
  Ok(window)
}

// The headless mode wants no display at all: SDL's offscreen video driver
// backs the window with an EGL pbuffer, so `sol render` and `sol test` run
// in CI, over SSH, on any headless box - and its fake display has no scale
// to inherit. Where the driver fails only because the GL stack lacks EGL
// device enumeration (ANGLE never implements it), SDL's dummy driver still
// provides the Window the run's size is read from, with no GL (the dummy
// driver loads no GL library): Backing::NoGl, and the caller builds its
// own EGL pbuffer context outside SDL (alloy's egl_headless.rs). Anything
// else falls back to a hidden window on the real display. Whichever
// surface results only keeps the context current: a headless frame is
// drawn into a framebuffer the drawing thread owns at the headless
// window's size, so the surface's own size and content never matter after
// setup. Each failed attempt dropped its video subsystem handles, so
// setting the hint and opening again re-initializes video on the next
// driver. The driver hint is cleared once the chain is done, so a later
// open (the caller's own hidden-window fallback) lands on the real display.
fn open_offscreen(sdl: &sdl3::Sdl, config: &WindowConfig<'_>) -> Result<Window, String> {
  sdl3::hint::set("SDL_VIDEO_DRIVER", "offscreen");
  let opened = match open_gl(sdl, config, false) {
    Ok(window) => Some(window),
    Err(e) if e.contains("EXT_device_enumeration") || e.contains("eglQueryDevicesEXT") => {
      log::info!("[ore] offscreen video driver needs EGL device enumeration, which this GL stack (ANGLE) does not provide; using a headless EGL context");
      sdl3::hint::set("SDL_VIDEO_DRIVER", "dummy");
      match open_no_gl(sdl, config) {
        Ok(window) => Some(window),
        Err(e) => {
          log::warn!("[ore] dummy video driver window unavailable ({e}); falling back to a hidden window");
          None
        }
      }
    }
    Err(e) => {
      log::warn!("[ore] offscreen video driver unavailable ({e}); falling back to a hidden window");
      None
    }
  };
  sdl3::hint::set("SDL_VIDEO_DRIVER", "");
  match opened {
    Some(window) => Ok(window),
    None => open_gl(sdl, config, false),
  }
}

// A GL window: centered and high pixel density; resizable and shown when
// `interactive`, hidden and fixed-size otherwise. Its context is created
// here, current on this thread.
fn open_gl(sdl: &sdl3::Sdl, config: &WindowConfig<'_>, interactive: bool) -> Result<Window, String> {
  let video = sdl.video().map_err(|e| format!("video subsystem: {e}"))?;
  // Platform fact worth having in every log: input behavior (pointer lock,
  // warp, coordinate spaces) differs per driver (wayland vs x11/XWayland).
  log::info!("[ore] video driver: {}", video.current_video_driver());

  configure_opengl(&video, config.backbuffer_samples);

  let mut builder = video.window(config.title, config.width, config.height);
  builder.opengl().position_centered().high_pixel_density();
  // A headless window is hidden and fixed-size: keeping it non-resizable
  // stops the compositor from negotiating a different surface size on a
  // scaled display, which would diverge from the requested dimensions.
  if interactive {
    builder.resizable();
  }
  let mut window = match builder.build() {
    Ok(window) => window,
    // No window config with a multisampled backbuffer on this EGL (the
    // Android emulator's GLES translator offers none): a single-sample
    // window instead, every frame through alloy's rig and its resolve (see
    // alloy's gl::rig::window_samples). The attribute stays cleared, so a
    // window recreated after a context loss matches.
    Err(e) if drop_window_multisampling(&video) => {
      log::warn!("[ore] GL window creation failed ({e}); retrying with a single-sample backbuffer");
      builder.build().map_err(|e| format!("window creation: {e}{}", gl_library_hint(&e.to_string())))?
    }
    Err(e) => return Err(format!("window creation: {e}{}", gl_library_hint(&e.to_string()))),
  };
  if !interactive {
    window.hide();
  }

  let gl_context = crate::gl_surface::create_context(&window).map_err(|e| format!("GL setup: {e}"))?;
  Ok(Window::new(video, window, Some(gl_context), Backing::Gl))
}

// The headless mode on SDL's dummy video driver: a windowless Window (no GL
// flag: the dummy driver loads no GL library) at the requested size.
fn open_no_gl(sdl: &sdl3::Sdl, config: &WindowConfig<'_>) -> Result<Window, String> {
  let video = sdl.video().map_err(|e| format!("video subsystem: {e}"))?;
  let window = video
    .window(config.title, config.width, config.height)
    .hidden()
    .build()
    .map_err(|e| format!("window creation: {e}"))?;
  Ok(Window::new(video, window, None, Backing::NoGl))
}

// On platforms where GLES comes from ANGLE's shipped libraries, SDL's "Could
// not initialize OpenGL / GLES library" almost always means those libraries
// were not found. Name them and where they were expected, instead of leaving
// the raw SDL error as the only clue.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn gl_library_hint(err: &str) -> String {
  if !err.contains("Could not initialize OpenGL / GLES library") {
    return String::new();
  }
  #[cfg(target_os = "windows")]
  let names = ["libEGL.dll", "libGLESv2.dll"];
  #[cfg(target_os = "macos")]
  let names = ["libEGL.dylib", "libGLESv2.dylib"];
  let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf)) else {
    return String::new();
  };
  let missing: Vec<&str> = names.iter().filter(|name| !dir.join(name).exists()).copied().collect();
  if missing.is_empty() {
    format!(" ({} are present next to the executable but failed to load)", names.join(" and "))
  } else {
    format!(" ({} not found next to the executable; the runtime needs ANGLE's GL libraries)", missing.join(" and "))
  }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn gl_library_hint(_err: &str) -> String {
  String::new()
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
    // SAFETY: loading ANGLE's libraries runs their initializers, which SDL
    // runs the same way a moment later.
    match unsafe { libloading::Library::new(name) } {
      Ok(library) => std::mem::forget(library),
      Err(e) => log::debug!("[ore] GL library {name} not pinned: {e}"),
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
fn drop_window_multisampling(video: &sdl3::VideoSubsystem) -> bool {
  if !WINDOW_MULTISAMPLING_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
    return false;
  }
  let gl_attr = video.gl_attr();
  gl_attr.set_multisample_buffers(0);
  gl_attr.set_multisample_samples(0);
  true
}

/// Must be called before window creation so SDL selects ANGLE (EGL) on macOS.
/// `samples` is the multisample count asked for on the window backbuffer
/// where the platform honors it (Android; see below), 0 for single-sample.
fn configure_opengl(video: &sdl3::VideoSubsystem, samples: u32) {
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
    if samples > 0 {
      gl_attr.set_multisample_buffers(1);
      gl_attr.set_multisample_samples(u8::try_from(samples).unwrap_or(u8::MAX));
      WINDOW_MULTISAMPLING_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
  }

  // On desktop the window is deliberately single-sample: every frame
  // rasterizes into alloy's multisampled offscreen rig and resolves into
  // FBO 0 (see alloy's render_display_list_to_window), so a multisampled
  // backbuffer would only duplicate that storage. This also removes the old
  // dependency on the driver exposing a multisampled EGL config at all (the
  // Android emulator does not, which used to force a retry-without-MSAA
  // window path).
  #[cfg(not(target_os = "android"))]
  let _ = samples;
}
