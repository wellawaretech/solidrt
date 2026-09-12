// Opus decode through the vendored libopus (the reference decoder, bound by
// hand in ffi.rs). One decoder per stream, mono or stereo, decoding at the
// 48 kHz every Opus stream is defined at. Used on every platform, Android
// included: Android's own "audio/opus" codec is libopus behind the codec
// framework, so going through it would cost buffer copies and gain
// nothing (okf/plans/android-video-punch-through.md).

mod ffi;

use std::ffi::CStr;

// The longest Opus packet is 120 ms: room for that many samples per
// channel at 48 kHz, so any packet decodes in one call.
const MAX_FRAME_SAMPLES: usize = 5760;

/// Decoded PCM: interleaved f32 at the stream's rate and channel count
/// (constant per stream, carried here so a chunk is self-describing).
pub struct PcmChunk {
  pub pts_us: i64,
  pub sample_rate: u32,
  pub channels: u16,
  pub samples: Vec<f32>,
}

pub struct OpusDecoder {
  ptr: *mut ffi::OpusDecoder,
  sample_rate: u32,
  channels: u16,
  buf: Vec<f32>,
}

// The decoder state is used from one thread at a time (the owner's);
// libopus itself keeps no global state.
unsafe impl Send for OpusDecoder {}

impl OpusDecoder {
  /// A decoder for `channels` (1 or 2) at `sample_rate` (48 kHz for Opus
  /// streams; other rates decode too, resampled by libopus).
  pub fn new(sample_rate: u32, channels: u16) -> Result<OpusDecoder, String> {
    let mut error = ffi::OPUS_OK;
    let ptr = unsafe { ffi::opus_decoder_create(sample_rate as i32, channels as i32, &mut error) };
    if ptr.is_null() || error != ffi::OPUS_OK {
      return Err(format!("create opus decoder ({channels} ch at {sample_rate} Hz): {}", describe(error)));
    }
    Ok(OpusDecoder { ptr, sample_rate, channels, buf: vec![0.0; MAX_FRAME_SAMPLES * channels as usize] })
  }

  /// Decode one packet to interleaved f32 PCM tagged with `pts_us`.
  pub fn decode(&mut self, pts_us: i64, packet: &[u8]) -> Result<PcmChunk, String> {
    let frames = unsafe {
      ffi::opus_decode_float(
        self.ptr,
        packet.as_ptr(),
        packet.len() as i32,
        self.buf.as_mut_ptr(),
        MAX_FRAME_SAMPLES as i32,
        0,
      )
    };
    if frames < 0 {
      return Err(format!("opus decode: {}", describe(frames)));
    }
    let samples = self.buf[..frames as usize * self.channels as usize].to_vec();
    Ok(PcmChunk { pts_us, sample_rate: self.sample_rate, channels: self.channels, samples })
  }

  /// Forget the decoder state, for decoding from a new position (after a
  /// seek). The packets before the target are then the preroll the
  /// stream's SeekPreRoll asks for.
  pub fn reset(&mut self) {
    let result = unsafe { ffi::opus_decoder_ctl(self.ptr, ffi::OPUS_RESET_STATE) };
    if result != ffi::OPUS_OK {
      log::warn!("[forge::video] opus reset: {}", describe(result));
    }
  }
}

impl Drop for OpusDecoder {
  fn drop(&mut self) {
    unsafe { ffi::opus_decoder_destroy(self.ptr) };
  }
}

fn describe(error: i32) -> String {
  let ptr = unsafe { ffi::opus_strerror(error) };
  if ptr.is_null() {
    return format!("error {error}");
  }
  unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}
