use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::{
  BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext, PropValue, XY,
};
use alloy::impellers::{DisplayListBuilder, Paint, Point, Rect, Size as ISize, TextureSampling};
use taffy::{AlignSelf, Display, Size as TaffySize, Style};

#[derive(Clone, Debug, Default)]
pub struct Texture {
  pub texture_id: Option<u64>,
  pub src_x: Option<f32>,
  pub src_y: Option<f32>,
  pub src_w: Option<f32>,
  pub src_h: Option<f32>,
}

impl Texture {
  fn source_rect(&self, tex_w: u32, tex_h: u32) -> Rect {
    Rect::new(
      Point::new(self.src_x.unwrap_or(0.0), self.src_y.unwrap_or(0.0)),
      ISize::new(
        self.src_w.unwrap_or(tex_w as f32),
        self.src_h.unwrap_or(tex_h as f32),
      ),
    )
  }
}

impl Buildable for Texture {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let Some(tex_id) = self.texture_id else { return };
    let Some(entry) = ctx.alloy.textures.get(tex_id) else {
      log::warn!("[texture] build: tex_id={} not in registry", tex_id);
      return;
    };

    let src_rect = self.source_rect(entry.width(), entry.height());
    let dst_rect = Rect::new(Point::new(0.0, 0.0), ISize::new(ctx.size.w, ctx.size.h));
    let paint = Paint::default();
    builder.draw_texture_rect(
      &entry.impeller,
      &src_rect,
      &dst_rect,
      TextureSampling::Linear,
      Some(&paint),
    );
  }
}

impl Hittable for Texture {
  fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
    point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h
  }
}

// HTML <img> sizing rules:
//   neither known  -> intrinsic w/h
//   one known      -> derive other from intrinsic aspect ratio
//   both known     -> honor both (explicit override)
// Intrinsic size honors src_* crop when set, else falls back to texture dims.
//TODO object-fit (cover/contain). Currently always stretches to the layout box.
impl Measurable for Texture {
  fn measure(&self, ctx: &MeasureContext) -> TaffySize<f32> {
    let (tex_w, tex_h) = self
      .texture_id
      .and_then(|id| ctx.alloy.textures.get(id).map(|e| (e.width() as f32, e.height() as f32)))
      .unwrap_or((0.0, 0.0));
    let iw = self.src_w.unwrap_or(tex_w);
    let ih = self.src_h.unwrap_or(tex_h);
    match (ctx.known.width, ctx.known.height) {
      (Some(w), Some(h)) => TaffySize { width: w, height: h },
      (Some(w), None) => {
        let h = if iw > 0.0 { w * ih / iw } else { ih };
        TaffySize { width: w, height: h }
      }
      (None, Some(h)) => {
        let w = if ih > 0.0 { h * iw / ih } else { iw };
        TaffySize { width: w, height: h }
      }
      (None, None) => TaffySize { width: iw, height: ih },
    }
  }
}

impl Texture {
  pub fn set_property(&mut self, property: &str, value: &PropValue) -> Option<bool> {
    match property {
      "src" => {
        // null/undefined clears, number sets id.
        if value.is_null() {
          self.texture_id = None;
        } else {
          let id = value.as_f64().expect("src must be a texture id (number)") as u64;
          self.texture_id = Some(id);
        }
        Some(true)
      }
      "srcX" => { self.src_x = Some(value.as_f64().expect("srcX must be a number") as f32); Some(true) }
      "srcY" => { self.src_y = Some(value.as_f64().expect("srcY must be a number") as f32); Some(true) }
      "srcW" => { self.src_w = Some(value.as_f64().expect("srcW must be a number") as f32); Some(true) }
      "srcH" => { self.src_h = Some(value.as_f64().expect("srcH must be a number") as f32); Some(true) }
      _ => None,
    }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::Texture(self),
      Style {
        display: Display::Block,
        // Replaced-element default: opt out of flex `align-items: stretch`,
        // matching HTML <img>. User can override via align-self prop.
        align_self: Some(AlignSelf::Start),
        ..Default::default()
      },
    )
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Texture(self))
  }
}