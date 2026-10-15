// Hand-written bindings to the libopus decode API, the subset forge uses,
// against the vendored libopus (forge/vendor/opus, pinned by the submodule;
// built and linked by build.rs). Declared by hand rather than generated:
// the surface is five functions and one opaque struct, and the libopus API
// has been frozen since 1.0 (its symbols carry no ABI version because the
// layout of the decoder state is never exposed). Nothing here is for the
// rest of forge to call: the safe surface is `super::OpusDecoder`.

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
