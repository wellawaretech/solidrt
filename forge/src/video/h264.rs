// The openh264 software decoder: the PoC and dev fallback, not the shipped
// path (hardware decoders per platform are, see the decoder decision in
// okf/backlog/video-playback.md). Known limitation, probed 2026-08-12: the
// openh264 DECODER has no B-slice support and errors on every B-frame, so
// dev/example content must be encoded without B-frames (`-bf 0`).

use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

use super::{PixelLayout, VideoAu, VideoDecoder, YuvFrame};

pub struct H264Decoder {
  inner: Decoder,
  // Presentation timestamps of AUs fed but not yet output. openh264 emits
  // at most one frame per AU and (without B-slices) in feed order, so a
  // FIFO pairs outputs back to their timestamps.
  pending_pts: std::collections::VecDeque<i64>,
}

impl H264Decoder {
  pub fn new() -> Result<Self, String> {
    let inner = Decoder::new().map_err(|e| format!("create openh264 decoder: {e}"))?;
    Ok(H264Decoder { inner, pending_pts: std::collections::VecDeque::new() })
  }

}

// Free-standing so a frame can be packed while the decoded planes still
// borrow the decoder (`DecodedYUV` borrows `Decoder`).
fn pack(pending_pts: &mut std::collections::VecDeque<i64>, yuv: &impl YUVSource) -> YuvFrame {
  let (width, height) = yuv.dimensions();
  let (width, height) = (width as u32, height as u32);
  let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
  let (sy, su, sv) = yuv.strides();
  let mut data = Vec::with_capacity(PixelLayout::I420.frame_size(width, height));
  // Tightly pack the decoder's strided planes (see PixelLayout).
  for row in 0..height as usize {
    data.extend_from_slice(&yuv.y()[row * sy..row * sy + width as usize]);
  }
  for row in 0..ch {
    data.extend_from_slice(&yuv.u()[row * su..row * su + cw]);
  }
  for row in 0..ch {
    data.extend_from_slice(&yuv.v()[row * sv..row * sv + cw]);
  }
  let pts_us = pending_pts.pop_front().unwrap_or(0);
  YuvFrame { pts_us, width, height, layout: PixelLayout::I420, data }
}

impl VideoDecoder for H264Decoder {
  fn decode(&mut self, au: &VideoAu) -> Result<Vec<YuvFrame>, String> {
    self.pending_pts.push_back(au.pts_us);
    match self.inner.decode(&au.data) {
      Ok(Some(yuv)) => {
        let frame = pack(&mut self.pending_pts, &yuv);
        Ok(vec![frame])
      }
      Ok(None) => Ok(Vec::new()),
      Err(e) => {
        // The AU produced nothing; drop its timestamp so later outputs
        // stay paired to their own AUs.
        self.pending_pts.pop_front();
        Err(format!("decode: {e}"))
      }
    }
  }

  fn flush(&mut self) -> Result<Vec<YuvFrame>, String> {
    let remaining = self.inner.flush_remaining().map_err(|e| format!("flush: {e}"))?;
    let mut frames = Vec::with_capacity(remaining.len());
    for yuv in &remaining {
      frames.push(pack(&mut self.pending_pts, yuv));
    }
    Ok(frames)
  }
}
