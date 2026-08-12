use crate::texture::TextureFormat;
use crate::yuv::{coefficients, fragment_src, frame_size, planes, YuvLayout, YuvMatrix, YuvRange};

#[test]
fn nv12_planes_match_the_probed_decoder_layout() {
  // 1920x1080: Y then interleaved UV at half resolution, tightly packed.
  let p = planes(YuvLayout::Nv12, 1920, 1080);
  assert_eq!(p.len(), 2);
  assert_eq!((p[0].name, p[0].width, p[0].height, p[0].format, p[0].offset), ("uY", 1920, 1080, TextureFormat::R8, 0));
  assert_eq!(
    (p[1].name, p[1].width, p[1].height, p[1].format, p[1].offset),
    ("uUV", 960, 540, TextureFormat::Rg8, 1920 * 1080)
  );
  assert_eq!(frame_size(YuvLayout::Nv12, 1920, 1080), 1920 * 1080 * 3 / 2);
}

#[test]
fn i420_planes_are_y_u_v_in_order() {
  let p = planes(YuvLayout::I420, 64, 48);
  assert_eq!(p.len(), 3);
  assert_eq!((p[0].name, p[0].offset), ("uY", 0));
  assert_eq!((p[1].name, p[1].width, p[1].height, p[1].format), ("uU", 32, 24, TextureFormat::R8));
  assert_eq!(p[1].offset, 64 * 48);
  assert_eq!((p[2].name, p[2].offset), ("uV", 64 * 48 + 32 * 24));
  assert_eq!(frame_size(YuvLayout::I420, 64, 48), 64 * 48 * 3 / 2);
}

#[test]
fn odd_sizes_round_chroma_up() {
  // 5x3: chroma covers 3x2; frame size counts the rounded planes.
  let p = planes(YuvLayout::Nv12, 5, 3);
  assert_eq!((p[1].width, p[1].height), (3, 2));
  assert_eq!(frame_size(YuvLayout::Nv12, 5, 3), 5 * 3 + 3 * 2 * 2);
  let p = planes(YuvLayout::I420, 5, 3);
  assert_eq!(frame_size(YuvLayout::I420, 5, 3), 5 * 3 + 3 * 2 * 2);
}

#[test]
fn coefficients_hit_the_standard_bt601_limited_values() {
  // The textbook BT.601 studio-range constants: R = 1.164(Y-16) + 1.596 Cr' etc.
  // (on 0..255 scales; ours are normalized, the ratios are what is pinned).
  let [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u] = coefficients(YuvMatrix::Bt601, YuvRange::Limited);
  assert!((y_scale - 255.0 / 219.0).abs() < 1e-6);
  assert!((y_offset - 16.0 / 255.0).abs() < 1e-6);
  assert!((r_v * c_scale - 1.596).abs() < 1e-3);
  assert!((r_v - 1.402).abs() < 1e-3);
  assert!((g_u + 0.344).abs() < 1e-3);
  assert!((g_v + 0.714).abs() < 1e-3);
  assert!((b_u - 1.772).abs() < 1e-3);
}

#[test]
fn full_range_is_identity_scaling() {
  let [y_scale, y_offset, c_scale, ..] = coefficients(YuvMatrix::Bt709, YuvRange::Full);
  assert_eq!((y_scale, y_offset, c_scale), (1.0, 0.0, 1.0));
}

#[test]
fn fragment_src_declares_the_layout_samplers() {
  let nv12 = fragment_src(YuvLayout::Nv12, YuvMatrix::Bt709, YuvRange::Limited);
  assert!(nv12.contains("uniform sampler2D uY;") && nv12.contains("uniform sampler2D uUV;"));
  assert!(!nv12.contains("uU;"));
  let i420 = fragment_src(YuvLayout::I420, YuvMatrix::Bt601, YuvRange::Full);
  assert!(i420.contains("uniform sampler2D uU;") && i420.contains("uniform sampler2D uV;"));
}
