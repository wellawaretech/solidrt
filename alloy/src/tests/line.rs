// The polyline form of Line: segments from a flat points array, the closing
// segment, precedence over the endpoints. Hit testing is the observable that
// needs no platform; build and measure walk the same segments/extent.

use crate::impellers::{DrawStyle, Point, Rect, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::kinds::line::{segments, walk_dashes, Dash, Pen};
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
}

fn dashes(points: &[f32], closed: bool, on: f32, off: f32, offset: f32) -> String {
  let mut pen = Trace::default();
  walk_dashes(segments(points, closed), Dash { on, off, offset }, &mut pen);
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
  let mut line = polyline(&[0.0, 0.0, 10.0, 0.0], false);
  assert_eq!(line.dash(), None);
  line.set_on_length(Some(4.0));
  assert_eq!(line.dash(), None);
  line.set_off_length(Some(0.0));
  assert_eq!(line.dash(), None, "no gap is solid");
  line.set_off_length(Some(2.0));
  assert_eq!(line.dash(), Some(Dash { on: 4.0, off: 2.0, offset: 0.0 }));
  line.set_dash_offset(Some(-3.0));
  line.set_on_length(Some(-1.0));
  assert_eq!(line.dash(), Some(Dash { on: 0.0, off: 2.0, offset: -3.0 }), "a negative on clamps to zero");
}
