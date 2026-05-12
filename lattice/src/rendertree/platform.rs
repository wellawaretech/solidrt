use alloy::impellers::TypographyContext;

pub struct PlatformContext {
  pub typography: TypographyContext,
}

// Safety: PlatformContext is only used on the UI thread.
unsafe impl Send for PlatformContext {}
unsafe impl Sync for PlatformContext {}

impl PlatformContext {
  pub fn new() -> Self {
    Self {
      typography: TypographyContext::default(),
    }
  }
}
