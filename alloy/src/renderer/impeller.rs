//! Impeller behind the renderer contract: every type a newtype over the
//! `impellers` crate's, every call forwarded. The only module in the
//! workspace that names `impellers`.
//!
//! Geometry needs no conversion: the crate's `Point`, `Rect`, `Size`,
//! `ISize` and `Matrix` are the same euclid types as the vocabulary's
//! (alloy's euclid pin tracks the crate's for exactly this). Colors and
//! enums convert by value.

use super::contract;
use super::{
  BlendMode, ClipOperation, Color, ColorMatrix, DrawStyle, FillType, ISize, Matrix, Point, Rect, RoundingRadii,
  StrokeCap, StrokeJoin, TextureSampling, TileMode,
};
use impellers as imp;
use std::num::NonZeroU32;
use std::os::raw::c_void;

pub struct Renderer(imp::Context);

impl contract::Renderer for Renderer {
  unsafe fn new(proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String> {
    unsafe { imp::Context::new_opengl_es(|name| proc_address(name)) }.map(Renderer).map_err(|e| e.to_string())
  }

  fn adopt_texture(&self, texture: glow::NativeTexture, size: ISize) -> Option<Texture> {
    // Adoption hands Impeller the GL name; it deletes the name with the
    // last handle. RGBA8 only: the one format the C API adopts.
    unsafe { self.0.adopt_opengl_texture(size.width as u32, size.height as u32, 1, texture.0.get() as u64) }
      .map(Texture)
  }

  fn draw(&mut self, target: contract::Target, dl: &DisplayList) -> Result<(), String> {
    // A wrapped-FBO surface is use-and-throw: one draw, then dropped. The
    // draw clears the surface first (Impeller's surface contract).
    let fbo = target.framebuffer.map_or(0, |f| f.0.get() as u64);
    let mut surface = unsafe { self.0.wrap_fbo(fbo, imp::PixelFormat::RGBA8888, target.size) }
      .ok_or_else(|| "failed to wrap the framebuffer".to_string())?;
    surface.draw_display_list(&dl.0).map_err(|e| e.to_string())
  }
}

#[derive(Clone)]
pub struct Texture(imp::Texture);

impl contract::Texture for Texture {
  fn gl_name(&self) -> Option<glow::NativeTexture> {
    // The handle is lazily created on Impeller's side; on the raster
    // thread it is always there for an adopted texture.
    NonZeroU32::new(self.0.get_opengl_handle() as u32).map(glow::NativeTexture)
  }
}

#[derive(Clone)]
pub struct DisplayList(imp::DisplayList);

impl contract::DisplayList for DisplayList {}

pub struct DisplayListBuilder(imp::DisplayListBuilder);

impl contract::DisplayListBuilder for DisplayListBuilder {
  fn new(cull: Option<&Rect>) -> Self {
    DisplayListBuilder(imp::DisplayListBuilder::new(cull))
  }

  fn build(mut self) -> Option<DisplayList> {
    self.0.build().map(DisplayList)
  }

  fn save(&mut self) -> &mut Self {
    self.0.save();
    self
  }

  fn save_layer(&mut self, bounds: &Rect, paint: Option<&Paint>, backdrop: Option<&ImageFilter>) -> &mut Self {
    self.0.save_layer(bounds, paint.map(|p| &p.0), backdrop.map(|b| &b.0));
    self
  }

  fn restore(&mut self) -> &mut Self {
    self.0.restore();
    self
  }

  fn translate(&mut self, x: f32, y: f32) -> &mut Self {
    self.0.translate(x, y);
    self
  }

  fn scale(&mut self, x: f32, y: f32) -> &mut Self {
    self.0.scale(x, y);
    self
  }

  fn transform(&mut self, matrix: &Matrix) -> &mut Self {
    self.0.transform(matrix);
    self
  }

  fn clip_rect(&mut self, rect: &Rect, op: ClipOperation) -> &mut Self {
    self.0.clip_rect(rect, clip_operation(op));
    self
  }

  fn clip_oval(&mut self, bounds: &Rect, op: ClipOperation) -> &mut Self {
    self.0.clip_oval(bounds, clip_operation(op));
    self
  }

  fn clip_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, op: ClipOperation) -> &mut Self {
    self.0.clip_rounded_rect(rect, &rounding_radii(radii), clip_operation(op));
    self
  }

  fn draw_line(&mut self, from: Point, to: Point, paint: &Paint) -> &mut Self {
    self.0.draw_line(from, to, &paint.0);
    self
  }

  fn draw_rect(&mut self, rect: &Rect, paint: &Paint) -> &mut Self {
    self.0.draw_rect(rect, &paint.0);
    self
  }

  fn draw_oval(&mut self, bounds: &Rect, paint: &Paint) -> &mut Self {
    self.0.draw_oval(bounds, &paint.0);
    self
  }

  fn draw_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, paint: &Paint) -> &mut Self {
    self.0.draw_rounded_rect(rect, &rounding_radii(radii), &paint.0);
    self
  }

  fn draw_path(&mut self, path: &Path, paint: &Paint) -> &mut Self {
    self.0.draw_path(&path.0, &paint.0);
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
    self.0.draw_texture_rect(&texture.0, src, dst, texture_sampling(sampling), paint.map(|p| &p.0));
    self
  }

  fn draw_display_list(&mut self, dl: &DisplayList, opacity: f32) -> &mut Self {
    self.0.draw_display_list(&dl.0, opacity);
    self
  }
}

#[derive(Default)]
pub struct Paint(imp::Paint);

impl contract::Paint for Paint {
  fn set_color(&mut self, color: Color) -> &mut Self {
    self.0.set_color(imp_color(color));
    self
  }

  fn set_color_source(&mut self, source: &ColorSource) -> &mut Self {
    self.0.set_color_source(&source.0);
    self
  }

  fn set_draw_style(&mut self, style: DrawStyle) -> &mut Self {
    self.0.set_draw_style(match style {
      DrawStyle::Fill => imp::DrawStyle::Fill,
      DrawStyle::Stroke => imp::DrawStyle::Stroke,
      DrawStyle::StrokeAndFill => imp::DrawStyle::StrokeAndFill,
    });
    self
  }

  fn set_blend_mode(&mut self, mode: BlendMode) -> &mut Self {
    self.0.set_blend_mode(blend_mode(mode));
    self
  }

  fn set_stroke_width(&mut self, width: f32) -> &mut Self {
    self.0.set_stroke_width(width);
    self
  }

  fn set_stroke_cap(&mut self, cap: StrokeCap) -> &mut Self {
    self.0.set_stroke_cap(match cap {
      StrokeCap::Butt => imp::StrokeCap::Butt,
      StrokeCap::Round => imp::StrokeCap::Round,
      StrokeCap::Square => imp::StrokeCap::Square,
    });
    self
  }

  fn set_stroke_join(&mut self, join: StrokeJoin) -> &mut Self {
    self.0.set_stroke_join(match join {
      StrokeJoin::Miter => imp::StrokeJoin::Miter,
      StrokeJoin::Round => imp::StrokeJoin::Round,
      StrokeJoin::Bevel => imp::StrokeJoin::Bevel,
    });
    self
  }

  fn set_stroke_miter(&mut self, miter: f32) -> &mut Self {
    self.0.set_stroke_miter(miter);
    self
  }

  fn set_color_filter(&mut self, filter: &ColorFilter) -> &mut Self {
    self.0.set_color_filter(&filter.0);
    self
  }

  fn set_image_filter(&mut self, filter: &ImageFilter) -> &mut Self {
    self.0.set_image_filter(&filter.0);
    self
  }

  fn set_mask_filter(&mut self, filter: &MaskFilter) -> &mut Self {
    self.0.set_mask_filter(&filter.0);
    self
  }
}

pub struct ColorSource(imp::ColorSource);

impl contract::ColorSource for ColorSource {
  fn linear_gradient(
    start: Point,
    end: Point,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    transform: Option<&Matrix>,
  ) -> Self {
    let colors: Vec<imp::Color> = colors.iter().map(|c| imp_color(*c)).collect();
    ColorSource(imp::ColorSource::new_linear_gradient(start, end, &colors, stops, tile_mode(tile), transform))
  }

  fn radial_gradient(
    center: Point,
    radius: f32,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    transform: Option<&Matrix>,
  ) -> Self {
    let colors: Vec<imp::Color> = colors.iter().map(|c| imp_color(*c)).collect();
    ColorSource(imp::ColorSource::new_radial_gradient(center, radius, &colors, stops, tile_mode(tile), transform))
  }

  fn image(
    texture: &Texture,
    tile_x: TileMode,
    tile_y: TileMode,
    sampling: TextureSampling,
    transform: Option<&Matrix>,
  ) -> Self {
    ColorSource(imp::ColorSource::new_image(
      &texture.0,
      tile_mode(tile_x),
      tile_mode(tile_y),
      texture_sampling(sampling),
      transform,
    ))
  }
}

pub struct ColorFilter(imp::ColorFilter);

impl contract::ColorFilter for ColorFilter {
  fn matrix(matrix: &ColorMatrix) -> Self {
    // The C header says the translation column is 0..255, but the shipped
    // implementation adds it raw in normalized color space (verified live,
    // okf/upstream/impeller-color-matrix-translation.md), which is the
    // contract's normalized definition: the matrix passes through unscaled.
    ColorFilter(imp::ColorFilter::new_matrix(imp::ColorMatrix { m: matrix.m }))
  }
}

pub struct ImageFilter(imp::ImageFilter);

impl contract::ImageFilter for ImageFilter {
  fn gaussian_blur(sigma_x: f32, sigma_y: f32, tile: TileMode) -> Self {
    ImageFilter(imp::ImageFilter::new_blur(sigma_x, sigma_y, tile_mode(tile)))
  }
}

pub struct MaskFilter(imp::MaskFilter);

impl contract::MaskFilter for MaskFilter {
  fn blur(sigma: f32) -> Self {
    MaskFilter(imp::MaskFilter::new_blur(imp::BlurStyle::Normal, sigma))
  }
}

#[derive(Clone)]
pub struct Path(imp::Path);

impl contract::Path for Path {}

#[derive(Default)]
pub struct PathBuilder(imp::PathBuilder);

impl contract::PathBuilder for PathBuilder {
  fn move_to(&mut self, p: Point) -> &mut Self {
    self.0.move_to(p);
    self
  }

  fn line_to(&mut self, p: Point) -> &mut Self {
    self.0.line_to(p);
    self
  }

  fn quadratic_to(&mut self, ctrl: Point, p: Point) -> &mut Self {
    self.0.quadratic_curve_to(ctrl, p);
    self
  }

  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point) -> &mut Self {
    self.0.cubic_curve_to(ctrl1, ctrl2, p);
    self
  }

  fn close(&mut self) -> &mut Self {
    self.0.close();
    self
  }

  fn build(mut self, fill: FillType) -> Path {
    Path(self.0.take_path_new(match fill {
      FillType::NonZero => imp::FillType::NonZero,
      FillType::EvenOdd => imp::FillType::Odd,
    }))
  }
}

fn imp_color(c: Color) -> imp::Color {
  imp::Color::new_srgba(c.red, c.green, c.blue, c.alpha)
}

fn rounding_radii(r: &RoundingRadii) -> imp::RoundingRadii {
  imp::RoundingRadii {
    top_left: r.top_left,
    top_right: r.top_right,
    bottom_left: r.bottom_left,
    bottom_right: r.bottom_right,
  }
}

fn clip_operation(op: ClipOperation) -> imp::ClipOperation {
  match op {
    ClipOperation::Intersect => imp::ClipOperation::Intersect,
    ClipOperation::Difference => imp::ClipOperation::Difference,
  }
}

fn tile_mode(tile: TileMode) -> imp::TileMode {
  match tile {
    TileMode::Clamp => imp::TileMode::Clamp,
    TileMode::Repeat => imp::TileMode::Repeat,
    TileMode::Mirror => imp::TileMode::Mirror,
    TileMode::Decal => imp::TileMode::Decal,
  }
}

fn texture_sampling(sampling: TextureSampling) -> imp::TextureSampling {
  match sampling {
    TextureSampling::Nearest => imp::TextureSampling::NearestNeighbor,
    TextureSampling::Linear => imp::TextureSampling::Linear,
  }
}

fn blend_mode(mode: BlendMode) -> imp::BlendMode {
  match mode {
    BlendMode::Clear => imp::BlendMode::Clear,
    BlendMode::Source => imp::BlendMode::Source,
    BlendMode::Destination => imp::BlendMode::Destination,
    BlendMode::SourceOver => imp::BlendMode::SourceOver,
    BlendMode::DestinationOver => imp::BlendMode::DestinationOver,
    BlendMode::SourceIn => imp::BlendMode::SourceIn,
    BlendMode::DestinationIn => imp::BlendMode::DestinationIn,
    BlendMode::SourceOut => imp::BlendMode::SourceOut,
    BlendMode::DestinationOut => imp::BlendMode::DestinationOut,
    BlendMode::SourceATop => imp::BlendMode::SourceATop,
    BlendMode::DestinationATop => imp::BlendMode::DestinationATop,
    BlendMode::Xor => imp::BlendMode::Xor,
    BlendMode::Plus => imp::BlendMode::Plus,
    BlendMode::Modulate => imp::BlendMode::Modulate,
    BlendMode::Screen => imp::BlendMode::Screen,
    BlendMode::Overlay => imp::BlendMode::Overlay,
    BlendMode::Darken => imp::BlendMode::Darken,
    BlendMode::Lighten => imp::BlendMode::Lighten,
    BlendMode::ColorDodge => imp::BlendMode::ColorDodge,
    BlendMode::ColorBurn => imp::BlendMode::ColorBurn,
    BlendMode::HardLight => imp::BlendMode::HardLight,
    BlendMode::SoftLight => imp::BlendMode::SoftLight,
    BlendMode::Difference => imp::BlendMode::Difference,
    BlendMode::Exclusion => imp::BlendMode::Exclusion,
    BlendMode::Multiply => imp::BlendMode::Multiply,
    BlendMode::Hue => imp::BlendMode::Hue,
    BlendMode::Saturation => imp::BlendMode::Saturation,
    BlendMode::Color => imp::BlendMode::Color,
    BlendMode::Luminosity => imp::BlendMode::Luminosity,
  }
}
