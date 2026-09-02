use super::shadow::{blur_reach, BLUR_RADIUS_TO_SIGMA};
use crate::impellers::{ColorFilter, ColorMatrix, ImageFilter, TileMode};

// The ImpellerColorMatrix header says the translation column is 0..255, but
// the shipped implementation adds it raw in normalized 0..1 color space
// (verified live: a 255 offset renders white, a normalized invert renders
// correctly - okf/upstream/impeller-color-matrix-translation.md). The
// composition below is normalized throughout, so the column passes through
// unscaled.

// Rec. 601-derived luminance weights, the constants SVG's feColorMatrix
// saturate/hueRotate types are specified with (CSS filter functions defer to
// them).
const LUM_R: f32 = 0.213;
const LUM_G: f32 = 0.715;
const LUM_B: f32 = 0.072;

/// A subtree filter on a view (CSS `filter` semantics through built-in
/// Impeller filters): the composited children, run through the set color
/// operations (one fused color matrix) and then the blur. Field semantics
/// match the CSS filter functions of the same names; `hue_rotate` is radians
/// (this API's rotate convention), `blur` a CSS-style radius in logical px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FilterState {
  pub blur: Option<f32>,
  pub grayscale: Option<f32>,
  pub sepia: Option<f32>,
  pub saturate: Option<f32>,
  pub hue_rotate: Option<f32>,
  pub brightness: Option<f32>,
  pub contrast: Option<f32>,
  pub invert: Option<f32>,
}

// A normalized 4x5 color transform (rows R,G,B,A; the 5th column is the
// translation, still in 0..1 here).
type Matrix4x5 = [[f32; 5]; 4];

const IDENTITY: Matrix4x5 =
  [[1.0, 0.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0, 0.0]];

// after * before, treating both as 5x5 with an identity last row: the
// combined transform applies `before` first.
fn compose(after: &Matrix4x5, before: &Matrix4x5) -> Matrix4x5 {
  let mut out = [[0.0; 5]; 4];
  for row in 0..4 {
    for col in 0..5 {
      let mut v = 0.0;
      for k in 0..4 {
        v += after[row][k] * before[k][col];
      }
      if col == 4 {
        v += after[row][4];
      }
      out[row][col] = v;
    }
  }
  out
}

// (1 - k) * identity + k * target on the RGB rows; alpha stays identity.
fn lerp_rgb(target: [[f32; 3]; 3], k: f32) -> Matrix4x5 {
  let mut m = IDENTITY;
  for row in 0..3 {
    for col in 0..3 {
      let id = if row == col { 1.0 } else { 0.0 };
      m[row][col] = id + (target[row][col] - id) * k;
    }
  }
  m
}

impl FilterState {
  pub fn is_empty(&self) -> bool {
    *self == FilterState::default()
  }

  /// The blur as an Impeller image filter; None when unset or zero. Decal
  /// tiling keeps the region's surroundings transparent, so a blurred panel
  /// fades at its edge instead of smearing clamped border pixels.
  pub fn to_image_filter(&self) -> Option<ImageFilter> {
    let radius = self.blur.filter(|r| *r > 0.0)?;
    let sigma = radius * BLUR_RADIUS_TO_SIGMA;
    Some(ImageFilter::new_blur(sigma, sigma, TileMode::Decal))
  }

  /// All set color operations fused into one Impeller color filter; None
  /// when no color key is set.
  pub fn to_color_filter(&self) -> Option<ColorFilter> {
    self.color_matrix().map(ColorFilter::new_matrix)
  }

  /// How far the blur can paint past the subtree's extent, per side.
  pub fn blur_outset(&self) -> f32 {
    blur_reach(self.blur.unwrap_or(0.0))
  }

  /// The image filter for a save_layer's backdrop argument: the blur, or a
  /// sub-pixel blur when only color keys are set - the backdrop argument is
  /// what makes save_layer capture the pixels beneath at all, and the color
  /// transform then rides the restore paint (composite::emit_backdrop). A
  /// blur stands in for the identity because the prebuilt Impeller's matrix
  /// image filter cannot be constructed at all
  /// (ImpellerImageFilterCreateMatrixNew returns null -
  /// okf/upstream/impeller-image-filter-matrix-null.md).
  /// Clamp tiling: the backdrop is a filled surface, so unlike the subtree
  /// blur there is no transparent edge for Decal to preserve, and Decal
  /// would darken the region's border where the kernel reads outside the
  /// captured bounds.
  pub fn to_backdrop_image_filter(&self) -> ImageFilter {
    // Far below one pixel: visually the identity, but a real, constructible
    // filter that still triggers the backdrop capture.
    const BACKDROP_CAPTURE_SIGMA: f32 = 0.001;
    let sigma = match self.blur.filter(|r| *r > 0.0) {
      Some(radius) => radius * BLUR_RADIUS_TO_SIGMA,
      None => BACKDROP_CAPTURE_SIGMA,
    };
    ImageFilter::new_blur(sigma, sigma, TileMode::Clamp)
  }

  // The fused normalized matrix, applied in a fixed documented order:
  // grayscale, sepia, saturate, hueRotate, brightness, contrast, invert.
  // Object props cannot express author ordering and these do not need it.
  fn normalized_matrix(&self) -> Option<Matrix4x5> {
    let mut m: Option<Matrix4x5> = None;
    let mut push = |op: Matrix4x5| {
      m = Some(match &m {
        Some(prev) => compose(&op, prev),
        None => op,
      });
    };

    // grayscale/sepia/invert saturate at 1, like the CSS functions.
    if let Some(k) = self.grayscale {
      let k = k.clamp(0.0, 1.0);
      push(lerp_rgb([[LUM_R, LUM_G, LUM_B]; 3], k));
    }
    if let Some(k) = self.sepia {
      let k = k.clamp(0.0, 1.0);
      push(lerp_rgb([[0.393, 0.769, 0.189], [0.349, 0.686, 0.168], [0.272, 0.534, 0.131]], k));
    }
    if let Some(s) = self.saturate {
      // The feColorMatrix "saturate" matrix: luminance plus s of the excess.
      push(lerp_rgb([[LUM_R, LUM_G, LUM_B]; 3], 1.0 - s));
    }
    if let Some(rad) = self.hue_rotate {
      let (sin, cos) = rad.sin_cos();
      #[rustfmt::skip]
      let rows = [
        [LUM_R + cos * (1.0 - LUM_R) - sin * LUM_R, LUM_G - cos * LUM_G - sin * LUM_G, LUM_B - cos * LUM_B + sin * (1.0 - LUM_B)],
        [LUM_R - cos * LUM_R + sin * 0.143,          LUM_G + cos * (1.0 - LUM_G) + sin * 0.140, LUM_B - cos * LUM_B - sin * 0.283],
        [LUM_R - cos * LUM_R - sin * (1.0 - LUM_R), LUM_G - cos * LUM_G + sin * LUM_G, LUM_B + cos * (1.0 - LUM_B) + sin * LUM_B],
      ];
      push(lerp_rgb(rows, 1.0));
    }
    if let Some(b) = self.brightness {
      let mut op = IDENTITY;
      for row in op.iter_mut().take(3) {
        for v in row.iter_mut().take(3) {
          *v *= b;
        }
      }
      push(op);
    }
    if let Some(c) = self.contrast {
      // Slope c around the 0.5 pivot: v' = c * v + (0.5 - 0.5 * c).
      let mut op = IDENTITY;
      for row in op.iter_mut().take(3) {
        for v in row.iter_mut().take(3) {
          *v *= c;
        }
        row[4] = 0.5 - 0.5 * c;
      }
      push(op);
    }
    if let Some(k) = self.invert {
      // Lerp toward the inverted channel: v' = (1 - 2k) * v + k.
      let k = k.clamp(0.0, 1.0);
      let mut op = IDENTITY;
      for row in op.iter_mut().take(3) {
        for v in row.iter_mut().take(3) {
          *v *= 1.0 - 2.0 * k;
        }
        row[4] = k;
      }
      push(op);
    }
    m
  }

  fn color_matrix(&self) -> Option<ColorMatrix> {
    let m = self.normalized_matrix()?;
    let mut out = [0.0f32; 20];
    for row in 0..4 {
      for col in 0..5 {
        out[row * 5 + col] = m[row][col];
      }
    }
    Some(ColorMatrix { m: out })
  }
}

#[cfg(test)]
pub(crate) fn matrix_for_tests(f: &FilterState) -> Option<[f32; 20]> {
  f.color_matrix().map(|cm| cm.m)
}
