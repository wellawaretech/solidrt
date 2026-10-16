// Opus encode through the vendored libopus (bound by hand in ffi.rs). One
// encoder per stream at the capture rate: libopus takes 8, 12, 16, 24 or
// 48 kHz input and encodes at 48 kHz internally, so a microphone at its
// default 16 kHz needs no resampler and its packets decode like any other
// Opus stream. One 20 ms frame per packet, the default and what WebM
// expects.

use std::ffi::{c_int, CStr};

use super::ffi;

/// The input rates libopus's encoder takes.
const OPUS_RATES: [u32; 5] = [8000, 12000, 16000, 24000, 48000];
/// One Opus frame per packet: 20 ms, the codec's default frame.
const FRAME_MS: u32 = 20;
/// Room for any packet at any bitrate: what libopus documents as a safe
/// `max_data_bytes` for one frame.
const MAX_PACKET_BYTES: usize = 4000;
/// The rate every Opus stream is defined at: what pre-skip counts in.
const OPUS_SAMPLE_RATE: u64 = 48_000;

/// What the stream is for: the encoder tunes for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpusApplication {
  /// Speech over a link: lower delay, speech-tuned.
  Voip,
  /// A recording: best fidelity for any signal.
  Audio,
}

pub struct OpusEncoder {
  ptr: *mut ffi::OpusEncoder,
  sample_rate: u32,
  channels: u16,
  buf: Vec<u8>,
}

// The encoder state is used from one thread at a time (the owner's);
// libopus itself keeps no global state.
unsafe impl Send for OpusEncoder {}

impl OpusEncoder {
  /// An encoder for `channels` (1 or 2) at `sample_rate` (one of
  /// `OPUS_RATES`) at `bitrate_bps`. Errs on a rate libopus does not take.
  pub fn new(
    sample_rate: u32,
    channels: u16,
    bitrate_bps: u32,
    application: OpusApplication,
  ) -> Result<OpusEncoder, String> {
    if !OPUS_RATES.contains(&sample_rate) {
      return Err(format!("opus encodes at 8, 12, 16, 24 or 48 kHz, not {sample_rate} Hz"));
    }
    let application = match application {
      OpusApplication::Voip => ffi::OPUS_APPLICATION_VOIP,
      OpusApplication::Audio => ffi::OPUS_APPLICATION_AUDIO,
    };
    let mut error = ffi::OPUS_OK;
    let ptr = unsafe { ffi::opus_encoder_create(sample_rate as i32, channels as c_int, application, &mut error) };
    if ptr.is_null() || error != ffi::OPUS_OK {
      return Err(format!("create opus encoder ({channels} ch at {sample_rate} Hz): {}", describe(error)));
    }
    let mut encoder = OpusEncoder { ptr, sample_rate, channels, buf: vec![0; MAX_PACKET_BYTES] };
    encoder.set_bitrate(bitrate_bps)?;
    Ok(encoder)
  }

  pub fn sample_rate(&self) -> u32 {
    self.sample_rate
  }

  pub fn channels(&self) -> u16 {
    self.channels
  }

  /// Samples per channel in one frame (20 ms at the encoder's rate).
  pub fn frame_samples(&self) -> usize {
    (self.sample_rate * FRAME_MS / 1000) as usize
  }

  /// The codec's total delay in samples at 48 kHz: the OpusHead pre-skip a
  /// container carries, so a decoder discards the priming samples.
  pub fn pre_skip(&self) -> u32 {
    let mut lookahead: c_int = 0;
    let result = unsafe { ffi::opus_encoder_ctl(self.ptr, ffi::OPUS_GET_LOOKAHEAD_REQUEST, &mut lookahead) };
    if result != ffi::OPUS_OK {
      log::warn!("[forge::audio] opus lookahead: {}", describe(result));
      return 0;
    }
    (lookahead.max(0) as u64 * OPUS_SAMPLE_RATE / self.sample_rate as u64) as u32
  }

  pub fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<(), String> {
    let result = unsafe { ffi::opus_encoder_ctl(self.ptr, ffi::OPUS_SET_BITRATE_REQUEST, bitrate_bps as c_int) };
    if result != ffi::OPUS_OK {
      return Err(format!("opus bitrate {bitrate_bps}: {}", describe(result)));
    }
    Ok(())
  }

  /// Encode exactly one frame of interleaved f32 PCM (`frame_samples()`
  /// per channel) into one packet.
  pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>, String> {
    let frame = self.frame_samples();
    if pcm.len() != frame * self.channels as usize {
      return Err(format!(
        "opus frame of {} samples is not {frame} per channel x {} channels",
        pcm.len(),
        self.channels
      ));
    }
    let len = unsafe {
      ffi::opus_encode_float(self.ptr, pcm.as_ptr(), frame as c_int, self.buf.as_mut_ptr(), self.buf.len() as i32)
    };
    if len < 0 {
      return Err(format!("opus encode: {}", describe(len)));
    }
    Ok(self.buf[..len as usize].to_vec())
  }
}

impl Drop for OpusEncoder {
  fn drop(&mut self) {
    unsafe { ffi::opus_encoder_destroy(self.ptr) };
  }
}

fn describe(error: i32) -> String {
  let ptr = unsafe { ffi::opus_strerror(error) };
  if ptr.is_null() {
    return format!("error {error}");
  }
  unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}
