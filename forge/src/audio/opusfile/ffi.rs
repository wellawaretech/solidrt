// Hand-written bindings to the libopusfile API, the subset forge uses,
// against the vendored libopusfile (forge/vendor/opusfile, pinned by the
// submodule; compiled and linked by build.rs with the vendored libogg).
// Declared by hand rather than generated: the surface is one callback
// struct and eight functions, and the opusfile API has been frozen since
// 0.1 (the OggOpusFile layout is never exposed). Nothing here is for the
// rest of forge to call: the safe surface is `super::OpusFileDecoder`.

#![allow(non_camel_case_types)]

use std::ffi::{c_int, c_uchar, c_void};

// Return codes (opusfile.h). Negative values from op_read_float and the
// openers; OP_HOLE is a gap in the data to read past, every other one an
// error.
pub const OP_HOLE: c_int = -3;
pub const OP_EREAD: c_int = -128;
pub const OP_EFAULT: c_int = -129;
pub const OP_EIMPL: c_int = -130;
pub const OP_EINVAL: c_int = -131;
pub const OP_ENOTFORMAT: c_int = -132;
pub const OP_EBADHEADER: c_int = -133;
pub const OP_EVERSION: c_int = -134;
pub const OP_ENOTAUDIO: c_int = -135;
pub const OP_EBADPACKET: c_int = -136;
pub const OP_EBADLINK: c_int = -137;
pub const OP_ENOSEEK: c_int = -138;
pub const OP_EBADTIMESTAMP: c_int = -139;

/// Opaque: the open file behind `op_open_callbacks`.
#[repr(C)]
pub struct OggOpusFile {
  _private: [u8; 0],
}

/// Read up to `nbytes` into `ptr`; bytes read, 0 at the end, negative on
/// error.
pub type op_read_func = Option<unsafe extern "C" fn(stream: *mut c_void, ptr: *mut c_uchar, nbytes: c_int) -> c_int>;
/// fseek semantics (`whence` is SEEK_SET/CUR/END); 0 on success, -1 when
/// the stream cannot seek.
pub type op_seek_func = Option<unsafe extern "C" fn(stream: *mut c_void, offset: i64, whence: c_int) -> c_int>;
/// The byte position, or -1.
pub type op_tell_func = Option<unsafe extern "C" fn(stream: *mut c_void) -> i64>;
/// Release the stream; called by `op_free` (not by a failed open).
pub type op_close_func = Option<unsafe extern "C" fn(stream: *mut c_void) -> c_int>;

#[repr(C)]
pub struct OpusFileCallbacks {
  pub read: op_read_func,
  pub seek: op_seek_func,
  pub tell: op_tell_func,
  pub close: op_close_func,
}

extern "C" {
  /// Open a stream through `cb`, reading its headers (and, when it can
  /// seek, its length and link structure). `initial_data` are bytes already
  /// read off the stream (none here). On failure returns null with `error`
  /// set and does NOT call the close callback.
  pub fn op_open_callbacks(
    stream: *mut c_void,
    cb: *const OpusFileCallbacks,
    initial_data: *const c_uchar,
    initial_bytes: usize,
    error: *mut c_int,
  ) -> *mut OggOpusFile;
  /// Release the file, closing its stream through the close callback.
  pub fn op_free(of: *mut OggOpusFile);
  /// Whether the stream could seek (so totals and `op_pcm_seek` work).
  pub fn op_seekable(of: *const OggOpusFile) -> c_int;
  /// Links (chained streams) in a seekable file.
  pub fn op_link_count(of: *const OggOpusFile) -> c_int;
  /// Channel count of link `li` (-1: the current one).
  pub fn op_channel_count(of: *const OggOpusFile, li: c_int) -> c_int;
  /// Total samples per channel at 48 kHz in link `li` (-1: the whole
  /// file), pre-skip and end trim excluded; negative when unseekable.
  pub fn op_pcm_total(of: *const OggOpusFile, li: c_int) -> i64;
  /// Seek so the next sample read is `pcm_offset` (48 kHz) into the file.
  pub fn op_pcm_seek(of: *mut OggOpusFile, pcm_offset: i64) -> c_int;
  /// Decode into `pcm` (room for `buf_size` floats, interleaved); samples
  /// per channel written (0 at the end), or a negative code. Buffers what
  /// a packet decoded beyond `buf_size`, so any size works.
  pub fn op_read_float(of: *mut OggOpusFile, pcm: *mut f32, buf_size: c_int, li: *mut c_int) -> c_int;
}
