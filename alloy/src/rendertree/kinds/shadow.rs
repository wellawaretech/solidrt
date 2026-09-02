use crate::impellers::{BlurStyle, Color, MaskFilter, Paint, Rect};
use crate::rendertree::Vector;

// CSS blur radius (the `blur` props) -> gaussian sigma, the browsers'
// box-shadow convention: the visible falloff spans about two sigmas.
pub const BLUR_RADIUS_TO_SIGMA: f32 = 0.5;
// How many sigmas of blurred falloff a damage envelope covers; past three
// the residue is under half a percent of a channel step.
pub const BLUR_EXTENT_SIGMAS: f32 = 3.0;

/// How far a blur with the given CSS-style radius can paint past its
/// geometry's edge. Shared by shadows and view filters.
pub fn blur_reach(radius: f32) -> f32 {
  radius.max(0.0) * BLUR_RADIUS_TO_SIGMA * BLUR_EXTENT_SIGMAS
}

/// A drop shadow behind a shape (CSS box-shadow semantics): the shape's
/// outer geometry, offset by (dx, dy), grown by `spread`, softened by
/// `blur` (a CSS-style radius in logical px), painted in `color` under the
/// shape. It casts from the shape's outer geometry whatever the draw style,
/// like CSS's border box - except on `path`, where the shadow mirrors the
/// element's own fill/stroke (an open path has no interior to cast from)
/// and `spread` is rejected at decode (an arbitrary path cannot be inflated
/// exactly).
#[derive(Clone, Copy, Debug)]
pub struct ShadowState {
  pub dx: f32,
  pub dy: f32,
  pub blur: f32,
  pub spread: f32,
  pub color: Color,
}

impl ShadowState {
  /// The paint the shadow shape draws with: the color, plus the mask blur
  /// when the radius is positive (zero blur is a hard-edged shadow).
  pub fn to_paint(&self) -> Paint {
    let mut paint = Paint::default();
    paint.set_color(self.color);
    if self.blur > 0.0 {
      paint.set_mask_filter(&MaskFilter::new_blur(BlurStyle::Normal, self.blur * BLUR_RADIUS_TO_SIGMA));
    }
    paint
  }

  /// How far past the casting geometry the shadow can paint, per side.
  pub fn outset(&self) -> f32 {
    self.spread.max(0.0) + blur_reach(self.blur)
  }

  /// The painted extent of the shadow cast by `geometry` (the shape's outer
  /// bounds): offset, then grown by the spread and the blur reach.
  pub fn extent_of(&self, geometry: Rect) -> Rect {
    let o = self.outset();
    geometry.translate(Vector::new(self.dx, self.dy)).inflate(o, o)
  }
}
