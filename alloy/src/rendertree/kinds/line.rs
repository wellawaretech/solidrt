use super::dash::{walk_dashes, walked_length, Dash, Piece};
use super::PaintState;
use crate::impellers::{DisplayListBuilder, DrawStyle, FillType, Path, PathBuilder, Point, Rect, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};

// Endpoints default to spanning the box: (0,0) to (box.w, box.h), matching how
// a rect with unset w/h fills its box. Explicit endpoints are detached-only.
//
// `points` is the polyline form: a flat [x0, y0, x1, y1, ...] in the same
// local space, taking precedence over the endpoints while set. It is content
// rather than box geometry (like a path's `d`), so it exists on the layout
// form too and drives that form's measure.
//
// The paint defaults to stroke (DEFAULT_DRAW_STYLE), unlike the box kinds: a
// segment has no interior. A polyline honours the draw style, so "fill" or
// "stroke-and-fill" makes it a polygon (nonzero, implicitly closed).
#[derive(Clone, Debug)]
pub struct Line {
  pub x1: Option<f32>,
  pub y1: Option<f32>,
  pub x2: Option<f32>,
  pub y2: Option<f32>,
  pub points: Option<Vec<f32>>,
  pub closed: bool,
  pub on_length: Option<f32>,
  pub off_length: Option<f32>,
  pub dash_offset: Option<f32>,
  pub path_length: Option<f32>,
  pub paint: PaintState,
}

impl Default for Line {
  fn default() -> Self {
    Self {
      x1: None,
      y1: None,
      x2: None,
      y2: None,
      points: None,
      closed: false,
      on_length: None,
      off_length: None,
      dash_offset: None,
      path_length: None,
      paint: PaintState { draw_style: Self::DEFAULT_DRAW_STYLE, ..PaintState::default() },
    }
  }
}

fn vertex(points: &[f32], i: usize) -> Point {
  Point::new(points[2 * i], points[2 * i + 1])
}

// The polyline's segments as consecutive vertex pairs, plus the closing pair
// when closed. A trailing odd number (the decoder rejects one, but the kind
// does not depend on that) is ignored.
pub(crate) fn segments(points: &[f32], closed: bool) -> impl Iterator<Item = (Point, Point)> + '_ {
  let n = points.len() / 2;
  let open = (1..n).map(move |i| (vertex(points, i - 1), vertex(points, i)));
  let closing = (closed && n >= 2).then(|| (vertex(points, n - 1), vertex(points, 0)));
  open.chain(closing)
}

// Axis-aligned extent of the vertices; None without any.
fn extent(points: &[f32]) -> Option<Rect> {
  let n = points.len() / 2;
  if n == 0 {
    return None;
  }
  let first = vertex(points, 0);
  let (mut min, mut max) = (first, first);
  for i in 1..n {
    let v = vertex(points, i);
    min = Point::new(min.x.min(v.x), min.y.min(v.y));
    max = Point::new(max.x.max(v.x), max.y.max(v.y));
  }
  Some(Rect::new(min, Size::new(max.x - min.x, max.y - min.y)))
}

// Nonzero winding number of `p` against the vertex ring (implicitly closed,
// as a fill is): 0 outside, otherwise inside.
fn winding(points: &[f32], p: Point) -> i32 {
  let n = points.len() / 2;
  let is_left = |a: Point, b: Point| (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
  let mut wn = 0;
  for i in 0..n {
    let a = vertex(points, i);
    let b = vertex(points, (i + 1) % n);
    if a.y <= p.y {
      if b.y > p.y && is_left(a, b) > 0.0 {
        wn += 1;
      }
    } else if b.y <= p.y && is_left(a, b) < 0.0 {
      wn -= 1;
    }
  }
  wn
}

fn polyline_path(points: &[f32], closed: bool) -> Path {
  let mut path = PathBuilder::default();
  path.move_to(vertex(points, 0));
  for i in 1..points.len() / 2 {
    path.line_to(vertex(points, i));
  }
  if closed {
    path.close();
  }
  path.take_path_new(FillType::NonZero)
}

// The polyline's segments as walker pieces.
pub(crate) fn pieces(points: &[f32], closed: bool) -> impl Iterator<Item = Piece> + '_ {
  segments(points, closed).map(|(a, b)| Piece::line(a, b))
}

fn dashed_path(points: &[f32], closed: bool, dash: Dash) -> Path {
  let mut path = PathBuilder::default();
  walk_dashes(pieces(points, closed), dash, &mut path);
  path.take_path_new(FillType::NonZero)
}

fn dist_sq_to_segment(p: Point, a: Point, b: Point) -> f32 {
  let dx = b.x - a.x;
  let dy = b.y - a.y;
  let len_sq = dx * dx + dy * dy;
  if len_sq == 0.0 {
    let ex = p.x - a.x;
    let ey = p.y - a.y;
    return ex * ex + ey * ey;
  }
  let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
  let t = t.clamp(0.0, 1.0);
  let proj_x = a.x + t * dx;
  let proj_y = a.y + t * dy;
  let ex = p.x - proj_x;
  let ey = p.y - proj_y;
  ex * ex + ey * ey
}

impl Line {
  pub const DEFAULT_DRAW_STYLE: DrawStyle = DrawStyle::Stroke;

  fn fills(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Fill | DrawStyle::StrokeAndFill)
  }

  fn strokes(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Stroke | DrawStyle::StrokeAndFill)
  }

  fn endpoints(&self, box_w: f32, box_h: f32) -> (Point, Point) {
    (
      Point::new(self.x1.unwrap_or(0.0), self.y1.unwrap_or(0.0)),
      Point::new(self.x2.unwrap_or(box_w), self.y2.unwrap_or(box_h)),
    )
  }

  fn dashed(&self) -> bool {
    Dash::new(self.on_length, self.off_length, self.dash_offset).is_some()
  }

  // The pattern in local units for the geometry being dashed; a declared
  // `pathLength` (SVG) maps the author's units onto its walked length.
  pub(crate) fn dash(&self, points: &[f32], closed: bool) -> Option<Dash> {
    let dash = Dash::new(self.on_length, self.off_length, self.dash_offset)?;
    match self.path_length.filter(|declared| *declared > 0.0) {
      Some(declared) => dash.scaled(walked_length(pieces(points, closed)) / declared),
      None => Some(dash),
    }
  }

  // The geometry the build draws, as an AABB in local space: the points, or
  // the endpoints with their box defaults resolved against `fallback`. None
  // when nothing is drawn (fewer than two points).
  fn geometry(&self, fallback: Size) -> Option<Rect> {
    match &self.points {
      Some(points) => (points.len() >= 4).then(|| extent(points)).flatten(),
      None => {
        let (a, b) = self.endpoints(fallback.width, fallback.height);
        extent(&[a.x, a.y, b.x, b.y])
      }
    }
  }

  // Caps sit on open ends: an open polyline's two, the segment's, and every
  // dash's; joins need a vertex between two segments.
  fn stroke_outset(&self) -> f32 {
    let vertices = self.points.as_ref().map_or(2, |p| p.len() / 2);
    let capped = self.points.is_none() || !self.closed || self.dashed();
    let joined = self.points.is_some() && (vertices >= 3 || (self.closed && vertices >= 2));
    self.paint.stroke_outset(capped, joined)
  }

  // A box-relative gradient resolves against the points' extent, as a
  // path's does against its bounds.
  fn build_polyline(&self, points: &[f32], builder: &mut DisplayListBuilder) {
    let Some(bounds) = extent(points).filter(|_| points.len() >= 4) else { return };
    if self.fills() {
      let mut paint = self.paint.to_paint_in(&bounds);
      paint.set_draw_style(DrawStyle::Fill);
      builder.draw_path(&polyline_path(points, true), &paint);
    }
    if self.strokes() {
      let mut paint = self.paint.to_paint_in(&bounds);
      paint.set_draw_style(DrawStyle::Stroke);
      let path = match self.dash(points, self.closed) {
        Some(dash) => dashed_path(points, self.closed, dash),
        None => polyline_path(points, self.closed),
      };
      builder.draw_path(&path, &paint);
    }
  }
}

impl Buildable for Line {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    if let Some(points) = &self.points {
      self.build_polyline(points, builder);
      return;
    }
    let (from, to) = self.endpoints(ctx.size.width, ctx.size.height);
    let mut paint = self.paint.to_paint();
    let points = [from.x, from.y, to.x, to.y];
    match self.dash(&points, false) {
      // Through the same walker as a polyline (dashOffset, phase); stroked
      // regardless of the style, like draw_line.
      Some(dash) => {
        paint.set_draw_style(DrawStyle::Stroke);
        builder.draw_path(&dashed_path(&points, false, dash), &paint);
      }
      None => {
        builder.draw_line(from, to, &paint);
      }
    }
  }
}

// The painted box: the geometry's AABB plus the stroke outset. A line's
// stroke is centered on its geometry, so unlike a rect's (painted inside the
// box) it straddles the box; the bounds say where the pixels are, which is
// what culling, a detached capture and getBoundingBox want.
impl Bounded for Line {
  fn local_bounds(&self, fallback: Size) -> Rect {
    match self.geometry(fallback) {
      Some(geometry) => {
        let outset = self.stroke_outset();
        geometry.inflate(outset, outset)
      }
      None => Rect::zero(),
    }
  }
}

// The two-point form has no intrinsic size: a layout line is sized by the
// width/height layout props (endpoints are detached-only geometry and never
// reach taffy). A polyline measures its points' extent, the way a path
// measures its `d`.
impl Measurable for Line {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size::new(w, h);
    }
    let intrinsic = self.points.as_deref().and_then(extent).map(|r| r.size).unwrap_or(Size::zero());
    Size::new(ctx.known.width.unwrap_or(intrinsic.width), ctx.known.height.unwrap_or(intrinsic.height))
  }
}

impl Line {
  pub fn set_x1(&mut self, v: Option<f32>) -> Damage {
    self.x1 = v;
    Damage::Paint
  }
  pub fn set_y1(&mut self, v: Option<f32>) -> Damage {
    self.y1 = v;
    Damage::Paint
  }
  pub fn set_x2(&mut self, v: Option<f32>) -> Damage {
    self.x2 = v;
    Damage::Paint
  }
  pub fn set_y2(&mut self, v: Option<f32>) -> Damage {
    self.y2 = v;
    Damage::Paint
  }
  // Layout, not Paint: the points size the laid-out form (see measure), the
  // same way `Path::set_d` does.
  pub fn set_points(&mut self, v: Option<Vec<f32>>) -> Damage {
    self.points = v;
    Damage::Layout
  }
  pub fn set_closed(&mut self, v: bool) -> Damage {
    self.closed = v;
    Damage::Paint
  }
  pub fn set_on_length(&mut self, v: Option<f32>) -> Damage {
    self.on_length = v;
    Damage::Paint
  }
  pub fn set_off_length(&mut self, v: Option<f32>) -> Damage {
    self.off_length = v;
    Damage::Paint
  }
  pub fn set_dash_offset(&mut self, v: Option<f32>) -> Damage {
    self.dash_offset = v;
    Damage::Paint
  }
  pub fn set_path_length(&mut self, v: Option<f32>) -> Damage {
    self.path_length = v;
    Damage::Paint
  }

  pub fn initial_style() -> taffy::Style {
    taffy::Style { display: taffy::Display::Block, ..Default::default() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Line(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Line(self))
  }
}

impl Hittable for Line {
  fn is_in_bounds(&self, pt: Point, ctx: &HitContext) -> bool {
    let half_sw = (self.paint.stroke_width / 2.0).max(2.0);
    let max_sq = half_sw * half_sw;
    match &self.points {
      Some(points) => {
        let on_stroke =
          self.strokes() && segments(points, self.closed).any(|(a, b)| dist_sq_to_segment(pt, a, b) <= max_sq);
        on_stroke || (self.fills() && winding(points, pt) != 0)
      }
      None => {
        let (from, to) = self.endpoints(ctx.size.width, ctx.size.height);
        dist_sq_to_segment(pt, from, to) <= max_sq
      }
    }
  }
}
