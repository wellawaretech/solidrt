// Path's painted box: the geometry's tight extent plus the stroke's reach,
// with caps and joins read off the parsed `d`. And its dashed stroke: the
// shared walker over the flattened subpaths.

use crate::impellers::{DrawStyle, Point, Rect, Size, StrokeCap, StrokeJoin};
use crate::rendertree::kinds::dash::Pen;
use crate::rendertree::{Bounded, Path};
use std::f32::consts::SQRT_2;

fn stroked(d: &str, width: f32) -> Path {
  let mut path = Path::default();
  path.set_d(d.to_string());
  path.paint.draw_style = DrawStyle::Stroke;
  path.paint.stroke_width = width;
  path
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn close(a: Rect, b: Rect) -> bool {
  let eps = 1e-3;
  (a.origin.x - b.origin.x).abs() < eps
    && (a.origin.y - b.origin.y).abs() < eps
    && (a.size.width - b.size.width).abs() < eps
    && (a.size.height - b.size.height).abs() < eps
}

fn bounds(path: &Path) -> Rect {
  path.local_bounds(Size::new(100.0, 50.0))
}

fn assert_bounds(path: &Path, expected: Rect) {
  let got = bounds(path);
  assert!(close(got, expected), "got {got:?}, expected {expected:?}");
}

#[test]
fn no_d_has_no_bounds() {
  assert_eq!(bounds(&Path::default()), Rect::zero());
}

#[test]
fn bounds_follow_the_curve_not_its_control_points() {
  let mut path = Path::default();
  path.set_d("M0 0 Q50 100 100 0".into());
  assert_bounds(&path, rect(0.0, 0.0, 100.0, 50.0));
}

#[test]
fn stroke_and_offset_inflate_the_box() {
  let mut path = stroked("M10 10 L50 10", 4.0);
  path.set_x(Some(5.0));
  path.set_y(Some(7.0));
  assert_bounds(&path, rect(13.0, 15.0, 44.0, 4.0));
}

#[test]
fn caps_and_joins_read_off_the_path() {
  let half = 2.0;
  // One open segment: capped, no join.
  let mut open = stroked("M0 0 L10 0", 4.0);
  open.paint.stroke_cap = StrokeCap::Square;
  assert_bounds(&open, rect(0.0, 0.0, 10.0, 0.0).inflate(half * SQRT_2, half * SQRT_2));
  // Closed: joined, not capped; the miter limit sets the reach.
  let mut closed = stroked("M0 0 L10 0 L10 10 Z", 4.0);
  closed.paint.stroke_cap = StrokeCap::Square;
  closed.paint.stroke_join = StrokeJoin::Miter;
  closed.paint.stroke_miter = 4.0;
  assert_bounds(&closed, rect(0.0, 0.0, 10.0, 10.0).inflate(half * 4.0, half * 4.0));
  // Open with a vertex: both apply, the larger wins.
  let mut bent = stroked("M0 0 L10 0 L10 10", 4.0);
  bent.paint.stroke_cap = StrokeCap::Square;
  bent.paint.stroke_join = StrokeJoin::Round;
  assert_bounds(&bent, rect(0.0, 0.0, 10.0, 10.0).inflate(half * SQRT_2, half * SQRT_2));
  // Round caps and joins: half the width.
  let mut round = stroked("M0 0 L10 0 L10 10", 4.0);
  round.paint.stroke_cap = StrokeCap::Round;
  round.paint.stroke_join = StrokeJoin::Round;
  assert_bounds(&round, rect(0.0, 0.0, 10.0, 10.0).inflate(half, half));
}

#[test]
fn fill_only_has_no_stroke_outset() {
  let mut path = Path::default();
  path.set_d("M0 0 L10 0 L10 10 Z".into());
  path.paint.stroke_width = 8.0;
  assert_bounds(&path, rect(0.0, 0.0, 10.0, 10.0));
}

#[test]
fn a_new_d_rebuilds_the_bounds() {
  let mut path = stroked("M0 0 L10 0", 2.0);
  assert_bounds(&path, rect(-1.0, -1.0, 12.0, 2.0));
  path.set_d("M20 20 L20 30".into());
  assert_bounds(&path, rect(19.0, 19.0, 2.0, 12.0));
}

#[test]
fn dashes_count_as_caps() {
  let mut path = stroked("M0 0 L10 0 L10 10 Z", 4.0);
  path.paint.stroke_cap = StrokeCap::Square;
  path.paint.stroke_join = StrokeJoin::Round;
  assert_bounds(&path, rect(0.0, 0.0, 10.0, 10.0).inflate(2.0, 2.0));
  path.set_on_length(Some(4.0));
  path.set_off_length(Some(2.0));
  assert_bounds(&path, rect(0.0, 0.0, 10.0, 10.0).inflate(2.0 * SQRT_2, 2.0 * SQRT_2));
}

// Records the walker's runs as "M{x},{y}" / "L{x},{y}" words.
struct Trace(Vec<String>);

impl Pen for Trace {
  fn move_to(&mut self, p: Point) {
    self.0.push(format!("M{},{}", p.x, p.y));
  }
  fn line_to(&mut self, p: Point) {
    self.0.push(format!("L{},{}", p.x, p.y));
  }
}

fn dashes(d: &str, on: f32, off: f32) -> String {
  let mut path = stroked(d, 1.0);
  path.set_on_length(Some(on));
  path.set_off_length(Some(off));
  let mut trace = Trace(Vec::new());
  path.walk_dashed(path.dash().expect("a dash pattern"), &mut trace);
  trace.0.join(" ")
}

#[test]
fn dashes_restart_at_each_subpath() {
  assert_eq!(dashes("M0 0 L10 0 M0 10 L10 10", 6.0, 4.0), "M0,0 L6,0 M0,10 L6,10");
}

#[test]
fn a_closed_subpath_is_walked_through_its_closing_segment() {
  // The first run turns the corner; the gap swallows the diagonal.
  assert_eq!(dashes("M0 0 L10 0 L10 10 Z", 20.0, 20.0), "M0,0 L10,0 L10,10");
}

#[test]
fn dashes_follow_the_curve() {
  // y = x (100 - x) / 100 along this quadratic; every run endpoint sits on it.
  let mut path = stroked("M0 0 Q50 50 100 0", 5.0);
  path.set_on_length(Some(5.0));
  path.set_off_length(Some(5.0));
  let mut trace = Trace(Vec::new());
  path.walk_dashed(path.dash().expect("a dash pattern"), &mut trace);
  assert!(trace.0.len() > 10, "{:?}", trace.0);
  for word in &trace.0 {
    let (x, y) = word[1..].split_once(',').expect("x,y");
    let (x, y): (f32, f32) = (x.parse().expect("x"), y.parse().expect("y"));
    assert!((y - x * (100.0 - x) / 100.0).abs() < 0.5, "{word} is off the curve");
  }
}

#[test]
fn a_solid_path_needs_no_walk() {
  let path = stroked("M0 0 L10 0", 1.0);
  assert!(path.dash().is_none());
}
