//! The OS window: the one window a run has, opened from the platform
//! (`Platform::open_window`), with its commands (title, raise, icon,
//! fullscreen, size, minimize, hide), its cursors, its text input session, the
//! pointer lock, and its GL context (`create_gl_surface`, and
//! `recreate_gl` after a context loss). Also the input device presence
//! facts (keyboard, mouse, touch, the screen keyboard) and the
//! platform-reported screen keyboard inset.
//!
//! Part of ore's window part (the `window` feature). `Window` is not
//! `Send`, and every method is main thread only: SDL's window calls are
//! main thread calls. The GL surface it hands out is the one `Send` piece,
//! for the drawing thread (`crate::gl_surface::GlSurface`).
//!
//! Coordinates: sizes from `size` and the safe area are in logical points;
//! only `size_in_pixels` is physical.
//!
//! Resizing through the size limits: with `SOLIDRT_RESIZE_VIA_LIMITS` set
//! to anything but an empty string or "0", `set_size` also pins the
//! window's minimum and maximum size to the requested size, and lifts them
//! again once the window has that size (`settle_forced_size`) or before a
//! fullscreen change. It exists for compositors that report every window
//! maximized (Hyprland), where SDL holds a plain size request forever and
//! the size limits are the one way a window can resize itself. While a
//! forced size is pending, the user cannot drag the window to another size.

mod cursors;
mod open;
mod surface;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use std::sync::OnceLock;

use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::video::SDL_Window;

use crate::cursor::{Cursor, CursorFrame};
use crate::display::{Orientation, Rect};
use crate::gl_surface::GlSurface;
use crate::input::InputDevices;
use crate::text_input::TextInputOptions;

#[cfg(test)]
pub(crate) use cursors::create_image_cursor;
pub(crate) use open::open;

/// What the window is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
  /// A visible, resizable window on the display, centered.
  Interactive,
  /// A hidden, fixed-size window on the real display (the last headless fallback).
  Hidden,
  /// No display at all: SDL's offscreen driver (an EGL pbuffer behind a
  /// window), or, where that driver needs EGL device enumeration the GL
  /// stack lacks (ANGLE), the dummy driver with no GL at all
  /// (Backing::NoGl), for the caller to bring its own context. Anything
  /// else falls back to `Hidden` on the real display.
  Offscreen,
}

/// How the window opens.
pub struct WindowConfig<'a> {
  pub title: &'a str,
  pub width: u32,
  pub height: u32,
  pub display: Display,
  /// Multisamples asked for on the window backbuffer; 0 for single-sample.
  /// Honored where the platform's GPUs resolve in tile at swap (Android:
  /// plain window frames then draw straight into FBO 0 with no offscreen
  /// pass and no resolve copy, the only MSAA configuration that class of
  /// GPU runs at full rate), ignored elsewhere. A config that has no
  /// multisampled window falls back to single-sample with a warning (the
  /// Android emulator case).
  pub backbuffer_samples: u32,
}

/// What draws into the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backing {
  /// The window has a GL surface to create (`create_gl_surface`).
  Gl,
  /// The dummy driver: a windowless Window at the requested size, no GL.
  NoGl,
}

/// The OS window: owns the SDL window, its video subsystem handle, the GL
/// context once created, the cursors it created, and the state it reapplies
/// on a recreate (icon, text input session, pointer lock). !Send; every
/// method is main thread only.
pub struct Window {
  // Field order is drop order. The cursors go first (SDL frees what is
  // left of them when video quits); the context before the window it was
  // created on; the window before the video subsystem handle, whose last
  // drop quits video.
  cursors: cursors::Cursors,
  gl_context: Option<sdl3::video::GLContext>,
  /// Always present: empty only inside `recreate_gl`, between destroying
  /// the old window and building the new (an error there leaves it empty,
  /// and the caller ends the process).
  window: Option<sdl3::video::Window>,
  /// Held so video stays initialized while `recreate_gl` destroys the old
  /// window and builds the new.
  video: sdl3::VideoSubsystem,
  backing: Backing,
  surface_created: bool,
  /// The icon as last set (straight-alpha RGBA8), reapplied on a recreate.
  icon: Option<(u32, u32, Vec<u8>)>,
  /// The text input session while one is active, with the options it was
  /// started with, reapplied on a recreate.
  text_input: Option<TextInputOptions>,
  /// The pointer lock as last applied, reapplied on a recreate.
  pointer_lock: bool,
  /// The size `set_size` imposed through the window's minimum and maximum
  /// size, in SDL window units, while the window has not reached it (see
  /// the module doc).
  forced_size: Option<(u32, u32)>,
}

/// How a `set_size` request went to the platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeRequest {
  /// The plain SDL request: the compositor answers with a resize event, or
  /// holds it.
  Requested,
  /// The size was imposed through the window's minimum and maximum size;
  /// the caller must present a frame so the compositor sees them (they
  /// reach it with the next surface commit).
  Forced,
}

/// The window is held for its life; only a failed recreate empties it.
const WINDOW_HELD: &str = "the window is held for its life";

/// The environment variable that turns on resizing through the size limits
/// (see the module doc).
const RESIZE_VIA_LIMITS_ENV: &str = "SOLIDRT_RESIZE_VIA_LIMITS";

/// Whether a `RESIZE_VIA_LIMITS_ENV` value turns the workaround on: set to
/// anything but an empty string or "0".
pub(crate) fn resize_via_limits_enabled(value: Option<&str>) -> bool {
  value.is_some_and(|v| !v.is_empty() && v != "0")
}

// The gate, read from the environment once per process.
fn resize_via_limits() -> bool {
  static ENABLED: OnceLock<bool> = OnceLock::new();
  *ENABLED.get_or_init(|| resize_via_limits_enabled(std::env::var(RESIZE_VIA_LIMITS_ENV).ok().as_deref()))
}

/// The live window's handle, the parent the dialogs show on
/// (`crate::dialog`): set when a `Window` is built and again when
/// `recreate_gl` builds a new one, cleared before the window it names is
/// destroyed. Null while there is no window. Written and read on the main
/// thread only, where the window lives and the dialogs show, so a handle
/// read here names a window that is still alive.
static DIALOG_PARENT: AtomicPtr<SDL_Window> = AtomicPtr::new(std::ptr::null_mut());

/// The parent for a dialog shown now (see `DIALOG_PARENT`); null while
/// there is no window. Main thread only. The desktop dialogs' alone: an
/// Android dialog is the activity's.
#[cfg(not(target_os = "android"))]
pub(crate) fn dialog_parent() -> *mut SDL_Window {
  DIALOG_PARENT.load(Ordering::Relaxed)
}

// Stop naming `window` as the dialog parent, before it is destroyed; a
// parent another window already took stays.
fn clear_dialog_parent(window: &sdl3::video::Window) {
  let _ = DIALOG_PARENT.compare_exchange(window.raw(), std::ptr::null_mut(), Ordering::Relaxed, Ordering::Relaxed);
}

impl Window {
  fn new(
    video: sdl3::VideoSubsystem,
    window: sdl3::video::Window,
    gl_context: Option<sdl3::video::GLContext>,
    backing: Backing,
  ) -> Window {
    DIALOG_PARENT.store(window.raw(), Ordering::Relaxed);
    Window {
      cursors: cursors::Cursors::default(),
      gl_context,
      window: Some(window),
      video,
      backing,
      surface_created: false,
      icon: None,
      text_input: None,
      pointer_lock: false,
      forced_size: None,
    }
  }

  fn held(&self) -> &sdl3::video::Window {
    self.window.as_ref().expect(WINDOW_HELD)
  }

  fn held_mut(&mut self) -> &mut sdl3::video::Window {
    self.window.as_mut().expect(WINDOW_HELD)
  }

  pub fn backing(&self) -> Backing {
    self.backing
  }

  /// The SDL video driver in use ("wayland", "x11", "android", "offscreen", "dummy").
  pub fn video_driver(&self) -> String {
    self.video.current_video_driver().to_string()
  }

  /// The window's size in logical points (SDL_GetWindowSize; see
  /// `safe_area` for the one platform where SDL's units differ).
  pub fn size(&self) -> (u32, u32) {
    self.held().size()
  }

  /// The window's size in physical pixels: the drawable's size.
  pub fn size_in_pixels(&self) -> (u32, u32) {
    self.held().size_in_pixels()
  }

  /// Physical pixels per logical point on the window's display.
  pub fn display_scale(&self) -> f32 {
    self.held().display_scale()
  }

  /// The area no notch, rounded corner or system bar covers, in logical
  /// points.
  pub fn safe_area(&self) -> Rect {
    let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
    // SAFETY: a live window and a valid out-parameter; on failure the rect
    // stays empty.
    unsafe { sdl3::sys::video::SDL_GetWindowSafeArea(self.held().raw(), &mut rect) };
    normalize_safe_area(
      Rect { x: rect.x, y: rect.y, width: rect.w, height: rect.h },
      self.size(),
      self.size_in_pixels(),
      self.display_scale(),
    )
  }

  /// Orientation of the display the window is on; Unknown without one.
  pub fn orientation(&self) -> Orientation {
    self
      .held()
      .get_display()
      .map(|d| orientation_from_sdl(sdl3::video::Orientation::from_ll(d.get_orientation())))
      .unwrap_or(Orientation::Unknown)
  }

  /// Refresh rate of the window's display in Hz; None when SDL reports none or 0.
  pub fn refresh_rate(&self) -> Option<f32> {
    self.held().get_display().and_then(|d| d.get_mode()).map(|m| m.refresh_rate).ok().filter(|&hz| hz > 0.0)
  }

  pub fn set_title(&mut self, title: &str) -> Result<(), String> {
    self.held_mut().set_title(title).map_err(|e| e.to_string())
  }

  pub fn raise(&mut self) -> Result<(), String> {
    if self.held_mut().raise() {
      Ok(())
    } else {
      Err(crate::sdl_error())
    }
  }

  /// Straight-alpha RGBA8; remembered for a recreate. Err on platforms
  /// without window icons (macOS reports failure on every app switch).
  pub fn set_icon(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    self.icon = Some((width, height, rgba.to_vec()));
    set_window_icon(self.held(), width, height, rgba)
  }

  pub fn set_fullscreen(&mut self, fullscreen: bool) -> Result<(), String> {
    // SDL re-applies the stored size limits when a window leaves
    // fullscreen; a forced size's stale limits would pin it there.
    self.release_forced_size();
    self.held_mut().set_fullscreen(fullscreen).map_err(|e| e.to_string())
  }

  /// Ask for a window size in logical points, the unit `size` and the
  /// resize event report. SDL's window coordinates are logical on macOS
  /// and Wayland and physical pixels on X11 and Windows (SDL's
  /// docs/README-highdpi.md), so the request is converted by display scale
  /// over pixel density (`window_units`): 1 where the coordinates are
  /// already logical, the content scale on X11 and Windows. The X11 and
  /// Windows factor comes from SDL's document; it is not measured on a
  /// scaled X11 display.
  ///
  /// SDL_SetWindowSize records the request as the window's pending size;
  /// the backends apply it when the window leaves fullscreen or maximized.
  /// A backend without a SetWindowSize (Android) returns unsupported and
  /// sends no event, and a tiling compositor may ignore or defer the
  /// request: Hyprland reports every toplevel as maximized from its first
  /// map (to suppress client-side decorations), so SDL holds the request
  /// there for good. The resulting resize arrives as the normal
  /// `PlatformEvent::Resize`.
  ///
  /// With `SOLIDRT_RESIZE_VIA_LIMITS` on (see the module doc), the window's
  /// minimum and maximum size are first pinned to the requested size, which
  /// a compositor applies even to a window it holds maximized, and the
  /// answer is `SizeRequest::Forced`: the caller presents a frame, whose
  /// surface commit carries the limits to the compositor. The platform loop
  /// lifts them through `settle_forced_size` once the window has the size.
  /// A window already at the size gets the plain request. Without the
  /// variable the answer is `SizeRequest::Requested`.
  pub fn set_size(&mut self, width: u32, height: u32) -> Result<SizeRequest, String> {
    let units = window_units((width, height), self.display_scale(), self.held().pixel_density());
    let forced = resize_via_limits() && self.held().size() != units;
    if resize_via_limits() && !forced {
      // Nothing to force, and an earlier forced size must not stay pinned.
      self.release_forced_size();
    }
    if forced {
      // A forced size still pending pins the old limits; a larger request
      // would then set a minimum above the maximum, which SDL refuses.
      self.release_forced_size();
      let (w, h) = units;
      let window = self.held_mut();
      window.set_minimum_size(w, h).map_err(|e| e.to_string())?;
      window.set_maximum_size(w, h).map_err(|e| e.to_string())?;
      self.forced_size = Some(units);
    }
    let w = i32::try_from(units.0).unwrap_or(i32::MAX);
    let h = i32::try_from(units.1).unwrap_or(i32::MAX);
    // SAFETY: a live window; takes plain sizes (SDL rejects non-positive
    // ones with an error).
    let ok = unsafe { sdl3::sys::video::SDL_SetWindowSize(self.held().raw(), w, h) };
    if !ok {
      let error = crate::sdl_error();
      // A refused size must not leave the limits pinned to it.
      self.release_forced_size();
      return Err(error);
    }
    Ok(if forced { SizeRequest::Forced } else { SizeRequest::Requested })
  }

  /// After a resize event: lift the limits a forced size pinned once the
  /// window has that size (SDL_GetWindowSize, in window units). A resize to
  /// any other size leaves them, so it cannot lift them early.
  pub fn settle_forced_size(&mut self) {
    let Some(target) = self.forced_size else {
      return;
    };
    if self.held().size() == target {
      log::debug!("[ore] forced window size {}x{} reached; lifting the size limits", target.0, target.1);
      self.release_forced_size();
    }
  }

  /// Lift the limits a forced size pinned, reached or not (before a
  /// fullscreen change). Nothing happens without a forced size.
  pub fn release_forced_size(&mut self) {
    if self.forced_size.take().is_none() {
      return;
    }
    // 0x0 is SDL's "no limit" for both.
    let window = self.held_mut();
    if let Err(e) = window.set_minimum_size(0, 0) {
      log::warn!("[ore] lifting the minimum window size failed: {e}");
    }
    if let Err(e) = window.set_maximum_size(0, 0) {
      log::warn!("[ore] lifting the maximum window size failed: {e}");
    }
  }

  pub fn minimize(&mut self) -> Result<(), String> {
    if self.held_mut().minimize() {
      Ok(())
    } else {
      Err(crate::sdl_error())
    }
  }

  pub fn hide(&mut self) {
    self.held_mut().hide();
  }

  /// Relative mouse mode; returns the applied state, not the request: SDL
  /// can refuse (a platform without relative mode).
  pub fn set_pointer_lock(&mut self, locked: bool) -> bool {
    // SAFETY: a live window; takes a plain flag.
    unsafe { sdl3::sys::mouse::SDL_SetWindowRelativeMouseMode(self.held().raw(), locked) };
    let applied = self.pointer_locked();
    self.pointer_lock = applied;
    applied
  }

  /// Whether relative mouse mode is on for the window now.
  pub fn pointer_locked(&self) -> bool {
    // SAFETY: a live window.
    unsafe { sdl3::sys::mouse::SDL_GetWindowRelativeMouseMode(self.held().raw()) }
  }

  /// Start (or restart with new options) a text input session. Remembered
  /// for a recreate.
  pub fn start_text_input(&mut self, options: &TextInputOptions) {
    self.text_input = Some(*options);
    // SDL's default policy ("auto": show the screen keyboard unless
    // SDL_HasKeyboard) is blind on Android, which never registers
    // keyboards; feed it the platform fact so an attached hardware keyboard
    // suppresses the on-screen one.
    let hint = if physical_keyboard() { "false" } else { "auto" };
    sdl3::hint::set("SDL_ENABLE_SCREEN_KEYBOARD", hint);
    start_text_input_with_options(self.held(), options);
  }

  pub fn stop_text_input(&mut self) {
    self.text_input = None;
    self.video.text_input().stop(self.held());
  }

  /// Whether the screen keyboard shows for this window now.
  pub fn screen_keyboard_shown(&self) -> bool {
    self.video.text_input().is_screen_keyboard_shown(self.held())
  }

  /// Show `cursor`. A system shape is created on first use; an unregistered
  /// custom id shows the default shape (and warns).
  pub fn set_cursor(&mut self, cursor: Cursor) {
    self.cursors.apply(cursor);
  }

  /// Create the image cursor `id` from straight-alpha RGBA8 frames (one
  /// frame is a still cursor, more an animated one); a failure warns.
  pub fn register_cursor(&mut self, id: u64, frames: &[CursorFrame], hot_x: u32, hot_y: u32) {
    self.cursors.register(id, frames, hot_x, hot_y);
  }

  pub fn drop_cursor(&mut self, id: u64) {
    self.cursors.unregister(id);
  }

  /// The window's GL context, created and released for another thread to
  /// make current; Backing::Gl only, once. The context is owned by the
  /// Window (SDL creates and destroys contexts on the main thread); the
  /// surface is a Send handle onto it.
  pub fn create_gl_surface(&mut self) -> Result<GlSurface, String> {
    if self.backing != Backing::Gl {
      return Err("the window has no GL backing".to_string());
    }
    if self.surface_created {
      return Err("the window's GL surface was already created".to_string());
    }
    // The process's single GL context. Creating it (at open) made it
    // current on this (main) thread; release it right away so the drawing
    // thread - the only GL user - can bind it (a GL context can be current
    // on at most one thread).
    let surface =
      self.release_context().map_err(|e| format!("GL setup: Failed to release GL context on main thread: {e}"))?;
    self.surface_created = true;
    Ok(surface)
  }

  /// After a context loss, with the old context released by its thread:
  /// destroy it, recreate the window when asked (same title, size,
  /// position, fullscreen and maximized state), create a fresh context
  /// released for the caller's thread, and reapply what was set on the
  /// window (the icon, the text input session, the pointer lock).
  pub fn recreate_gl(&mut self, recreate_window: bool) -> Result<GlSurface, String> {
    if self.backing != Backing::Gl {
      return Err("the window has no GL context to recreate".to_string());
    }
    // The old context goes first (SDL_GL_DestroyContext; its thread
    // released it, so it is current nowhere), before the window it was
    // created on.
    self.gl_context = None;
    if recreate_window {
      let old = self.window.take().expect(WINDOW_HELD);
      let title = old.title().to_string();
      let (width, height) = old.size();
      let (x, y) = old.position();
      let fullscreen = old.fullscreen_state() != sdl3::video::FullscreenType::Off;
      let maximized = old.is_maximized();
      // SDL_DestroyWindow: the last GL window unloads the GL library, which
      // terminates the lost EGL display; the next GL window initializes it
      // afresh.
      clear_dialog_parent(&old);
      drop(old);
      let mut builder = self.video.window(&title, width, height);
      builder.opengl().position(x, y).high_pixel_density().resizable();
      let mut window = builder.build().map_err(|e| format!("window recreation: {e}"))?;
      if maximized && !window.maximize() {
        log::warn!("[ore] recreated window: maximize failed: {}", crate::sdl_error());
      }
      if fullscreen {
        if let Err(e) = window.set_fullscreen(true) {
          log::warn!("[ore] recreated window: set_fullscreen failed: {e}");
        }
      }
      self.window = Some(window);
    }
    self.gl_context = Some(crate::gl_surface::create_context(self.held())?);
    let surface = self
      .release_context()
      .map_err(|e| format!("Failed to release the recovered GL context on the main thread: {e}"))?;
    // The window the drawing thread draws into now may be a new one:
    // reapply what was set on the old.
    if let Some((width, height, rgba)) = &self.icon {
      if let Err(e) = set_window_icon(self.held(), *width, *height, rgba) {
        log::debug!("set_window_icon failed: {e}");
      }
    }
    if let Some(options) = &self.text_input {
      start_text_input_with_options(self.held(), options);
    }
    if self.pointer_lock {
      // SAFETY: a live window; takes a plain flag.
      unsafe { sdl3::sys::mouse::SDL_SetWindowRelativeMouseMode(self.held().raw(), true) };
    }
    DIALOG_PARENT.store(self.held().raw(), Ordering::Relaxed);
    Ok(surface)
  }

  // Release the held context from this thread and hand out a surface onto
  // it; the error is SDL's.
  fn release_context(&self) -> Result<GlSurface, String> {
    let window = self.held().raw();
    let context = self.gl_context.as_ref().expect("a GL window holds its context");
    // SAFETY: a live window; a null context unbinds this thread.
    if !unsafe { sdl3::sys::video::SDL_GL_MakeCurrent(window, std::ptr::null_mut()) } {
      return Err(crate::sdl_error());
    }
    // SAFETY: the raw handle is only read here; the Window keeps the
    // context alive for every use of the surface (see GlSurface).
    Ok(GlSurface::new(window, unsafe { context.raw() }))
  }
}

impl Drop for Window {
  // The fields drop after this, in their declared order (see `Window`).
  fn drop(&mut self) {
    if let Some(window) = &self.window {
      clear_dialog_parent(window);
    }
  }
}

/// SDL reports the safe area in SDL_GetWindowSize units (it builds the rect
/// from window->w/h minus the platform insets). Those units differ per
/// platform: desktop reports logical points there, but Android bakes the
/// display density into window->w/h (window->w == the physical surface
/// width), so the safe-area rect comes back in physical pixels. The layout
/// works in logical points, which are size_in_pixels / display_scale, so
/// normalize the rect into that same space by the ratio (size_in_pixels /
/// scale) / window_size. That ratio is 1.0 on desktop (including
/// fractional-scaled displays, where window_size is already logical) and
/// 1/scale on Android.
pub(crate) fn normalize_safe_area(rect: Rect, logical: (u32, u32), pixels: (u32, u32), scale: f32) -> Rect {
  let (lw, lh) = logical;
  let (pw, ph) = pixels;
  if lw == 0 || lh == 0 || scale <= 0.0 {
    return rect;
  }
  let fx = (pw as f32 / scale) / lw as f32;
  let fy = (ph as f32 / scale) / lh as f32;
  Rect {
    x: (rect.x as f32 * fx).round() as i32,
    y: (rect.y as f32 * fy).round() as i32,
    width: (rect.width as f32 * fx).round() as i32,
    height: (rect.height as f32 * fy).round() as i32,
  }
}

/// A size in logical points as SDL window units: logical * display_scale
/// / pixel_density, rounded to whole units. Display scale over pixel
/// density is 1 where SDL's window coordinates are already logical (macOS,
/// Wayland) and the content scale where they are physical (X11, Windows).
/// A non-positive scale or density means a factor of 1.
pub(crate) fn window_units(logical: (u32, u32), display_scale: f32, pixel_density: f32) -> (u32, u32) {
  let (w, h) = logical;
  if display_scale <= 0.0 || pixel_density <= 0.0 {
    return logical;
  }
  let factor = display_scale / pixel_density;
  ((w as f32 * factor).round() as u32, (h as f32 * factor).round() as u32)
}

// The display orientation SDL reports, as ore's vocabulary.
fn orientation_from_sdl(o: sdl3::video::Orientation) -> Orientation {
  use sdl3::video::Orientation as Sdl;
  match o {
    Sdl::Landscape => Orientation::Landscape,
    Sdl::LandscapeFlipped => Orientation::LandscapeFlipped,
    Sdl::Portrait => Orientation::Portrait,
    Sdl::PortraitFlipped => Orientation::PortraitFlipped,
    Sdl::Unknown => Orientation::Unknown,
  }
}

// Window icon from straight-alpha RGBA8 pixels. The sdl3 crate's set_icon
// wants its own surface type, so this goes through sdl3-sys directly. SDL
// copies the pixels into the surface's own representation on SetWindowIcon
// platforms and the surface only borrows `rgba`, so it is created, applied
// and destroyed within the call. Platforms without window icons (macOS)
// return Err.
fn set_window_icon(window: &sdl3::video::Window, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
  let surface = surface::rgba_surface(width, height, rgba)?;
  // SAFETY: a live window and a live surface over `rgba`, which outlives
  // the call; SDL copies the pixels.
  let ok = unsafe { sdl3::sys::video::SDL_SetWindowIcon(window.raw(), surface) };
  // SAFETY: the surface created above, destroyed once.
  unsafe { sdl3::sys::surface::SDL_DestroySurface(surface) };
  if ok {
    Ok(())
  } else {
    Err(crate::sdl_error())
  }
}

// Session start with IME configuration. The crate wraps only the plain
// SDL_StartTextInput, so the typed variant goes through the raw properties
// API; unset options keep SDL's defaults (notably capitalization defaults to
// Sentences for plain text).
fn start_text_input_with_options(window: &sdl3::video::Window, opts: &TextInputOptions) {
  use crate::text_input::{TextCapitalization, TextInputType};
  use sdl3::sys::keyboard::*;
  use sdl3::sys::properties::*;

  let input_type = opts.input_type.map(|t| match t {
    TextInputType::Text => SDL_TEXTINPUT_TYPE_TEXT,
    TextInputType::Name => SDL_TEXTINPUT_TYPE_TEXT_NAME,
    TextInputType::Email => SDL_TEXTINPUT_TYPE_TEXT_EMAIL,
    TextInputType::Username => SDL_TEXTINPUT_TYPE_TEXT_USERNAME,
    TextInputType::PasswordHidden => SDL_TEXTINPUT_TYPE_TEXT_PASSWORD_HIDDEN,
    TextInputType::PasswordVisible => SDL_TEXTINPUT_TYPE_TEXT_PASSWORD_VISIBLE,
    TextInputType::Number => SDL_TEXTINPUT_TYPE_NUMBER,
    TextInputType::NumberPasswordHidden => SDL_TEXTINPUT_TYPE_NUMBER_PASSWORD_HIDDEN,
    TextInputType::NumberPasswordVisible => SDL_TEXTINPUT_TYPE_NUMBER_PASSWORD_VISIBLE,
  });
  let capitalize = opts.capitalize.map(|c| match c {
    TextCapitalization::None => SDL_CAPITALIZE_NONE,
    TextCapitalization::Sentences => SDL_CAPITALIZE_SENTENCES,
    TextCapitalization::Words => SDL_CAPITALIZE_WORDS,
    TextCapitalization::Letters => SDL_CAPITALIZE_LETTERS,
  });

  // SAFETY: the property set is created, filled with plain values under
  // SDL's own property names, read by SDL_StartTextInputWithProperties on
  // a live window, and destroyed once.
  unsafe {
    let props = SDL_CreateProperties();
    if let Some(t) = input_type {
      SDL_SetNumberProperty(props, SDL_PROP_TEXTINPUT_TYPE_NUMBER, t.0 as i64);
    }
    if let Some(c) = capitalize {
      SDL_SetNumberProperty(props, SDL_PROP_TEXTINPUT_CAPITALIZATION_NUMBER, c.0 as i64);
    }
    if let Some(a) = opts.autocorrect {
      SDL_SetBooleanProperty(props, SDL_PROP_TEXTINPUT_AUTOCORRECT_BOOLEAN, a);
    }
    if let Some(m) = opts.multiline {
      SDL_SetBooleanProperty(props, SDL_PROP_TEXTINPUT_MULTILINE_BOOLEAN, m);
    }
    SDL_StartTextInputWithProperties(window.raw(), props);
    SDL_DestroyProperties(props);
  }
}

// Input device presence (connected, not necessarily in use); the sdl3 crate
// does not wrap SDL_HasKeyboard / SDL_HasMouse.
pub fn has_keyboard() -> bool {
  // SAFETY: takes no arguments.
  unsafe { sdl3::sys::keyboard::SDL_HasKeyboard() }
}

pub fn has_mouse() -> bool {
  // SAFETY: takes no arguments.
  unsafe { sdl3::sys::mouse::SDL_HasMouse() }
}

/// Whether SDL lists a touch device. On Android that list over-reports
/// (TV boxes with virtual input drivers claiming pointer sources); a
/// caller gates it on `crate::android::has_touchscreen_feature` there.
pub fn has_touch_device() -> bool {
  !sdl3::touch::num_touch_devices().is_empty()
}

// The crate wraps this on KeyboardUtil, but the input-devices snapshot is
// built without an sdl handle in scope; the raw call takes no arguments.
pub fn has_screen_keyboard_support() -> bool {
  // SAFETY: takes no arguments.
  unsafe { sdl3::sys::keyboard::SDL_HasScreenKeyboardSupport() }
}

/// SDL's device list OR the platform-reported fact (Android, where SDL
/// never registers keyboards; see set_hardware_keyboard).
pub fn physical_keyboard() -> bool {
  has_keyboard() || hardware_keyboard()
}

// Hardware keyboard presence reported by the platform. SDL's Android backend
// never registers keyboards (SDL_HasKeyboard is permanently false there), so
// the platform reports the Configuration fact via JNI, same route as the
// keyboard inset. Elsewhere this stays false and SDL's own device list is
// authoritative; readers OR the two (physical_keyboard).
static HARDWARE_KEYBOARD: AtomicBool = AtomicBool::new(false);

/// Set from the platform (the Android JNI native, forwarded by alloy until
/// step 3 of okf/done/ore-pump-and-window.md); thread-safe.
pub fn set_hardware_keyboard(present: bool) {
  HARDWARE_KEYBOARD.store(present, Ordering::Relaxed);
}

/// Whether the platform reported an attached hardware keyboard (false where
/// SDL's own device list covers it; see [`set_hardware_keyboard`]).
pub fn hardware_keyboard() -> bool {
  HARDWARE_KEYBOARD.load(Ordering::Relaxed)
}

/// The connected input device classes now (see `InputDevices`).
pub fn input_devices() -> InputDevices {
  // On Android the SDL touch list over-reports (TV boxes with virtual input
  // drivers claiming pointer sources), so the fact is gated on the
  // platform's own touchscreen feature declaration.
  #[cfg(target_os = "android")]
  let touch = has_touch_device() && crate::android::has_touchscreen_feature();
  #[cfg(not(target_os = "android"))]
  let touch = has_touch_device();
  InputDevices {
    keyboard: physical_keyboard(),
    mouse: has_mouse(),
    touch,
    screen_keyboard: has_screen_keyboard_support(),
  }
}

// Soft-keyboard (IME) inset height in raw pixels. On Android the platform
// reports it via JNI (the cdylib re-exports a symbol that calls the setter);
// elsewhere it stays 0. The pump reads it on every wait and emits a
// KeyboardVisibility change when it moves.
static KEYBOARD_INSET_PX: AtomicI32 = AtomicI32::new(0);

/// Set the current soft-keyboard inset in raw pixels. Called from the platform
/// (Android JNI); thread-safe.
pub fn set_keyboard_inset_px(px: i32) {
  KEYBOARD_INSET_PX.store(px.max(0), Ordering::Relaxed);
}

/// Current soft-keyboard inset in raw pixels (0 when hidden or unsupported).
pub fn keyboard_inset_px() -> i32 {
  KEYBOARD_INSET_PX.load(Ordering::Relaxed)
}
