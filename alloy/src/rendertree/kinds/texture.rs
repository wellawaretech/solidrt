use super::PaintState;
use crate::gpu::PxRect;
use crate::renderer::prelude::*;
use crate::renderer::{
  ColorSource, DisplayListBuilder, Matrix, Point, Rect, RoundingRadii, Size, TextureSampling, TileMode,
};
use crate::rendertree::cull;
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{
  base_style, replaced_size, Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext,
};
use std::cell::Cell;
use taffy::{Display, Style};

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
  let src_out =
    Rect::new(Point::new(src.origin.x + (sw - vw) / 2.0, src.origin.y + (sh - vh) / 2.0), Size::new(vw, vh));
  // Destination extent of that portion, centered; never larger than the box.
  let ow = (vw * scale).min(dw);
  let oh = (vh * scale).min(dh);
  let dst_out =
    Rect::new(Point::new(dst.origin.x + (dw - ow) / 2.0, dst.origin.y + (dh - oh) / 2.0), Size::new(ow, oh));
  (src_out, dst_out)
}

// How far past a changed texel a draw of the texture shows the change, in
// texels: linear filtering blends each texel into the pixels of its
// neighbours.
const CONTENT_SAMPLE_REACH_TEXELS: f32 = 1.0;

// What a content rectangle mapped to the window grows by on every side, in
// logical px: the box offset that rounds the node's origin onto a device
// pixel (under one) and the rasterization edge.
const CONTENT_EDGE_PX: f32 = 1.0;

/// How the last paint walk that built a texture node drew it: the source
/// rectangle it took (texels, after the fit's crop), the rectangle of its
/// box frame that one landed on, and that frame's map. Under no Recording
/// boundary the map is to the window. Built inside a Recording boundary's
/// record walk, it is to that boundary's frame (cull::RecordingFrame), and
/// `within` names the boundary: a replay is not walked, so the build's
/// window position goes stale, but its place in the recording holds until
/// the boundary records again. What turns a change inside rectangles of
/// the texture into window damage smaller than the node
/// (`RenderTree::texture_content_changed`), which composes the current map
/// to the window from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
  pub src: Rect,
  pub dst: Rect,
  /// The nearest Recording boundary the build was recorded inside; None
  /// under none.
  pub within: Option<u64>,
  /// The box frame's map to the frame of `within`, to the window under
  /// none; None where it is not known (not 2d, a boundary frame with no
  /// inverse).
  pub to_frame: cull::WindowMap,
}

impl Shown {
  /// The window extent of what the draw shows of `rects`, rectangles of the
  /// texture's pixels (image space, top-left origin), with `map` the box
  /// frame's current map to the window: each grown by the filter's reach,
  /// cut to the source rectangle, scaled onto the destination, mapped to
  /// the window (its bounding box under a rotation) and grown by the edge.
  /// Empty when the draw shows none of them (all outside a crop). None for
  /// a source rectangle with no area, and the caller falls back to the
  /// whole node.
  pub fn window_extent(&self, map: &euclid::default::Transform2D<f32>, rects: &[PxRect]) -> Option<cull::Extent> {
    let (sw, sh) = (self.src.size.width, self.src.size.height);
    if sw <= 0.0 || sh <= 0.0 {
      return None;
    }
    let (sx, sy) = (self.dst.size.width / sw, self.dst.size.height / sh);
    let reach = CONTENT_SAMPLE_REACH_TEXELS;
    let mut extent = cull::Extent::Empty;
    for r in rects {
      let texels = Rect::new(
        Point::new(r.x as f32 - reach, r.y as f32 - reach),
        Size::new(r.w as f32 + 2.0 * reach, r.h as f32 + 2.0 * reach),
      );
      let Some(seen) = texels.intersection(&self.src) else { continue };
      let local = Rect::new(
        Point::new(
          self.dst.origin.x + (seen.origin.x - self.src.origin.x) * sx,
          self.dst.origin.y + (seen.origin.y - self.src.origin.y) * sy,
        ),
        Size::new(seen.size.width * sx, seen.size.height * sy),
      );
      let window = map.outer_transformed_rect(&local).inflate(CONTENT_EDGE_PX, CONTENT_EDGE_PX);
      extent = extent.union(cull::Extent::Bounded(window));
    }
    Some(extent)
  }

  /// The window extent of the whole draw, with `map` the box frame's
  /// current map to the window: the destination rectangle mapped and grown
  /// by the edge. What a change of all of the texture shows as.
  pub fn drawn_extent(&self, map: &euclid::default::Transform2D<f32>) -> cull::Extent {
    cull::Extent::Bounded(map.outer_transformed_rect(&self.dst).inflate(CONTENT_EDGE_PX, CONTENT_EDGE_PX))
  }
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
  // Corner radii [top-left, top-right, bottom-right, bottom-left], CSS
  // border-radius order, like Rectangle's. Set, the image is drawn as a
  // rounded rect filled with the texture (an image color source) instead of
  // a plain texture draw: one draw, no clip.
  pub radius: Option<[f32; 4]>,
  // The same paint every other kind carries, so a texture composites like one:
  // `blend_mode` is the reason it is here (stacking GPU layers additively in
  // the tree instead of hand-writing a compositing shader pass). A raster draw
  // uses only part of a paint - alpha multiplies opacity, blend mode composites
  // - and ignores the color's RGB, any color source, and the stroke fields.
  // See examples/texture_paint.rs, which asserts each of those.
  pub paint: PaintState,
  // How the last paint walk that built the node placed its draw (`Shown`);
  // None until a walk builds it, and after one that found no texture.
  // Written by the build, so a node inside a recording a walk replays
  // keeps the value of the walk that recorded it, stated relative to that
  // recording's frame. Read back for a node whose every boundary above is
  // a Recording one, and no view above it blurs.
  pub shown: Cell<Option<Shown>>,
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
    self.shown.set(None);
    let Some(tex_id) = self.texture_id else {
      return;
    };
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
    // A texture has no matrix of its own, so the frame the walk is in is
    // its box frame. Kept ahead of the handle check: the placement needs
    // only the entry's size, and a node that draws nothing damages nothing
    // anyone sees. Inside a recording the map is stated relative to the
    // recording's frame, where the draw stays put.
    let (within, to_frame) = match ctx.recording {
      Some((id, frame)) => (Some(id), cull::map_relative(&ctx.to_window, &frame)),
      None => (None, ctx.to_window),
    };
    self.shown.set(Some(Shown { src: src_rect, dst: dst_rect, within, to_frame }));

    // No renderer handle, nothing to draw: a non-2D shape, or a draw target
    // of a format other than rgba8 (sampler-only, see TextureEntry::handle).
    let Some(handle) = &entry.handle else {
      log::warn!(
        "[texture] build: texture {tex_id} is sampler-only ({} {}); <texture> cannot display it (sample it from a pass)",
        entry.format.name(),
        if entry.shape == crate::gpu::TextureShape::D2 { "draw target" } else { entry.shape.describe() }
      );
      return;
    };

    // `to_paint`, not `to_paint_in`: the draw ignores color sources, so
    // resolving a box-relative gradient would build one per frame for nothing.
    let paint = self.paint.to_paint();
    // Display sampling follows the texture's declared filter (the renderer
    // applies it per draw; the GL-side sampler objects cover shader passes).
    let sampling = match entry.sampler().filter {
      crate::gpu::SamplerFilter::Linear => TextureSampling::Linear,
      crate::gpu::SamplerFilter::Nearest => TextureSampling::Nearest,
    };
    match self.radius {
      // A rounded image: the rounded rect filled with the texture as its
      // color source, transformed so the source rect lands on the
      // destination rect. The same geometry path a rounded d-rect takes,
      // and nothing is clipped: a rounded clip around a texture costs a
      // tiled GPU a third of the frame per ten panes while the clip's box
      // is in flight (okf/backlog/rounded-clip-cost-android.md); this
      // draw costs what the plain texture draw does.
      Some([tl, tr, br, bl]) if src_rect.size.width > 0.0 && src_rect.size.height > 0.0 => {
        let sx = dst_rect.size.width / src_rect.size.width;
        let sy = dst_rect.size.height / src_rect.size.height;
        let to_dst = Matrix::new_2d(
          sx,
          0.0,
          0.0,
          sy,
          dst_rect.origin.x - src_rect.origin.x * sx,
          dst_rect.origin.y - src_rect.origin.y * sy,
        );
        let source = ColorSource::image(handle, TileMode::Clamp, TileMode::Clamp, sampling, Some(&to_dst));
        let mut paint = paint;
        paint.set_color_source(&source);
        let radii = RoundingRadii::circular([tl, tr, br, bl]);
        crate::rendertree::counters::note_draw();
        builder.draw_rounded_rect(&dst_rect, &radii, &paint);
      }
      _ => {
        crate::rendertree::counters::note_draw();
        builder.draw_texture_rect(handle, &src_rect, &dst_rect, sampling, Some(&paint));
      }
    }
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

// HTML <img> sizing rules (rendertree::replaced_size). Intrinsic size honors
// src_* crop when set, else falls back to texture dims. `fit` is paint-only
// (see fit_rects) and never enters measurement.
impl Measurable for Texture {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    let (tex_w, tex_h) = self
      .texture_id
      .and_then(|id| ctx.alloy.textures.get(id).map(|e| (e.width() as f32, e.height() as f32)))
      .unwrap_or((0.0, 0.0));
    let intrinsic = Size::new(self.src_w.unwrap_or(tex_w), self.src_h.unwrap_or(tex_h));
    replaced_size(ctx.known, intrinsic)
  }
}

impl Texture {
  // Fit never changes the element box, only how pixels map into it. None
  // resets to the default fit.
  pub fn set_fit(&mut self, fit: Option<TextureFit>) -> Damage {
    self.fit = fit.unwrap_or_default();
    Damage::Paint
  }

  // Source id and crop size feed measurement (intrinsic size), so they affect
  // layout; the crop position only moves which pixels are sampled. None clears
  // the texture; the null-vs-number decoding happens in the binding layer.
  pub fn set_src(&mut self, id: Option<u64>) -> Damage {
    self.texture_id = id;
    Damage::Layout
  }
  pub fn set_src_x(&mut self, v: Option<f32>) -> Damage {
    self.src_x = v;
    Damage::Paint
  }
  pub fn set_src_y(&mut self, v: Option<f32>) -> Damage {
    self.src_y = v;
    Damage::Paint
  }
  pub fn set_src_w(&mut self, v: Option<f32>) -> Damage {
    self.src_w = v;
    Damage::Layout
  }
  pub fn set_src_h(&mut self, v: Option<f32>) -> Damage {
    self.src_h = v;
    Damage::Layout
  }

  // Detached-form geometry: painted within the parent's box (or the
  // explicit x/y/w/h given), never affects the taffy layout tree.
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_w(&mut self, v: Option<f32>) -> Damage {
    self.w = v;
    Damage::Paint
  }
  pub fn set_h(&mut self, v: Option<f32>) -> Damage {
    self.h = v;
    Damage::Paint
  }
  // Paint-only: the rounding changes which pixels the draw covers, never
  // the element box.
  pub fn set_radius(&mut self, radius: Option<[f32; 4]>) -> Damage {
    self.radius = radius;
    Damage::Paint
  }

  pub fn initial_style() -> Style {
    // No align_self override: parent alignment applies in full, like any
    // element (and like HTML <img>, which flex containers do stretch).
    // Stretch only reaches a texture whose cross size is auto; the
    // measured axis then follows the intrinsic ratio via replaced_size,
    // and alignSelf="flex-start" is the per-node opt-out, as on the web.
    Style { display: Display::Block, ..base_style() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Texture(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Texture(self))
  }
}
