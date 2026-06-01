use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{Bounded, BoundingBox, BuildContext, Buildable, Element, ElementKind, WH, XY};
use alloy::impellers::DisplayListBuilder;
use taffy::{FlexDirection, Size, Style};

#[derive(Clone, Debug, Default)]
pub struct View {
  pub rotate: Option<f32>,
  pub scale: Option<f32>,
  pub pos: Option<XY>,
  pub center: Option<XY>,
  // Scroll offset applied to children at build time, after the clip is set.
  // Positive values shift content leftward/upward (web convention: positive
  // scrollX means scrolled "into" the content from the left).
  pub scroll: Option<XY>,
}

impl View {
  fn resolve_pos(&self) -> XY {
    self.pos.unwrap_or_default()
  }

  fn resolve_center(&self, size: WH) -> XY {
    self.center.unwrap_or(XY::new(size.w / 2.0, size.h / 2.0))
  }
}

impl Buildable for View {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let p = self.resolve_pos();
    let c = self.resolve_center(ctx.size);
    builder.translate(p.x, p.y);
    builder.translate(c.x, c.y);
    if let Some(value) = self.scale {
      builder.scale(value, value);
    }
    if let Some(value) = self.rotate {
      builder.rotate(value.to_degrees());
    }
    builder.translate(-c.x, -c.y);
  }
}

impl Bounded for View {
  fn local_bounds(&self, fallback: Size<f32>) -> BoundingBox {
    let p = self.pos.unwrap_or_default();
    BoundingBox { x: p.x, y: p.y, width: fallback.width, height: fallback.height }
  }
}

impl Hittable for View {
  fn transform_to_local(&self, point: XY, ctx: &HitContext) -> XY {
    let p = self.resolve_pos();
    let c = self.resolve_center(ctx.size);

    // Inverse of: T(pos) · T(c) · S(s) · R(θ) · T(-c)
    // = T(c) · R(-θ) · S(1/s) · T(-c) · T(-pos)

    let mut lx = point.x - p.x;
    let mut ly = point.y - p.y;

    lx -= c.x;
    ly -= c.y;

    if let Some(scale) = self.scale {
      if scale != 0.0 {
        lx /= scale;
        ly /= scale;
      }
    }

    if let Some(angle) = self.rotate {
      let cos_a = (-angle).cos();
      let sin_a = (-angle).sin();
      let rx = lx * cos_a - ly * sin_a;
      let ry = lx * sin_a + ly * cos_a;
      lx = rx;
      ly = ry;
    }

    lx += c.x;
    ly += c.y;

    XY::new(lx, ly)
  }
}

impl View {
  // Setters return whether the change affects layout. View transforms (pos,
  // scale, rotate, scroll) are paint-time only, so they never do.
  pub fn set_rotate(&mut self, v: f32) -> bool {
    self.rotate = Some(v);
    false
  }
  pub fn set_scale(&mut self, v: f32) -> bool {
    self.scale = Some(v);
    false
  }
  pub fn set_x(&mut self, v: f32) -> bool {
    self.pos.get_or_insert_with(XY::default).x = v;
    false
  }
  pub fn set_y(&mut self, v: f32) -> bool {
    self.pos.get_or_insert_with(XY::default).y = v;
    false
  }
  pub fn set_cx(&mut self, v: f32) -> bool {
    self.center.get_or_insert_with(XY::default).x = v;
    false
  }
  pub fn set_cy(&mut self, v: f32) -> bool {
    self.center.get_or_insert_with(XY::default).y = v;
    false
  }
  pub fn set_scroll_x(&mut self, v: f32) -> bool {
    self.scroll.get_or_insert_with(XY::default).x = v;
    false
  }
  pub fn set_scroll_y(&mut self, v: f32) -> bool {
    self.scroll.get_or_insert_with(XY::default).y = v;
    false
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::View(self), Style { flex_direction: FlexDirection::Column, ..Style::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::View(self))
  }
}
