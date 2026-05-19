use alloy::impellers::TypographyContext;
use std::cell::Cell;

pub struct PlatformContext {
  pub typography: TypographyContext,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
  pointer_pos: Cell<(f32, f32)>,
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
      pointer_pos: Cell::new((0.0, 0.0)),
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

  pub fn pointer_pos(&self) -> (f32, f32) {
    self.pointer_pos.get()
  }

  pub fn set_pointer_pos(&self, x: f32, y: f32) {
    self.pointer_pos.set((x, y));
  }
}