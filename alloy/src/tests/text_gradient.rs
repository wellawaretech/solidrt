// The text layer's gradient mapping (rendertree/text/gradient.rs): a run's
// gradient paint resolved to the layer's pixels for the glyph pass.
use crate::gpu::{GradientKind, GradientTile, RAMP_WIDTH};
use crate::impellers::{Color, Point, Size};
use crate::rendertree::text::gradient::layer_gradient;
use crate::rendertree::{Gradient, GradientStop};

// The text box the box gradients resolve against, logical px.
const BOX: Size = Size::new(100.0, 20.0);
// The layer: two device pixels per logical one, its origin five logical
// pixels up and left of the text origin (the ink slack).
const SCALE: f32 = 2.0;
const SLACK: f32 = 5.0;
const EPSILON: f32 = 1e-4;

fn stops() -> Vec<GradientStop> {
  vec![
    GradientStop { offset: 0.0, color: Color::new_srgba(1.0, 0.0, 0.0, 1.0) },
    GradientStop { offset: 1.0, color: Color::new_srgba(0.0, 0.0, 1.0, 0.5) },
  ]
}

// The parameter at a text-local point, through the layer mapping.
fn param_at(rows: ([f32; 3], [f32; 3]), kind: GradientKind, local: Point) -> f32 {
  let layer = ((local.x + SLACK) * SCALE, (local.y + SLACK) * SCALE);
  let dot = |r: [f32; 3]| r[0] * layer.0 + r[1] * layer.1 + r[2];
  match kind {
    GradientKind::Linear => dot(rows.0),
    GradientKind::Radial => (dot(rows.0).powi(2) + dot(rows.1).powi(2)).sqrt(),
  }
}

#[test]
fn a_box_linear_gradient_runs_across_the_text_box() {
  let gradient = Gradient::linear_box(Point::new(0.0, 0.0), Point::new(1.0, 0.0), stops());
  let g = layer_gradient(&gradient, BOX, Point::new(-SLACK, -SLACK), SCALE).expect("resolved");
  assert_eq!(g.kind, GradientKind::Linear);
  assert_eq!(g.tile, GradientTile::Clamp);
  let rows = (g.x_row, g.y_row);
  assert!((param_at(rows, g.kind, Point::new(0.0, 10.0)) - 0.0).abs() < EPSILON, "the box's left edge is t 0");
  assert!((param_at(rows, g.kind, Point::new(100.0, 3.0)) - 1.0).abs() < EPSILON, "its right edge is t 1");
  assert!((param_at(rows, g.kind, Point::new(25.0, 0.0)) - 0.25).abs() < EPSILON);
  // The ramp runs red to half-transparent blue.
  assert_eq!(g.ramp.len(), RAMP_WIDTH);
  assert_eq!(g.ramp[0], [255, 0, 0, 255]);
  assert_eq!(g.ramp[RAMP_WIDTH - 1], [0, 0, 255, 128]);
  let middle = g.ramp[RAMP_WIDTH / 2];
  assert!(middle[0] > 100 && middle[2] > 100, "halfway mixes the two: {middle:?}");
}

#[test]
fn a_box_radial_circle_measures_from_the_center_in_pixels() {
  let gradient = Gradient::radial_box(Point::new(0.5, 0.5), 0.5, true, stops());
  let g = layer_gradient(&gradient, BOX, Point::new(-SLACK, -SLACK), SCALE).expect("resolved");
  assert_eq!(g.kind, GradientKind::Radial);
  let rows = (g.x_row, g.y_row);
  // The radius is half the shorter side: 10 px around (50, 10).
  assert!(param_at(rows, g.kind, Point::new(50.0, 10.0)).abs() < EPSILON, "the center is t 0");
  assert!((param_at(rows, g.kind, Point::new(60.0, 10.0)) - 1.0).abs() < EPSILON, "ten px right is t 1");
  assert!((param_at(rows, g.kind, Point::new(50.0, 0.0)) - 1.0).abs() < EPSILON, "ten px up is t 1");
}

#[test]
fn a_gradient_without_stops_falls_back_to_solid() {
  let gradient = Gradient::linear_box(Point::new(0.0, 0.0), Point::new(1.0, 0.0), Vec::new());
  assert!(layer_gradient(&gradient, BOX, Point::new(0.0, 0.0), 1.0).is_none());
  let flat = Gradient::linear_box(Point::new(0.5, 0.5), Point::new(0.5, 0.5), stops());
  assert!(layer_gradient(&flat, BOX, Point::new(0.0, 0.0), 1.0).is_none(), "a zero-length axis has no parameter");
}
