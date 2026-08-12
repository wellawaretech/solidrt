// The Android hardware decoder: AMediaCodec in buffer mode. Probed on the
// Philips TPM171E 2026-08-12 (see okf/backlog/video-playback.md): the buffer
// tap emits an honest layout (color-format 21 = NV12, stride/slice-height
// padded) at ~3x realtime for 1080p; surface-attached taps are per-device
// untrustworthy and are not used. Unlike openh264 this decoder handles
// B-frames, so arbitrary real-world H.264 plays.
//
// The codec's padded output (stride, slice-height, crop) is repacked into
// the tightly packed frame contract during the mandatory copy out of the
// codec buffer. Planar output (color-format 19) is interleaved to NV12 in
// the same pass, so every device feeds the same layout downstream.

use std::time::Duration;

use ndk::media::media_codec::{
  DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;

use super::{PixelLayout, VideoAu, VideoDecoder, YuvFrame};

const FLAG_END_OF_STREAM: u32 = ndk_sys::AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM as u32;
const FLAG_CODEC_CONFIG: u32 = ndk_sys::AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG as u32;
// MediaCodecInfo.CodecCapabilities color formats (Java-level constants, no
// ndk-sys symbols): 21 = YUV420SemiPlanar (NV12), 19 = YUV420Planar (I420).
const COLOR_NV12: i32 = 21;
const COLOR_I420: i32 = 19;

// The codec accepting no input for this long means it is wedged, not busy
// (decode itself runs at 3x realtime on the slowest target).
const STALL_MS: u32 = 2000;

/// Geometry of the codec's output buffers, read from the output format at
/// the format-changed event (which precedes the first buffer). Crop keys are
/// read as plain i32s: `format.rect()` is API-28 gated and the TV is API 26.
struct OutputFacts {
  color_format: i32,
  stride: usize,
  slice_height: usize,
  crop_left: usize,
  crop_top: usize,
  width: u32,
  height: u32,
}

pub struct MediaCodecDecoder {
  codec: MediaCodec,
  configured: (u32, u32),
  facts: Option<OutputFacts>,
}

impl MediaCodecDecoder {
  /// Create and start a `video/avc` decoder for the stream. `sps`/`pps` are
  /// the raw parameter sets from the container (no start codes); they go in
  /// as csd, and the demuxer also prepends them in-band at sync samples -
  /// the probed-working combination.
  pub fn new(width: u32, height: u32, sps: &[u8], pps: &[u8]) -> Result<Self, String> {
    let codec = MediaCodec::from_decoder_type("video/avc")
      .ok_or_else(|| "no video/avc decoder on this device".to_string())?;
    let mut format = MediaFormat::new();
    format.set_str("mime", "video/avc");
    format.set_i32("width", width as i32);
    format.set_i32("height", height as i32);
    let mut csd0 = vec![0, 0, 0, 1];
    csd0.extend_from_slice(sps);
    let mut csd1 = vec![0, 0, 0, 1];
    csd1.extend_from_slice(pps);
    format.set_buffer("csd-0", &csd0);
    format.set_buffer("csd-1", &csd1);
    codec
      .configure(&format, None, MediaCodecDirection::Decoder)
      .map_err(|e| format!("configure video/avc decoder: {e:?}"))?;
    codec.start().map_err(|e| format!("start video/avc decoder: {e:?}"))?;
    Ok(MediaCodecDecoder { codec, configured: (width, height), facts: None })
  }

  /// Pull every ready output buffer into `frames`. Each dequeue waits up to
  /// `timeout`; returns whether the end-of-stream flag came out.
  fn drain(&mut self, frames: &mut Vec<YuvFrame>, timeout: Duration) -> Result<bool, String> {
    loop {
      match self.codec.dequeue_output_buffer(timeout).map_err(|e| format!("dequeue output: {e:?}"))? {
        DequeuedOutputBufferInfoResult::Buffer(buf) => {
          let info = *buf.info();
          let eos = info.flags() & FLAG_END_OF_STREAM != 0;
          if info.size() > 0 && info.flags() & FLAG_CODEC_CONFIG == 0 {
            if self.facts.is_none() {
              // Some codecs skip the format-changed event; read on demand.
              self.facts = Some(read_facts(&self.codec, self.configured)?);
            }
            let facts = self.facts.as_ref().expect("facts read above");
            frames.push(repack(facts, buf.buffer(), info.offset() as usize, info.presentation_time_us())?);
          }
          self.codec.release_output_buffer(buf, false).map_err(|e| format!("release output: {e:?}"))?;
          if eos {
            return Ok(true);
          }
        }
        DequeuedOutputBufferInfoResult::OutputFormatChanged => {
          self.facts = Some(read_facts(&self.codec, self.configured)?);
        }
        DequeuedOutputBufferInfoResult::OutputBuffersChanged => {}
        DequeuedOutputBufferInfoResult::TryAgainLater => return Ok(false),
      }
    }
  }

  /// Queue one input payload, draining ready outputs between dequeue
  /// attempts (all input slots can be in flight while the codec works;
  /// freeing outputs is what unblocks them).
  fn feed(&mut self, data: &[u8], pts_us: u64, flags: u32, frames: &mut Vec<YuvFrame>) -> Result<(), String> {
    let mut waited_ms = 0u32;
    loop {
      let dequeued = self
        .codec
        .dequeue_input_buffer(Duration::from_millis(10))
        .map_err(|e| format!("dequeue input: {e:?}"))?;
      let sent = match dequeued {
        DequeuedInputBufferResult::Buffer(mut buf) => {
          let target = buf.buffer_mut();
          if target.len() < data.len() {
            return Err(format!("input buffer too small: {} < {}", target.len(), data.len()));
          }
          for (dst, src) in target.iter_mut().zip(data) {
            dst.write(*src);
          }
          self
            .codec
            .queue_input_buffer(buf, 0, data.len(), pts_us, flags)
            .map_err(|e| format!("queue input: {e:?}"))?;
          true
        }
        DequeuedInputBufferResult::TryAgainLater => {
          waited_ms += 10;
          if waited_ms >= STALL_MS {
            return Err(format!("decoder accepted no input for {STALL_MS}ms"));
          }
          false
        }
      };
      if sent {
        return Ok(());
      }
      self.drain(frames, Duration::ZERO)?;
    }
  }
}

impl VideoDecoder for MediaCodecDecoder {
  fn decode(&mut self, au: &VideoAu) -> Result<Vec<YuvFrame>, String> {
    let mut frames = Vec::new();
    self.feed(&au.data, au.pts_us.max(0) as u64, 0, &mut frames)?;
    self.drain(&mut frames, Duration::ZERO)?;
    Ok(frames)
  }

  fn flush(&mut self) -> Result<Vec<YuvFrame>, String> {
    let mut frames = Vec::new();
    self.feed(&[], 0, FLAG_END_OF_STREAM, &mut frames)?;
    let mut waited_ms = 0u32;
    loop {
      if self.drain(&mut frames, Duration::from_millis(100))? {
        return Ok(frames);
      }
      waited_ms += 100;
      if waited_ms >= STALL_MS {
        return Err(format!("no end-of-stream from decoder within {STALL_MS}ms"));
      }
    }
  }
}

fn read_facts(codec: &MediaCodec, configured: (u32, u32)) -> Result<OutputFacts, String> {
  let format = codec.output_format();
  let color_format = format.i32("color-format").ok_or("output format missing color-format")?;
  let coded_w = format.i32("width").unwrap_or(configured.0 as i32);
  let coded_h = format.i32("height").unwrap_or(configured.1 as i32);
  let stride = format.i32("stride").filter(|&v| v > 0).unwrap_or(coded_w) as usize;
  let slice_height = format.i32("slice-height").filter(|&v| v > 0).unwrap_or(coded_h) as usize;
  let crop_left = format.i32("crop-left").unwrap_or(0).max(0) as usize;
  let crop_top = format.i32("crop-top").unwrap_or(0).max(0) as usize;
  // Display size: the crop when present, the configured stream size
  // otherwise (the probed TV emits no crop keys and pads height to 1088;
  // the container's size is the display truth there).
  let (width, height) = match (format.i32("crop-right"), format.i32("crop-bottom")) {
    (Some(right), Some(bottom)) => (
      (right - crop_left as i32 + 1).max(0) as u32,
      (bottom - crop_top as i32 + 1).max(0) as u32,
    ),
    _ => configured,
  };
  Ok(OutputFacts { color_format, stride, slice_height, crop_left, crop_top, width, height })
}

/// Repack one padded codec buffer into a tightly packed NV12 frame (see
/// `PixelLayout`), honoring stride, slice-height, and crop offsets. Chroma
/// crops land on even pixels for 4:2:0 content.
fn repack(f: &OutputFacts, src: &[u8], offset: usize, pts_us: i64) -> Result<YuvFrame, String> {
  let (w, h) = (f.width as usize, f.height as usize);
  let (cw, ch) = (f.width.div_ceil(2) as usize, f.height.div_ceil(2) as usize);
  let src = src.get(offset..).ok_or("output buffer offset out of bounds")?;
  let y_base = f.crop_top * f.stride + f.crop_left;
  let chroma_base = f.stride * f.slice_height;
  let mut data = Vec::with_capacity(PixelLayout::Nv12.frame_size(f.width, f.height));
  match f.color_format {
    COLOR_NV12 => {
      let uv_base = chroma_base + (f.crop_top / 2) * f.stride + f.crop_left;
      let need = (uv_base + ch.saturating_sub(1) * f.stride + cw * 2)
        .max(y_base + h.saturating_sub(1) * f.stride + w);
      if src.len() < need {
        return Err(format!("output buffer too small: {} < {need}", src.len()));
      }
      for row in 0..h {
        data.extend_from_slice(&src[y_base + row * f.stride..][..w]);
      }
      for row in 0..ch {
        data.extend_from_slice(&src[uv_base + row * f.stride..][..cw * 2]);
      }
    }
    COLOR_I420 => {
      let cstride = f.stride / 2;
      let cslice = f.slice_height / 2;
      let u_base = chroma_base + (f.crop_top / 2) * cstride + f.crop_left / 2;
      let v_base = chroma_base + cstride * cslice + (f.crop_top / 2) * cstride + f.crop_left / 2;
      let need = (v_base + ch.saturating_sub(1) * cstride + cw)
        .max(y_base + h.saturating_sub(1) * f.stride + w);
      if src.len() < need {
        return Err(format!("output buffer too small: {} < {need}", src.len()));
      }
      for row in 0..h {
        data.extend_from_slice(&src[y_base + row * f.stride..][..w]);
      }
      for row in 0..ch {
        let u = &src[u_base + row * cstride..][..cw];
        let v = &src[v_base + row * cstride..][..cw];
        for i in 0..cw {
          data.push(u[i]);
          data.push(v[i]);
        }
      }
    }
    other => return Err(format!("unsupported decoder color-format {other} (expected 21 NV12 or 19 planar)")),
  }
  Ok(YuvFrame { pts_us, width: f.width, height: f.height, layout: PixelLayout::Nv12, data })
}
