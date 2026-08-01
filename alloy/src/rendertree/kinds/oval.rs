use super::PaintState;
use crate::impellers::{DisplayListBuilder, DrawStyle, Point, Rect, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};

#[derive(Clone, Debug, Default)]
pub struct Oval {
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  pub paint: PaintState,
}

impl Buildable for Oval {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let x = self.x.unwrap_or(0.0);
    let y = self.y.unwrap_or(0.0);
    let w = self.w.unwrap_or(ctx.size.width);
    let h = self.h.unwrap_or(ctx.size.height);

    let rect = Rect::new(Point::new(x, y), Size::new(w, h));
    let paint = self.paint.to_paint_in(&rect);
    builder.draw_oval(&rect, &paint);
  }
}

// An oval has no intrinsic size: a layout oval is sized by the width/height
// layout props (w/h are detached-only geometry and never reach taffy).
impl Measurable for Oval {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    Size::new(ctx.known.width.unwrap_or(0.0), ctx.known.height.unwrap_or(0.0))
  }
}

impl Bounded for Oval {
  fn local_bounds(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }
}

impl Oval {
  pub fn set_x(&mut self, v: f32) -> Damage {
    self.x = Some(v);
    Damage::Paint
  }
  pub fn set_y(&mut self, v: f32) -> Damage {
    self.y = Some(v);
    Damage::Paint
  }
  pub fn set_w(&mut self, v: f32) -> Damage {
    self.w = Some(v);
    Damage::Paint
  }
  pub fn set_h(&mut self, v: f32) -> Damage {
    self.h = Some(v);
    Damage::Paint
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Oval(self), taffy::Style { display: taffy::Display::Block, ..Default::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Oval(self))
  }
}

impl Hittable for Oval {
  fn is_in_bounds(&self, pt: Point, ctx: &HitContext) -> bool {
    let ox = self.x.unwrap_or(0.0);
    let oy = self.y.unwrap_or(0.0);
    let ow = self.w.unwrap_or(ctx.size.width);
    let oh = self.h.unwrap_or(ctx.size.height);
    let half_sw = self.paint.stroke_width / 2.0;
    let cx = ox + ow / 2.0;
    let cy = oy + oh / 2.0;
    let dx = pt.x - cx;
    let dy = pt.y - cy;

    match self.paint.draw_style {
      DrawStyle::Fill => {
        let rx = ow / 2.0;
        let ry = oh / 2.0;
        if rx <= 0.0 || ry <= 0.0 {
          return false;
        }
        (dx / rx) * (dx / rx) + (dy / ry) * (dy / ry) <= 1.0
      }
      DrawStyle::Stroke => {
        let rx_outer = ow / 2.0 + half_sw;
        let ry_outer = oh / 2.0 + half_sw;
        let rx_inner = (ow / 2.0 - half_sw).max(0.0);
        let ry_inner = (oh / 2.0 - half_sw).max(0.0);
        if rx_outer <= 0.0 || ry_outer <= 0.0 {
          return false;
        }
        let d_outer = (dx / rx_outer) * (dx / rx_outer) + (dy / ry_outer) * (dy / ry_outer);
        if d_outer > 1.0 {
          return false;
        }
        if rx_inner <= 0.0 || ry_inner <= 0.0 {
          return true;
        }
        let d_inner = (dx / rx_inner) * (dx / rx_inner) + (dy / ry_inner) * (dy / ry_inner);
        d_inner >= 1.0
      }
      DrawStyle::StrokeAndFill => {
        let rx = ow / 2.0 + half_sw;
        let ry = oh / 2.0 + half_sw;
        if rx <= 0.0 || ry <= 0.0 {
          return false;
        }
        (dx / rx) * (dx / rx) + (dy / ry) * (dy / ry) <= 1.0
      }
    }
  }
}
