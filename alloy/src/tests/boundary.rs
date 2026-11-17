// A snapshot boundary's texel arithmetic: the dimensions a canvas takes
// (boundary::canvas_texels, what SnapshotKey::texture_dims reads per axis)
// and the whole-texel box a raster made under a running track holds
// (boundary::whole_texels). The rasters themselves are the GL example's
// (alloy/examples/boundary_scale.rs): the headless raster here refuses them.
use crate::rendertree::boundary::{canvas_texels, whole_texels};

// A density a box divided by it and multiplied back lands a float noise
// above a whole count at (60 texels at it come out at 60.000004).
const NOISY_SCALE: f32 = 0.741462;
const NOISY_TEXELS: f32 = 60.0;
// A canvas large enough that the noise of a held scale (a fraction of
// 1e-5, text::SCALE_HELD_EPSILON) passes the absolute tolerance.
const LARGE_TEXELS: f32 = 4000.0;
const HELD_NOISE: f32 = 1e-5;
// Densities and sides the whole-texel box is checked over.
const DENSITIES: [f32; 5] = [0.5, 0.75, 1.0, 1.35, 2.0 / 3.0];
const SIDES: [f32; 5] = [0.3, 27.47, 81.07, 100.3, 1234.56];

#[test]
fn a_canvas_a_float_noise_above_whole_texels_takes_those_texels() {
  let side = NOISY_TEXELS / NOISY_SCALE;
  assert!(side * NOISY_SCALE > NOISY_TEXELS, "the case is noisy: {}", side * NOISY_SCALE);
  assert_eq!(canvas_texels(side, NOISY_SCALE), NOISY_TEXELS as u32);
  // Exactly whole, and a hair below whole, are that count too.
  assert_eq!(canvas_texels(NOISY_TEXELS, 1.0), NOISY_TEXELS as u32);
  assert_eq!(canvas_texels(NOISY_TEXELS - 1e-4, 1.0), NOISY_TEXELS as u32);
}

#[test]
fn a_real_sliver_takes_one_more_texel() {
  assert_eq!(canvas_texels(60.3, 1.0), 61);
  assert_eq!(canvas_texels(60.01, 1.0), 61);
  assert_eq!(canvas_texels(0.5, 1.0), 1);
  // Past the relative tolerance at a large count too.
  assert_eq!(canvas_texels(LARGE_TEXELS + 0.5, 1.0), LARGE_TEXELS as u32 + 1);
}

#[test]
fn the_tolerance_grows_with_the_canvas() {
  // A box snapped on a grid held at the raster's density is off by up to
  // that fraction of its texels: past the absolute tolerance at this size,
  // within the relative one.
  let noisy = LARGE_TEXELS * (1.0 + HELD_NOISE);
  assert!(noisy - LARGE_TEXELS > 1e-3, "the noise passes the absolute tolerance");
  assert_eq!(canvas_texels(noisy, 1.0), LARGE_TEXELS as u32);
}

#[test]
fn no_canvas_takes_no_texels() {
  assert_eq!(canvas_texels(0.0, 1.0), 0);
  assert_eq!(canvas_texels(-3.0, 1.0), 0);
  assert_eq!(canvas_texels(10.0, 0.0), 0);
  assert_eq!(canvas_texels(f32::NAN, 1.0), 0);
}

#[test]
fn a_whole_texel_box_takes_exactly_its_rounded_texels() {
  for scale in DENSITIES {
    for side in SIDES {
      let whole = whole_texels(side, scale);
      let texels = (side * scale).round().max(1.0);
      assert_eq!(canvas_texels(whole, scale), texels as u32, "{side} at {scale}");
      // Within half a texel of the side it was made for, or the one-texel
      // minimum.
      assert!((whole - side).abs() * scale <= 0.5 || texels == 1.0, "{side} at {scale}: {whole}");
    }
  }
}

#[test]
fn a_whole_texel_box_is_at_least_one_texel_and_nothing_stays_nothing() {
  assert_eq!(whole_texels(0.2, 1.0), 1.0);
  assert_eq!(canvas_texels(whole_texels(0.2, 0.5), 0.5), 1);
  assert_eq!(whole_texels(0.0, 1.0), 0.0);
  assert_eq!(whole_texels(-2.0, 1.0), -2.0);
  // A scale that is no scale leaves the side as it is.
  assert_eq!(whole_texels(12.5, 0.0), 12.5);
  assert_eq!(whole_texels(12.5, f32::INFINITY), 12.5);
}
