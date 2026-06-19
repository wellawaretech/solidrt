use super::PaintState;
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext, XY};
use crate::impellers::{DisplayListBuilder, Point};
use taffy::Size as TaffySize;

#[derive(Clone, Debug, Default)]
pub struct Line {
  pub x1: f32,
  pub y1: f32,
  pub x2: f32,
  pub y2: f32,
  pub on_length: Option<f32>,
  pub off_length: Option<f32>,
  pub paint: PaintState,
}

impl Buildable for Line {
  fn build<'a>(&'a self, _ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let from = Point::new(self.x1, self.y1);
    let to = Point::new(self.x2, self.y2);
    let paint = self.paint.to_paint();
    match (self.on_length, self.off_length) {
      (Some(on), Some(off)) => {
        builder.draw_dashed_line(from, to, on, off, &paint);
      }
      _ => {
        builder.draw_line(from, to, &paint);
      }
    }
  }
}

impl Measurable for Line {
  fn measure(&self, ctx: &MeasureContext) -> TaffySize<f32> {
    TaffySize {
      width: ctx.known.width.unwrap_or((self.x2 - self.x1).abs()),
      height: ctx.known.height.unwrap_or((self.y2 - self.y1).abs()),
    }
  }
}

impl Line {
  pub fn set_x1(&mut self, v: f32) -> bool {
    self.x1 = v;
    false
  }
  pub fn set_y1(&mut self, v: f32) -> bool {
    self.y1 = v;
    false
  }
  pub fn set_x2(&mut self, v: f32) -> bool {
    self.x2 = v;
    false
  }
  pub fn set_y2(&mut self, v: f32) -> bool {
    self.y2 = v;
    false
  }
  pub fn set_on_length(&mut self, v: f32) -> bool {
    self.on_length = Some(v);
    false
  }
  pub fn set_off_length(&mut self, v: f32) -> bool {
    self.off_length = Some(v);
    false
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Line(self), taffy::Style { display: taffy::Display::Block, ..Default::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Line(self))
  }
}

impl Hittable for Line {
  fn is_in_bounds(&self, pt: XY, _ctx: &HitContext) -> bool {
    let half_sw = (self.paint.stroke_width / 2.0).max(2.0);
    let dx = self.x2 - self.x1;
    let dy = self.y2 - self.y1;
    let len_sq = dx * dx + dy * dy;
    let dist_sq = if len_sq == 0.0 {
      let ex = pt.x - self.x1;
      let ey = pt.y - self.y1;
      ex * ex + ey * ey
    } else {
      let t = ((pt.x - self.x1) * dx + (pt.y - self.y1) * dy) / len_sq;
      let t = t.clamp(0.0, 1.0);
      let proj_x = self.x1 + t * dx;
      let proj_y = self.y1 + t * dy;
      let ex = pt.x - proj_x;
      let ey = pt.y - proj_y;
      ex * ex + ey * ey
    };
    dist_sq <= half_sw * half_sw
  }
}
