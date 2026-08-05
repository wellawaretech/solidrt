use crate::image::{decode, encode_jpeg, encode_png};

// 2x1 RGBA with distinct pixels and a non-opaque alpha.
const PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 255, 0, 128];

#[test]
fn png_round_trips_exactly() {
  let png = encode_png(&PIXELS, 2, 1).expect("encode png");
  let decoded = decode(&png).expect("decode png");
  assert_eq!((decoded.width, decoded.height), (2, 1));
  assert_eq!(decoded.data, PIXELS);
}

#[test]
fn jpeg_encodes_and_decodes_opaque() {
  let jpg = encode_jpeg(&PIXELS, 2, 1, 90).expect("encode jpeg");
  let decoded = decode(&jpg).expect("decode jpeg");
  assert_eq!((decoded.width, decoded.height), (2, 1));
  // JPEG is lossy and has no alpha; decode re-expands to opaque RGBA.
  assert_eq!(decoded.data.len(), PIXELS.len());
  assert!(decoded.data.iter().skip(3).step_by(4).all(|&a| a == 255));
}

#[test]
fn garbage_bytes_fail_to_decode() {
  assert!(decode(b"not an image").is_err());
}

#[test]
fn length_mismatch_rejected() {
  let err = encode_png(&PIXELS, 3, 1).expect_err("length check");
  assert!(err.contains("3x1"), "unexpected error: {err}");
}