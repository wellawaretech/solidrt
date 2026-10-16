//! The Android VP9 encoder: AMediaCodec in buffer mode behind the
//! `VideoEncoder` contract (encode.rs), the platform's first encoder for the
//! VP9 mime (hardware where the device has one, the platform's software
//! encoder otherwise). The input colour format is the first the codec
//! configures with: planar I420, which the frames already are, else NV12
//! with the chroma interleaved in the copy into the input buffer. Nothing
//! here needs an API above 26 (the TV's), so the keyframe request and the
//! bitrate change go through `setParameters` with the documented keys.
//!
//! The codec is asynchronous inside: a frame queued now comes out a few
//! frames later, so `encode` returns whatever is ready after queueing and
//! `flush` signals the end and drains the rest, bounded.

use std::time::Duration;

use ndk::media::media_codec::{
  DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;

use super::{FLAG_CODEC_CONFIG, FLAG_END_OF_STREAM, MIME_VP9};
use crate::video::encode::{EncodeMode, EncodedFrame, EncoderConfig, VideoEncoder};
use crate::video::PixelLayout;

const FLAG_KEY_FRAME: u32 = ndk_sys::AMEDIACODEC_BUFFER_FLAG_KEY_FRAME as u32;

// MediaCodecInfo.CodecCapabilities colour formats (Java-level constants, no
// ndk-sys symbols): 19 = YUV420Planar (I420), 21 = YUV420SemiPlanar (NV12).
// Tried in this order: the frames arrive as I420.
const INPUT_COLOR_FORMATS: [i32; 2] = [19, 21];
const COLOR_I420: i32 = 19;
// MediaCodecInfo.EncoderCapabilities bitrate modes: 1 = VBR, 2 = CBR.
const BITRATE_MODE_VBR: i32 = 1;
const BITRATE_MODE_CBR: i32 = 2;
// MediaFormat.KEY_PRIORITY: 0 asks for realtime scheduling of the codec.
const PRIORITY_REALTIME: i32 = 0;
const US_PER_S: i64 = 1_000_000;

// How long one wait for a free input buffer lasts, and how many such waits
// (each followed by a drain of the outputs, which is what frees inputs on
// a codec that holds them until its outputs are taken) before the encoder
// is declared stalled: a second in all, far beyond any frame interval.
const INPUT_WAIT: Duration = Duration::from_millis(50);
const INPUT_ATTEMPTS: u32 = 20;
// The flush drains until the end-of-stream flag comes out, one wait at a
// time, up to this many: two seconds for a codec to finish what it holds.
const FLUSH_WAIT: Duration = Duration::from_millis(100);
const FLUSH_ATTEMPTS: u32 = 20;

pub struct MediaCodecEncoder {
  codec: MediaCodec,
  width: u32,
  height: u32,
  color_format: i32,
  // Chroma scratch for the NV12 interleave (empty on an I420 codec).
  nv12: Vec<u8>,
  ended: bool,
}

// SAFETY: the encoder is moved to the recorder's thread at creation and
// used there alone; AMediaCodec has no thread affinity (the NDK documents
// its calls as usable from any thread), and nothing here is shared.
unsafe impl Send for MediaCodecEncoder {}

impl MediaCodecEncoder {
  pub fn new(config: &EncoderConfig) -> Result<MediaCodecEncoder, String> {
    if config.width == 0 || config.height == 0 {
      return Err("encoder size must be at least 1x1".to_string());
    }
    let mut last = String::new();
    for &color_format in &INPUT_COLOR_FORMATS {
      match configure(config, color_format) {
        Ok(codec) => {
          log::info!(
            "[forge::video] VP9 encoder opened: {}x{}, input colour format {color_format}",
            config.width,
            config.height
          );
          return Ok(MediaCodecEncoder {
            codec,
            width: config.width,
            height: config.height,
            color_format,
            nv12: Vec::new(),
            ended: false,
          });
        }
        Err(e) => last = e,
      }
    }
    Err(format!("no VP9 encoder takes a planar or semi-planar input: {last}"))
  }

  /// Every output the codec has ready, without waiting; true when the
  /// end-of-stream came out.
  fn drain(&mut self, wait: Duration, out: &mut Vec<EncodedFrame>) -> Result<bool, String> {
    loop {
      match self.codec.dequeue_output_buffer(wait).map_err(|e| format!("dequeue output: {e:?}"))? {
        DequeuedOutputBufferInfoResult::Buffer(buf) => {
          let info = *buf.info();
          let eos = info.flags() & FLAG_END_OF_STREAM != 0;
          if info.size() > 0 && info.flags() & FLAG_CODEC_CONFIG == 0 {
            let (offset, size) = (info.offset().max(0) as usize, info.size() as usize);
            let data = buf
              .buffer()
              .get(offset..offset + size)
              .ok_or_else(|| format!("output buffer of {} bytes holds no {size} at {offset}", buf.buffer().len()))?
              .to_vec();
            out.push(EncodedFrame {
              pts_us: info.presentation_time_us(),
              keyframe: info.flags() & FLAG_KEY_FRAME != 0,
              data,
            });
          }
          self.codec.release_output_buffer(buf, false).map_err(|e| format!("release output: {e:?}"))?;
          if eos {
            return Ok(true);
          }
        }
        DequeuedOutputBufferInfoResult::OutputFormatChanged | DequeuedOutputBufferInfoResult::OutputBuffersChanged => {}
        DequeuedOutputBufferInfoResult::TryAgainLater => return Ok(false),
      }
    }
  }

  /// Queue `fill` into the next free input buffer with `pts_us` and
  /// `flags`, draining outputs between waits (what frees an input on a
  /// codec that holds them until its outputs are taken).
  fn queue(
    &mut self,
    pts_us: i64,
    flags: u32,
    fill: impl Fn(&mut [std::mem::MaybeUninit<u8>]) -> Result<usize, String>,
    out: &mut Vec<EncodedFrame>,
  ) -> Result<(), String> {
    for _ in 0..INPUT_ATTEMPTS {
      match self.codec.dequeue_input_buffer(INPUT_WAIT).map_err(|e| format!("dequeue input: {e:?}"))? {
        DequeuedInputBufferResult::Buffer(mut buf) => {
          let size = fill(buf.buffer_mut())?;
          return self
            .codec
            .queue_input_buffer(buf, 0, size, pts_us.max(0) as u64, flags)
            .map_err(|e| format!("queue input: {e:?}"));
        }
        DequeuedInputBufferResult::TryAgainLater => {
          self.drain(Duration::ZERO, out)?;
        }
      }
    }
    Err("the encoder stalled: no input buffer freed in a second".to_string())
  }
}

impl VideoEncoder for MediaCodecEncoder {
  fn encode(&mut self, i420: &[u8], pts_us: i64, force_keyframe: bool) -> Result<Vec<EncodedFrame>, String> {
    if self.ended {
      return Err("the encoder was flushed".to_string());
    }
    let (width, height) = (self.width, self.height);
    let expected = PixelLayout::I420.frame_size(width, height);
    if i420.len() != expected {
      return Err(format!("frame of {} bytes is not {width}x{height} I420 ({expected})", i420.len()));
    }
    if force_keyframe {
      let mut params = MediaFormat::new();
      params.set_i32("request-sync", 0);
      self.codec.set_parameters(params).map_err(|e| format!("request a keyframe: {e:?}"))?;
    }
    let mut out = Vec::new();
    if self.color_format == COLOR_I420 {
      self.queue(pts_us, 0, |target| copy_into(target, i420), &mut out)?;
    } else {
      // NV12: the luma as it is, then the chroma interleaved.
      let luma = width as usize * height as usize;
      let (u, v) = i420[luma..].split_at((i420.len() - luma) / 2);
      self.nv12.clear();
      self.nv12.reserve(i420.len());
      self.nv12.extend_from_slice(&i420[..luma]);
      for (u, v) in u.iter().zip(v) {
        self.nv12.push(*u);
        self.nv12.push(*v);
      }
      let nv12 = std::mem::take(&mut self.nv12);
      let queued = self.queue(pts_us, 0, |target| copy_into(target, &nv12), &mut out);
      self.nv12 = nv12;
      queued?;
    }
    self.drain(Duration::ZERO, &mut out)?;
    Ok(out)
  }

  fn flush(&mut self) -> Result<Vec<EncodedFrame>, String> {
    let mut out = Vec::new();
    if self.ended {
      return Ok(out);
    }
    self.ended = true;
    self.queue(0, FLAG_END_OF_STREAM, |_| Ok(0), &mut out)?;
    for _ in 0..FLUSH_ATTEMPTS {
      if self.drain(FLUSH_WAIT, &mut out)? {
        return Ok(out);
      }
    }
    log::warn!("[forge::video] VP9 encoder did not finish its stream in time; the clip ends with what came out");
    Ok(out)
  }

  fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<(), String> {
    let mut params = MediaFormat::new();
    params.set_i32("video-bitrate", bitrate_bps.min(i32::MAX as u32) as i32);
    self.codec.set_parameters(params).map_err(|e| format!("set bitrate: {e:?}"))
  }
}

impl Drop for MediaCodecEncoder {
  fn drop(&mut self) {
    // A stop before the delete the ndk handle does, so a codec mid-stream
    // releases its buffers in order.
    let _ = self.codec.stop();
  }
}

/// Create, configure and start a VP9 encoder for `config` with the input
/// colour format `color_format`.
fn configure(config: &EncoderConfig, color_format: i32) -> Result<MediaCodec, String> {
  let codec = MediaCodec::from_encoder_type(MIME_VP9).ok_or_else(|| format!("no {MIME_VP9} encoder on this device"))?;
  let mut format = MediaFormat::new();
  format.set_str("mime", MIME_VP9);
  format.set_i32("width", config.width as i32);
  format.set_i32("height", config.height as i32);
  format.set_i32("color-format", color_format);
  format.set_i32("bitrate", config.bitrate_bps.min(i32::MAX as u32) as i32);
  format.set_i32("frame-rate", config.frame_rate.max(1).min(i32::MAX as u32) as i32);
  // Whole seconds between keyframes, at least one (the key takes an int on
  // the API floor).
  let interval_s = (config.keyframe_interval_us.max(1) + US_PER_S - 1) / US_PER_S;
  format.set_i32("i-frame-interval", interval_s.min(i32::MAX as i64) as i32);
  format.set_i32(
    "bitrate-mode",
    match config.mode {
      EncodeMode::Recording => BITRATE_MODE_VBR,
      EncodeMode::Live => BITRATE_MODE_CBR,
    },
  );
  format.set_i32("priority", PRIORITY_REALTIME);
  codec
    .configure(&format, None, MediaCodecDirection::Encoder)
    .map_err(|e| format!("configure {MIME_VP9} encoder with colour format {color_format}: {e:?}"))?;
  codec.start().map_err(|e| format!("start {MIME_VP9} encoder: {e:?}"))?;
  Ok(codec)
}

/// Copy a frame into a codec input buffer; its length, or an error when
/// the buffer is too small for the frame.
fn copy_into(target: &mut [std::mem::MaybeUninit<u8>], frame: &[u8]) -> Result<usize, String> {
  if target.len() < frame.len() {
    return Err(format!("input buffer too small: {} < {}", target.len(), frame.len()));
  }
  for (dst, src) in target.iter_mut().zip(frame) {
    dst.write(*src);
  }
  Ok(frame.len())
}
