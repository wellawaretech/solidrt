use super::PaintState;
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{
  BuildContext, Buildable, Element, ElementKind, Measurable, PlatformContext, XY,
};
use alloy::impellers::{
  DisplayListBuilder, DrawStyle, FillType, Path as ImpPath, PathBuilder, Point,
};
use lyon_algorithms::hit_test::hit_test_path;
use lyon_path::geom::{point, vector, Angle, ArcFlags, CubicBezierSegment, SvgArc};
use lyon_path::iterator::PathIterator;
use rquickjs::Value;
use std::cell::RefCell;
use svgtypes::{PathParser, PathSegment};
use taffy::{AvailableSpace, Size as TaffySize};

pub struct Path {
  pub d: String,
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub paint: PaintState,
  pub fill_rule: FillType,
  path: RefCell<Option<ImpPath>>,
  bounds: RefCell<Option<(f32, f32, f32, f32)>>,
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
      path: RefCell::new(None),
      bounds: RefCell::new(None),
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
      path: RefCell::new(None),
      bounds: RefCell::new(None),
      lyon_path: RefCell::new(None),
    }
  }
}

impl std::fmt::Debug for Path {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Path")
      .field("d", &self.d)
      .field("x", &self.x)
      .field("y", &self.y)
      .finish()
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

    let offset_x = self.x.unwrap_or(0.0);
    let offset_y = self.y.unwrap_or(0.0);

    let mut path_builder = PathBuilder::default();
    let mut lyon_builder = lyon_path::Path::builder();
    let mut cursor = (offset_x, offset_y);
    let mut subpath_start = cursor;
    let mut bb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);

    let resolve = |abs: bool, x: f64, y: f64, cursor: &(f32, f32)| -> Point {
      if abs {
        Point::new(offset_x + x as f32, offset_y + y as f32)
      } else {
        Point::new(cursor.0 + x as f32, cursor.1 + y as f32)
      }
    };

    let include = |bb: &mut (f32, f32, f32, f32), pt: &Point| {
      if pt.x < bb.0 {
        bb.0 = pt.x;
      }
      if pt.y < bb.1 {
        bb.1 = pt.y;
      }
      if pt.x > bb.2 {
        bb.2 = pt.x;
      }
      if pt.y > bb.3 {
        bb.3 = pt.y;
      }
    };

    let mut lyon_open = false;
    let ensure_lyon_begun =
      |lb: &mut lyon_path::path::Builder, cx: f32, cy: f32, open: &mut bool| {
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
          include(&mut bb, &pt);
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
          include(&mut bb, &pt);
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
          include(&mut bb, &pt);
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
          include(&mut bb, &pt);
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
          include(&mut bb, &cp1);
          include(&mut bb, &cp2);
          include(&mut bb, &end);
          path_builder.cubic_curve_to(cp1, cp2, end);
          lyon_builder.cubic_bezier_to(
            point(cp1.x, cp1.y),
            point(cp2.x, cp2.y),
            point(end.x, end.y),
          );
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
          include(&mut bb, &cp1);
          include(&mut bb, &cp2);
          include(&mut bb, &end);
          path_builder.cubic_curve_to(cp1, cp2, end);
          lyon_builder.cubic_bezier_to(
            point(cp1.x, cp1.y),
            point(cp2.x, cp2.y),
            point(end.x, end.y),
          );
          cursor = (end.x, end.y);
          last_cubic_cp2 = Some(cp2);
          last_quad_cp = None;
        }
        PathSegment::Quadratic { abs, x1, y1, x, y } => {
          ensure_lyon_begun(&mut lyon_builder, cursor.0, cursor.1, &mut lyon_open);
          let cp = resolve(abs, x1, y1, &cursor);
          let end = resolve(abs, x, y, &cursor);
          include(&mut bb, &cp);
          include(&mut bb, &end);
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
          include(&mut bb, &cp);
          include(&mut bb, &end);
          path_builder.quadratic_curve_to(cp, end);
          lyon_builder.quadratic_bezier_to(point(cp.x, cp.y), point(end.x, end.y));
          cursor = (end.x, end.y);
          last_quad_cp = Some(cp);
          last_cubic_cp2 = None;
        }
        PathSegment::EllipticalArc {
          abs,
          rx,
          ry,
          x_axis_rotation,
          large_arc,
          sweep,
          x,
          y,
        } => {
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
            include(&mut bb, &cp1);
            include(&mut bb, &cp2);
            include(&mut bb, &end_pt);
            path_builder.cubic_curve_to(cp1, cp2, end_pt);
            lyon_builder.cubic_bezier_to(
              point(cp1.x, cp1.y),
              point(cp2.x, cp2.y),
              point(end_pt.x, end_pt.y),
            );
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

    *self.path.borrow_mut() = Some(path_builder.take_path_new(self.fill_rule));
    *self.lyon_path.borrow_mut() = Some(lyon_builder.build());

    if bb.0 <= bb.2 {
      *self.bounds.borrow_mut() = Some((bb.0, bb.1, bb.2 - bb.0, bb.3 - bb.1));
    }
  }

  pub fn invalidate(&self) {
    *self.path.borrow_mut() = None;
    *self.bounds.borrow_mut() = None;
    *self.lyon_path.borrow_mut() = None;
  }

  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "d" => {
        self.d = value.get::<String>().expect("d must be a string");
        self.invalidate();
        Some(true)
      }
      "x" => {
        self.x = Some(value.get::<f64>().expect("x must be a number") as f32);
        self.invalidate();
        Some(true)
      }
      "y" => {
        self.y = Some(value.get::<f64>().expect("y must be a number") as f32);
        self.invalidate();
        Some(true)
      }
      "fillRule" => {
        self.fill_rule = match value.get::<String>().expect("fillRule must be a string").as_str() {
          "nonZero" => FillType::NonZero,
          "evenOdd" => FillType::Odd,
          v => panic!("unknown fillRule '{v}'"),
        };
        self.invalidate();
        Some(false)
      }
      _ => None,
    }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::Path(self),
      taffy::Style {
        display: taffy::Display::Block,
        ..Default::default()
      },
    )
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
    let paint = self.paint.to_paint();
    builder.draw_path(path, &paint);
  }
}

impl Measurable for Path {
  fn measure(
    &self,
    known_dimensions: TaffySize<Option<f32>>,
    _available_space: TaffySize<AvailableSpace>,
    _platform: &PlatformContext,
  ) -> TaffySize<f32> {
    if let (Some(w), Some(h)) = (known_dimensions.width, known_dimensions.height) {
      return TaffySize { width: w, height: h };
    }
    self.ensure_built();
    let bounds = self.bounds.borrow();
    let Some((_, _, w, h)) = *bounds else { return TaffySize::ZERO };
    TaffySize {
      width: known_dimensions.width.unwrap_or(w),
      height: known_dimensions.height.unwrap_or(h),
    }
  }
}

impl Hittable for Path {
  fn is_in_bounds(&self, pt: XY, _ctx: &HitContext) -> bool {
    self.ensure_built();
    let bounds = self.bounds.borrow();
    let Some((x, y, w, h)) = *bounds else { return false };

    let half_stroke = self.paint.stroke_width / 2.0;
    if pt.x < x - half_stroke
      || pt.x > x + w + half_stroke
      || pt.y < y - half_stroke
      || pt.y > y + h + half_stroke
    {
      return false;
    }

    let lyon_path = self.lyon_path.borrow();
    let Some(ref path) = *lyon_path else { return false };
    let test_pt = point(pt.x, pt.y);

    let lyon_fill_rule = match self.fill_rule {
      FillType::Odd => lyon_path::FillRule::EvenOdd,
      _ => lyon_path::FillRule::NonZero,
    };

    match self.paint.draw_style {
      DrawStyle::Fill => hit_test_path(&test_pt, path.iter(), lyon_fill_rule, 0.1),
      DrawStyle::Stroke => point_near_path(&test_pt, path, half_stroke),
      DrawStyle::StrokeAndFill => {
        hit_test_path(&test_pt, path.iter(), lyon_fill_rule, 0.1)
          || point_near_path(&test_pt, path, half_stroke)
      }
    }
  }
}

/// Test if a point is within `max_dist` of any segment in the path.
/// Uses flattening + point-to-segment distance instead of tessellating the stroke
/// into a filled outline, to avoid pulling in lyon_tessellation and the associated
/// memory allocation for the stroke mesh.
fn point_near_path(
  pt: &lyon_path::geom::Point<f32>,
  path: &lyon_path::Path,
  max_dist: f32,
) -> bool {
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