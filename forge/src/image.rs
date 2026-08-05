//! Engine-free image codec: encoded bytes (PNG, JPEG, ...) to tightly-packed
//! RGBA8 pixels and back. Pure CPU, no GPU or scripting types; the `flux:image`
//! module is the marshalling layer over this.

use image::ImageEncoder as _;

/// Decoded pixels: tightly-packed RGBA8 plus the pixel dimensions.
pub struct DecodedImage {
  pub data: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

/// Decode encoded image bytes (any enabled format: png, jpeg, webp, gif, bmp,
/// ico) into RGBA8.
pub fn decode(bytes: &[u8]) -> Result<DecodedImage, String> {
  let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
  let rgba = img.to_rgba8();
  let width = rgba.width();
  let height = rgba.height();
  Ok(DecodedImage { data: rgba.into_raw(), width, height })
}

fn check_len(data: &[u8], width: u32, height: u32) -> Result<(), String> {
  let expected = width as usize * height as usize * 4;
  if data.len() != expected {
    return Err(format!("data length {} does not match {width}x{height} RGBA ({expected})", data.len()));
  }
  Ok(())
}

/// Encode RGBA8 pixels as PNG (lossless, alpha preserved).
pub fn encode_png(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
  check_len(data, width, height)?;
  let mut out = Vec::new();
  image::codecs::png::PngEncoder::new(&mut out)
    .write_image(data, width, height, image::ExtendedColorType::Rgba8)
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