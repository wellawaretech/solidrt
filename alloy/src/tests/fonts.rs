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
  assert!(platform.glyphs().is_empty(), "the bad font registered nothing");
  assert!(platform.words().is_empty(), "the word cache was cleared");
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
  assert_eq!(platform.glyphs().resolve("sans"), Some(0), "the good font of the last set is face 0");
  assert_eq!(platform.glyphs().resolve("bogus"), Some(0), "the bad one resolves to the fallback");
}
