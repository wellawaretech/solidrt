use crate::impellers::{Color, Point, Rect, Size};
use crate::rendertree::{Gradient, GradientStop, GradientUnits, PaintState};

fn stop(offset: f32, r: f32, g: f32, b: f32) -> GradientStop {
  GradientStop { offset, color: Color::new_srgba(r, g, b, 1.0) }
}

#[test]
fn set_gradient_derives_average_fallback() {
  let mut p = PaintState::default();
  p.set_gradient(Gradient::linear_box(
    Point::new(0.0, 0.0),
    Point::new(1.0, 1.0),
    vec![stop(0.0, 0.0, 0.0, 0.0), stop(1.0, 1.0, 1.0, 1.0)],
  ));
  // Black + white averages to mid-gray.
  assert!((p.color.red - 0.5).abs() < 0.01);
  assert!(matches!(p.gradient, Some(Gradient::Linear { units: GradientUnits::BoundingBox, .. })));
}

#[test]
fn set_color_clears_gradient() {
  let mut p = PaintState::default();
  p.set_gradient(Gradient::radial_box(Point::new(0.5, 0.5), 0.5, true, vec![stop(0.0, 1.0, 0.0, 0.0)]));
  assert!(p.gradient.is_some());
  p.set_color(Some(Color::new_srgba(0.0, 0.0, 1.0, 1.0)));
  assert!(p.gradient.is_none());
}

#[test]
fn box_relative_resolution_does_not_panic() {
  let mut p = PaintState::default();
  p.set_gradient(Gradient::linear_box(
    Point::new(0.0, 0.0),
    Point::new(1.0, 1.0),
    vec![stop(0.0, 1.0, 0.0, 0.0), stop(1.0, 0.0, 0.0, 1.0)],
  ));
  // Resolving against a box builds a color source; with no bounds the
  // box-relative gradient is skipped and the fallback color is used.
  let _ = p.to_paint_in(&Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0)));
  let _ = p.to_paint();
}

#[test]
fn circle_radial_resolves_with_bounds() {
  let mut p = PaintState::default();
  p.set_gradient(Gradient::radial_box(
    Point::new(0.5, 0.5),
    0.5,
    true,
    vec![stop(0.0, 1.0, 1.0, 1.0), stop(1.0, 0.0, 0.0, 0.0)],
  ));
  let _ = p.to_paint_in(&Rect::new(Point::new(10.0, 20.0), Size::new(80.0, 40.0)));
}
