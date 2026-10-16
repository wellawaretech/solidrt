// Hand-written bindings to the libopus decode API, the subset forge uses,
// and under `video-encode` the encode API beside it, against the vendored
// libopus (forge/vendor/opus, pinned by the submodule; built and linked by
// build.rs). Declared by hand rather than generated: the surface is a few
// functions and two opaque structs, and the libopus API has been frozen
// since 1.0 (its symbols carry no ABI version because the layout of the
// codec state is never exposed). Nothing here is for the rest of forge to
// call: the safe surfaces are `super::OpusDecoder` and `super::OpusEncoder`.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int};

/// The one return value that is not an error.
pub const OPUS_OK: c_int = 0;

/// opus_decoder_ctl request: reset the decoder state (after a seek).
pub const OPUS_RESET_STATE: c_int = 4028;

/// Opaque: the decoder state behind `opus_decoder_create`.
#[repr(C)]
pub struct OpusDecoder {
  _private: [u8; 0],
}

extern "C" {
  /// Allocate a decoder for `channels` (1 or 2) at `fs` Hz (48000 here);
  /// `error` receives OPUS_OK or an error code.
  pub fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;
  /// Decode one packet into interleaved float PCM; `frame_size` is the
  /// room in `pcm` in samples per channel. Returns samples per channel
  /// decoded, or a negative error.
  pub fn opus_decode_float(
    st: *mut OpusDecoder,
    data: *const u8,
    len: i32,
    pcm: *mut f32,
    frame_size: c_int,
    decode_fec: c_int,
  ) -> c_int;
  /// Variadic control; only the argument-less OPUS_RESET_STATE is used.
  pub fn opus_decoder_ctl(st: *mut OpusDecoder, request: c_int, ...) -> c_int;
  pub fn opus_decoder_destroy(st: *mut OpusDecoder);
  /// A static description of an error code.
  pub fn opus_strerror(error: c_int) -> *const c_char;
}

/// The encode half: opus_encoder_create's applications, the controls the
/// encoder uses, the state and the entry points (opus.h, opus_defines.h).
#[cfg(feature = "video-encode")]
pub use encoder::*;

#[cfg(feature = "video-encode")]
mod encoder {
  use super::*;

  pub const OPUS_APPLICATION_VOIP: c_int = 2048;
  pub const OPUS_APPLICATION_AUDIO: c_int = 2049;
  /// opus_encoder_ctl requests: set the bitrate (bits per second), read the
  /// codec's total delay in samples at the encoder's rate.
  pub const OPUS_SET_BITRATE_REQUEST: c_int = 4002;
  pub const OPUS_GET_LOOKAHEAD_REQUEST: c_int = 4027;

  /// Opaque: the encoder state behind `opus_encoder_create`.
  #[repr(C)]
  pub struct OpusEncoder {
    _private: [u8; 0],
  }

  extern "C" {
    /// Allocate an encoder for `channels` (1 or 2) at `fs` Hz (8000, 12000,
    /// 16000, 24000 or 48000) for `application`; `error` receives OPUS_OK
    /// or an error code.
    pub fn opus_encoder_create(fs: i32, channels: c_int, application: c_int, error: *mut c_int) -> *mut OpusEncoder;
    /// Encode one frame of interleaved float PCM, `frame_size` samples per
    /// channel (a 2.5, 5, 10, 20, 40 or 60 ms frame), into `data` of
    /// `max_data_bytes`. Returns the packet length, or a negative error.
    pub fn opus_encode_float(
      st: *mut OpusEncoder,
      pcm: *const f32,
      frame_size: c_int,
      data: *mut u8,
      max_data_bytes: i32,
    ) -> c_int;
    /// Variadic control: a set request takes an int, a get request an int
    /// pointer.
    pub fn opus_encoder_ctl(st: *mut OpusEncoder, request: c_int, ...) -> c_int;
    pub fn opus_encoder_destroy(st: *mut OpusEncoder);
  }
}
