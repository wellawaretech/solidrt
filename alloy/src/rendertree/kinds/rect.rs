use super::dash::{box_outline, dashed_path, walked_length, Dash, Piece};
use super::PaintState;
use crate::impellers::{DisplayListBuilder, DrawStyle, Paint, Point, Rect, RoundingRadii, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};

#[derive(Clone, Debug, Default)]
pub struct Rectangle {
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  // [top-left, top-right, bottom-right, bottom-left], CSS border-radius order.
  pub radius: Option<[f32; 4]>,
  pub on_length: Option<f32>,
  pub off_length: Option<f32>,
  pub dash_offset: Option<f32>,
  pub path_length: Option<f32>,
  pub paint: PaintState,
}

fn draw(builder: &mut DisplayListBuilder, path: &Rect, radii: Option<[f32; 4]>, paint: &Paint) {
  match radii {
    Some([tl, tr, br, bl]) => {
      let corner = |r: f32| Point::new(r, r);
      let radii = RoundingRadii {
        top_left: corner(tl),
        top_right: corner(tr),
        bottom_right: corner(br),
        bottom_left: corner(bl),
      };
      builder.draw_rounded_rect(path, &radii, paint);
    }
    None => {
      builder.draw_rect(path, paint);
    }
  }
}

impl Buildable for Rectangle {
  // A stroke paints inside the box, like a CSS border: the drawn path is the
  // box inset by half the stroke width, so the stroke's outer edge lands on
  // the box edge instead of straddling it, and the radii shrink by the same
  // inset to keep the stroke parallel to the box corner. A centered stroke
  // bleeds half its width past the box, which any clip (a scroll viewport, a
  // repaint boundary) then cuts to a hairline on the sides it bounds, and
  // which makes `local_bounds` understate the painted area. Path and line
  // strokes stay centered - there the geometry is the stroke, not a box.
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let rect = self.geometry(ctx.size);
    let (path, radii) = self.stroke_path(ctx.size);
    match self.dashed_outline(ctx.size) {
      // Dashing is a stroke property: the fill keeps the whole inset shape,
      // the stroke gets its dashed pieces. Both paints are built from the
      // authored box, so a box-relative gradient stays anchored to the
      // element rather than to the inset stroke path.
      Some((outline, dash)) => {
        if self.fills() {
          let mut fill = self.paint.to_paint_in(&rect);
          fill.set_draw_style(DrawStyle::Fill);
          draw(builder, &path, radii, &fill);
        }
        let mut stroke = self.paint.to_paint_in(&rect);
        stroke.set_draw_style(DrawStyle::Stroke);
        builder.draw_path(&dashed_path(outline.into_iter(), dash), &stroke);
      }
      None => draw(builder, &path, radii, &self.paint.to_paint_in(&rect)),
    }
  }
}

impl Rectangle {
  fn fills(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Fill | DrawStyle::StrokeAndFill)
  }

  fn strokes(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Stroke | DrawStyle::StrokeAndFill)
  }

  // The authored box, its x/y/w/h resolved against the layout size.
  fn geometry(&self, size: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(size.width), self.h.unwrap_or(size.height)),
    )
  }

  // The drawn shape: the box inset by the stroke inset, its radii shrunk by
  // the same amount to keep the stroke parallel to the box corner.
  fn stroke_path(&self, size: Size) -> (Rect, Option<[f32; 4]>) {
    let rect = self.geometry(size);
    let d = self.paint.stroke_inset(rect.size.width, rect.size.height);
    let path = Rect::new(
      Point::new(rect.origin.x + d, rect.origin.y + d),
      Size::new(rect.size.width - d * 2.0, rect.size.height - d * 2.0),
    );
    (path, self.radius.map(|radii| radii.map(|r| (r - d).max(0.0))))
  }

  // The dashed stroke, when the paint strokes and dashes: the inset outline
  // as walker pieces, and the pattern in local units, a declared
  // `pathLength` (SVG) mapping the author's units onto the outline's length.
  // A dash's cap reaches along the outline by half the stroke width, which
  // is exactly the inset, so nothing paints past the box.
  pub(crate) fn dashed_outline(&self, size: Size) -> Option<(Vec<Piece>, Dash)> {
    if !self.strokes() {
      return None;
    }
    let dash = Dash::new(self.on_length, self.off_length, self.dash_offset)?;
    let (path, radii) = self.stroke_path(size);
    let outline = box_outline(path, radii.unwrap_or([0.0; 4]));
    let dash = match self.path_length.filter(|declared| *declared > 0.0) {
      Some(declared) => dash.scaled(walked_length(outline.iter().cloned()) / declared)?,
      None => dash,
    };
    Some((outline, dash))
  }
}

// A rectangle has no intrinsic size: a layout rect is sized by the width/height
// layout props (w/h are detached-only geometry and never reach taffy).
impl Measurable for Rectangle {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    Size::new(ctx.known.width.unwrap_or(0.0), ctx.known.height.unwrap_or(0.0))
  }
}

impl Bounded for Rectangle {
  fn local_bounds(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }
}

impl Rectangle {
  // Rectangle geometry is painted within its layout box, so none of these
  // affect layout. None resets to unset (x/y to 0, w/h to fill the box).
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_w(&mut self, v: Option<f32>) -> Damage {
    self.w = v;
    Damage::Paint
  }
  pub fn set_h(&mut self, v: Option<f32>) -> Damage {
    self.h = v;
    Damage::Paint
  }
  // [top-left, top-right, bottom-right, bottom-left].
  pub fn set_radius(&mut self, radius: Option<[f32; 4]>) -> Damage {
    self.radius = radius;
    Damage::Paint
  }
  // The dash props are paint-only: the outline is walked again at build.
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
    Element::with_layout(ElementKind::Rectangle(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Rectangle(self))
  }
}

impl Hittable for Rectangle {
  fn is_in_bounds(&self, point: Point, ctx: &HitContext) -> bool {
    let rx = self.x.unwrap_or(0.0);
    let ry = self.y.unwrap_or(0.0);
    let rw = self.w.unwrap_or(ctx.size.width);
    let rh = self.h.unwrap_or(ctx.size.height);
    let [tl, tr, br, bl] = self.radius.unwrap_or([0.0; 4]);

    // Strokes paint inside the box (see `build`), so the box is the outer edge
    // for every draw style; only a plain stroke has a hole, one stroke width in.
    match self.paint.draw_style {
      DrawStyle::Fill | DrawStyle::StrokeAndFill => in_rounded_rect(point, rx, ry, rw, rh, [tl, tr, br, bl]),
      DrawStyle::Stroke => {
        let sw = self.paint.stroke_width.max(0.0);
        let inner = [(tl - sw).max(0.0), (tr - sw).max(0.0), (br - sw).max(0.0), (bl - sw).max(0.0)];
        let in_outer = in_rounded_rect(point, rx, ry, rw, rh, [tl, tr, br, bl]);
        let in_inner =
          in_rounded_rect(point, rx + sw, ry + sw, (rw - sw * 2.0).max(0.0), (rh - sw * 2.0).max(0.0), inner);
        in_outer && !in_inner
      }
    }
  }
}

/// Test if a point is inside a rounded rectangle with per-corner radii.
/// Radii are [top-left, top-right, bottom-right, bottom-left].
/// When all radii are 0 this reduces to a plain AABB check.
fn in_rounded_rect(point: Point, rx: f32, ry: f32, rw: f32, rh: f32, radii: [f32; 4]) -> bool {
  if point.x < rx || point.x >= rx + rw || point.y < ry || point.y >= ry + rh {
    return false;
  }
  let max_r = (rw / 2.0).min(rh / 2.0);
  let tl = radii[0].min(max_r).max(0.0);
  let tr = radii[1].min(max_r).max(0.0);
  let br = radii[2].min(max_r).max(0.0);
  let bl = radii[3].min(max_r).max(0.0);

  // Determine which corner region the point is in, if any.
  let (cx, cy, r) = if point.x < rx + tl && point.y < ry + tl {
    (rx + tl, ry + tl, tl)
  } else if point.x >= rx + rw - tr && point.y < ry + tr {
    (rx + rw - tr, ry + tr, tr)
  } else if point.x >= rx + rw - br && point.y >= ry + rh - br {
    (rx + rw - br, ry + rh - br, br)
  } else if point.x < rx + bl && point.y >= ry + rh - bl {
    (rx + bl, ry + rh - bl, bl)
  } else {
    return true;
  };

  if r <= 0.0 {
    return true;
  }
  let dx = point.x - cx;
  let dy = point.y - cy;
  dx * dx + dy * dy <= r * r
}
