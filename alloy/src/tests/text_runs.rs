// A paragraph's runs (rendertree/text/runs.rs): which differences between
// two runs the shaping cache key sees (`TextRun::shapes_like`).
use crate::renderer::Color;
use crate::rendertree::text::{RunOverrides, TextRun};
use crate::rendertree::PaintState;

// A size other than the paragraph's default, for the shaping difference.
const LARGER_SIZE: f32 = 32.0;
// An underline offset, px: any value, the key does not read it.
const UNDERLINE_OFFSET: f32 = 2.0;

fn run() -> TextRun {
  TextRun { text: "Hello".to_string(), ..TextRun::default() }
}

#[test]
fn a_paint_only_difference_shapes_alike() {
  let plain = run();
  let mut painted = run();
  let mut paint = PaintState::default();
  paint.set_color(Some(Color::new_srgba(1.0, 0.0, 0.0, 1.0)));
  painted.overrides = RunOverrides {
    paint: Some(paint),
    underline: Some(true),
    underline_offset: Some(UNDERLINE_OFFSET),
    ..RunOverrides::default()
  };
  assert_ne!(plain, painted, "the runs differ");
  assert!(plain.shapes_like(&painted), "a paint and underline difference reshapes");
  assert!(plain.overrides.shapes_like(&painted.overrides));
}

#[test]
fn a_font_size_difference_does_not_shape_alike() {
  let plain = run();
  let mut larger = run();
  larger.overrides.font_size = Some(LARGER_SIZE);
  assert!(!plain.shapes_like(&larger));
  assert!(!plain.overrides.shapes_like(&larger.overrides));
  // Nor does a text difference.
  let other = TextRun { text: "World".to_string(), ..TextRun::default() };
  assert!(!plain.shapes_like(&other));
}
