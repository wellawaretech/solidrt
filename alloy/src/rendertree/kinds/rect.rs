use super::PaintState;
use crate::impellers::{DisplayListBuilder, DrawStyle, Point, Rect, RoundingRadii, Size};
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
  pub paint: PaintState,
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
    let x = self.x.unwrap_or(0.0);
    let y = self.y.unwrap_or(0.0);
    let w = self.w.unwrap_or(ctx.size.width);
    let h = self.h.unwrap_or(ctx.size.height);

    let rect = Rect::new(Point::new(x, y), Size::new(w, h));
    // Built from the authored box, so a box-relative gradient stays anchored
    // to the element rather than to the inset stroke path.
    let paint = self.paint.to_paint_in(&rect);

    let d = self.paint.stroke_inset(w, h);
    let path = Rect::new(Point::new(x + d, y + d), Size::new(w - d * 2.0, h - d * 2.0));

    if let Some([tl, tr, br, bl]) = self.radius {
      let inner = |r: f32| Point::new((r - d).max(0.0), (r - d).max(0.0));
      let radii = RoundingRadii {
        top_left: inner(tl),
        top_right: inner(tr),
        bottom_right: inner(br),
        bottom_left: inner(bl),
      };
      builder.draw_rounded_rect(&path, &radii, &paint);
    } else {
      builder.draw_rect(&path, &paint);
    }
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
