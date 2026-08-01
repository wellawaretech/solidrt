use std::cell::RefCell;

use super::PaintState;
use crate::impellers::{DisplayListBuilder, Point, Rect, Size, TextureSampling};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::gpu::ParamValue;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};
use taffy::{AlignSelf, Display, Style};

// CSS object-fit semantics: how the source pixels map to the element box.
// Fill stretches (the default, like CSS); Cover/None crop; Contain/ScaleDown
// letterbox. Everything centers - there is no object-position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureFit {
  #[default]
  Fill,
  Cover,
  Contain,
  None,
  ScaleDown,
}

// Maps a source rect into a destination box per `fit`. Cropping is expressed
// by shrinking the source rect (a sub-rect of the texture is drawn to the full
// box), letterboxing by shrinking the destination rect; both center. Returns
// the rects unchanged when either rect is degenerate.
pub fn fit_rects(fit: TextureFit, src: Rect, dst: Rect) -> (Rect, Rect) {
  let (sw, sh) = (src.size.width, src.size.height);
  let (dw, dh) = (dst.size.width, dst.size.height);
  if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
    return (src, dst);
  }
  let scale = match fit {
    TextureFit::Fill => return (src, dst),
    TextureFit::Cover => (dw / sw).max(dh / sh),
    TextureFit::Contain => (dw / sw).min(dh / sh),
    TextureFit::None => 1.0,
    TextureFit::ScaleDown => (dw / sw).min(dh / sh).min(1.0),
  };
  // Visible portion of the source at this scale, centered; never larger than
  // the source itself (the axis that fits entirely keeps its full extent).
  let vw = (dw / scale).min(sw);
  let vh = (dh / scale).min(sh);
  let src_out = Rect::new(
    Point::new(src.origin.x + (sw - vw) / 2.0, src.origin.y + (sh - vh) / 2.0),
    Size::new(vw, vh),
  );
  // Destination extent of that portion, centered; never larger than the box.
  let ow = (vw * scale).min(dw);
  let oh = (vh * scale).min(dh);
  let dst_out = Rect::new(
    Point::new(dst.origin.x + (dw - ow) / 2.0, dst.origin.y + (dh - oh) / 2.0),
    Size::new(ow, oh),
  );
  (src_out, dst_out)
}

#[derive(Clone, Debug, Default)]
pub struct Texture {
  pub texture_id: Option<u64>,
  pub fit: TextureFit,
  pub src_x: Option<f32>,
  pub src_y: Option<f32>,
  pub src_w: Option<f32>,
  pub src_h: Option<f32>,
  // Paint-only geometry for the detached (`d-texture`) form, which has no
  // layout Style to draw width/height/position from. See Rectangle's x/y/w/h.
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  // Shader uniform values set since the last build. Applied (a GL render pass
  // into this texture id) and cleared the next time this element is actually
  // built, so a prop write only ever costs a field write - the GPU work stays
  // paced to real frames instead of firing once per reactive update.
  pending_params: RefCell<Option<Vec<(String, ParamValue)>>>,
  // The same paint every other kind carries, so a texture composites like one:
  // `blend_mode` is the reason it is here (stacking GPU layers additively in
  // the tree instead of hand-writing a compositing shader pass). A raster draw
  // uses only part of a paint - alpha multiplies opacity, blend mode composites
  // - and ignores the color's RGB, any color source, and the stroke fields.
  // See examples/texture_paint.rs, which asserts each of those.
  pub paint: PaintState,
}

impl Texture {
  fn source_rect(&self, tex_w: u32, tex_h: u32) -> Rect {
    Rect::new(
      Point::new(self.src_x.unwrap_or(0.0), self.src_y.unwrap_or(0.0)),
      Size::new(self.src_w.unwrap_or(tex_w as f32), self.src_h.unwrap_or(tex_h as f32)),
    )
  }
}

impl Buildable for Texture {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let Some(tex_id) = self.texture_id else {
      return;
    };
    if let Some(params) = self.pending_params.borrow_mut().take() {
      if let Err(e) = ctx.alloy.update_shader_params(tex_id, &params) {
        log::warn!("[texture] build: setShaderParams tex_id={tex_id}: {e}");
      }
    }
    let Some(entry) = ctx.alloy.textures.get(tex_id) else {
      log::warn!("[texture] build: tex_id={} not in registry", tex_id);
      return;
    };

    let src_rect = self.source_rect(entry.width(), entry.height());
    let x = self.x.unwrap_or(0.0);
    let y = self.y.unwrap_or(0.0);
    let w = self.w.unwrap_or(ctx.size.width);
    let h = self.h.unwrap_or(ctx.size.height);
    let dst_rect = Rect::new(Point::new(x, y), Size::new(w, h));
    let (src_rect, dst_rect) = fit_rects(self.fit, src_rect, dst_rect);
    // `to_paint`, not `to_paint_in`: the draw ignores color sources, so
    // resolving a box-relative gradient would build one per frame for nothing.
    let paint = self.paint.to_paint();
    // Display sampling follows the texture's declared filter (Impeller
    // applies it per draw; the GL-side sampler objects cover shader passes).
    let sampling = match entry.sampler().filter {
      crate::texture::SamplerFilter::Linear => TextureSampling::Linear,
      crate::texture::SamplerFilter::Nearest => TextureSampling::NearestNeighbor,
    };
    builder.draw_texture_rect(&entry.impeller, &src_rect, &dst_rect, sampling, Some(&paint));
  }
}

impl Hittable for Texture {
  fn is_in_bounds(&self, point: Point, ctx: &HitContext) -> bool {
    let x = self.x.unwrap_or(0.0);
    let y = self.y.unwrap_or(0.0);
    let w = self.w.unwrap_or(ctx.size.width);
    let h = self.h.unwrap_or(ctx.size.height);
    point.x >= x && point.x < x + w && point.y >= y && point.y < y + h
  }
}

impl Bounded for Texture {
  fn local_bounds(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }
}

// HTML <img> sizing rules:
//   neither known  -> intrinsic w/h
//   one known      -> derive other from intrinsic aspect ratio
//   both known     -> honor both (explicit override)
// Intrinsic size honors src_* crop when set, else falls back to texture dims.
// `fit` is paint-only (see fit_rects) and never enters measurement.
impl Measurable for Texture {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    let (tex_w, tex_h) = self
      .texture_id
      .and_then(|id| ctx.alloy.textures.get(id).map(|e| (e.width() as f32, e.height() as f32)))
      .unwrap_or((0.0, 0.0));
    let iw = self.src_w.unwrap_or(tex_w);
    let ih = self.src_h.unwrap_or(tex_h);
    match (ctx.known.width, ctx.known.height) {
      (Some(w), Some(h)) => Size::new(w, h),
      (Some(w), None) => {
        let h = if iw > 0.0 { w * ih / iw } else { ih };
        Size::new(w, h)
      }
      (None, Some(h)) => {
        let w = if ih > 0.0 { h * iw / ih } else { iw };
        Size::new(w, h)
      }
      (None, None) => Size::new(iw, ih),
    }
  }
}

impl Texture {
  // Fit never changes the element box, only how pixels map into it.
  pub fn set_fit(&mut self, fit: TextureFit) -> Damage {
    self.fit = fit;
    Damage::Paint
  }

  // Source id and crop size feed measurement (intrinsic size), so they affect
  // layout; the crop position only moves which pixels are sampled. None clears
  // the texture; the null-vs-number decoding happens in the binding layer.
  pub fn set_src(&mut self, id: Option<u64>) -> Damage {
    self.texture_id = id;
    Damage::Layout
  }
  pub fn set_src_x(&mut self, v: f32) -> Damage {
    self.src_x = Some(v);
    Damage::Paint
  }
  pub fn set_src_y(&mut self, v: f32) -> Damage {
    self.src_y = Some(v);
    Damage::Paint
  }
  pub fn set_src_w(&mut self, v: f32) -> Damage {
    self.src_w = Some(v);
    Damage::Layout
  }
  pub fn set_src_h(&mut self, v: f32) -> Damage {
    self.src_h = Some(v);
    Damage::Layout
  }

  // Detached-form geometry: painted within the parent's box (or the
  // explicit x/y/w/h given), never affects the taffy layout tree.
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

  // Only meaningful when texture_id names a shader texture; applied at the
  // next build (see pending_params above), not here.
  pub fn set_params(&mut self, params: Vec<(String, ParamValue)>) -> Damage {
    *self.pending_params.get_mut() = Some(params);
    Damage::Paint
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::Texture(self),
      Style {
        display: Display::Block,
        // Replaced-element default: opt out of flex `align-items: stretch`,
        // matching HTML <img>. User can override via align-self prop.
        align_self: Some(AlignSelf::START),
        ..Default::default()
      },
    )
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Texture(self))
  }
}
