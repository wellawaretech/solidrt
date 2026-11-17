//! The recording half: what a list is built from, as plain data over the
//! vocabulary. In this step nothing executes, so a builder keeps only the
//! test-build log (`super::super::oplog`) and every op warns once that it
//! is not supported yet; a paint keeps its fields for the executor of
//! later steps. Each forwarder has one `cfg(test)` statement for the log,
//! as the module this one replaced had beside its forward.

use super::super::contract;
#[cfg(test)]
use super::super::oplog::{Op, PaintLog, PathLog, PathSegment};
use super::super::{
  BlendMode, ClipOperation, Color, ColorMatrix, DrawStyle, FillType, Matrix, Point, Rect, RoundingRadii, StrokeCap,
  StrokeJoin, TextureSampling, TileMode,
};
use super::Texture;
#[cfg(test)]
use std::sync::Arc;

/// A finished recording. Holds only the test log in this step; step 2
/// adds the executable ops (`Arc<[Op]>` of anvil's own op enum).
#[derive(Clone)]
pub struct DisplayList {
  // A list is cloned and crosses threads: the log is shared.
  #[cfg(test)]
  ops: Arc<[Op]>,
}

impl contract::DisplayList for DisplayList {
  #[cfg(test)]
  fn ops(&self) -> &[Op] {
    &self.ops
  }
}

pub struct DisplayListBuilder {
  #[cfg(test)]
  ops: Vec<Op>,
}

// The arguments are read only by the test log until the executor records
// them.
#[cfg_attr(not(test), allow(unused_variables))]
impl contract::DisplayListBuilder for DisplayListBuilder {
  fn new(_cull: Option<&Rect>) -> Self {
    // The cull rect is ignored by design: the executor's clip rejection
    // covers it.
    DisplayListBuilder {
      #[cfg(test)]
      ops: Vec::new(),
    }
  }

  fn build(self) -> Option<DisplayList> {
    Some(DisplayList {
      #[cfg(test)]
      ops: self.ops.into(),
    })
  }

  fn save(&mut self) -> &mut Self {
    unsupported!("save", "the state op is ignored");
    #[cfg(test)]
    self.ops.push(Op::Save);
    self
  }

  fn save_layer(&mut self, bounds: &Rect, paint: Option<&Paint>) -> &mut Self {
    unsupported!("save_layer", "the content draws without a layer, at full opacity and unfiltered");
    #[cfg(test)]
    self.ops.push(Op::SaveLayer { bounds: *bounds, paint: paint.map(|p| p.log) });
    self
  }

  fn save_backdrop_layer(&mut self, bounds: &Rect, paint: Option<&Paint>, filter: Option<&ImageFilter>) -> &mut Self {
    unsupported!("save_backdrop_layer", "the backdrop is not captured and the content draws in place");
    #[cfg(test)]
    self.ops.push(Op::SaveBackdropLayer { bounds: *bounds, paint: paint.map(|p| p.log), filter: filter.is_some() });
    self
  }

  fn restore(&mut self) -> &mut Self {
    unsupported!("restore", "the state op is ignored");
    #[cfg(test)]
    self.ops.push(Op::Restore);
    self
  }

  fn translate(&mut self, x: f32, y: f32) -> &mut Self {
    unsupported!("translate", "the transform is ignored");
    #[cfg(test)]
    self.ops.push(Op::Translate { x, y });
    self
  }

  fn scale(&mut self, x: f32, y: f32) -> &mut Self {
    unsupported!("scale", "the transform is ignored");
    #[cfg(test)]
    self.ops.push(Op::Scale { x, y });
    self
  }

  fn transform(&mut self, matrix: &Matrix) -> &mut Self {
    unsupported!("transform", "the transform is ignored");
    #[cfg(test)]
    self.ops.push(Op::Transform(*matrix));
    self
  }

  fn clip_rect(&mut self, rect: &Rect, op: ClipOperation) -> &mut Self {
    unsupported!("clip_rect", "the clip is ignored");
    #[cfg(test)]
    self.ops.push(Op::ClipRect { rect: *rect, op });
    self
  }

  fn clip_oval(&mut self, bounds: &Rect, op: ClipOperation) -> &mut Self {
    unsupported!("clip_oval", "the clip is ignored");
    #[cfg(test)]
    self.ops.push(Op::ClipOval { bounds: *bounds, op });
    self
  }

  fn clip_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, op: ClipOperation) -> &mut Self {
    unsupported!("clip_rounded_rect", "the clip is ignored");
    #[cfg(test)]
    self.ops.push(Op::ClipRoundedRect { rect: *rect, radii: *radii, op });
    self
  }

  fn draw_line(&mut self, from: Point, to: Point, paint: &Paint) -> &mut Self {
    unsupported!("draw_line", "the line is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawLine { from, to, paint: paint.log });
    self
  }

  fn draw_rect(&mut self, rect: &Rect, paint: &Paint) -> &mut Self {
    unsupported!("draw_rect", "the rect is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawRect { rect: *rect, paint: paint.log });
    self
  }

  fn draw_oval(&mut self, bounds: &Rect, paint: &Paint) -> &mut Self {
    unsupported!("draw_oval", "the oval is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawOval { bounds: *bounds, paint: paint.log });
    self
  }

  fn draw_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, paint: &Paint) -> &mut Self {
    unsupported!("draw_rounded_rect", "the rounded rect is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawRoundedRect { rect: *rect, radii: *radii, paint: paint.log });
    self
  }

  fn draw_path(&mut self, path: &Path, paint: &Paint) -> &mut Self {
    unsupported!("draw_path", "the path is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawPath { path: path.log.clone(), paint: paint.log });
    self
  }

  fn draw_texture_rect(
    &mut self,
    // The log carries no texture handle, so nothing reads it until step 3.
    _texture: &Texture,
    src: &Rect,
    dst: &Rect,
    sampling: TextureSampling,
    paint: Option<&Paint>,
  ) -> &mut Self {
    unsupported!("draw_texture_rect", "the texture is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawTextureRect { src: *src, dst: *dst, sampling, paint: paint.map(|p| p.log) });
    self
  }

  fn draw_display_list(&mut self, dl: &DisplayList, opacity: f32) -> &mut Self {
    unsupported!("draw_display_list", "the nested list is not drawn");
    #[cfg(test)]
    self.ops.push(Op::DrawDisplayList { ops: dl.ops.clone(), opacity });
    self
  }
}

/// How a draw paints, as plain data; the executor reads it from step 2.
// Nothing reads the fields until the executor does.
#[allow(dead_code)]
pub struct Paint {
  pub(super) color: Color,
  pub(super) color_source: Option<ColorSource>,
  pub(super) draw_style: DrawStyle,
  pub(super) blend_mode: BlendMode,
  pub(super) stroke_width: f32,
  pub(super) stroke_cap: StrokeCap,
  pub(super) stroke_join: StrokeJoin,
  pub(super) stroke_miter: f32,
  pub(super) color_filter: Option<ColorFilter>,
  pub(super) image_filter: Option<ImageFilter>,
  pub(super) mask_filter: Option<MaskFilter>,
  #[cfg(test)]
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
      #[cfg(test)]
      log: PaintLog::default(),
    }
  }
}

impl contract::Paint for Paint {
  fn set_color(&mut self, color: Color) -> &mut Self {
    self.color = color;
    #[cfg(test)]
    {
      self.log.color = color;
    }
    self
  }

  fn set_color_source(&mut self, source: &ColorSource) -> &mut Self {
    unsupported!("a paint color source", "the paint's color is used instead");
    self.color_source = Some(source.clone());
    #[cfg(test)]
    {
      self.log.color_source = true;
    }
    self
  }

  fn set_draw_style(&mut self, style: DrawStyle) -> &mut Self {
    self.draw_style = style;
    #[cfg(test)]
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
    #[cfg(test)]
    {
      self.log.blend_mode = mode;
    }
    self
  }

  fn set_stroke_width(&mut self, width: f32) -> &mut Self {
    self.stroke_width = width;
    #[cfg(test)]
    {
      self.log.stroke_width = width;
    }
    self
  }

  fn set_stroke_cap(&mut self, cap: StrokeCap) -> &mut Self {
    self.stroke_cap = cap;
    #[cfg(test)]
    {
      self.log.stroke_cap = cap;
    }
    self
  }

  fn set_stroke_join(&mut self, join: StrokeJoin) -> &mut Self {
    self.stroke_join = join;
    #[cfg(test)]
    {
      self.log.stroke_join = join;
    }
    self
  }

  fn set_stroke_miter(&mut self, miter: f32) -> &mut Self {
    self.stroke_miter = miter;
    #[cfg(test)]
    {
      self.log.stroke_miter = miter;
    }
    self
  }

  fn set_color_filter(&mut self, filter: &ColorFilter) -> &mut Self {
    unsupported!("a paint color filter", "the filter is ignored");
    self.color_filter = Some(filter.clone());
    #[cfg(test)]
    {
      self.log.color_filter = true;
    }
    self
  }

  fn set_image_filter(&mut self, filter: &ImageFilter) -> &mut Self {
    unsupported!("a paint image filter", "the filter is ignored");
    self.image_filter = Some(filter.clone());
    #[cfg(test)]
    {
      self.log.image_filter = true;
    }
    self
  }

  fn set_mask_filter(&mut self, filter: &MaskFilter) -> &mut Self {
    unsupported!("a paint mask filter", "the filter is ignored");
    self.mask_filter = Some(filter.clone());
    #[cfg(test)]
    {
      self.log.mask_filter = true;
    }
    self
  }
}

// The sources and filters below hold their constructor arguments; they are
// read by the executor from stage 3 (gradients, images) and stage 6
// (filters), and by nothing before.

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
  #[cfg(test)]
  log: Arc<PathLog>,
}

impl contract::Path for Path {}

#[derive(Default)]
pub struct PathBuilder {
  #[cfg(test)]
  segments: Vec<PathSegment>,
}

// The points are read only by the test log until stage 3 keeps the
// geometry.
#[cfg_attr(not(test), allow(unused_variables))]
impl contract::Pen for PathBuilder {
  fn move_to(&mut self, p: Point) -> &mut Self {
    #[cfg(test)]
    self.segments.push(PathSegment::MoveTo(p));
    self
  }

  fn line_to(&mut self, p: Point) -> &mut Self {
    #[cfg(test)]
    self.segments.push(PathSegment::LineTo(p));
    self
  }

  fn quadratic_to(&mut self, ctrl: Point, p: Point) -> &mut Self {
    #[cfg(test)]
    self.segments.push(PathSegment::QuadraticTo { ctrl, to: p });
    self
  }

  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point) -> &mut Self {
    #[cfg(test)]
    self.segments.push(PathSegment::CubicTo { ctrl1, ctrl2, to: p });
    self
  }
}

impl contract::PathBuilder for PathBuilder {
  fn close(&mut self) -> &mut Self {
    #[cfg(test)]
    self.segments.push(PathSegment::Close);
    self
  }

  fn build(self, fill: FillType) -> Path {
    Path {
      fill,
      #[cfg(test)]
      log: Arc::new(PathLog { segments: self.segments, fill }),
    }
  }
}
