// The polyline form of Line: segments from a flat points array, the closing
// segment, precedence over the endpoints. Hit testing is the observable that
// needs no platform; build and measure walk the same segments/extent.

use crate::impellers::{DrawStyle, Point, Rect, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Line;

fn ctx() -> HitContext {
  let size = Size::new(100.0, 100.0);
  HitContext { size, content: Rect::new(Point::zero(), size) }
}

fn polyline(points: &[f32], closed: bool) -> Line {
  let mut line = Line::default();
  line.set_points(Some(points.to_vec()));
  line.set_closed(closed);
  line.paint.stroke_width = 4.0;
  line
}

fn hits(line: &Line, x: f32, y: f32) -> bool {
  line.is_in_bounds(Point::new(x, y), &ctx())
}

#[test]
fn polyline_hits_along_every_segment() {
  let line = polyline(&[0.0, 0.0, 50.0, 0.0, 50.0, 50.0], false);
  assert!(hits(&line, 25.0, 1.0));
  assert!(hits(&line, 51.0, 25.0));
  assert!(hits(&line, 50.0, 0.0));
  assert!(!hits(&line, 10.0, 10.0));
  assert!(!hits(&line, 25.0, 25.0));
}

#[test]
fn closed_adds_the_segment_back_to_the_first_point() {
  let open = polyline(&[0.0, 0.0, 50.0, 0.0, 50.0, 50.0], false);
  let closed = polyline(&[0.0, 0.0, 50.0, 0.0, 50.0, 50.0], true);
  assert!(!hits(&open, 25.0, 25.0));
  assert!(hits(&closed, 25.0, 25.0));
}

#[test]
fn fewer_than_two_points_never_hit() {
  assert!(!hits(&polyline(&[], false), 0.0, 0.0));
  assert!(!hits(&polyline(&[10.0, 10.0], true), 10.0, 10.0));
}

#[test]
fn points_take_precedence_over_endpoints() {
  let mut line = polyline(&[0.0, 0.0, 100.0, 0.0], false);
  line.set_x1(Some(0.0));
  line.set_y1(Some(0.0));
  line.set_x2(Some(100.0));
  line.set_y2(Some(100.0));
  assert!(hits(&line, 50.0, 0.0));
  assert!(!hits(&line, 50.0, 50.0));
  line.set_points(None);
  assert!(hits(&line, 50.0, 50.0));
  assert!(!hits(&line, 50.0, 0.0));
}

#[test]
fn line_paint_defaults_to_stroke() {
  assert_eq!(Line::default().paint.draw_style, DrawStyle::Stroke);
  assert_eq!(Line::DEFAULT_DRAW_STYLE, DrawStyle::Stroke);
}

#[test]
fn fill_styles_hit_the_interior_and_stroke_styles_the_outline() {
  let mut line = polyline(&[20.0, 100.0, 70.0, 20.0, 120.0, 100.0], false);
  let inside = (70.0, 70.0);
  let on_base = (70.0, 100.0);
  let outside = (10.0, 10.0);

  assert!(!hits(&line, inside.0, inside.1));
  assert!(hits(&line, on_base.0, on_base.1) == false, "an open outline has no base segment");

  line.paint.draw_style = DrawStyle::Fill;
  assert!(hits(&line, inside.0, inside.1));
  assert!(!hits(&line, outside.0, outside.1));
  assert!(!hits(&line, 70.0, 104.0), "fill has no stroke tolerance past the edge");

  line.paint.draw_style = DrawStyle::StrokeAndFill;
  assert!(hits(&line, inside.0, inside.1));
  assert!(hits(&line, 21.0, 100.0));
  assert!(!hits(&line, outside.0, outside.1));
}
