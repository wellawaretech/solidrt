use alloy::impellers::{Rect, TypographyContext};
use std::cell::Cell;

pub struct PlatformContext {
  pub typography: TypographyContext,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
  safe_area: Cell<Rect>,
  display_scale: Cell<f32>,
}

// Safety: PlatformContext is only used on the UI thread.
unsafe impl Send for PlatformContext {}
unsafe impl Sync for PlatformContext {}

impl PlatformContext {
  pub fn new() -> Self {
    Self {
      typography: TypographyContext::default(),
      window_size: Cell::new((0.0, 0.0)),
      window_size_dirty: Cell::new(false),
      safe_area: Cell::new(Rect::zero()),
      display_scale: Cell::new(1.0),
    }
  }

  pub fn window_size(&self) -> (f32, f32) {
    self.window_size.get()
  }

  pub fn safe_area(&self) -> Rect {
    self.safe_area.get()
  }

  pub fn display_scale(&self) -> f32 {
    self.display_scale.get()
  }

  pub fn set_window_size(&self, width: f32, height: f32) {
    self.window_size.set((width, height));
    self.window_size_dirty.set(true);
  }

  pub fn set_resize(&self, width: f32, height: f32, safe_area: Rect, display_scale: f32) {
    self.window_size.set((width, height));
    self.window_size_dirty.set(true);
    self.safe_area.set(safe_area);
    self.display_scale.set(display_scale);
  }

  pub fn take_window_size_dirty(&self) -> bool {
    self.window_size_dirty.replace(false)
  }
}