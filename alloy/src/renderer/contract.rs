//! The contract a renderer implements: one trait per renderer type, written
//! against this build's aliases (`super::Texture`, `super::DisplayList`,
//! ...), so a second implementation swaps the aliases and the compiler lists
//! what it is missing.
//!
//! Recording and drawing are separate halves. Recording (a
//! `DisplayListBuilder` and what it takes) runs on any thread and touches no
//! GL: the rendertree records on the UI thread, the raster thread records
//! the small wrappers it needs. Drawing (`Renderer`) runs only on the
//! raster thread, with the process's GL context current.
//!
//! Coordinates are logical pixels, origin top-left, y down; the caller
//! scales to device pixels in the list itself. A framebuffer is drawn as GL
//! stores it, bottom-up, so a list meant for a texture that is later sampled
//! upright is pre-flipped by the caller (`raster::offscreen::flip_for_fbo`).
//! Anti-aliasing is the caller's: it draws into multisampled storage and
//! resolves; the renderer rasterizes without coverage AA.

use super::vocab::{
  BlendMode, ClipOperation, Color, ColorMatrix, DrawStyle, FillType, ISize, Matrix, Point, Rect, RoundingRadii,
  StrokeCap, StrokeJoin, TextureSampling, TileMode,
};
use std::os::raw::c_void;

/// Where a draw lands: a framebuffer by GL name (None is the window's
/// default framebuffer, FBO 0) and its size in pixels. Every target is
/// RGBA8.
#[derive(Clone, Copy, Debug)]
pub struct Target {
  pub framebuffer: Option<glow::NativeFramebuffer>,
  pub size: ISize,
}

/// The drawing half, owned by the raster thread beside the GL context.
pub trait Renderer: Sized {
  /// Create the renderer over the GL context current on the calling
  /// thread, resolving entry points through `proc_address`.
  ///
  /// # Safety
  /// The calling thread must have the GL context current for the
  /// renderer's whole life, and `proc_address` must resolve that context's
  /// entry points.
  unsafe fn new(proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String>;

  /// Adopt a live 2D RGBA8 GL texture of `size` into the renderer as a
  /// `Texture` the lists can draw. The renderer owns the GL name from here
  /// on and deletes it when the last handle drops; the caller never deletes
  /// it. None when the renderer cannot draw textures of this kind (today a
  /// format other than RGBA8), in which case the name stays the caller's.
  fn adopt_texture(&self, texture: glow::NativeTexture, size: ISize) -> Option<super::Texture>;

  /// Draw a display list into `target`, replacing its contents: the target
  /// is cleared first. The renderer may leave any GL state changed; the
  /// caller rebinds what it relies on afterwards.
  fn draw(&mut self, target: Target, dl: &super::DisplayList) -> Result<(), String>;
}

/// A texture the renderer can draw: a refcounted handle, shared across
/// threads, whose last drop releases the storage.
pub trait Texture: Clone + Send + Sync {
  /// The GL name behind the handle, for alloy's own passes to sample or
  /// render into; None when the renderer has not materialized one.
  fn gl_name(&self) -> Option<glow::NativeTexture>;
}

/// A finished recording: immutable, refcounted, shared across threads.
/// Drawn by `Renderer::draw` or nested into another list by
/// `DisplayListBuilder::draw_display_list`, which keeps it by reference, so
/// a retained list (a repaint boundary's) is recorded once and replayed
/// from every frame that nests it.
pub trait DisplayList: Clone + Send + Sync {}

/// Records a display list. Every call appends one op; the state ops
/// (save, clip, transform) scope the draws after them until the matching
/// restore. Returns `&mut Self` so calls chain.
pub trait DisplayListBuilder: Sized {
  /// A builder, optionally with a cull rect: ops entirely outside it may
  /// be dropped at record time.
  fn new(cull: Option<&Rect>) -> Self;
  /// The recording; None when nothing could be recorded.
  fn build(self) -> Option<super::DisplayList>;

  /// Push the transform and clip state; `restore` pops it.
  fn save(&mut self) -> &mut Self;
  /// Push state and open a layer over `bounds`: the draws until the
  /// matching restore render offscreen and composite as one with `paint`
  /// (its alpha as group opacity, its filters over the group).
  fn save_layer(&mut self, bounds: &Rect, paint: Option<&super::Paint>) -> &mut Self;
  /// `save_layer` whose layer starts from the pixels already beneath
  /// `bounds` (within the clip in effect), run through `filter` when one
  /// is given; the restore paints them back with `paint`. A backdrop
  /// filter is this with the restore immediately after.
  fn save_backdrop_layer(
    &mut self,
    bounds: &Rect,
    paint: Option<&super::Paint>,
    filter: Option<&super::ImageFilter>,
  ) -> &mut Self;
  fn restore(&mut self) -> &mut Self;

  fn translate(&mut self, x: f32, y: f32) -> &mut Self;
  fn scale(&mut self, x: f32, y: f32) -> &mut Self;
  /// Compose `matrix` onto the current transform (applied before it).
  fn transform(&mut self, matrix: &Matrix) -> &mut Self;

  fn clip_rect(&mut self, rect: &Rect, op: ClipOperation) -> &mut Self;
  fn clip_oval(&mut self, bounds: &Rect, op: ClipOperation) -> &mut Self;
  fn clip_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, op: ClipOperation) -> &mut Self;

  /// A segment, always stroked with the paint's stroke whatever its draw
  /// style.
  fn draw_line(&mut self, from: Point, to: Point, paint: &super::Paint) -> &mut Self;
  fn draw_rect(&mut self, rect: &Rect, paint: &super::Paint) -> &mut Self;
  fn draw_oval(&mut self, bounds: &Rect, paint: &super::Paint) -> &mut Self;
  fn draw_rounded_rect(&mut self, rect: &Rect, radii: &RoundingRadii, paint: &super::Paint) -> &mut Self;
  fn draw_path(&mut self, path: &super::Path, paint: &super::Paint) -> &mut Self;
  /// The `src` texels of `texture` mapped onto `dst`. With a paint, its
  /// color modulates the texels (white keeps them), its alpha fades them
  /// and its filters and blend apply; without, a plain source-over copy.
  fn draw_texture_rect(
    &mut self,
    texture: &super::Texture,
    src: &Rect,
    dst: &Rect,
    sampling: TextureSampling,
    paint: Option<&super::Paint>,
  ) -> &mut Self;
  /// Nest `dl` under the current state, faded by `opacity` as a group.
  fn draw_display_list(&mut self, dl: &super::DisplayList, opacity: f32) -> &mut Self;
}

/// How a draw paints: a solid color or a source, the style and stroke
/// metrics, the blend mode, and the filters. Starts at a solid opaque
/// fill, source-over, width 1, butt caps, miter joins at limit 4.
pub trait Paint: Default {
  fn set_color(&mut self, color: Color) -> &mut Self;
  /// Paint from `source` instead of the color; the color's alpha still
  /// fades it.
  fn set_color_source(&mut self, source: &super::ColorSource) -> &mut Self;
  fn set_draw_style(&mut self, style: DrawStyle) -> &mut Self;
  fn set_blend_mode(&mut self, mode: BlendMode) -> &mut Self;
  fn set_stroke_width(&mut self, width: f32) -> &mut Self;
  fn set_stroke_cap(&mut self, cap: StrokeCap) -> &mut Self;
  fn set_stroke_join(&mut self, join: StrokeJoin) -> &mut Self;
  /// The miter limit, as a ratio of miter length to stroke width.
  fn set_stroke_miter(&mut self, miter: f32) -> &mut Self;
  /// Transform the draw's colors after they are produced.
  fn set_color_filter(&mut self, filter: &super::ColorFilter) -> &mut Self;
  /// Filter the draw's pixels (or a layer's, on a `save_layer` paint).
  fn set_image_filter(&mut self, filter: &super::ImageFilter) -> &mut Self;
  /// Filter the draw's coverage mask before it is painted.
  fn set_mask_filter(&mut self, filter: &super::MaskFilter) -> &mut Self;
}

/// What a paint draws with in place of a solid color.
pub trait ColorSource: Sized {
  /// `colors[i]` at `stops[i]` (0..1 along start to end), `tile` beyond
  /// the ends; `transform` maps gradient space into the draw's space.
  fn linear_gradient(
    start: Point,
    end: Point,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    transform: Option<&Matrix>,
  ) -> Self;
  /// `colors[i]` at `stops[i]` (0..1 from the center to the radius).
  fn radial_gradient(
    center: Point,
    radius: f32,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    transform: Option<&Matrix>,
  ) -> Self;
  /// The texture's texels, `transform` mapping texel space into the
  /// draw's space.
  fn image(
    texture: &super::Texture,
    tile_x: TileMode,
    tile_y: TileMode,
    sampling: TextureSampling,
    transform: Option<&Matrix>,
  ) -> Self;
}

/// A per-pixel color transform.
pub trait ColorFilter: Sized {
  fn matrix(matrix: &ColorMatrix) -> Self;
}

/// A filter over a draw's or a layer's pixels.
pub trait ImageFilter: Sized {
  /// A gaussian blur with the given sigmas, `tile` deciding what the
  /// kernel reads past the edge.
  fn gaussian_blur(sigma_x: f32, sigma_y: f32, tile: TileMode) -> Self;
}

/// A filter over a draw's coverage mask.
pub trait MaskFilter: Sized {
  /// A gaussian blur of the mask with sigma `sigma`, inside and out.
  fn blur(sigma: f32) -> Self;
}

/// A finished path: immutable, cheap to clone, kept by the element that
/// draws it across frames.
pub trait Path: Clone {}

/// Where a path walk draws: the segment vocabulary of a `PathBuilder`,
/// and of anything else that wants to see a walk (the dash walker's
/// output, a recording pen in a test). A curve arrives as a curve, so a
/// stroker sees the geometry a solid stroke would.
pub trait Pen {
  fn move_to(&mut self, p: Point) -> &mut Self;
  fn line_to(&mut self, p: Point) -> &mut Self;
  fn quadratic_to(&mut self, ctrl: Point, p: Point) -> &mut Self;
  fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, p: Point) -> &mut Self;
}

/// Builds a path from a pen walk. Starts empty.
pub trait PathBuilder: Default + Pen {
  /// Close the current subpath back to its start.
  fn close(&mut self) -> &mut Self;
  /// The path, with `fill` deciding its inside.
  fn build(self, fill: FillType) -> super::Path;
}
