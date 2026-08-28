use crate::image::{decode, encode_jpeg, encode_png, premultiply_alpha, unpremultiply_alpha};

// 2x1 RGBA with distinct pixels and a non-opaque alpha.
const PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 255, 0, 128];

#[test]
fn png_round_trips_exactly_straight() {
  let png = encode_png(&PIXELS, 2, 1, false).expect("encode png");
  let decoded = decode(&png, false).expect("decode png");
  assert_eq!((decoded.width, decoded.height), (2, 1));
  assert_eq!(decoded.data, PIXELS);
}

#[test]
fn decode_premultiplies_by_request() {
  // Straight-alpha file: white under a = 0 (a keyed-out backdrop), a half
  // transparent green, and an opaque red that must come through untouched.
  let straight: [u8; 12] = [255, 255, 255, 0, 0, 255, 0, 128, 255, 0, 0, 255];
  let png = encode_png(&straight, 3, 1, false).expect("encode png");
  let decoded = decode(&png, true).expect("decode png");
  assert_eq!(decoded.data, [0, 0, 0, 0, 0, 128, 0, 128, 255, 0, 0, 255]);
}

#[test]
fn premultiplied_round_trip_within_rounding() {
  let mut premultiplied = PIXELS;
  premultiply_alpha(&mut premultiplied);
  assert_eq!(premultiplied, [255, 0, 0, 255, 0, 128, 0, 128]);
  let png = encode_png(&premultiplied, 2, 1, true).expect("encode png");
  let back = decode(&png, true).expect("decode png");
  for (got, want) in back.data.iter().zip(premultiplied.iter()) {
    assert!(got.abs_diff(*want) <= 1, "{:?} vs {:?}", back.data, premultiplied);
  }
  let mut straight = premultiplied;
  unpremultiply_alpha(&mut straight);
  assert_eq!(straight, [255, 0, 0, 255, 0, 255, 0, 128]);
}

#[test]
fn jpeg_encodes_and_decodes_opaque() {
  let jpg = encode_jpeg(&PIXELS, 2, 1, 90).expect("encode jpeg");
  let decoded = decode(&jpg, true).expect("decode jpeg");
  assert_eq!((decoded.width, decoded.height), (2, 1));
  // JPEG is lossy and has no alpha; decode re-expands to opaque RGBA.
  assert_eq!(decoded.data.len(), PIXELS.len());
  assert!(decoded.data.iter().skip(3).step_by(4).all(|&a| a == 255));
}

#[test]
fn garbage_bytes_fail_to_decode() {
  assert!(decode(b"not an image", true).is_err());
}

#[test]
fn length_mismatch_rejected() {
  let err = encode_png(&PIXELS, 3, 1, false).expect_err("length check");
  assert!(err.contains("3x1"), "unexpected error: {err}");
}
