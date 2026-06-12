use alloy::impellers::{Point, Rect, Size, TypographyContext};
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans.ttf");
const NOTO_SANS_MONO: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono.ttf");

pub struct PlatformContext {
  pub typography: TypographyContext,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
  display_scale: Cell<f32>,
  safe_area: Cell<Rect>,
  fps: Cell<u32>,
  // Frame-request latch (Flutter-style scheduleFrame). Atomic because change
  // sources latch from both the UI thread (ffi mutations) and the alloy event
  // thread (pointer input, resize). The per-second stats cells below are only
  // touched by take_frame_requested on the UI thread.
  frame_requested: AtomicBool,
  req_window_start: Cell<Instant>,
  req_window_count: Cell<u32>,
  requests_per_second: Cell<u32>,
  // Bypass the demand-driven gate and render every frame (record mode).
  always_render: Cell<bool>,
}

// Safety: PlatformContext is only used on the UI thread.
unsafe impl Send for PlatformContext {}
unsafe impl Sync for PlatformContext {}

impl PlatformContext {
  pub fn new() -> Self {
    let mut typography = TypographyContext::default();
    typography.register_font(Cow::Borrowed(NOTO_SANS), Some("Noto Sans")).expect("Failed to register Noto Sans font");
    typography
      .register_font(Cow::Borrowed(NOTO_SANS_MONO), Some("Noto Sans Mono"))
      .expect("Failed to register Noto Sans Mono font");
    Self {
      typography,
      window_size: Cell::new((0.0, 0.0)),
      window_size_dirty: Cell::new(false),
      display_scale: Cell::new(1.0),
      safe_area: Cell::new(Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0))),
      fps: Cell::new(0),
      frame_requested: AtomicBool::new(false),
      req_window_start: Cell::new(Instant::now()),
      req_window_count: Cell::new(0),
      requests_per_second: Cell::new(0),
      always_render: Cell::new(false),
    }
  }

  pub fn set_always_render(&self, always: bool) {
    self.always_render.set(always);
  }

  pub fn always_render(&self) -> bool {
    self.always_render.get()
  }

  /// Latch a frame request (Flutter's scheduleFrame). Idempotent; callable
  /// from any thread. The draw gate consumes it via take_frame_requested:
  /// no request, no frame.
  pub fn request_frame(&self) {
    self.frame_requested.store(true, Ordering::Relaxed);
  }

  /// Consume the latch. Called once per render tick (from draw); also rolls
  /// the requested-frames-per-second window used by the debug overlay.
  pub fn take_frame_requested(&self) -> bool {
    let requested = self.frame_requested.swap(false, Ordering::Relaxed);
    if requested {
      self.req_window_count.set(self.req_window_count.get() + 1);
    }
    if self.req_window_start.get().elapsed().as_secs_f32() >= 1.0 {
      self.requests_per_second.set(self.req_window_count.get());
      self.req_window_count.set(0);
      self.req_window_start.set(Instant::now());
    }
    requested
  }

  /// Requested frames in the last completed one-second window.
  pub fn requests_per_second(&self) -> u32 {
    self.requests_per_second.get()
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
