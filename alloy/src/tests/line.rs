// The polyline form of Line: segments from a flat points array, the closing
// segment, precedence over the endpoints. Hit testing is the observable that
// needs no platform; build and measure walk the same segments/extent.

use crate::impellers::{DrawStyle, Point, Rect, Size, StrokeCap, StrokeJoin};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::kinds::dash::{walk_dashes, Dash, Pen};
use crate::rendertree::kinds::line::pieces;
use crate::rendertree::{Bounded, Line};

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

// The dash walker, observed through a recording pen: "M" opens a run,
// "L" extends it.
#[derive(Default)]
struct Trace(Vec<String>);

impl Pen for Trace {
  fn move_to(&mut self, p: Point) {
    self.0.push(format!("M{},{}", p.x, p.y));
  }
  fn line_to(&mut self, p: Point) {
    self.0.push(format!("L{},{}", p.x, p.y));
  }
  fn quadratic_to(&mut self, ctrl: Point, p: Point) {
    self.0.push(format!("Q{},{} {},{}", ctrl.x, ctrl.y, p.x, p.y));
  }
  fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
    self.0.push(format!("C{},{} {},{} {},{}", c1.x, c1.y, c2.x, c2.y, p.x, p.y));
  }
}

fn dashes(points: &[f32], closed: bool, on: f32, off: f32, offset: f32) -> String {
  let mut pen = Trace::default();
  walk_dashes(pieces(points, closed), Dash { on, off, offset }, &mut pen);
  pen.0.join(" ")
}

#[test]
fn dash_runs_continue_across_a_vertex() {
  // 15 on: the run turns the corner inside one subpath (a join, not two
  // caps) and ends 5 into the second segment; the 5 off then consumes the
  // rest.
  assert_eq!(dashes(&[0.0, 0.0, 10.0, 0.0, 10.0, 10.0], false, 15.0, 5.0, 0.0), "M0,0 L10,0 L10,5");
}

#[test]
fn dash_offset_shifts_the_pattern_and_wraps() {
  let seg = [0.0, 0.0, 40.0, 0.0];
  assert_eq!(dashes(&seg, false, 10.0, 10.0, 0.0), "M0,0 L10,0 M20,0 L30,0");
  // 5 into the pattern: the first dash is cut short, the pattern is pulled
  // toward the start, and the tail of a third dash appears.
  let shifted = "M0,0 L5,0 M15,0 L25,0 M35,0 L40,0";
  assert_eq!(dashes(&seg, false, 10.0, 10.0, 5.0), shifted);
  assert_eq!(dashes(&seg, false, 10.0, 10.0, 25.0), shifted, "wraps around the period");
  assert_eq!(dashes(&seg, false, 10.0, 10.0, -15.0), shifted, "negative offsets wrap too");
}

#[test]
fn closed_walks_the_closing_segment_in_phase() {
  let square = [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0];
  assert_eq!(dashes(&square, false, 25.0, 5.0, 0.0), "M0,0 L10,0 L10,10 L5,10");
  assert_eq!(dashes(&square, true, 25.0, 5.0, 0.0), "M0,0 L10,0 L10,10 L5,10 M0,10 L0,0");
}

#[test]
fn a_zero_on_length_is_a_dot_per_period() {
  // Runs toggle strictly inside a segment, so the boundary at the very end
  // does not open one more dot.
  assert_eq!(dashes(&[0.0, 0.0, 20.0, 0.0], false, 0.0, 10.0, 0.0), "M0,0 L0,0 M10,0 L10,0");
}

#[test]
fn dash_needs_both_lengths_and_a_positive_gap() {
  let points = [0.0, 0.0, 10.0, 0.0];
  let mut line = polyline(&points, false);
  assert_eq!(line.dash(&points, false), None);
  line.set_on_length(Some(4.0));
  assert_eq!(line.dash(&points, false), None);
  line.set_off_length(Some(0.0));
  assert_eq!(line.dash(&points, false), None, "no gap is solid");
  line.set_off_length(Some(2.0));
  assert_eq!(line.dash(&points, false), Some(Dash { on: 4.0, off: 2.0, offset: 0.0 }));
  line.set_dash_offset(Some(-3.0));
  line.set_on_length(Some(-1.0));
  let clamped = Dash { on: 0.0, off: 2.0, offset: -3.0 };
  assert_eq!(line.dash(&points, false), Some(clamped), "a negative on clamps to zero");
}

#[test]
fn path_length_makes_the_pattern_fractional() {
  // 70 long, declared as 1: 0.5 on is the first 35, so the run turns the
  // corner and ends 5 into the second segment, and the gap covers the rest.
  let points = [0.0, 0.0, 30.0, 0.0, 30.0, 40.0];
  let mut line = polyline(&points, false);
  line.set_on_length(Some(0.5));
  line.set_off_length(Some(1.0));
  line.set_path_length(Some(1.0));
  let dash = line.dash(&points, false).expect("a dash pattern");
  assert_eq!(dash, Dash { on: 35.0, off: 70.0, offset: 0.0 });
  let mut pen = Trace::default();
  walk_dashes(pieces(&points, false), dash, &mut pen);
  assert_eq!(pen.0.join(" "), "M0,0 L30,0 L30,5");
  // Closing adds the hypotenuse (50) to what a unit stands for.
  assert_eq!(line.dash(&points, true).expect("a dash pattern").on, 60.0);
  // A non-positive declaration is no declaration.
  line.set_path_length(Some(0.0));
  assert_eq!(line.dash(&points, true).expect("a dash pattern").on, 0.5);
}

// Painted bounds: geometry AABB plus the stroke outset.
fn close(a: Rect, b: Rect) -> bool {
  let eps = 1e-3;
  (a.origin.x - b.origin.x).abs() < eps
    && (a.origin.y - b.origin.y).abs() < eps
    && (a.size.width - b.size.width).abs() < eps
    && (a.size.height - b.size.height).abs() < eps
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn bounds(line: &Line) -> Rect {
  line.local_bounds(Size::new(100.0, 50.0))
}

#[test]
fn endpoint_form_bounds_resolve_defaults_against_the_fallback() {
  let mut line = Line::default();
  line.paint.stroke_width = 4.0;
  // Defaults span the box; butt caps reach half the width past it.
  assert!(close(bounds(&line), rect(-2.0, -2.0, 104.0, 54.0)), "{:?}", bounds(&line));
  line.set_x1(Some(30.0));
  line.set_y1(Some(10.0));
  line.set_x2(Some(10.0));
  line.set_y2(Some(40.0));
  assert!(close(bounds(&line), rect(8.0, 8.0, 24.0, 34.0)), "{:?}", bounds(&line));
}

#[test]
fn polyline_bounds_are_the_points_extent_plus_the_stroke() {
  let line = polyline(&[10.0, 20.0, 60.0, 20.0, 60.0, 50.0], false);
  // Round joins and butt caps: half the 4 px stroke all round.
  let mut round = line.clone();
  round.paint.stroke_join = StrokeJoin::Round;
  assert!(close(bounds(&round), rect(8.0, 18.0, 54.0, 34.0)), "{:?}", bounds(&round));
  // Fewer than two points paint nothing.
  assert!(close(bounds(&polyline(&[5.0, 5.0], false)), Rect::zero()));
  assert!(close(bounds(&polyline(&[], true)), Rect::zero()));
}

#[test]
fn caps_and_miter_joins_grow_the_outset() {
  let segment = polyline(&[0.0, 0.0, 100.0, 0.0], false);
  let mut square = segment.clone();
  square.paint.stroke_cap = StrokeCap::Square;
  let half = 2.0;
  assert!(close(bounds(&segment), rect(-half, -half, 100.0 + 2.0 * half, 2.0 * half)));
  let sq = half * std::f32::consts::SQRT_2;
  assert!(close(bounds(&square), rect(-sq, -sq, 100.0 + 2.0 * sq, 2.0 * sq)), "{:?}", bounds(&square));

  // Miter joins need a vertex: two points have none, three do, and a closed
  // pair does. The outset is half the width times the miter limit (4).
  let mut miter = segment.clone();
  miter.paint.stroke_join = StrokeJoin::Miter;
  assert!(close(bounds(&miter), rect(-half, -half, 104.0, 4.0)), "{:?}", bounds(&miter));
  let corner = polyline(&[0.0, 0.0, 100.0, 0.0, 100.0, 50.0], false);
  let m = half * 4.0;
  assert!(close(bounds(&corner), rect(-m, -m, 100.0 + 2.0 * m, 50.0 + 2.0 * m)), "{:?}", bounds(&corner));
  let closed_pair = polyline(&[0.0, 0.0, 100.0, 0.0], true);
  assert!(close(bounds(&closed_pair), rect(-m, -m, 100.0 + 2.0 * m, 2.0 * m)), "{:?}", bounds(&closed_pair));
}

#[test]
fn fill_only_has_no_stroke_outset() {
  let mut line = polyline(&[20.0, 100.0, 70.0, 20.0, 120.0, 100.0], false);
  line.paint.draw_style = DrawStyle::Fill;
  assert!(close(bounds(&line), rect(20.0, 20.0, 100.0, 80.0)), "{:?}", bounds(&line));
  line.paint.draw_style = DrawStyle::StrokeAndFill;
  line.paint.stroke_join = StrokeJoin::Bevel;
  assert!(close(bounds(&line), rect(18.0, 18.0, 104.0, 84.0)), "{:?}", bounds(&line));
}

// x/y move the whole geometry - endpoints or points - at paint time, and the
// bounds and hit test follow, the way a path's x/y work.
#[test]
fn x_y_offset_endpoints_and_points_alike() {
  let mut line = Line::default();
  line.paint.stroke_width = 4.0;
  line.set_x1(Some(0.0));
  line.set_y1(Some(0.0));
  line.set_x2(Some(50.0));
  line.set_y2(Some(0.0));
  line.set_x(Some(20.0));
  line.set_y(Some(30.0));
  assert!(close(bounds(&line), rect(18.0, 28.0, 54.0, 4.0)), "{:?}", bounds(&line));
  assert!(hits(&line, 45.0, 30.0));
  assert!(!hits(&line, 25.0, 0.0), "the un-offset position no longer hits");

  let mut poly = polyline(&[0.0, 0.0, 50.0, 0.0, 50.0, 50.0], false);
  let unmoved = bounds(&poly);
  poly.set_x(Some(20.0));
  poly.set_y(Some(30.0));
  let moved = bounds(&poly);
  assert!(
    close(moved, rect(unmoved.origin.x + 20.0, unmoved.origin.y + 30.0, unmoved.size.width, unmoved.size.height)),
    "{moved:?}"
  );
  assert!(hits(&poly, 70.0, 60.0));
  assert!(!hits(&poly, 50.0, 25.0));
}
