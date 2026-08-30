// The dash walker shared by line and path: a pattern in local units, the
// pieces of geometry it walks, and the pen it draws every "on" run into.

use crate::impellers::{FillType, Path as ImpPath, PathBuilder, Point, Rect};
use lyon_path::geom::{point, CubicBezierSegment, LineSegment, QuadraticBezierSegment};
use std::ops::Range;

// How far the flattening the dash walker measures arc length with may
// stray from the curve, in local units. It only places the dash boundaries
// along a curve (the dashes drawn are pieces of the curve itself), so it
// never shows as facets; a quarter unit keeps the pattern where a polyline
// of the curve would have it.
pub const DASH_TOLERANCE: f32 = 0.25;

// The cubic Bezier control distance, as a fraction of the radius, that fits
// a quarter circle: 4/3 (sqrt 2 - 1). The curve strays at most 0.027% of
// the radius from the true arc, far under a pixel at any drawn size.
const QUARTER_ARC_KAPPA: f32 = 0.552_284_8;

// A box outline's corners in CSS order (top-left, top-right, bottom-right,
// bottom-left) as the (cos, sin) where each corner's arc starts, y down;
// the arc sweeps a clockwise quarter turn on screen to the next entry.
const CORNER_START: [(f32, f32); 4] = [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0)];

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

// The quarter ellipse of `corner` around `center`, clockwise on screen.
fn quarter_arc(center: Point, rx: f32, ry: f32, corner: usize) -> Piece {
  let (c0, s0) = CORNER_START[corner];
  let (c1, s1) = CORNER_START[(corner + 1) % 4];
  let k = QUARTER_ARC_KAPPA;
  // The tangent at (cos a, sin a) along the sweep is (-sin a, cos a).
  let seg = CubicBezierSegment {
    from: point(center.x + rx * c0, center.y + ry * s0),
    ctrl1: point(center.x + rx * (c0 - k * s0), center.y + ry * (s0 + k * c0)),
    ctrl2: point(center.x + rx * (c1 + k * s1), center.y + ry * (s1 - k * c1)),
    to: point(center.x + rx * c1, center.y + ry * s1),
  };
  Piece::cubic(seg, DASH_TOLERANCE)
}

fn push_line(pieces: &mut Vec<Piece>, a: Point, b: Point) {
  if a != b {
    pieces.push(Piece::line(a, b));
  }
}

// A rounded box's outline as walker pieces: the edges and the corner arcs
// for `radii` (CSS order, each clamped to the half box), starting where
// SVG's rect does, on the top edge after the top-left corner, and running
// clockwise. This is the box primitives' inset stroke path, so a dashed
// rect dashes exactly the outline its solid stroke draws.
pub fn box_outline(rect: Rect, radii: [f32; 4]) -> Vec<Piece> {
  let (x, y, w, h) = (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
  let limit = (w / 2.0).min(h / 2.0).max(0.0);
  let [tl, tr, br, bl] = radii.map(|r| r.clamp(0.0, limit));
  let mut pieces = Vec::new();
  let corner = |pieces: &mut Vec<Piece>, center: Point, r: f32, corner: usize| {
    if r > 0.0 {
      pieces.push(quarter_arc(center, r, r, corner));
    }
  };
  push_line(&mut pieces, Point::new(x + tl, y), Point::new(x + w - tr, y));
  corner(&mut pieces, Point::new(x + w - tr, y + tr), tr, 1);
  push_line(&mut pieces, Point::new(x + w, y + tr), Point::new(x + w, y + h - br));
  corner(&mut pieces, Point::new(x + w - br, y + h - br), br, 2);
  push_line(&mut pieces, Point::new(x + w - br, y + h), Point::new(x + bl, y + h));
  corner(&mut pieces, Point::new(x + bl, y + h - bl), bl, 3);
  push_line(&mut pieces, Point::new(x, y + h - bl), Point::new(x, y + tl));
  corner(&mut pieces, Point::new(x + tl, y + tl), tl, 0);
  pieces
}

// An oval's outline as four quarter arcs, starting where SVG's ellipse
// does (3 o'clock) and running clockwise. Empty for a box without area.
pub fn oval_outline(rect: Rect) -> Vec<Piece> {
  let (rx, ry) = (rect.size.width / 2.0, rect.size.height / 2.0);
  if rx <= 0.0 || ry <= 0.0 {
    return Vec::new();
  }
  let center = Point::new(rect.origin.x + rx, rect.origin.y + ry);
  [2, 3, 0, 1].into_iter().map(|corner| quarter_arc(center, rx, ry, corner)).collect()
}

// The dashed stroke of `pieces` as one path for Impeller to stroke.
pub fn dashed_path(pieces: impl Iterator<Item = Piece>, dash: Dash) -> ImpPath {
  let mut out = PathBuilder::default();
  walk_dashes(pieces, dash, &mut out);
  out.take_path_new(FillType::NonZero)
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
