//! The drawing vocabulary every renderer speaks: geometry, color and the
//! enums a paint or an op carries. Plain data, owned by alloy, so the
//! rendertree, the flux plugins and the overlay describe a frame without
//! naming the renderer that draws it.

/// A rectangle in logical pixels, origin top-left, y down.
pub type Rect = euclid::Rect<f32, euclid::UnknownUnit>;
/// A point in logical pixels.
pub type Point = euclid::Point2D<f32, euclid::UnknownUnit>;
/// An offset between points, in logical pixels.
pub type Vector = euclid::Vector2D<f32, euclid::UnknownUnit>;
/// An extent in logical pixels.
pub type Size = euclid::Size2D<f32, euclid::UnknownUnit>;
/// An extent in whole pixels: a texture, a framebuffer, a window.
pub type ISize = euclid::Size2D<i64, euclid::UnknownUnit>;
/// A transform in euclid's row-vector convention: `p' = p * m`, so
/// `a.then(&b)` applies `a` first. A builder's `transform` composes it onto
/// the current one the same way.
pub type Matrix = euclid::Transform3D<f32, euclid::UnknownUnit, euclid::UnknownUnit>;

/// A straight-alpha sRGB color, channels 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
  pub red: f32,
  pub green: f32,
  pub blue: f32,
  pub alpha: f32,
}

impl Color {
  /// Every channel zero: paints nothing under source-over.
  pub const TRANSPARENT: Color = Color::new_srgba(0.0, 0.0, 0.0, 0.0);

  pub const fn new_srgba(red: f32, green: f32, blue: f32, alpha: f32) -> Color {
    Color { red, green, blue, alpha }
  }
}

/// The corner radii of a rounded rectangle, each an (x, y) radius pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundingRadii {
  pub top_left: Point,
  pub top_right: Point,
  pub bottom_left: Point,
  pub bottom_right: Point,
}

impl RoundingRadii {
  /// Circular corners from CSS border-radius order: top-left, top-right,
  /// bottom-right, bottom-left.
  pub fn circular([tl, tr, br, bl]: [f32; 4]) -> RoundingRadii {
    let corner = |r: f32| Point::new(r, r);
    RoundingRadii { top_left: corner(tl), top_right: corner(tr), bottom_right: corner(br), bottom_left: corner(bl) }
  }
}

/// A 4x5 color transform, row-major over (R, G, B, A, 1): rows R, G, B, A,
/// the fifth column a translation. Normalized throughout: channels and the
/// translation are both in 0..1, applied in straight-alpha sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorMatrix {
  pub m: [f32; 20],
}

/// Whether a shape's interior, its outline, or both are painted. A stroke
/// is centered on the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrawStyle {
  Fill,
  Stroke,
  StrokeAndFill,
}

/// How a stroke ends at an open end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StrokeCap {
  Butt,
  Round,
  Square,
}

/// How a stroke turns at a vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StrokeJoin {
  Miter,
  Round,
  Bevel,
}

/// Which side of a path is its inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillType {
  NonZero,
  EvenOdd,
}

/// How a clip op combines with the clip in effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipOperation {
  /// Keep what is inside both.
  Intersect,
  /// Cut the shape out of the clip in effect.
  Difference,
}

/// What a gradient or image source shows outside its own extent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TileMode {
  /// The edge color continues.
  Clamp,
  /// The source repeats.
  Repeat,
  /// The source repeats, every other copy reflected.
  Mirror,
  /// Transparent.
  Decal,
}

/// How a texture is sampled when drawn or used as a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureSampling {
  Nearest,
  Linear,
}

/// The Porter-Duff and separable blend modes, in Skia's order and with
/// Skia's semantics (the web canvas's `globalCompositeOperation` names map
/// onto these one to one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
  Clear,
  Source,
  Destination,
  SourceOver,
  DestinationOver,
  SourceIn,
  DestinationIn,
  SourceOut,
  DestinationOut,
  SourceATop,
  DestinationATop,
  Xor,
  Plus,
  Modulate,
  Screen,
  Overlay,
  Darken,
  Lighten,
  ColorDodge,
  ColorBurn,
  HardLight,
  SoftLight,
  Difference,
  Exclusion,
  Multiply,
  Hue,
  Saturation,
  Color,
  Luminosity,
}
