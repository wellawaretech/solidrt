//! The gaussian blur as anvil draws it, its pure half: a layer's blur in
//! device pixels (`DeviceBlur`), the kernel's weights (`gaussian_weights`),
//! how often a large blur halves its content first (`blur_levels`) and the
//! passes it then draws, a backdrop's fill among them (`blur_passes`). The executor runs the passes
//! (exec.rs BLUR_FRAGMENT); the walk sizes a layer by the reach
//! (`blur_reach`). Pure functions, so the tests read them without a
//! context.

use crate::TileMode;

/// How many standard deviations a blur reaches: past three sigmas the
/// residue is under half a percent of a channel step, the same rule the
/// rendertree uses.
pub const BLUR_REACH_SIGMAS: f32 = 3.0;

/// The largest kernel radius, in texels, a blur pass samples: a sigma
/// above BLUR_SIGMA_MAX halves its content first (`blur_levels`), so the
/// radius after downsampling never exceeds it.
pub const BLUR_RADIUS_MAX: usize = 12;

/// The most taps one pass samples: the kernel's full width at
/// BLUR_RADIUS_MAX. exec.rs's GLSL const of the same name must match.
pub const BLUR_TAPS_MAX: usize = 2 * BLUR_RADIUS_MAX + 1;

/// The sigma, in texels, at which the kernel radius reaches
/// BLUR_RADIUS_MAX (BLUR_RADIUS_MAX / BLUR_REACH_SIGMAS): a larger one
/// halves the content until it is at most this.
pub const BLUR_SIGMA_MAX: f32 = 4.0;

/// The smallest sigma, in device pixels, a pass is drawn for: below it the
/// kernel is one tap, the content itself.
pub const BLUR_SIGMA_MIN: f32 = 0.1;

/// The most halvings a blur makes: a 65536-pixel side is one pixel after
/// them, so a larger sigma blurs no further.
pub const BLUR_LEVELS_MAX: u32 = 16;

/// A layer's blur in device pixels: the local sigmas times the map's axis
/// scales (walk.rs `device_blur`), and the tile mode, `Clamp` or `Decal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceBlur {
  pub sigma_x: f32,
  pub sigma_y: f32,
  pub tile: TileMode,
}

/// How far a blur of `sigma` reaches past its content, whole pixels:
/// ceil(BLUR_REACH_SIGMAS * sigma), zero below BLUR_SIGMA_MIN.
pub fn blur_reach(sigma: f32) -> f32 {
  if sigma < BLUR_SIGMA_MIN {
    return 0.0;
  }
  (BLUR_REACH_SIGMAS * sigma).ceil()
}

/// The normalized gaussian of `sigma` over the integer offsets -r..=r, r =
/// ceil(BLUR_REACH_SIGMAS * sigma) (at most BLUR_RADIUS_MAX), the weight of
/// offset k at index k + r; one tap of weight 1 below BLUR_SIGMA_MIN (or a
/// sigma that is not finite).
pub fn gaussian_weights(sigma: f32) -> Vec<f32> {
  if !sigma.is_finite() || sigma < BLUR_SIGMA_MIN {
    return vec![1.0];
  }
  let r = (blur_reach(sigma) as usize).min(BLUR_RADIUS_MAX) as i32;
  let weights: Vec<f32> = (-r..=r).map(|k| (-((k * k) as f32) / (2.0 * sigma * sigma)).exp()).collect();
  let sum: f32 = weights.iter().sum();
  weights.into_iter().map(|w| w / sum).collect()
}

/// How often a blur of the device sigmas (`sigma_x`, `sigma_y`) halves its
/// content before its passes: until the larger sigma, halved with it, is
/// at most BLUR_SIGMA_MAX; at most BLUR_LEVELS_MAX.
pub fn blur_levels(sigma_x: f32, sigma_y: f32) -> u32 {
  let mut sigma = sigma_x.max(sigma_y);
  let mut levels = 0;
  while sigma > BLUR_SIGMA_MAX && levels < BLUR_LEVELS_MAX {
    sigma *= 0.5;
    levels += 1;
  }
  levels
}

/// The sigmas a blur's passes draw at after `levels` halvings, in the
/// halved content's texels.
pub fn level_sigmas(sigma_x: f32, sigma_y: f32, levels: u32) -> (f32, f32) {
  let scale = 0.5f32.powi(levels as i32);
  (sigma_x * scale, sigma_y * scale)
}

/// The passes the executor draws for a layer: the backdrop fill's copy
/// when it is a `backdrop` layer, then for a blur of the device sigmas
/// (zero for none) one per halving (`blur_levels`) and a horizontal and a
/// vertical gaussian pass, each only where its axis's sigma at that level is
/// at least BLUR_SIGMA_MIN; no blur pass when neither sigma is.
pub fn blur_passes(sigma_x: f32, sigma_y: f32, backdrop: bool) -> u32 {
  let fill = u32::from(backdrop);
  if !(sigma_x >= BLUR_SIGMA_MIN || sigma_y >= BLUR_SIGMA_MIN) {
    return fill;
  }
  let levels = blur_levels(sigma_x, sigma_y);
  let (lx, ly) = level_sigmas(sigma_x, sigma_y, levels);
  fill + levels + u32::from(lx >= BLUR_SIGMA_MIN) + u32::from(ly >= BLUR_SIGMA_MIN)
}

/// `content` halved `levels` times, each side rounded up, at least one
/// pixel.
pub fn halved(content: (i32, i32), levels: u32) -> (i32, i32) {
  let mut size = content;
  for _ in 0..levels {
    size = (((size.0 + 1) / 2).max(1), ((size.1 + 1) / 2).max(1));
  }
  size
}
