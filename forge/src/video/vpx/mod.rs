// The software VP9 decoder: libvpx, Google's reference implementation (the
// one in Chrome, Firefox and ffmpeg), vendored at forge/vendor/libvpx, built
// by build.rs and bound by hand in ffi.rs. BSD-3 with Google's VP8/VP9
// patent grant, so unlike H.264 a bundled decoder carries no licensing, and
// it is the decoder on every platform without a hardware rung (Android
// decodes through MediaCodec instead). One frame out per coded frame, in
// decode order, which for VP9 is presentation order: a superframe (a hidden
// ALT-REF packed with its shown frame) yields only the shown frame.
//
// `unsafe` stays inside this file; the rest of forge sees `Vp9Decoder`.

#[cfg(feature = "video-encode")]
mod encode;
mod ffi;

#[cfg(feature = "video-encode")]
pub use encode::Vp9Encoder;

use std::ffi::{c_long, c_uint, CStr};
use std::ptr;

use super::{PixelLayout, VideoAu, VideoDecoder, YuvFrame};

// libvpx parallelizes a frame across its tiles and rows. Past four threads
// the gain at 1080p is marginal and the cores are the app's.
const MAX_DECODE_THREADS: usize = 4;

pub struct Vp9Decoder {
  ctx: Box<ffi::vpx_codec_ctx_t>,
  // The pts handed to frames drained at flush, which have no AU of their
  // own (libvpx emits VP9 frames synchronously, so this is a formality).
  last_pts_us: i64,
}

impl Vp9Decoder {
  /// Create a decoder sized for the stream (the size is a hint for the
  /// frame-buffer pool; the bitstream is the truth). Errs on an ABI
  /// mismatch with the linked libvpx or an allocation failure.
  pub fn new(width: u32, height: u32) -> Result<Self, String> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(MAX_DECODE_THREADS);
    let cfg = ffi::vpx_codec_dec_cfg_t { threads: threads as c_uint, w: width, h: height };
    // A zeroed context is what libvpx expects before init (it fills every
    // field); the Box keeps the address stable for the library's lifetime.
    let mut ctx: Box<ffi::vpx_codec_ctx_t> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: ctx and cfg are valid for the call; the iface pointer is a
    // static inside libvpx. On failure the context needs no destroy.
    let err =
      unsafe { ffi::vpx_codec_dec_init_ver(&mut *ctx, ffi::vpx_codec_vp9_dx(), &cfg, 0, ffi::VPX_DECODER_ABI_VERSION) };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("init libvpx VP9 decoder: {}", err_string(err)));
    }
    Ok(Vp9Decoder { ctx, last_pts_us: 0 })
  }

  /// Feed one coded frame (`None` signals end of stream) and collect every
  /// picture the decoder has ready.
  fn decode_into(&mut self, data: Option<&[u8]>, pts_us: i64) -> Result<Vec<YuvFrame>, String> {
    let (ptr, len) = match data {
      Some(d) => (d.as_ptr(), d.len()),
      None => (ptr::null(), 0),
    };
    let len =
      c_uint::try_from(len).map_err(|_| format!("coded frame of {len} bytes exceeds the libvpx input limit"))?;
    // SAFETY: the context was initialized in `new`; `ptr`/`len` describe a
    // live slice (or the null/0 end-of-stream pair); libvpx copies what it
    // needs before returning.
    let err = unsafe { ffi::vpx_codec_decode(&mut *self.ctx, ptr, len, ptr::null_mut(), 0 as c_long) };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("{}{}", err_string(err), error_detail(&self.ctx)));
    }
    let mut frames = Vec::new();
    let mut iter: ffi::vpx_codec_iter_t = ptr::null();
    loop {
      // SAFETY: the iterator protocol of vpx_codec_get_frame; the returned
      // image stays valid until the next decode on this context, and it is
      // copied out before that.
      let img = unsafe { ffi::vpx_codec_get_frame(&mut *self.ctx, &mut iter) };
      if img.is_null() {
        break;
      }
      frames.push(unsafe { copy_frame(&*img, pts_us) }?);
    }
    Ok(frames)
  }
}

// A VP9 superframe index (Annex B): a marker byte `110 mag(2) count(3)`,
// `count + 1` little-endian frame sizes of `mag + 1` bytes each, and the
// marker byte again. The decoder reads it from the end of the buffer.
const SUPERFRAME_MARKER: u8 = 0b1100_0000;
const SUPERFRAME_MARKER_MASK: u8 = 0b1110_0000;
const SUPERFRAME_MAG_SHIFT: u8 = 3;
const SUPERFRAME_COUNT_MASK: u8 = 0b0000_0111;
const SUPERFRAME_MAG_MASK: u8 = 0b0000_0011;

/// Whether `data` already ends in a superframe index (libvpx's own test:
/// the marker byte last, and again at the index's start).
fn has_superframe_index(data: &[u8]) -> bool {
  let Some(&marker) = data.last() else { return false };
  if marker & SUPERFRAME_MARKER_MASK != SUPERFRAME_MARKER {
    return false;
  }
  let frames = (marker & SUPERFRAME_COUNT_MASK) as usize + 1;
  let mag = ((marker >> SUPERFRAME_MAG_SHIFT) & SUPERFRAME_MAG_MASK) as usize + 1;
  let index = 2 + mag * frames;
  data.len() >= index && data[data.len() - index] == marker
}

/// Append a one-frame superframe index stating the buffer's length: how a
/// frame with padding after its last tile (a hardware encoder's output
/// buffer, padded to its alignment) is told apart from a frame followed by
/// another. libvpx finds a frame's end where its arithmetic decoder stopped
/// and takes any nonzero bytes after it for the next frame; with an index
/// it decodes the stated length and nothing more.
fn append_superframe_index(data: &mut Vec<u8>) {
  let size = data.len();
  let mag = match size {
    0..=0xff => 1,
    0x100..=0xffff => 2,
    0x1_0000..=0xff_ffff => 3,
    _ => 4,
  };
  let marker = SUPERFRAME_MARKER | (((mag - 1) as u8) << SUPERFRAME_MAG_SHIFT);
  data.push(marker);
  data.extend_from_slice(&size.to_le_bytes()[..mag]);
  data.push(marker);
}

/// The context's detail on its last error, as a `: ...` suffix, or empty.
fn error_detail(ctx: &ffi::vpx_codec_ctx_t) -> String {
  // SAFETY: the context is initialized; the detail is a static or
  // context-owned NUL-terminated string, or null when there is none.
  let detail = unsafe { ffi::vpx_codec_error_detail(ctx) };
  if detail.is_null() {
    return String::new();
  }
  format!(": {}", unsafe { CStr::from_ptr(detail) }.to_string_lossy())
}

impl VideoDecoder for Vp9Decoder {
  fn decode(&mut self, au: &VideoAu) -> Result<Vec<YuvFrame>, String> {
    self.last_pts_us = au.pts_us;
    if has_superframe_index(&au.data) {
      return self.decode_into(Some(&au.data), au.pts_us);
    }
    // One frame, its length stated: libvpx then decodes exactly the block
    // and never reads what follows the frame's last tile as another frame.
    let mut framed = au.data.clone();
    append_superframe_index(&mut framed);
    self.decode_into(Some(&framed), au.pts_us)
  }

  fn flush(&mut self) -> Result<Vec<YuvFrame>, String> {
    self.decode_into(None, self.last_pts_us)
  }
}

impl Drop for Vp9Decoder {
  fn drop(&mut self) {
    // SAFETY: initialized in `new`, destroyed exactly once here.
    unsafe { ffi::vpx_codec_destroy(&mut *self.ctx) };
  }
}

fn err_string(err: std::ffi::c_int) -> String {
  // SAFETY: vpx_codec_err_to_string returns a static string for every
  // value, "Unrecognized error code" included.
  unsafe { CStr::from_ptr(ffi::vpx_codec_err_to_string(err)) }.to_string_lossy().into_owned()
}

/// Copy one decoded picture out of libvpx's frame buffer into the tightly
/// packed I420 contract (see `PixelLayout`), dropping the buffer's row
/// padding. Only 8-bit 4:2:0 is accepted; the demuxer already rejected
/// other profiles at open, so this is the per-frame guard behind it.
///
/// SAFETY (caller): `img` must be the image `vpx_codec_get_frame` returned
/// for the context's latest decode, whose planes libvpx keeps valid until
/// the next decode.
unsafe fn copy_frame(img: &ffi::vpx_image_t, pts_us: i64) -> Result<YuvFrame, String> {
  if img.fmt != ffi::VPX_IMG_FMT_I420 || img.bit_depth != 8 {
    return Err(format!("decoded frame is not 8-bit I420 (fmt {:#x}, {} bit)", img.fmt, img.bit_depth));
  }
  let (w, h) = (img.d_w, img.d_h);
  let (cw, ch) = (w.div_ceil(2) as usize, h.div_ceil(2) as usize);
  let mut data = Vec::with_capacity(PixelLayout::I420.frame_size(w, h));
  let rows = [(0usize, w as usize, h as usize), (1, cw, ch), (2, cw, ch)];
  for (plane, width, height) in rows {
    let base = img.planes[plane];
    let stride = usize::try_from(img.stride[plane]).map_err(|_| format!("negative stride on plane {plane}"))?;
    if base.is_null() || stride < width {
      return Err(format!("plane {plane} unusable (stride {stride} for width {width})"));
    }
    for row in 0..height {
      data.extend_from_slice(std::slice::from_raw_parts(base.add(row * stride), width));
    }
  }
  Ok(YuvFrame { pts_us, width: w, height: h, layout: PixelLayout::I420, data })
}
