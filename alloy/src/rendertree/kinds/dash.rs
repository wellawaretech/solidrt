// The dash walker shared by line and path: a pattern in local units, the
// pieces of geometry it walks, and the pen it draws every "on" run into.

use crate::impellers::{PathBuilder, Point};
use lyon_path::geom::{point, CubicBezierSegment, LineSegment, QuadraticBezierSegment};
use std::ops::Range;

// A dash pattern in local units: `on` drawn, `off` skipped, starting
// `offset` into the pattern (SVG stroke-dashoffset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dash {
  pub on: f32,
  pub off: f32,
  pub offset: f32,
}

impl Dash {
  // Both lengths set and a positive gap; a non-positive gap has nothing to
  // skip and draws solid. A zero `on` is a dot per period (caps permitting),
  // as in SVG. The positive period is what the walker relies on.
  pub fn new(on: Option<f32>, off: Option<f32>, offset: Option<f32>) -> Option<Dash> {
    let (Some(on), Some(off)) = (on, off) else { return None };
    (off > 0.0).then(|| Dash { on: on.max(0.0), off, offset: offset.unwrap_or(0.0) })
  }

  // The pattern with its units mapped onto the geometry, `factor` being the
  // walked length over the author's declared one (SVG pathLength). A
  // geometry without length scales the gap away: nothing to dash.
  pub fn scaled(self, factor: f32) -> Option<Dash> {
    let off = self.off * factor;
    (off > 0.0).then(|| Dash { on: self.on * factor, off, offset: self.offset * factor })
  }
}

// Where the dash walker draws: the PathBuilder in a build, a recording pen
// in the tests. A curve's dashes arrive as curves, so the stroker sees the
// geometry a solid stroke would.
pub trait Pen {
  fn move_to(&mut self, p: Point);
  fn line_to(&mut self, p: Point);
  fn quadratic_to(&mut self, ctrl: Point, p: Point);
  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point);
}

impl Pen for PathBuilder {
  fn move_to(&mut self, p: Point) {
    PathBuilder::move_to(self, p);
  }
  fn line_to(&mut self, p: Point) {
    PathBuilder::line_to(self, p);
  }
  fn quadratic_to(&mut self, ctrl: Point, p: Point) {
    PathBuilder::quadratic_curve_to(self, ctrl, p);
  }
  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point) {
    PathBuilder::cubic_curve_to(self, ctrl1, ctrl2, p);
  }
}

type LyonPoint = lyon_path::geom::Point<f32>;

fn pt(p: LyonPoint) -> Point {
  Point::new(p.x, p.y)
}

#[derive(Clone, Debug)]
enum Curve {
  Line(LineSegment<f32>),
  Quadratic(QuadraticBezierSegment<f32>),
  Cubic(CubicBezierSegment<f32>),
}

// One piece of walked geometry, a segment or a curve, with its arc length
// in flattened form. The walker measures the pattern along the piece and
// asks it for the part between two positions, which for a curve is the
// curve split there, not the flattening.
#[derive(Clone, Debug)]
pub struct Piece {
  curve: Curve,
  // (cumulative length, t) at the end of each flattened step; empty for a
  // segment, whose t is the length fraction.
  steps: Vec<(f32, f32)>,
  length: f32,
}

impl Piece {
  pub fn line(a: Point, b: Point) -> Piece {
    let seg = LineSegment { from: point(a.x, a.y), to: point(b.x, b.y) };
    Piece { length: seg.length(), curve: Curve::Line(seg), steps: Vec::new() }
  }

  // A curve's arc length is its flattening's within `tolerance`, the same
  // measure a polyline of it has, so the pattern (and `pathLength`) land
  // where they would on that polyline while the curve itself is drawn.
  pub fn quadratic(seg: QuadraticBezierSegment<f32>, tolerance: f32) -> Piece {
    let mut piece = Piece { curve: Curve::Quadratic(seg), steps: Vec::new(), length: 0.0 };
    seg.for_each_flattened_with_t(tolerance, &mut |step, t| piece.push_step(step, t));
    piece
  }

  pub fn cubic(seg: CubicBezierSegment<f32>, tolerance: f32) -> Piece {
    let mut piece = Piece { curve: Curve::Cubic(seg), steps: Vec::new(), length: 0.0 };
    seg.for_each_flattened_with_t(tolerance, &mut |step, t| piece.push_step(step, t));
    piece
  }

  fn push_step(&mut self, step: &LineSegment<f32>, t: Range<f32>) {
    self.length += step.length();
    self.steps.push((self.length, t.end));
  }

  pub fn length(&self) -> f32 {
    self.length
  }

  // The curve parameter at arc length `s`, interpolated within the
  // flattened step that holds it.
  fn t_at(&self, s: f32) -> f32 {
    if s <= 0.0 {
      return 0.0;
    }
    if s >= self.length {
      return 1.0;
    }
    if self.steps.is_empty() {
      return s / self.length;
    }
    let (mut at, mut t0) = (0.0, 0.0);
    for &(end, t1) in &self.steps {
      if s <= end {
        return if end > at { t0 + (t1 - t0) * (s - at) / (end - at) } else { t1 };
      }
      at = end;
      t0 = t1;
    }
    1.0
  }

  fn point_at(&self, s: f32) -> Point {
    match &self.curve {
      // The segment's own arithmetic, so a run boundary lands exactly where
      // a polyline walk puts it.
      Curve::Line(seg) => {
        if s >= self.length {
          return pt(seg.to);
        }
        let d = seg.to - seg.from;
        Point::new(seg.from.x + d.x * s / self.length, seg.from.y + d.y * s / self.length)
      }
      Curve::Quadratic(seg) => pt(seg.sample(self.t_at(s))),
      Curve::Cubic(seg) => pt(seg.sample(self.t_at(s))),
    }
  }

  // Draws the part between arc lengths `from` and `to`, the pen standing
  // at `from`.
  fn emit(&self, from: f32, to: f32, pen: &mut impl Pen) {
    match &self.curve {
      Curve::Line(_) => pen.line_to(self.point_at(to)),
      Curve::Quadratic(seg) => {
        let part = seg.split_range(self.t_at(from)..self.t_at(to));
        pen.quadratic_to(pt(part.ctrl), pt(part.to));
      }
      Curve::Cubic(seg) => {
        let part = seg.split_range(self.t_at(from)..self.t_at(to));
        pen.cubic_to(pt(part.ctrl1), pt(part.ctrl2), pt(part.to));
      }
    }
  }
}

// The length the walker travels over `pieces`.
pub fn walked_length(pieces: impl Iterator<Item = Piece>) -> f32 {
  pieces.map(|piece| piece.length()).sum()
}

// Emits every "on" run of the pattern as one subpath, carrying the phase
// across pieces: a run that crosses a piece boundary keeps its subpath (so
// the join applies inside it), a run that ends mid-piece is capped. Impeller
// dashes single segments only, so this is ours. Requires a positive period
// (see `Dash::new`).
pub fn walk_dashes(pieces: impl Iterator<Item = Piece>, dash: Dash, pen: &mut impl Pen) {
  let period = dash.on + dash.off;
  let phase = dash.offset.rem_euclid(period);
  // The pattern opens with the on run, even a zero-length one (a dot).
  let mut drawing = phase == 0.0 || phase < dash.on;
  let mut remaining = if drawing { dash.on - phase } else { period - phase };
  let mut pen_down = false;
  for piece in pieces {
    let len = piece.length();
    if drawing && !pen_down {
      pen.move_to(piece.point_at(0.0));
      pen_down = true;
    }
    let mut at = 0.0;
    while at + remaining < len {
      let next = at + remaining;
      if drawing {
        piece.emit(at, next, pen);
        remaining = dash.off;
      } else {
        pen.move_to(piece.point_at(next));
        remaining = dash.on;
      }
      at = next;
      pen_down = !drawing;
      drawing = !drawing;
    }
    remaining -= len - at;
    if drawing {
      piece.emit(at, len, pen);
      // A run ending exactly on the piece's end closes here, so the next
      // piece does not re-emit the point as a zero-length step.
      if remaining <= 0.0 {
        drawing = false;
        remaining = dash.off;
        pen_down = false;
      }
    }
  }
}
