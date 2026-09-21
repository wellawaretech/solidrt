use crate::impellers::{Color, DrawStyle, Point, Rect, Size};
use crate::rendertree::{Gradient, GradientStop, GradientUnits, PaintState, DEFAULT_STROKE_WIDTH};

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

// A stroke needs a positive width: at 0 the stroking styles paint nothing
// (a stroke-only shape draws nothing at all, never a hairline) and every
// stroke metric collapses with it; an unset width is SVG's 1.
#[test]
fn zero_stroke_width_strokes_nothing() {
  let mut p = PaintState::default();
  assert_eq!(p.stroke_width, DEFAULT_STROKE_WIDTH);
  assert_eq!(p.painted_style(), Some(DrawStyle::Fill));

  p.draw_style = DrawStyle::Stroke;
  assert!(p.strokes());
  assert_eq!(p.painted_style(), Some(DrawStyle::Stroke));
  p.stroke_width = 0.0;
  assert!(!p.strokes());
  assert_eq!(p.painted_style(), None);
  assert_eq!(p.stroke_outset(true, true), 0.0);
  assert_eq!(p.stroke_inset(100.0, 50.0), 0.0);

  p.draw_style = DrawStyle::StrokeAndFill;
  assert!(p.fills());
  assert_eq!(p.painted_style(), Some(DrawStyle::Fill));
  p.stroke_width = 4.0;
  assert_eq!(p.painted_style(), Some(DrawStyle::StrokeAndFill));
  assert_eq!(p.stroke_inset(100.0, 50.0), 2.0);
  // A null write resets to the default width, not to nothing.
  p.set_stroke_width(None);
  assert_eq!(p.stroke_width, DEFAULT_STROKE_WIDTH);
}
