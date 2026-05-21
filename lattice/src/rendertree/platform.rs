use alloy::impellers::TypographyContext;
use std::borrow::Cow;
use std::cell::Cell;

const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans.ttf");

pub struct PlatformContext {
  pub typography: TypographyContext,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
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
}