use super::PaintState;
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{
  BuildContext, Buildable, Element, ElementKind, Measurable, PlatformContext, XY,
};
use alloy::impellers::{DisplayListBuilder, DrawStyle, Point, Rect, RoundingRadii, Size};
use rquickjs::Value;
use taffy::{AvailableSpace, Size as TaffySize};

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
  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "x" => { self.x = Some(value.get::<f64>().expect("x must be a number") as f32); Some(false) }
      "y" => { self.y = Some(value.get::<f64>().expect("y must be a number") as f32); Some(false) }
      "w" => { self.w = Some(value.get::<f64>().expect("w must be a number") as f32); Some(false) }
      "h" => { self.h = Some(value.get::<f64>().expect("h must be a number") as f32); Some(false) }
      "radius" => {
        if let Some(arr) = value.as_array() {
          if arr.len() != 4 {
            panic!("radius array must have 4 elements [top-left, top-right, bottom-right, bottom-left]");
          }
          let tl = arr.get::<f64>(0).expect("radius[0] must be a number") as f32;
          let tr = arr.get::<f64>(1).expect("radius[1] must be a number") as f32;
          let br = arr.get::<f64>(2).expect("radius[2] must be a number") as f32;
          let bl = arr.get::<f64>(3).expect("radius[3] must be a number") as f32;
          self.radius = Some([tl, tr, br, bl]);
        } else {
          let v = value.get::<f64>().expect("radius must be a number or an array of 4 numbers") as f32;
          self.radius = Some([v, v, v, v]);
        }
        Some(false)
      }
      _ => None,
    }
  }

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
