//! Barcode decoding on CPU pixel buffers, shared by the camera stream scanner
//! (see camera.rs) and the standalone image scan API. Decoder tolerance is
//! pinned by tests/qr_decode.rs.

/// RGBA8 -> luma, reusing `gray`. Cheap (r + 2g + b) / 4 approximation.
pub fn to_greyscale(pixels: &[u8], gray: &mut Vec<u8>) {
  gray.clear();
  gray.extend(pixels.chunks_exact(4).map(|px| {
    let (r, g, b) = (px[0] as u16, px[1] as u16, px[2] as u16);
    ((r + 2 * g + b) / 4) as u8
  }));
}

/// First decodable QR code in the luma image, if any. The camera pump's fast
/// path: a stream frame realistically holds one code, and callers rescan
/// continuously anyway.
pub fn decode_qr(gray: &[u8], width: u32, height: u32) -> Option<String> {
  match rxing::helpers::detect_in_luma(gray.to_vec(), width, height, Some(rxing::BarcodeFormat::QR_CODE)) {
    Ok(result) => Some(result.getText().to_string()),
    // The overwhelmingly common case is simply "no code in this frame".
    Err(e) => {
      log::trace!("[barcode] no QR in frame: {e}");
      None
    }
  }
}

/// All QR codes in a tightly-packed RGBA8 image (one-shot scan).
pub fn scan_rgba(pixels: &[u8], width: u32, height: u32) -> Vec<String> {
  let mut gray = Vec::new();
  to_greyscale(pixels, &mut gray);
  match rxing::helpers::detect_multiple_in_luma(gray, width, height) {
    Ok(results) => results
      .into_iter()
      .filter(|r| *r.getBarcodeFormat() == rxing::BarcodeFormat::QR_CODE)
      .map(|r| r.getText().to_string())
      .collect(),
    Err(e) => {
      log::trace!("[barcode] no codes in image: {e}");
      Vec::new()
    }
  }
}