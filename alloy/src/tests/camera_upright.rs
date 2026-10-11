//! The camera producer's upright rotation (see camera/producer.rs): the
//! tiled quarter turns and the row-reversed half turn against a per-pixel
//! reference, at sizes that are not tile multiples, with a padded pitch.

use crate::camera::producer::upright_into;

/// Bytes of padding after each source row, so a pitch wider than the row
/// is exercised.
const PITCH_PADDING: usize = 12;

/// A source image with a distinct pixel per position (the position bytes
/// themselves), `w` x `h` at a padded pitch.
fn source(w: usize, h: usize) -> (Vec<u8>, usize) {
  let pitch = w * 4 + PITCH_PADDING;
  let mut pixels = vec![0u8; pitch * h];
  for r in 0..h {
    for c in 0..w {
      let at = r * pitch + c * 4;
      pixels[at..at + 4].copy_from_slice(&[r as u8, c as u8, (r + c) as u8, 255]);
    }
  }
  (pixels, pitch)
}

/// The rotation one pixel at a time: destination (row, column) of source
/// (r, c), and the destination's row length.
fn reference(src: &[u8], pitch: usize, w: usize, h: usize, rotation: u32) -> Vec<u8> {
  let (out_w, out_h) = if rotation == 90 || rotation == 270 { (h, w) } else { (w, h) };
  let mut dst = vec![0u8; out_w * out_h * 4];
  for r in 0..h {
    for c in 0..w {
      let (dr, dc) = match rotation {
        90 => (c, h - 1 - r),
        180 => (h - 1 - r, w - 1 - c),
        270 => (w - 1 - c, r),
        _ => (r, c),
      };
      let si = r * pitch + c * 4;
      let di = (dr * out_w + dc) * 4;
      dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
    }
  }
  dst
}

fn check(w: usize, h: usize, rotation: u32) {
  let (src, pitch) = source(w, h);
  let mut dst = Vec::new();
  upright_into(src.as_ptr(), pitch, w, h, rotation, &mut dst);
  assert_eq!(dst.len(), w * h * 4, "{w}x{h} at {rotation}: packed size");
  assert!(dst == reference(&src, pitch, w, h, rotation), "{w}x{h} at {rotation}: pixels differ from the reference");
}

#[test]
fn every_rotation_matches_the_reference_at_odd_sizes() {
  for &(w, h) in &[(1, 1), (3, 2), (16, 16), (17, 5), (5, 17), (40, 33), (64, 48)] {
    for rotation in [0, 90, 180, 270] {
      check(w, h, rotation);
    }
  }
}

#[test]
fn a_quarter_turn_swaps_the_sides() {
  let (src, pitch) = source(7, 3);
  let mut dst = Vec::new();
  upright_into(src.as_ptr(), pitch, 7, 3, 90, &mut dst);
  // The rotated image is 3 wide and 7 tall: its first row is the source's
  // first column read bottom to top.
  let first_row: Vec<u8> = dst[..3 * 4].to_vec();
  assert_eq!(first_row, [2, 0, 2, 255, 1, 0, 1, 255, 0, 0, 0, 255]);
}

#[test]
fn an_oversized_destination_is_cut_to_the_frame() {
  let (src, pitch) = source(4, 4);
  let mut dst = vec![9u8; 4 * 4 * 4 + 64];
  upright_into(src.as_ptr(), pitch, 4, 4, 0, &mut dst);
  assert_eq!(dst.len(), 4 * 4 * 4);
}
