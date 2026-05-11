use alloy::impellers::TypographyContext;

pub struct PlatformContext {
    pub typography: TypographyContext,
}

impl PlatformContext {
    pub fn new() -> Self {
        Self {
            typography: TypographyContext::default(),
        }
    }
}
