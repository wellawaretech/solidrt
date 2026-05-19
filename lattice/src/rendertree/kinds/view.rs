use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{BuildContext, Buildable, Element, ElementKind, WH, XY};
use rquickjs::Value;
use alloy::impellers::DisplayListBuilder;
use taffy::{FlexDirection, Style};

#[derive(Clone, Debug, Default)]
pub struct View {
  pub rotate: Option<f32>,
  pub scale: Option<f32>,
  pub pos: Option<XY>,
  pub center: Option<XY>,
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
  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "rotate" => { self.rotate = Some(value.get::<f64>().expect("rotate must be a number") as f32); Some(false) }
      "scale" => { self.scale = Some(value.get::<f64>().expect("scale must be a number") as f32); Some(false) }
      "x" => { let pos = self.pos.get_or_insert_with(XY::default); pos.x = value.get::<f64>().expect("x must be a number") as f32; Some(false) }
      "y" => { let pos = self.pos.get_or_insert_with(XY::default); pos.y = value.get::<f64>().expect("y must be a number") as f32; Some(false) }
      "cx" => { let center = self.center.get_or_insert_with(XY::default); center.x = value.get::<f64>().expect("cx must be a number") as f32; Some(false) }
      "cy" => { let center = self.center.get_or_insert_with(XY::default); center.y = value.get::<f64>().expect("cy must be a number") as f32; Some(false) }
      _ => None,
    }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::View(self),
      Style {
        flex_direction: FlexDirection::Column,
        ..Style::default()
      },
    )
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::View(self))
  }
}
