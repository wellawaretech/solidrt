use crate::rendertree::{FontPayload, PlatformContext};
use std::borrow::Cow;

// A payload that is not a font; registration must fail without panicking.
fn garbage() -> FontPayload {
  FontPayload { alias: Some("bogus".to_string()), bytes: Cow::Borrowed(b"not a font" as &[u8]) }
}

#[test]
fn reset_fonts_skips_unparseable_fonts() {
  let platform = PlatformContext::new(Vec::new());
  // A bad font mid-session is skipped with a warning, never a panic (a
  // hostile or corrupt manifest font must not kill the client), and the
  // replaced context stays usable for shaping.
  platform.reset_fonts(vec![garbage()]);
  assert!(crate::impellers::ParagraphBuilder::new(&platform.typography()).is_some());
}

#[test]
fn reset_fonts_replaces_previous_set() {
  let noto = FontPayload {
    alias: Some("sans".to_string()),
    bytes: Cow::Borrowed(include_bytes!("../../assets/fonts/NotoSans.ttf") as &[u8]),
  };
  let platform = PlatformContext::new(vec![noto.clone()]);
  // Each reset builds a fresh context from exactly the given set; registering
  // the same alias again through a reset must not error (nothing accumulates
  // across resets).
  platform.reset_fonts(vec![noto.clone()]);
  platform.reset_fonts(vec![noto, garbage()]);
  assert!(crate::impellers::ParagraphBuilder::new(&platform.typography()).is_some());
}
