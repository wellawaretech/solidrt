use super::dash::{walk_dashes, walked_length, Dash, Pen};
use super::PaintState;
use crate::impellers::{DisplayListBuilder, DrawStyle, FillType, Path as ImpPath, PathBuilder, Point, Rect, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};
use lyon_algorithms::aabb::bounding_box;
use lyon_algorithms::hit_test::hit_test_path;
use lyon_path::geom::{point, vector, Angle, ArcFlags, CubicBezierSegment, SvgArc};
use lyon_path::iterator::PathIterator;
use std::cell::{Cell, RefCell};
use svgtypes::{PathParser, PathSegment};

// How far a flattened curve may stray from the true one, in local units,
// when the dash walker needs segments. A quarter pixel is invisible at 1x;
// a d-path under a large scale transform would show facets on its dashes.
const DASH_TOLERANCE: f32 = 0.25;

// What the stroke's reach past the geometry depends on, read off the parsed
// path: a subpath left open (with a segment) takes caps, one with two or
// more segments, or a close, takes joins.
#[derive(Clone, Copy, Debug, Default)]
struct StrokeShape {
  capped: bool,
  joined: bool,
}

fn stroke_shape(path: &lyon_path::Path) -> StrokeShape {
  let mut shape = StrokeShape::default();
  let mut segments = 0;
  for evt in path.iter() {
    match evt {
      lyon_path::Event::Begin { .. } => segments = 0,
      lyon_path::Event::End { close, .. } => {
        shape.capped |= !close && segments >= 1;
        shape.joined |= segments >= 2 || (close && segments >= 1);
      }
      _ => segments += 1,
    }
  }
  shape
}

pub struct Path {
  pub d: String,
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub paint: PaintState,
  pub fill_rule: FillType,
  pub on_length: Option<f32>,
  pub off_length: Option<f32>,
  pub dash_offset: Option<f32>,
  pub path_length: Option<f32>,
  path: RefCell<Option<ImpPath>>,
  // The geometry's tight extent (curve extrema, not control points), in the
  // path's own space before the x/y translate; None while nothing is drawn.
  bounds: RefCell<Option<Rect>>,
  shape: Cell<StrokeShape>,
  // The walked (flattened) length, for a declared `pathLength`; computed on
  // demand and kept with the geometry.
  length: Cell<Option<f32>>,
  lyon_path: RefCell<Option<lyon_path::Path>>,
}

impl Default for Path {
  fn default() -> Self {
    Self {
      d: String::new(),
      x: None,
      y: None,
      paint: PaintState::default(),
      fill_rule: FillType::NonZero,
      on_length: None,
      off_length: None,
      dash_offset: None,
      path_length: None,
      path: RefCell::new(None),
      bounds: RefCell::new(None),
      shape: Cell::new(StrokeShape::default()),
      length: Cell::new(None),
      lyon_path: RefCell::new(None),
    }
  }
}

impl Clone for Path {
  fn clone(&self) -> Self {
    Self {
      d: self.d.clone(),
      x: self.x,
      y: self.y,
      paint: self.paint.clone(),
      fill_rule: self.fill_rule,
      on_length: self.on_length,
      off_length: self.off_length,
      dash_offset: self.dash_offset,
      path_length: self.path_length,
      path: RefCell::new(None),
      bounds: RefCell::new(None),
      shape: Cell::new(StrokeShape::default()),
      length: Cell::new(None),
      lyon_path: RefCell::new(None),
    }
  }
}

impl std::fmt::Debug for Path {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Path").field("d", &self.d).field("x", &self.x).field("y", &self.y).finish()
  }
}

impl Path {
  fn ensure_built(&self) {
    if self.path.borrow().is_some() {
      return;
    }
    if self.d.is_empty() {
      return;
    }

    let mut path_builder = PathBuilder::default();
    let mut lyon_builder = lyon_path::Path::builder();
    let mut cursor = (0.0f32, 0.0f32);
    let mut subpath_start = cursor;

    let resolve = |abs: bool, x: f64, y: f64, cursor: &(f32, f32)| -> Point {
      if abs {
        Point::new(x as f32, y as f32)
      } else {
        Point::new(cursor.0 + x as f32, cursor.1 + y as f32)
      }
    };

    let mut lyon_open = false;
    let ensure_lyon_begun = |lb: &mut lyon_path::path::Builder, cx: f32, cy: f32, open: &mut bool| {
      if !*open {
        lb.begin(point(cx, cy));
        *open = true;
      }
    };

    // Tracked for SmoothCurveTo (S) and SmoothQuadratic (T) reflection.
    // Reset to None whenever the previous segment was not C/S or Q/T respectively.
    let mut last_cubic_cp2: Option<Point> = None;
    let mut last_quad_cp: Option<Point> = None;

    for segment in PathParser::from(self.d.as_str()) {
      let Ok(seg) = segment else { continue };
      match seg {
        PathSegment::MoveTo { abs, x, y } => {
          if lyon_open {
            lyon_builder.end(false);
          }
          let pt = resolve(abs, x, y, &cursor);
          path_builder.move_to(pt);
          lyon_builder.begin(point(pt.x, pt.y));
          lyon_open = true;
          cursor = (pt.x, pt.y);
          subpath_start = cursor;
          last_cubic_cp2 = None;
          last_quad_cp = None;
        }
        PathSegment::LineTo { abs, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let pt = resolve(abs, x, y, &cursor);
          path_builder.line_to(pt);
          lyon_builder.line_to(point(pt.x, pt.y));
          cursor = (pt.x, pt.y);
          last_cubic_cp2 = None;
          last_quad_cp = None;
        }
        PathSegment::HorizontalLineTo { abs, x } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let pt = resolve(abs, x, 0.0, &cursor);
          let pt = Point::new(pt.x, cursor.1);
          path_builder.line_to(pt);
          lyon_builder.line_to(point(pt.x, pt.y));
          cursor = (pt.x, pt.y);
          last_cubic_cp2 = None;
          last_quad_cp = None;
        }
        PathSegment::VerticalLineTo { abs, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let pt = resolve(abs, 0.0, y, &cursor);
          let pt = Point::new(cursor.0, pt.y);
          path_builder.line_to(pt);
          lyon_builder.line_to(point(pt.x, pt.y));
          cursor = (pt.x, pt.y);
          last_cubic_cp2 = None;
          last_quad_cp = None;
        }
        PathSegment::CurveTo { abs, x1, y1, x2, y2, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let cp1 = resolve(abs, x1, y1, &cursor);
          let cp2 = resolve(abs, x2, y2, &cursor);
          let end = resolve(abs, x, y, &cursor);
          path_builder.cubic_curve_to(cp1, cp2, end);
          lyon_builder.cubic_bezier_to(point(cp1.x, cp1.y), point(cp2.x, cp2.y), point(end.x, end.y));
          cursor = (end.x, end.y);
          last_cubic_cp2 = Some(cp2);
          last_quad_cp = None;
        }
        PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          // Reflected first control point.
          let cp1 = match last_cubic_cp2 {
            Some(prev) => Point::new(2.0 * cursor.0 - prev.x, 2.0 * cursor.1 - prev.y),
            None => Point::new(cursor.0, cursor.1),
          };
          let cp2 = resolve(abs, x2, y2, &cursor);
          let end = resolve(abs, x, y, &cursor);
          path_builder.cubic_curve_to(cp1, cp2, end);
          lyon_builder.cubic_bezier_to(point(cp1.x, cp1.y), point(cp2.x, cp2.y), point(end.x, end.y));
          cursor = (end.x, end.y);
          last_cubic_cp2 = Some(cp2);
          last_quad_cp = None;
        }
        PathSegment::Quadratic { abs, x1, y1, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let cp = resolve(abs, x1, y1, &cursor);
          let end = resolve(abs, x, y, &cursor);
          path_builder.quadratic_curve_to(cp, end);
          lyon_builder.quadratic_bezier_to(point(cp.x, cp.y), point(end.x, end.y));
          cursor = (end.x, end.y);
          last_quad_cp = Some(cp);
          last_cubic_cp2 = None;
        }
        PathSegment::SmoothQuadratic { abs, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          // Reflected control point.
          let cp = match last_quad_cp {
            Some(prev) => Point::new(2.0 * cursor.0 - prev.x, 2.0 * cursor.1 - prev.y),
            None => Point::new(cursor.0, cursor.1),
          };
          let end = resolve(abs, x, y, &cursor);
          path_builder.quadratic_curve_to(cp, end);
          lyon_builder.quadratic_bezier_to(point(cp.x, cp.y), point(end.x, end.y));
          cursor = (end.x, end.y);
          last_quad_cp = Some(cp);
          last_cubic_cp2 = None;
        }
        PathSegment::EllipticalArc { abs, rx, ry, x_axis_rotation, large_arc, sweep, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let end = resolve(abs, x, y, &cursor);
          let svg_arc = SvgArc {
            from: point(cursor.0, cursor.1),
            to: point(end.x, end.y),
            radii: vector(rx as f32, ry as f32),
            x_rotation: Angle::degrees(x_axis_rotation as f32),
            flags: ArcFlags { large_arc, sweep },
          };
          let mut beziers: Vec<CubicBezierSegment<f32>> = Vec::new();
          svg_arc.for_each_cubic_bezier(&mut |seg: &CubicBezierSegment<f32>| {
            beziers.push(*seg);
          });
          for cb in beziers {
            let cp1 = Point::new(cb.ctrl1.x, cb.ctrl1.y);
            let cp2 = Point::new(cb.ctrl2.x, cb.ctrl2.y);
            let end_pt = Point::new(cb.to.x, cb.to.y);
            path_builder.cubic_curve_to(cp1, cp2, end_pt);
            lyon_builder.cubic_bezier_to(point(cp1.x, cp1.y), point(cp2.x, cp2.y), point(end_pt.x, end_pt.y));
          }
          cursor = (end.x, end.y);
          last_cubic_cp2 = None;
          last_quad_cp = None;
        }
        PathSegment::ClosePath { .. } => {
          path_builder.close();
          if lyon_open {
            lyon_builder.close();
            lyon_open = false;
          }
          cursor = subpath_start;
          last_cubic_cp2 = None;
          last_quad_cp = None;
        }
      }
    }

    if lyon_open {
      lyon_builder.end(false);
    }

    let lyon = lyon_builder.build();
    if lyon.iter().next().is_some() {
      let bb = bounding_box(lyon.iter());
      let size = Size::new(bb.max.x - bb.min.x, bb.max.y - bb.min.y);
      *self.bounds.borrow_mut() = Some(Rect::new(Point::new(bb.min.x, bb.min.y), size));
    }
    self.shape.set(stroke_shape(&lyon));
    *self.path.borrow_mut() = Some(path_builder.take_path_new(self.fill_rule));
    *self.lyon_path.borrow_mut() = Some(lyon);
  }

  pub fn invalidate(&self) {
    *self.path.borrow_mut() = None;
    *self.bounds.borrow_mut() = None;
    self.shape.set(StrokeShape::default());
    self.length.set(None);
    *self.lyon_path.borrow_mut() = None;
  }

  // The path's geometry is cached, so shape changes invalidate that cache; `d`
  // also determines the measured size (layout). x/y are a draw-time translate:
  // the cached geometry (and thus measure) is offset-independent, so they cost
  // a repaint only.
  pub fn set_d(&mut self, d: String) -> Damage {
    self.d = d;
    self.invalidate();
    Damage::Layout
  }
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_fill_rule(&mut self, rule: Option<FillType>) -> Damage {
    self.fill_rule = rule.unwrap_or(FillType::NonZero);
    self.invalidate();
    Damage::Paint
  }
  // The dashed stroke is walked at build time from the cached geometry, so
  // the pattern props cost a repaint, not a rebuild.
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

  fn fills(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Fill | DrawStyle::StrokeAndFill)
  }

  fn strokes(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Stroke | DrawStyle::StrokeAndFill)
  }

  // The pattern in local units; a declared `pathLength` (SVG) maps the
  // author's units onto the walked length.
  pub(crate) fn dash(&self) -> Option<Dash> {
    let dash = Dash::new(self.on_length, self.off_length, self.dash_offset)?;
    match self.path_length.filter(|declared| *declared > 0.0) {
      Some(declared) => dash.scaled(self.length() / declared),
      None => Some(dash),
    }
  }

  // Every subpath flattened (curves become segments within DASH_TOLERANCE),
  // a close adding its closing segment: what the dash walker travels and
  // what `length` measures, so the two agree exactly.
  fn flattened_subpaths(&self) -> Vec<Vec<(Point, Point)>> {
    self.ensure_built();
    let lyon = self.lyon_path.borrow();
    let Some(lyon) = lyon.as_ref() else { return Vec::new() };
    let pt = |p: lyon_path::geom::Point<f32>| Point::new(p.x, p.y);
    let mut subpaths = Vec::new();
    let mut segments = Vec::new();
    for evt in lyon.iter().flattened(DASH_TOLERANCE) {
      match evt {
        lyon_path::Event::Begin { .. } => segments.clear(),
        lyon_path::Event::Line { from, to } => segments.push((pt(from), pt(to))),
        lyon_path::Event::End { last, first, close } => {
          if close {
            segments.push((pt(last), pt(first)));
          }
          subpaths.push(std::mem::take(&mut segments));
        }
        _ => {}
      }
    }
    subpaths
  }

  fn length(&self) -> f32 {
    if let Some(length) = self.length.get() {
      return length;
    }
    let total = walked_length(self.flattened_subpaths().into_iter().flatten());
    self.length.set(Some(total));
    total
  }

  // The dashed stroke: each subpath walked on its own, since a dash pattern
  // restarts at each subpath, as SVG's does.
  pub(crate) fn walk_dashed(&self, dash: Dash, pen: &mut impl Pen) {
    for subpath in self.flattened_subpaths() {
      walk_dashes(subpath.into_iter(), dash, pen);
    }
  }

  fn dashed_path(&self, dash: Dash) -> ImpPath {
    let mut out = PathBuilder::default();
    self.walk_dashed(dash, &mut out);
    out.take_path_new(FillType::NonZero)
  }

  // A box-relative gradient resolves against the path's bounding box.
  fn paint_in_bounds(&self) -> crate::impellers::Paint {
    match *self.bounds.borrow() {
      Some(rect) => self.paint.to_paint_in(&rect),
      None => self.paint.to_paint(),
    }
  }

  pub fn initial_style() -> taffy::Style {
    taffy::Style { display: taffy::Display::Block, ..Default::default() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Path(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Path(self))
  }
}

impl Buildable for Path {
  fn build<'a>(&'a self, _ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    self.ensure_built();
    let path = self.path.borrow();
    let Some(path) = path.as_ref() else { return };
    let (dx, dy) = (self.x.unwrap_or(0.0), self.y.unwrap_or(0.0));
    let translated = dx != 0.0 || dy != 0.0;
    if translated {
      builder.save();
      builder.translate(dx, dy);
    }
    match self.dash().filter(|_| self.strokes()) {
      // Dashing is a stroke property: the fill keeps the true curve, the
      // stroke walks the flattened one.
      Some(dash) => {
        if self.fills() {
          let mut fill = self.paint_in_bounds();
          fill.set_draw_style(DrawStyle::Fill);
          builder.draw_path(path, &fill);
        }
        let mut stroke = self.paint_in_bounds();
        stroke.set_draw_style(DrawStyle::Stroke);
        builder.draw_path(&self.dashed_path(dash), &stroke);
      }
      None => {
        builder.draw_path(path, &self.paint_in_bounds());
      }
    }
    if translated {
      builder.restore();
    }
  }
}

// The painted box: the geometry's extent plus the stroke's reach, at the
// draw-time offset. Nothing drawn (no `d`) is an empty rect at the origin.
impl Bounded for Path {
  fn local_bounds(&self, _fallback: Size) -> Rect {
    self.ensure_built();
    let Some(rect) = *self.bounds.borrow() else {
      return Rect::zero();
    };
    let StrokeShape { capped, joined } = self.shape.get();
    // Every dash has open ends, whatever the subpaths do.
    let capped = capped || self.dash().is_some();
    let outset = self.paint.stroke_outset(capped, joined);
    let origin = Point::new(rect.origin.x + self.x.unwrap_or(0.0), rect.origin.y + self.y.unwrap_or(0.0));
    Rect::new(origin, rect.size).inflate(outset, outset)
  }
}

impl Measurable for Path {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size::new(w, h);
    }
    self.ensure_built();
    let bounds = self.bounds.borrow();
    let Some(rect) = *bounds else {
      return Size::zero();
    };
    Size::new(ctx.known.width.unwrap_or(rect.size.width), ctx.known.height.unwrap_or(rect.size.height))
  }
}

impl Hittable for Path {
  fn is_in_bounds(&self, pt: Point, _ctx: &HitContext) -> bool {
    // The cached geometry is offset-independent; undo the draw-time translate.
    let pt = Point::new(pt.x - self.x.unwrap_or(0.0), pt.y - self.y.unwrap_or(0.0));
    self.ensure_built();
    let bounds = self.bounds.borrow();
    let Some(rect) = *bounds else {
      return false;
    };
    let (x, y, w, h) = (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);

    let half_stroke = self.paint.stroke_width / 2.0;
    if pt.x < x - half_stroke || pt.x > x + w + half_stroke || pt.y < y - half_stroke || pt.y > y + h + half_stroke {
      return false;
    }

    let lyon_path = self.lyon_path.borrow();
    let Some(ref path) = *lyon_path else {
      return false;
    };
    let test_pt = point(pt.x, pt.y);

    let lyon_fill_rule = match self.fill_rule {
      FillType::Odd => lyon_path::FillRule::EvenOdd,
      _ => lyon_path::FillRule::NonZero,
    };

    match self.paint.draw_style {
      DrawStyle::Fill => hit_test_path(&test_pt, path.iter(), lyon_fill_rule, 0.1),
      DrawStyle::Stroke => point_near_path(&test_pt, path, half_stroke),
      DrawStyle::StrokeAndFill => {
        hit_test_path(&test_pt, path.iter(), lyon_fill_rule, 0.1) || point_near_path(&test_pt, path, half_stroke)
      }
    }
  }
}

/// Test if a point is within `max_dist` of any segment in the path.
/// Uses flattening + point-to-segment distance instead of tessellating the stroke
/// into a filled outline, to avoid pulling in lyon_tessellation and the associated
/// memory allocation for the stroke mesh.
fn point_near_path(pt: &lyon_path::geom::Point<f32>, path: &lyon_path::Path, max_dist: f32) -> bool {
  let max_dist_sq = max_dist * max_dist;
  let mut last = point(0.0, 0.0);

  for evt in path.iter().flattened(0.5) {
    match evt {
      lyon_path::Event::Begin { at } => {
        last = at;
      }
      lyon_path::Event::Line { from: _, to } => {
        if dist_sq_to_segment(pt, &last, &to) <= max_dist_sq {
          return true;
        }
        last = to;
      }
      lyon_path::Event::End { last: end, first, close } => {
        if close && dist_sq_to_segment(pt, &end, &first) <= max_dist_sq {
          return true;
        }
      }
      _ => {}
    }
  }
  false
}

fn dist_sq_to_segment(
  p: &lyon_path::geom::Point<f32>,
  a: &lyon_path::geom::Point<f32>,
  b: &lyon_path::geom::Point<f32>,
) -> f32 {
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
