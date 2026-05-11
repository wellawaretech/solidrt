use super::PaintState;
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{
  BuildContext, Buildable, Element, ElementKind, Measurable, PlatformContext, XY,
};
use alloy::impellers::{DisplayListBuilder, DrawStyle, Point, Rect, RoundingRadii, Size};
use taffy::{AvailableSpace, Size as TaffySize};

#[derive(Clone, Debug, Default)]
pub struct Rectangle {
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  pub r: Option<f32>,
  pub paint: PaintState,
}

impl Buildable for Rectangle {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let x = self.x.unwrap_or(0.0);
    let y = self.y.unwrap_or(0.0);
    let w = self.w.unwrap_or(ctx.size.w);
    let h = self.h.unwrap_or(ctx.size.h);

    let rect = Rect::new(Point::new(x, y), Size::new(w, h));

    if let Some(r) = self.r {
      let corner = Point::new(r, r);
      let radii = RoundingRadii {
        top_left: corner,
        top_right: corner,
        bottom_left: corner,
        bottom_right: corner,
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
  fn measure(
    &self,
    known_dimensions: TaffySize<Option<f32>>,
    _available_space: TaffySize<AvailableSpace>,
    _platform: &PlatformContext,
  ) -> TaffySize<f32> {
    TaffySize {
      width: known_dimensions.width.unwrap_or(self.w.unwrap_or(0.0)),
      height: known_dimensions.height.unwrap_or(self.h.unwrap_or(0.0)),
    }
  }
}

impl Rectangle {
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
    let r = self.r.unwrap_or(0.0);

    match self.paint.draw_style {
      DrawStyle::Fill => in_rounded_rect(point, rx, ry, rw, rh, r),
      DrawStyle::Stroke => {
        let in_outer = in_rounded_rect(
          point,
          rx - half_sw,
          ry - half_sw,
          rw + half_sw * 2.0,
          rh + half_sw * 2.0,
          r + half_sw,
        );
        let in_inner = in_rounded_rect(
          point,
          rx + half_sw,
          ry + half_sw,
          rw - half_sw * 2.0,
          rh - half_sw * 2.0,
          (r - half_sw).max(0.0),
        );
        in_outer && !in_inner
      }
      DrawStyle::StrokeAndFill => in_rounded_rect(
        point,
        rx - half_sw,
        ry - half_sw,
        rw + half_sw * 2.0,
        rh + half_sw * 2.0,
        r + half_sw,
      ),
    }
  }
}

/// Test if a point is inside a rounded rectangle.
/// When r is 0 this reduces to a plain AABB check.
fn in_rounded_rect(point: XY, rx: f32, ry: f32, rw: f32, rh: f32, r: f32) -> bool {
  if point.x < rx || point.x >= rx + rw || point.y < ry || point.y >= ry + rh {
    return false;
  }
  if r <= 0.0 {
    return true;
  }
  let r = r.min(rw / 2.0).min(rh / 2.0);
  // Check each corner region
  let dx;
  let dy;
  if point.x < rx + r && point.y < ry + r {
    // top-left corner
    dx = rx + r - point.x;
    dy = ry + r - point.y;
  } else if point.x >= rx + rw - r && point.y < ry + r {
    // top-right corner
    dx = point.x - (rx + rw - r);
    dy = ry + r - point.y;
  } else if point.x < rx + r && point.y >= ry + rh - r {
    // bottom-left corner
    dx = rx + r - point.x;
    dy = point.y - (ry + rh - r);
  } else if point.x >= rx + rw - r && point.y >= ry + rh - r {
    // bottom-right corner
    dx = point.x - (rx + rw - r);
    dy = point.y - (ry + rh - r);
  } else {
    return true;
  }
  dx * dx + dy * dy <= r * r
}
