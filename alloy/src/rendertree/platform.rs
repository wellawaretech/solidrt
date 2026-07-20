use crate::impellers::{Point, Rect, Size, TypographyContext};
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A font to register at startup: raw TTF/OTF bytes plus an optional alias the
/// font registers under instead of its intrinsic family name ("sans", "serif"
/// and "mono" by convention). Alloy ships no font data itself; callers supply
/// fonts (embedded, unpacked from a trailer, read from disk). With none
/// registered, text falls back to the platform font manager.
pub struct FontPayload {
  pub alias: Option<String>,
  pub bytes: Cow<'static, [u8]>,
}

pub struct PlatformContext {
  pub typography: TypographyContext,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
  display_scale: Cell<f32>,
  safe_area: Cell<Rect>,
  fps: Cell<u32>,
  // Frame-request latch (Flutter-style scheduleFrame). Atomic and Arc'd because
  // change sources latch from the UI thread (ffi mutations), the alloy event
  // thread (pointer input, resize), and the dev-server connection thread (see
  // go/connection.rs).
  frame_requested: Arc<AtomicBool>,
  // Bypass the demand-driven gate and render every frame (playback mode).
  always_render: Cell<bool>,
  // Whether the debug stats overlay (HUD) is drawn. Arc'd so the dev-server
  // connection (a different thread, see go/connection.rs) can toggle it.
  stats_enabled: Arc<AtomicBool>,
}

// Safety: PlatformContext is only used on the UI thread.
unsafe impl Send for PlatformContext {}
unsafe impl Sync for PlatformContext {}

impl PlatformContext {
  pub fn new(fonts: Vec<FontPayload>) -> Self {
    let mut typography = TypographyContext::default();
    for font in fonts {
      let FontPayload { alias, bytes } = font;
      typography
        .register_font(bytes, alias.as_deref())
        .unwrap_or_else(|e| panic!("Failed to register font '{}': {e}", alias.as_deref().unwrap_or("<unaliased>")));
    }
    Self {
      typography,
      window_size: Cell::new((0.0, 0.0)),
      window_size_dirty: Cell::new(false),
      display_scale: Cell::new(1.0),
      safe_area: Cell::new(Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0))),
      fps: Cell::new(0),
      frame_requested: Arc::new(AtomicBool::new(false)),
      always_render: Cell::new(false),
      stats_enabled: Arc::new(AtomicBool::new(false)),
    }
  }

  pub fn set_always_render(&self, always: bool) {
    self.always_render.set(always);
  }

  pub fn always_render(&self) -> bool {
    self.always_render.get()
  }

  /// Toggle the debug stats overlay. Requests a frame so the change is drawn
  /// even when the app is otherwise idle.
  pub fn set_stats_enabled(&self, enabled: bool) {
    self.stats_enabled.store(enabled, Ordering::Relaxed);
    self.request_frame();
  }

  pub fn stats_enabled(&self) -> bool {
    self.stats_enabled.load(Ordering::Relaxed)
  }

  /// Shared handles for toggling the stats overlay from another thread (the
  /// dev-server connection): set `stats_enabled` and latch `frame_requested`
  /// so the change is drawn even when the app is otherwise idle.
  pub fn stats_handles(&self) -> (Arc<AtomicBool>, Arc<AtomicBool>) {
    (self.stats_enabled.clone(), self.frame_requested.clone())
  }

  /// Latch a frame request (Flutter's scheduleFrame). Idempotent; callable
  /// from any thread. The draw gate consumes it via take_frame_requested:
  /// no request, no frame.
  pub fn request_frame(&self) {
    self.frame_requested.store(true, Ordering::Relaxed);
  }

  /// Consume the latch. Called once per render tick (from draw).
  pub fn take_frame_requested(&self) -> bool {
    self.frame_requested.swap(false, Ordering::Relaxed)
  }

  pub fn window_size(&self) -> (f32, f32) {
    self.window_size.get()
  }

  pub fn set_window_size(&self, width: f32, height: f32) {
    self.window_size.set((width, height));
    self.window_size_dirty.set(true);
    self.request_frame();
  }

  pub fn take_window_size_dirty(&self) -> bool {
    self.window_size_dirty.replace(false)
  }

  pub fn display_scale(&self) -> f32 {
    self.display_scale.get()
  }

  pub fn set_display_scale(&self, scale: f32) {
    self.display_scale.set(scale);
  }

  pub fn safe_area(&self) -> Rect {
    self.safe_area.get()
  }

  pub fn set_safe_area(&self, safe_area: Rect) {
    self.safe_area.set(safe_area);
  }

  pub fn fps(&self) -> u32 {
    self.fps.get()
  }

  pub fn set_fps(&self, fps: u32) {
    self.fps.set(fps);
  }
}
