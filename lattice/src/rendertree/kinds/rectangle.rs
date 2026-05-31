use super::PaintState;
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{
  BoundingBox, Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext, XY,
};
use alloy::impellers::{DisplayListBuilder, DrawStyle, Point, Rect, RoundingRadii, Size};
use taffy::Size as TaffySize;

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
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let x = self.x.unwrap_or(0.0);
    let y = self.y.unwrap_or(0.0);
    let w = self.w.unwrap_or(ctx.size.w);
    let h = self.h.unwrap_or(ctx.size.h);

    let rect = Rect::new(Point::new(x, y), Size::new(w, h));

    if let Some([tl, tr, br, bl]) = self.radius {
      let radii = RoundingRadii {
        top_left: Point::new(tl, tl),
        top_right: Point::new(tr, tr),
        bottom_right: Point::new(br, br),
        bottom_left: Point::new(bl, bl),
      };
      let paint = self.paint.to_paint();
      builder.draw_rounded_rect(&rect, &radii, &paint);
    } else {
      let paint = self.paint.to_paint();
      builder.draw_rect(&rect, &paint);
    }
  }
}

impl Measurable for Rectangle {
  fn measure(&self, ctx: &MeasureContext) -> TaffySize<f32> {
    TaffySize {
      width: ctx.known.width.unwrap_or(self.w.unwrap_or(0.0)),
      height: ctx.known.height.unwrap_or(self.h.unwrap_or(0.0)),
    }
  }
}

impl Bounded for Rectangle {
  fn local_bounds(&self, fallback: TaffySize<f32>) -> BoundingBox {
    BoundingBox {
      x: self.x.unwrap_or(0.0),
      y: self.y.unwrap_or(0.0),
      width: self.w.unwrap_or(fallback.width),
      height: self.h.unwrap_or(fallback.height),
    }
  }
}

impl Rectangle {
  // Rectangle geometry is painted within its layout box, so none of these
  // affect layout.
  pub fn set_x(&mut self, v: f32) -> bool { self.x = Some(v); false }
  pub fn set_y(&mut self, v: f32) -> bool { self.y = Some(v); false }
  pub fn set_w(&mut self, v: f32) -> bool { self.w = Some(v); false }
  pub fn set_h(&mut self, v: f32) -> bool { self.h = Some(v); false }
  // [top-left, top-right, bottom-right, bottom-left].
  pub fn set_radius(&mut self, radius: [f32; 4]) -> bool { self.radius = Some(radius); false }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::Rectangle(self),
      taffy::Style {
        display: taffy::Display::Block,
        ..Default::default()
      },
    )
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Rectangle(self))
  }
}

impl Hittable for Rectangle {
  fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
    let rx = self.x.unwrap_or(0.0);
    let ry = self.y.unwrap_or(0.0);
    let rw = self.w.unwrap_or(ctx.size.w);
    let rh = self.h.unwrap_or(ctx.size.h);
    let half_sw = self.paint.stroke_width / 2.0;
    let [tl, tr, br, bl] = self.radius.unwrap_or([0.0; 4]);

    match self.paint.draw_style {
      DrawStyle::Fill => in_rounded_rect(point, rx, ry, rw, rh, [tl, tr, br, bl]),
      DrawStyle::Stroke => {
        let outer = [tl + half_sw, tr + half_sw, br + half_sw, bl + half_sw];
        let inner = [
          (tl - half_sw).max(0.0),
          (tr - half_sw).max(0.0),
          (br - half_sw).max(0.0),
          (bl - half_sw).max(0.0),
        ];
        let in_outer = in_rounded_rect(
          point,
          rx - half_sw,
          ry - half_sw,
          rw + half_sw * 2.0,
          rh + half_sw * 2.0,
          outer,
        );
        let in_inner = in_rounded_rect(
          point,
          rx + half_sw,
          ry + half_sw,
          rw - half_sw * 2.0,
          rh - half_sw * 2.0,
          inner,
        );
        in_outer && !in_inner
      }
      DrawStyle::StrokeAndFill => {
        let outer = [tl + half_sw, tr + half_sw, br + half_sw, bl + half_sw];
        in_rounded_rect(
          point,
          rx - half_sw,
          ry - half_sw,
          rw + half_sw * 2.0,
          rh + half_sw * 2.0,
          outer,
        )
      }
    }
  }
}

/// Test if a point is inside a rounded rectangle with per-corner radii.
/// Radii are [top-left, top-right, bottom-right, bottom-left].
/// When all radii are 0 this reduces to a plain AABB check.
fn in_rounded_rect(point: XY, rx: f32, ry: f32, rw: f32, rh: f32, radii: [f32; 4]) -> bool {
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
