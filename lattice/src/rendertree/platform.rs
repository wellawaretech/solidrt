use alloy::impellers::{Point, Rect, Size, TypographyContext};
use std::borrow::Cow;
use std::cell::Cell;

const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans.ttf");

pub struct PlatformContext {
  pub typography: TypographyContext,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
  display_scale: Cell<f32>,
  safe_area: Cell<Rect>,
  fps: Cell<u32>,
}

// Safety: PlatformContext is only used on the UI thread.
unsafe impl Send for PlatformContext {}
unsafe impl Sync for PlatformContext {}

impl PlatformContext {
  pub fn new() -> Self {
    let mut typography = TypographyContext::default();
    typography
      .register_font(Cow::Borrowed(NOTO_SANS), Some("Noto Sans"))
      .expect("Failed to register Noto Sans font");
    Self {
      typography,
      window_size: Cell::new((0.0, 0.0)),
      window_size_dirty: Cell::new(false),
      display_scale: Cell::new(1.0),
      safe_area: Cell::new(Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0))),
      fps: Cell::new(0),
    }
  }

  pub fn window_size(&self) -> (f32, f32) {
    self.window_size.get()
  }

  pub fn set_window_size(&self, width: f32, height: f32) {
    self.window_size.set((width, height));
    self.window_size_dirty.set(true);
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