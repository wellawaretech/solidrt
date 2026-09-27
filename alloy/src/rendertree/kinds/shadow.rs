use super::paint::PaintState;
use crate::impellers::{BlurStyle, Color, ImageFilter, MaskFilter, Paint, Rect, TileMode};
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

  /// Whether this shadow must draw inside a save_layer of its own to
  /// composite correctly under an ancestor's group opacity. Impeller's
  /// opacity peephole hands a group's alpha down to the ops inside the
  /// group when they do not overlap, and the mask-blurred draw a shadow
  /// normally is refuses inherited alpha (its Contents::CanAcceptOpacity
  /// is false), which logs an ImpellerValidationBreak per frame and draws
  /// the shadow unfaded. A visibly painted shape whose box meets the
  /// shadow's extent overlaps it in the peephole's bounds test and blocks
  /// the handdown by itself; a shape whose paint is elided (a transparent
  /// fill, the glass idiom), or whose offset moves the shadow clear of its
  /// own box, leaves the blurred draw distributable, so it goes into a
  /// layer of its own via `to_layer_paint`. `geometry` is the shape's
  /// painted outer box, the same box the shadow casts from. A hard-edged
  /// shadow (no blur) is a plain solid draw that accepts the alpha and
  /// needs no layer.
  pub fn needs_own_layer(&self, paint: &PaintState, geometry: Rect) -> bool {
    if self.blur <= 0.0 {
      return false;
    }
    !paint.paints_visibly() || !self.extent_of(geometry).intersects(&geometry)
  }

  /// The save_layer paint for a shadow in its own layer (`needs_own_layer`):
  /// the blur moves off the draw's mask filter onto the layer as an image
  /// filter - blurring the hard shape's layer is the same convolution the
  /// mask filter runs on the shape, but a bare or paint-only layer is
  /// collapsed back into the parent pass (which re-exposes the blurred draw
  /// to the parent's opacity peephole), while a filtered layer must render,
  /// and it takes a handed-down alpha on its composite correctly. Draw the
  /// shape inside with `to_hard_paint`.
  pub fn to_layer_paint(&self) -> Paint {
    let sigma = self.blur * BLUR_RADIUS_TO_SIGMA;
    let mut paint = Paint::default();
    paint.set_image_filter(&ImageFilter::new_blur(sigma, sigma, TileMode::Decal));
    paint
  }

  /// The shadow shape's paint without the blur: the color alone, for the
  /// draw inside a `to_layer_paint` layer, which blurs the whole layer.
  pub fn to_hard_paint(&self) -> Paint {
    let mut paint = Paint::default();
    paint.set_color(self.color);
    paint
  }
}
