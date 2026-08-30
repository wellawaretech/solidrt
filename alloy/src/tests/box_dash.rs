// The box primitives' dashed strokes: the inset outline as walker pieces
// (edges and quarter arcs), what it measures, where the pattern starts,
// and that a dash never leaves the box.

use crate::impellers::{DrawStyle, Point, Rect, Size};
use crate::rendertree::kinds::dash::{box_outline, oval_outline, walk_dashes, walked_length, Dash, Pen};
use crate::rendertree::{Bounded, Oval, Rectangle};
use std::f32::consts::PI;

// The kappa cubic plus the flattening tolerance keep an arc's measured
// length within this fraction of the true one.
const ARC_LENGTH_TOLERANCE: f32 = 0.002;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn assert_close(actual: f32, expected: f32) {
  assert!((actual - expected).abs() <= expected * ARC_LENGTH_TOLERANCE, "{actual} vs {expected}");
}

// Records every point the walker hands the pen.
#[derive(Default)]
struct Points(Vec<Point>);

impl Pen for Points {
  fn move_to(&mut self, p: Point) {
    self.0.push(p);
  }
  fn line_to(&mut self, p: Point) {
    self.0.push(p);
  }
  fn quadratic_to(&mut self, ctrl: Point, p: Point) {
    self.0.extend([ctrl, p]);
  }
  fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
    self.0.extend([c1, c2, p]);
  }
}

#[test]
fn a_sharp_box_outline_is_four_edges() {
  let outline = box_outline(rect(10.0, 20.0, 100.0, 50.0), [0.0; 4]);
  assert_eq!(outline.len(), 4);
  assert!((walked_length(outline.into_iter()) - 300.0).abs() < 1e-3);
}

#[test]
fn rounded_corners_measure_as_quarter_circles() {
  let outline = box_outline(rect(0.0, 0.0, 100.0, 50.0), [10.0; 4]);
  assert_eq!(outline.len(), 8);
  assert_close(walked_length(outline.into_iter()), 2.0 * 80.0 + 2.0 * 30.0 + 2.0 * PI * 10.0);
}

#[test]
fn radii_clamp_to_the_half_box() {
  // Radii past the half box collapse the edges: a 50-wide box with r=40
  // is a stadium of two semicircles and two 50-long edges.
  let outline = box_outline(rect(0.0, 0.0, 50.0, 100.0), [40.0; 4]);
  assert_eq!(outline.len(), 6);
  assert_close(walked_length(outline.into_iter()), 2.0 * 50.0 + 2.0 * PI * 25.0);
}

#[test]
fn an_oval_outline_is_its_circumference() {
  let outline = oval_outline(rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(outline.len(), 4);
  assert_close(walked_length(outline.into_iter()), PI * 100.0);
  assert!(oval_outline(rect(0.0, 0.0, 0.0, 40.0)).is_empty());
}

#[test]
fn outlines_start_where_svg_does() {
  let first = |pieces| {
    let mut pen = Points::default();
    walk_dashes(pieces, Dash { on: 1.0, off: 1000.0, offset: 0.0 }, &mut pen);
    pen.0[0]
  };
  assert_eq!(first(box_outline(rect(0.0, 0.0, 100.0, 50.0), [10.0; 4]).into_iter()), Point::new(10.0, 0.0));
  assert_eq!(first(oval_outline(rect(0.0, 0.0, 100.0, 50.0)).into_iter()), Point::new(100.0, 25.0));
}

#[test]
fn dashes_stay_inside_the_box_and_bounds_stay_the_box() {
  let size = Size::new(100.0, 60.0);
  let mut r = Rectangle::default();
  r.paint.draw_style = DrawStyle::Stroke;
  r.paint.stroke_width = 8.0;
  r.set_radius(Some([12.0; 4]));
  r.set_on_length(Some(7.0));
  r.set_off_length(Some(5.0));
  let (outline, dash) = r.dashed_outline(size).expect("stroked and dashed");
  let mut pen = Points::default();
  walk_dashes(outline.into_iter(), dash, &mut pen);
  assert!(!pen.0.is_empty());
  // The outline is the box inset by half the stroke width; a dash's cap
  // reaches along it by that same half width, so the box contains it all.
  let inset = 4.0;
  for p in &pen.0 {
    assert!(p.x >= inset - 1e-3 && p.x <= 100.0 - inset + 1e-3, "{p:?}");
    assert!(p.y >= inset - 1e-3 && p.y <= 60.0 - inset + 1e-3, "{p:?}");
  }
  assert_eq!(r.local_bounds(size), rect(0.0, 0.0, 100.0, 60.0));
}

#[test]
fn only_a_stroked_and_dashed_paint_walks() {
  let size = Size::new(100.0, 60.0);
  let mut o = Oval::default();
  o.set_on_length(Some(4.0));
  o.set_off_length(Some(4.0));
  assert!(o.dashed_outline(size).is_none(), "a fill has no stroke to dash");
  o.paint.draw_style = DrawStyle::StrokeAndFill;
  assert!(o.dashed_outline(size).is_some());
  o.set_off_length(Some(0.0));
  assert!(o.dashed_outline(size).is_none(), "a zero gap is solid");
}

#[test]
fn path_length_scales_the_pattern_to_the_outline() {
  let size = Size::new(100.0, 50.0);
  let mut r = Rectangle::default();
  r.paint.draw_style = DrawStyle::Stroke;
  r.paint.stroke_width = 2.0;
  r.set_on_length(Some(0.5));
  r.set_off_length(Some(1.0));
  r.set_path_length(Some(1.0));
  // Inset by 1 on every side: a 98 x 48 outline, 292 around.
  let (_, dash) = r.dashed_outline(size).expect("dashed");
  assert!((dash.on - 146.0).abs() < 1e-3, "{dash:?}");
  assert!((dash.off - 292.0).abs() < 1e-3, "{dash:?}");
}
