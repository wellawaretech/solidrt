//! The recording half: what a list is built from, as plain data over the
//! vocabulary. A builder records anvil's executable ops (`super::ops`),
//! resolved for the executor: colors premultiplied, a paint reduced to what
//! stage 1 draws, the ops stage 1 cannot draw warned once and recorded as
//! their nearest supported op or not at all. With the op log on (the
//! `oplog` feature, or this crate's tests) it also keeps the plain-data log
//! (`crate::oplog`), one cfg'd statement per forwarder beside the
//! executable push.

use super::ops::{premultiply, Image, Op, Premultiplied, Radii};
use super::Texture;
use crate::contract;
#[cfg(any(test, feature = "oplog"))]
use crate::oplog::{Op as LogOp, PaintLog, PathLog, PathSegment};
use crate::{
  BlendMode, ClipOperation, Color, ColorMatrix, DrawStyle, FillType, Matrix, Point, Rect, RoundingRadii, Size,
  StrokeCap, StrokeJoin, TextureSampling, TileMode,
};
use crate::{CoveragePolicy, GlyphGradient, GlyphQuad};
use std::sync::Arc;

/// A finished recording: the executable ops and, in a test build, the
/// log. A list is cloned and crosses threads, so both are shared.
#[derive(Clone)]
pub struct DisplayList {
  ops: Arc<[Op]>,
  #[cfg(any(test, feature = "oplog"))]
  log: Arc<[LogOp]>,
}

impl DisplayList {
  /// The executable ops, for the executor and the walk. Not named `ops`:
  /// the contract's test-build `ops` returns the log, and an inherent
  /// method of that name would shadow it inside this module.
  pub(super) fn executable(&self) -> &[Op] {
    &self.ops
  }

  /// The glyph runs the list draws, nested lists flattened in place, each
  /// as the shared slice its op holds and the atlas it samples: a test's
  /// read of whether a run was re-recorded by reference or remade.
  #[cfg(any(test, feature = "oplog"))]
  pub fn glyph_runs(&self) -> Vec<(Arc<[GlyphQuad]>, Texture)> {
    let mut out = Vec::new();
    for op in self.executable() {
      match op {
        Op::DrawGlyphs { atlas, quads, .. } => out.push((quads.clone(), atlas.clone())),
        Op::DrawList { list, .. } => out.extend(list.glyph_runs()),
        _ => {}
      }
    }
    out
  }
}

impl contract::DisplayList for DisplayList {
  #[cfg(any(test, feature = "oplog"))]
  fn ops(&self) -> &[LogOp] {
    &self.log
  }
}

pub struct DisplayListBuilder {
  ops: Vec<Op>,
  #[cfg(any(test, feature = "oplog"))]
  log: Vec<LogOp>,
}

impl DisplayListBuilder {
  /// The ops of a clip that cuts its shape out: none, warned once. One
  /// call site, so the warning is one line whichever clip method sent it.
  fn clip_difference_unsupported() {
    unsupported!("clip_rect with the difference op", "the clip is ignored");
  }
}

/// What anvil draws of a paint: its color, premultiplied, and the stroke
/// width when the style strokes (an image source rides beside it, see
/// `image_of`). A gradient source, a blend mode other than source-over and
/// the filters were warned about when they were set. A stroke-and-fill
/// covers the union of the fill and the stroke once: for the convex shapes
/// here that is the fill outset by half the stroke width, its non-zero
/// corner radii grown by the same (`fill` takes the outset and the grown
/// radii), pushed as one op, so a translucent color is not blended twice
/// where the stroke's inner half would lie over the fill. A stroke of zero
/// or negative width pushes nothing. `radii` are the shape's, clamped;
/// zero for the shapes without corners.
fn shape_ops(
  paint: &Paint,
  radii: Radii,
  fill: impl Fn(Premultiplied, f32, Radii) -> Op,
  stroke: impl Fn(Premultiplied, f32) -> Op,
  ops: &mut Vec<Op>,
) {
  let color = premultiply(paint.color);
  let strokes = paint.stroke_width > 0.0;
  match paint.draw_style {
    DrawStyle::Fill => ops.push(fill(color, 0.0, radii)),
    DrawStyle::Stroke => {
      if strokes {
        ops.push(stroke(color, paint.stroke_width));
      }
    }
    DrawStyle::StrokeAndFill => {
      let outset = if strokes { paint.stroke_width * 0.5 } else { 0.0 };
      ops.push(fill(color, outset, grow_radii(radii, outset)));
    }
  }
}

/// `radii` for the shape outset by `outset`: a rounded corner's radius
/// grows by the outset (the outer edge of a centered stroke), a sharp
/// corner stays sharp (a miter join's outer corner).
fn grow_radii(radii: Radii, outset: f32) -> Radii {
  radii.map(|r| if r > 0.0 { r + outset } else { r })
}

/// `radii` as anvil draws them on `rect`, clamped as CSS clamps
/// border-radius: negative radii to zero, then every radius scaled by one
/// factor so that adjacent corners never overlap along a side, which keeps
/// the rounded-box distance valid. The rendertree does not clamp its draw
/// radii, so this is the one place. A corner whose x and y radii differ is
/// warned once and uses its x radius.
fn clamp_radii(rect: &Rect, radii: &RoundingRadii) -> Radii {
  let corners = [radii.top_left, radii.top_right, radii.bottom_right, radii.bottom_left];
  if corners.iter().any(|c| c.x != c.y) {
    unsupported!("elliptical corner radii", "the corner uses its x radius");
  }
  let [tl, tr, br, bl] = corners.map(|c| c.x.max(0.0));
  let (w, h) = (rect.size.width.max(0.0), rect.size.height.max(0.0));
  // One factor for all four corners, the smallest any side asks for; a
  // side whose two radii are zero asks for none.
  let factor = [(w, tl + tr), (w, br + bl), (h, tl + bl), (h, tr + br)]
    .iter()
    .filter(|(_, sum)| *sum > 0.0)
    .fold(1.0f32, |f, (side, sum)| f.min(side / sum));
  [tl, tr, br, bl].map(|r| r * factor)
}

/// The image a paint's color source fills a shape with: Some for an image
/// source whose transform (identity when none) is scale plus translate,
/// which is what the rounded `<texture>` builds; any other transform is
/// warned once and the shape draws its color. A tile mode other than clamp
/// is warned once and clamps. A gradient source is None here; its warning
/// fired when it was set.
fn image_of(paint: &Paint) -> Option<Image> {
  let Some(ColorSource::Image { texture, tile_x, tile_y, sampling, transform }) = &paint.color_source else {
    return None;
  };
  let to_draw = transform.unwrap_or_else(Matrix::identity);
  let to_texel =
    if to_draw.is_2d() && to_draw.m12 == 0.0 && to_draw.m21 == 0.0 { to_draw.to_2d().inverse() } else { None };
  // A rotated, skewed, perspective or singular map: one warning for all.
  let Some(to_texel) = to_texel else {
    unsupported!("an image source under a rotated transform", "the paint's color is used instead");
    return None;
  };
  if *tile_x != TileMode::Clamp || *tile_y != TileMode::Clamp {
    unsupported!("an image source tile mode other than clamp", "the source clamps");
  }
  Some(Image { texture: texture.clone(), sampling: *sampling, to_texel })
}

/// The union of the quads' `dst` rects; an empty rect at the origin for
/// no quads.
fn glyph_bounds(quads: &[GlyphQuad]) -> Rect {
  let rect = |q: &GlyphQuad| Rect::new(Point::new(q.dst[0], q.dst[1]), Size::new(q.dst[2], q.dst[3]));
  let mut iter = quads.iter().map(rect);
  let Some(first) = iter.next() else { return Rect::zero() };
  iter.fold(first, |acc, r| acc.union(&r))
}

/// A rect's stroke turns its corners with a miter join (sharp corners)
/// only; any other join is warned once and drawn as miter. One call site
/// for both rect methods, so the warning is one line whichever sent it.
/// The miter limit never matters for a rect: a right angle's miter is
/// sqrt(2) times the stroke width, under the default limit
/// (`contract::DEFAULT_STROKE_MITER`), so `stroke_miter` stays unread.
fn warn_non_miter_join(paint: &Paint) {
  let strokes = paint.stroke_width > 0.0 && matches!(paint.draw_style, DrawStyle::Stroke | DrawStyle::StrokeAndFill);
  if strokes && paint.stroke_join != StrokeJoin::Miter {
    unsupported!("a stroke join other than miter", "the rect draws with miter joins");
  }
}

// The arguments of the ops anvil does not draw yet (the layers, the oval
// clip, the path) are read only by the test log.
#[cfg_attr(not(any(test, feature = "oplog")), allow(unused_variables))]
impl contract::DisplayListBuilder for DisplayListBuilder {
  fn new(_cull: Option<&Rect>) -> Self {
    // The cull rect is ignored by design: the executor's clip rejection
    // covers it.
    DisplayListBuilder {
      ops: Vec::new(),
      #[cfg(any(test, feature = "oplog"))]
      log: Vec::new(),
    }
  }

  fn build(self) -> Option<DisplayList> {
    Some(DisplayList {
      ops: self.ops.into(),
      #[cfg(any(test, feature = "oplog"))]
      log: self.log.into(),
    })
  }

  fn save(&mut self) -> &mut Self {
    self.ops.push(Op::Save);
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::Save);
    self
  }

  fn save_layer(&mut self, bounds: &Rect, paint: Option<&Paint>) -> &mut Self {
    unsupported!("save_layer", "the content draws without a layer, at full opacity and unfiltered");
    // A plain save, so the matching restore still pops.
    self.ops.push(Op::Save);
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::SaveLayer { bounds: *bounds, paint: paint.map(|p| p.log) });
    self
  }

  fn save_backdrop_layer(&mut self, bounds: &Rect, paint: Option<&Paint>, filter: Option<&ImageFilter>) -> &mut Self {
    unsupported!("save_backdrop_layer", "the backdrop is not captured and the content draws in place");
    self.ops.push(Op::Save);
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::SaveBackdropLayer { bounds: *bounds, paint: paint.map(|p| p.log), filter: filter.is_some() });
    self
  }

  fn restore(&mut self) -> &mut Self {
    self.ops.push(Op::Restore);
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::Restore);
    self
  }

  fn translate(&mut self, x: f32, y: f32) -> &mut Self {
    self.ops.push(Op::Translate { x, y });
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::Translate { x, y });
    self
  }

  fn scale(&mut self, x: f32, y: f32) -> &mut Self {
    self.ops.push(Op::Scale { x, y });
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::Scale { x, y });
    self
  }

  fn transform(&mut self, matrix: &Matrix) -> &mut Self {
    self.ops.push(Op::Transform(*matrix));
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::Transform(*matrix));
    self
  }

  fn clip_rect(&mut self, rect: &Rect, op: ClipOperation) -> &mut Self {
    match op {
      ClipOperation::Intersect => self.ops.push(Op::ClipRect(*rect)),
      ClipOperation::Difference => Self::clip_difference_unsupported(),
    }
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::ClipRect { rect: *rect, op });
    self
  }

  fn clip_oval(&mut self, bounds: &Rect, op: ClipOperation) -> &mut Self {
    unsupported!("clip_oval", "the clip is ignored");
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::ClipOval { bounds: *bounds, op });
    self
  }

  fn clip_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, op: ClipOperation) -> &mut Self {
    match op {
      ClipOperation::Intersect => {
        self.ops.push(Op::ClipRoundedRect { rect: *rect, radii: clamp_radii(rect, radii) });
      }
      ClipOperation::Difference => Self::clip_difference_unsupported(),
    }
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::ClipRoundedRect { rect: *rect, radii: *radii, op });
    self
  }

  fn draw_line(&mut self, from: Point, to: Point, paint: &Paint) -> &mut Self {
    // A line always strokes, whatever the paint's draw style.
    if matches!(paint.color_source, Some(ColorSource::Image { .. })) {
      unsupported!("an image source on a line", "the line draws the paint's color");
    }
    self.ops.push(Op::DrawLine {
      from,
      to,
      color: premultiply(paint.color),
      width: paint.stroke_width,
      cap: paint.stroke_cap,
    });
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawLine { from, to, paint: paint.log });
    self
  }

  fn draw_rect(&mut self, rect: &Rect, paint: &Paint) -> &mut Self {
    warn_non_miter_join(paint);
    let rect = *rect;
    let image = image_of(paint);
    shape_ops(
      paint,
      [0.0; 4],
      |color, outset, radii| Op::DrawRect {
        rect: rect.inflate(outset, outset),
        radii,
        color,
        stroke: None,
        image: image.clone(),
      },
      |color, width| Op::DrawRect { rect, radii: [0.0; 4], color, stroke: Some(width), image: image.clone() },
      &mut self.ops,
    );
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawRect { rect, paint: paint.log });
    self
  }

  fn draw_oval(&mut self, bounds: &Rect, paint: &Paint) -> &mut Self {
    let bounds = *bounds;
    let image = image_of(paint);
    shape_ops(
      paint,
      [0.0; 4],
      |color, outset, _| Op::DrawOval {
        bounds: bounds.inflate(outset, outset),
        color,
        stroke: None,
        image: image.clone(),
      },
      |color, width| Op::DrawOval { bounds, color, stroke: Some(width), image: image.clone() },
      &mut self.ops,
    );
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawOval { bounds, paint: paint.log });
    self
  }

  fn draw_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, paint: &Paint) -> &mut Self {
    // A miter join still turns a corner whose radius is zero.
    warn_non_miter_join(paint);
    let rect = *rect;
    let clamped = clamp_radii(&rect, radii);
    let image = image_of(paint);
    shape_ops(
      paint,
      clamped,
      |color, outset, radii| Op::DrawRect {
        rect: rect.inflate(outset, outset),
        radii,
        color,
        stroke: None,
        image: image.clone(),
      },
      |color, width| Op::DrawRect { rect, radii: clamped, color, stroke: Some(width), image: image.clone() },
      &mut self.ops,
    );
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawRoundedRect { rect, radii: *radii, paint: paint.log });
    self
  }

  fn draw_path(&mut self, path: &Path, paint: &Paint) -> &mut Self {
    unsupported!("draw_path", "the path is not drawn");
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawPath { path: path.log.clone(), paint: paint.log });
    self
  }

  fn draw_texture_rect(
    &mut self,
    texture: &Texture,
    src: &Rect,
    dst: &Rect,
    sampling: TextureSampling,
    paint: Option<&Paint>,
  ) -> &mut Self {
    self.ops.push(Op::DrawTexture {
      texture: texture.clone(),
      src: *src,
      dst: *dst,
      sampling,
      alpha: paint.map_or(1.0, |p| p.color.alpha),
    });
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawTextureRect { src: *src, dst: *dst, sampling, paint: paint.map(|p| p.log) });
    self
  }

  fn draw_glyphs(
    &mut self,
    atlas: &Texture,
    quads: &Arc<[GlyphQuad]>,
    policy: CoveragePolicy,
    gradient: Option<&Arc<GlyphGradient>>,
  ) -> &mut Self {
    let bounds = glyph_bounds(quads);
    self.ops.push(Op::DrawGlyphs {
      atlas: atlas.clone(),
      quads: quads.clone(),
      bounds,
      policy,
      gradient: gradient.cloned(),
    });
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawGlyphs { bounds, quads: quads.len(), policy, gradient: gradient.is_some() });
    self
  }

  fn draw_display_list(&mut self, dl: &DisplayList, opacity: f32) -> &mut Self {
    self.ops.push(Op::DrawList { list: dl.clone(), opacity });
    #[cfg(any(test, feature = "oplog"))]
    self.log.push(LogOp::DrawDisplayList { ops: dl.log.clone(), opacity });
    self
  }
}

/// How a draw paints, as plain data; the recording reads what anvil draws
/// of it (the color, an image source, the draw style, the stroke width and
/// cap, and the join, to warn when it is not miter).
pub struct Paint {
  pub(super) color: Color,
  pub(super) color_source: Option<ColorSource>,
  pub(super) draw_style: DrawStyle,
  #[allow(dead_code)] // read by the executor once it blends other than source-over
  pub(super) blend_mode: BlendMode,
  pub(super) stroke_width: f32,
  pub(super) stroke_cap: StrokeCap,
  pub(super) stroke_join: StrokeJoin,
  #[allow(dead_code)] // read by the executor once it draws path joins
  pub(super) stroke_miter: f32,
  #[allow(dead_code)] // read by the executor from stage 6 (filters)
  pub(super) color_filter: Option<ColorFilter>,
  #[allow(dead_code)] // read by the executor from stage 6 (filters)
  pub(super) image_filter: Option<ImageFilter>,
  #[allow(dead_code)] // read by the executor from stage 6 (filters)
  pub(super) mask_filter: Option<MaskFilter>,
  #[cfg(any(test, feature = "oplog"))]
  log: PaintLog,
}

impl Default for Paint {
  /// The contract's default: a solid opaque black fill, source-over,
  /// `contract::DEFAULT_STROKE_WIDTH`, butt caps, miter joins at
  /// `contract::DEFAULT_STROKE_MITER`, no source and no filters.
  fn default() -> Self {
    Paint {
      color: Color::new_srgba(0.0, 0.0, 0.0, 1.0),
      color_source: None,
      draw_style: DrawStyle::Fill,
      blend_mode: BlendMode::SourceOver,
      stroke_width: contract::DEFAULT_STROKE_WIDTH,
      stroke_cap: StrokeCap::Butt,
      stroke_join: StrokeJoin::Miter,
      stroke_miter: contract::DEFAULT_STROKE_MITER,
      color_filter: None,
      image_filter: None,
      mask_filter: None,
      #[cfg(any(test, feature = "oplog"))]
      log: PaintLog::default(),
    }
  }
}

impl contract::Paint for Paint {
  fn set_color(&mut self, color: Color) -> &mut Self {
    self.color = color;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.color = color;
    }
    self
  }

  fn set_color_source(&mut self, source: &ColorSource) -> &mut Self {
    // An image source draws (`image_of`); the gradients wait for stage 3.
    if !matches!(source, ColorSource::Image { .. }) {
      unsupported!("a gradient color source", "the paint's color is used instead");
    }
    self.color_source = Some(source.clone());
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.color_source = true;
    }
    self
  }

  fn set_draw_style(&mut self, style: DrawStyle) -> &mut Self {
    self.draw_style = style;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.draw_style = style;
    }
    self
  }

  fn set_blend_mode(&mut self, mode: BlendMode) -> &mut Self {
    if mode != BlendMode::SourceOver {
      unsupported!("a blend mode other than source-over", "the draw blends source-over");
    }
    self.blend_mode = mode;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.blend_mode = mode;
    }
    self
  }

  fn set_stroke_width(&mut self, width: f32) -> &mut Self {
    self.stroke_width = width;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.stroke_width = width;
    }
    self
  }

  fn set_stroke_cap(&mut self, cap: StrokeCap) -> &mut Self {
    self.stroke_cap = cap;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.stroke_cap = cap;
    }
    self
  }

  fn set_stroke_join(&mut self, join: StrokeJoin) -> &mut Self {
    self.stroke_join = join;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.stroke_join = join;
    }
    self
  }

  fn set_stroke_miter(&mut self, miter: f32) -> &mut Self {
    self.stroke_miter = miter;
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.stroke_miter = miter;
    }
    self
  }

  fn set_color_filter(&mut self, filter: &ColorFilter) -> &mut Self {
    unsupported!("a paint color filter", "the filter is ignored");
    self.color_filter = Some(filter.clone());
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.color_filter = true;
    }
    self
  }

  fn set_image_filter(&mut self, filter: &ImageFilter) -> &mut Self {
    unsupported!("a paint image filter", "the filter is ignored");
    self.image_filter = Some(filter.clone());
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.image_filter = true;
    }
    self
  }

  fn set_mask_filter(&mut self, filter: &MaskFilter) -> &mut Self {
    unsupported!("a paint mask filter", "the filter is ignored");
    self.mask_filter = Some(filter.clone());
    #[cfg(any(test, feature = "oplog"))]
    {
      self.log.mask_filter = true;
    }
    self
  }
}

// The sources and filters below hold their constructor arguments; an image
// source is read at record time (`image_of`), the gradients from stage 3
// and the filters from stage 6, and by nothing before.

/// What a paint draws with in place of its color.
#[allow(dead_code)]
#[derive(Clone)]
pub enum ColorSource {
  LinearGradient {
    start: Point,
    end: Point,
    colors: Vec<Color>,
    stops: Vec<f32>,
    tile: TileMode,
    transform: Option<Matrix>,
  },
  RadialGradient {
    center: Point,
    radius: f32,
    colors: Vec<Color>,
    stops: Vec<f32>,
    tile: TileMode,
    transform: Option<Matrix>,
  },
  Image {
    texture: Texture,
    tile_x: TileMode,
    tile_y: TileMode,
    sampling: TextureSampling,
    transform: Option<Matrix>,
  },
}

impl contract::ColorSource for ColorSource {
  fn linear_gradient(
    start: Point,
    end: Point,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    transform: Option<&Matrix>,
  ) -> Self {
    ColorSource::LinearGradient {
      start,
      end,
      colors: colors.to_vec(),
      stops: stops.to_vec(),
      tile,
      transform: transform.copied(),
    }
  }

  fn radial_gradient(
    center: Point,
    radius: f32,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    transform: Option<&Matrix>,
  ) -> Self {
    ColorSource::RadialGradient {
      center,
      radius,
      colors: colors.to_vec(),
      stops: stops.to_vec(),
      tile,
      transform: transform.copied(),
    }
  }

  fn image(
    texture: &Texture,
    tile_x: TileMode,
    tile_y: TileMode,
    sampling: TextureSampling,
    transform: Option<&Matrix>,
  ) -> Self {
    ColorSource::Image { texture: texture.clone(), tile_x, tile_y, sampling, transform: transform.copied() }
  }
}

/// A per-pixel color transform.
#[allow(dead_code)]
#[derive(Clone)]
pub struct ColorFilter {
  pub(super) matrix: ColorMatrix,
}

impl contract::ColorFilter for ColorFilter {
  fn matrix(matrix: &ColorMatrix) -> Self {
    ColorFilter { matrix: *matrix }
  }
}

/// A gaussian blur over a draw's or a layer's pixels.
#[allow(dead_code)]
#[derive(Clone)]
pub struct ImageFilter {
  pub(super) sigma_x: f32,
  pub(super) sigma_y: f32,
  pub(super) tile: TileMode,
}

impl contract::ImageFilter for ImageFilter {
  fn gaussian_blur(sigma_x: f32, sigma_y: f32, tile: TileMode) -> Self {
    ImageFilter { sigma_x, sigma_y, tile }
  }
}

/// A gaussian blur over a draw's coverage mask.
#[allow(dead_code)]
#[derive(Clone)]
pub struct MaskFilter {
  pub(super) sigma: f32,
}

impl contract::MaskFilter for MaskFilter {
  fn blur(sigma: f32) -> Self {
    MaskFilter { sigma }
  }
}

/// A finished path. Carries its fill rule and, in a test build, its
/// segments; stage 3 decides the executable geometry (lyon).
#[derive(Clone)]
pub struct Path {
  #[allow(dead_code)] // read by the executor from stage 3
  pub(super) fill: FillType,
  // A path is cloned and kept across frames: the log is shared.
  #[cfg(any(test, feature = "oplog"))]
  log: Arc<PathLog>,
}

impl contract::Path for Path {}

#[derive(Default)]
pub struct PathBuilder {
  #[cfg(any(test, feature = "oplog"))]
  segments: Vec<PathSegment>,
}

// The points are read only by the test log until stage 3 keeps the
// geometry.
#[cfg_attr(not(any(test, feature = "oplog")), allow(unused_variables))]
impl contract::Pen for PathBuilder {
  fn move_to(&mut self, p: Point) -> &mut Self {
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::MoveTo(p));
    self
  }

  fn line_to(&mut self, p: Point) -> &mut Self {
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::LineTo(p));
    self
  }

  fn quadratic_to(&mut self, ctrl: Point, p: Point) -> &mut Self {
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::QuadraticTo { ctrl, to: p });
    self
  }

  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point) -> &mut Self {
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::CubicTo { ctrl1, ctrl2, to: p });
    self
  }
}

impl contract::PathBuilder for PathBuilder {
  fn close(&mut self) -> &mut Self {
    #[cfg(any(test, feature = "oplog"))]
    self.segments.push(PathSegment::Close);
    self
  }

  fn build(self, fill: FillType) -> Path {
    Path {
      fill,
      #[cfg(any(test, feature = "oplog"))]
      log: Arc::new(PathLog { segments: self.segments, fill }),
    }
  }
}
