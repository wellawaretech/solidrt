use super::PaintState;
use crate::impellers::{DisplayListBuilder, Point, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};

// Endpoints default to spanning the box: (0,0) to (box.w, box.h), matching how
// a rect with unset w/h fills its box. Explicit endpoints are detached-only.
#[derive(Clone, Debug, Default)]
pub struct Line {
  pub x1: Option<f32>,
  pub y1: Option<f32>,
  pub x2: Option<f32>,
  pub y2: Option<f32>,
  pub on_length: Option<f32>,
  pub off_length: Option<f32>,
  pub paint: PaintState,
}

impl Line {
  fn endpoints(&self, box_w: f32, box_h: f32) -> (Point, Point) {
    (
      Point::new(self.x1.unwrap_or(0.0), self.y1.unwrap_or(0.0)),
      Point::new(self.x2.unwrap_or(box_w), self.y2.unwrap_or(box_h)),
    )
  }
}

impl Buildable for Line {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let (from, to) = self.endpoints(ctx.size.width, ctx.size.height);
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

// A line has no intrinsic size: a layout line is sized by the width/height
// layout props (endpoints are detached-only geometry and never reach taffy).
impl Measurable for Line {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    Size::new(ctx.known.width.unwrap_or(0.0), ctx.known.height.unwrap_or(0.0))
  }
}

impl Line {
  pub fn set_x1(&mut self, v: f32) -> Damage {
    self.x1 = Some(v);
    Damage::Paint
  }
  pub fn set_y1(&mut self, v: f32) -> Damage {
    self.y1 = Some(v);
    Damage::Paint
  }
  pub fn set_x2(&mut self, v: f32) -> Damage {
    self.x2 = Some(v);
    Damage::Paint
  }
  pub fn set_y2(&mut self, v: f32) -> Damage {
    self.y2 = Some(v);
    Damage::Paint
  }
  pub fn set_on_length(&mut self, v: f32) -> Damage {
    self.on_length = Some(v);
    Damage::Paint
  }
  pub fn set_off_length(&mut self, v: f32) -> Damage {
    self.off_length = Some(v);
    Damage::Paint
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Line(self), taffy::Style { display: taffy::Display::Block, ..Default::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Line(self))
  }
}

impl Hittable for Line {
  fn is_in_bounds(&self, pt: Point, ctx: &HitContext) -> bool {
    let (from, to) = self.endpoints(ctx.size.width, ctx.size.height);
    let half_sw = (self.paint.stroke_width / 2.0).max(2.0);
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len_sq = dx * dx + dy * dy;
    let dist_sq = if len_sq == 0.0 {
      let ex = pt.x - from.x;
      let ey = pt.y - from.y;
      ex * ex + ey * ey
    } else {
      let t = ((pt.x - from.x) * dx + (pt.y - from.y) * dy) / len_sq;
      let t = t.clamp(0.0, 1.0);
      let proj_x = from.x + t * dx;
      let proj_y = from.y + t * dy;
      let ex = pt.x - proj_x;
      let ey = pt.y - proj_y;
      ex * ex + ey * ey
    };
    dist_sq <= half_sw * half_sw
  }
}
