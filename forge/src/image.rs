//! Engine-free image codec: encoded bytes (PNG, JPEG, ...) to tightly-packed
//! RGBA8 pixels and back. Pure CPU, no GPU or scripting types; the `flux:image`
//! module is the marshalling layer over this.
//!
//! Alpha: image files store straight (non-premultiplied) alpha, while every
//! pixel on the GPU side is premultiplied (the pixel contract). The codec is
//! the boundary, so `decode` premultiplies on the way in and `encode_png`
//! unpremultiplies on the way out, each behind a flag the caller sets when it
//! wants the file's bytes verbatim.

use image::ImageEncoder as _;

/// Decoded pixels: tightly-packed RGBA8 plus the pixel dimensions.
pub struct DecodedImage {
  pub data: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

/// Decode encoded image bytes (any enabled format: png, jpeg, webp, gif, bmp,
/// ico) into RGBA8. With `premultiply` each color channel is scaled by its
/// alpha, so the pixels are ready for the GPU; without it they are the file's
/// straight alpha.
pub fn decode(bytes: &[u8], premultiply: bool) -> Result<DecodedImage, String> {
  let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
  let rgba = img.to_rgba8();
  let width = rgba.width();
  let height = rgba.height();
  let mut data = rgba.into_raw();
  if premultiply {
    premultiply_alpha(&mut data);
  }
  Ok(DecodedImage { data, width, height })
}

/// Rounding bias for the /255 in premultiplication (half of 255).
const HALF_UNIT: u16 = 127;

/// `c = c * a / 255` per color channel, rounded. Opaque pixels are unchanged.
pub fn premultiply_alpha(data: &mut [u8]) {
  for px in data.chunks_exact_mut(4) {
    let a = px[3] as u16;
    if a == 255 {
      continue;
    }
    for c in &mut px[..3] {
      *c = ((*c as u16 * a + HALF_UNIT) / 255) as u8;
    }
  }
}

/// The inverse of `premultiply_alpha`: `c = c * 255 / a`, rounded and clamped.
/// Lossy at low alpha (a texel with `a = 3` keeps two bits of color), exact
/// for opaque pixels; fully transparent pixels come out black.
pub fn unpremultiply_alpha(data: &mut [u8]) {
  for px in data.chunks_exact_mut(4) {
    let a = px[3] as u32;
    if a == 255 {
      continue;
    }
    for c in &mut px[..3] {
      *c = if a == 0 { 0 } else { ((*c as u32 * 255 + a / 2) / a).min(255) as u8 };
    }
  }
}

fn check_len(data: &[u8], width: u32, height: u32) -> Result<(), String> {
  let expected = width as usize * height as usize * 4;
  if data.len() != expected {
    return Err(format!("data length {} does not match {width}x{height} RGBA ({expected})", data.len()));
  }
  Ok(())
}

/// Encode RGBA8 pixels as PNG (lossless, alpha preserved). PNG stores straight
/// alpha, so premultiplied input (`unpremultiply`) is converted first; straight
/// input is written verbatim.
pub fn encode_png(data: &[u8], width: u32, height: u32, unpremultiply: bool) -> Result<Vec<u8>, String> {
  check_len(data, width, height)?;
  let straight;
  let pixels = if unpremultiply {
    let mut copy = data.to_vec();
    unpremultiply_alpha(&mut copy);
    straight = copy;
    straight.as_slice()
  } else {
    data
  };
  let mut out = Vec::new();
  image::codecs::png::PngEncoder::new(&mut out)
    .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
    .map_err(|e| e.to_string())?;
  Ok(out)
}

/// Encode RGBA8 pixels as JPEG at `quality` (clamped to 1..=100). JPEG has no
/// alpha channel; it is dropped.
pub fn encode_jpeg(data: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>, String> {
  check_len(data, width, height)?;
  let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
  for px in data.chunks_exact(4) {
    rgb.extend_from_slice(&px[..3]);
  }
  let mut out = Vec::new();
  image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
    .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
    .map_err(|e| e.to_string())?;
  Ok(out)
}
