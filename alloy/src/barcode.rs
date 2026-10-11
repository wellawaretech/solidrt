//! Barcode decoding on CPU pixel buffers, shared by the camera stream scanner
//! (see camera/) and the standalone image scan API. Decoder tolerance is
//! pinned by tests/qr_decode.rs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

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

/// How often the stream scanner attempts a decode: about three a second,
/// the cadence a scanner app expects (a code held up decodes within a
/// third of a second), while a frame with no code costs around a
/// millisecond per attempt and one with a code several (desktop release).
const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(333);

/// Re-report a code still in view at most this often, matching typical
/// scanner behavior; consumers connect/dedupe on their side.
const REPORT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// A camera stream's QR scanner, shared between its `sol-qr` thread and the
/// session: the payloads decoded and not yet taken, and how far the thread
/// has looked.
pub struct StreamScanner {
  results: Mutex<Vec<String>>,
  /// The seq of the newest frame the thread has examined (decoded, or seen
  /// and passed over before an attempt was due); 0 before the first.
  examined_seq: AtomicU64,
}

impl StreamScanner {
  /// Drain the payloads decoded since the last call.
  pub fn take(&self) -> Vec<String> {
    std::mem::take(&mut *self.results.lock().unwrap_or_else(PoisonError::into_inner))
  }

  /// Whether the tap holds a frame the thread has not examined yet: a
  /// payload from it may still be on its way.
  pub fn pending(&self, tap: &crate::camera::FrameTap) -> bool {
    tap.latest().is_some_and(|frame| frame.seq > self.examined_seq.load(Ordering::Acquire))
  }
}

/// Decode QR codes from a camera stream on a `sol-qr` thread over `tap`:
/// every `SCAN_INTERVAL` the newest frame is decoded and a payload pushed
/// into the scanner's list, which `Context::take_camera_barcodes` drains.
/// The thread ends when the tap closes. A thread that cannot start leaves
/// the list empty for good, with a warning.
pub fn spawn_stream_scanner(tap: crate::camera::FrameTap) -> Arc<StreamScanner> {
  let scanner = Arc::new(StreamScanner { results: Mutex::new(Vec::new()), examined_seq: AtomicU64::new(0) });
  let shared = scanner.clone();
  let spawned = std::thread::Builder::new().name("sol-qr".into()).spawn(move || scan_stream(tap, shared));
  if let Err(e) = spawned {
    log::warn!("[barcode] scanner thread failed to start: {e}");
  }
  scanner
}

fn scan_stream(tap: crate::camera::FrameTap, scanner: Arc<StreamScanner>) {
  use crate::camera::TapWait;
  use std::time::Instant;
  let mut gray = Vec::new();
  let mut seen = 0;
  let mut due = Instant::now();
  let mut last_report: Option<Instant> = None;
  loop {
    let now = Instant::now();
    // Frames before the next attempt is due only advance `seen`, so the
    // attempt decodes the newest frame at that moment; past the due time
    // the wait is for the next frame, however long it takes.
    let timeout = if now >= due { SCAN_INTERVAL } else { due - now };
    match tap.wait_newer(seen, timeout) {
      TapWait::Frame(frame) => {
        seen = frame.seq;
        if Instant::now() >= due {
          to_greyscale(&frame.pixels, &mut gray);
          if let Some(content) = decode_qr(&gray, frame.width, frame.height) {
            let report = last_report.map_or(true, |t| t.elapsed() >= REPORT_INTERVAL);
            if report {
              last_report = Some(Instant::now());
              scanner.results.lock().unwrap_or_else(PoisonError::into_inner).push(content);
            }
          }
          due = Instant::now() + SCAN_INTERVAL;
        }
        scanner.examined_seq.store(seen, Ordering::Release);
      }
      TapWait::Timeout => {}
      TapWait::Closed => return,
    }
  }
}
