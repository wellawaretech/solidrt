// The software VP9 encoder: libvpx's, over the same vendored library as the
// decoder (build.rs configures the encoder in under `video-encode`). Tuned
// for realtime: the realtime deadline, no lag, no hidden ALT-REF frames,
// row-based multithreading, cyclic-refresh adaptive quantization. So one
// coded frame comes out per frame in, in order, and a packet is exactly one
// shown frame: what a WebM SimpleBlock holds and what the decoder's
// superframe handling never has to split.
//
// `unsafe` stays inside this file; the rest of forge sees `Vp9Encoder`
// through the `VideoEncoder` contract.

use std::ffi::{c_int, c_long, c_uint, c_ulong};
use std::ptr;

use super::{err_string, error_detail, ffi};
use crate::video::encode::{EncodeMode, EncodedFrame, EncoderConfig, VideoEncoder};
use crate::video::PixelLayout;

// libvpx splits a frame across tiles and rows; past four threads the gain
// is marginal and the cores are the app's (the decoder's cap too).
const MAX_ENCODE_THREADS: usize = 4;
// VP8E_SET_CPUUSED for VP9 realtime: 5..9 trade quality for speed. 7 keeps
// a laptop core well inside a 30 fps frame at 720p and still looks fine;
// the tablet's ceiling is measured in okf/plans/video-encode.md.
const CPU_USED: c_int = 7;
// The narrowest tile column VP9 allows is 256 px: the tile-column count is
// the largest power of two that keeps every column at least this wide.
const MIN_TILE_WIDTH: u32 = 256;
// VP9E_SET_AQ_MODE 3: cyclic refresh, the adaptive quantization for
// realtime encoding (what WebRTC runs).
const AQ_MODE_CYCLIC_REFRESH: c_int = 3;
// The encoder's timebase is the microsecond, so a frame's pts is passed
// through unchanged and a packet's pts comes back the same.
const TIMEBASE_DEN: c_int = 1_000_000;
// libvpx's bitrate is in kilobits per second; below one it refuses.
const MIN_BITRATE_KBPS: u32 = 1;

pub struct Vp9Encoder {
  ctx: Box<ffi::vpx_codec_ctx_t>,
  // The live configuration, re-applied whole on a bitrate change
  // (`vpx_codec_enc_config_set` takes the full struct).
  cfg: Box<ffi::vpx_codec_enc_cfg_t>,
  width: u32,
  height: u32,
  // The nominal frame duration in timebase units, the rate control's
  // notion of a frame; frames keep their real timestamps regardless.
  frame_duration_us: c_ulong,
}

// The encoder state is used from one thread at a time (its owner's);
// libvpx keeps no global state across contexts.
unsafe impl Send for Vp9Encoder {}

impl Vp9Encoder {
  /// Create an encoder for `config`. Errs on an ABI mismatch with the
  /// linked libvpx, a size it refuses, or a failed control.
  pub fn new(config: &EncoderConfig) -> Result<Vp9Encoder, String> {
    if config.width == 0 || config.height == 0 {
      return Err(format!("encoder size {}x{} is empty", config.width, config.height));
    }
    if config.frame_rate == 0 {
      return Err("encoder frame rate is zero".to_string());
    }
    // SAFETY: the iface pointer is a static inside libvpx.
    let iface = unsafe { ffi::vpx_codec_vp9_cx() };
    // A zeroed config is what libvpx fills; the Box keeps both structs at
    // stable addresses for the library's lifetime.
    let mut cfg: Box<ffi::vpx_codec_enc_cfg_t> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: iface and cfg are valid for the call; usage 0 is the one
    // usage VP9 defines.
    let err = unsafe { ffi::vpx_codec_enc_config_default(iface, &mut *cfg, 0) };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("libvpx VP9 encoder defaults: {}", err_string(err)));
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(MAX_ENCODE_THREADS);
    cfg.g_w = config.width;
    cfg.g_h = config.height;
    cfg.g_threads = threads as c_uint;
    cfg.g_timebase = ffi::vpx_rational_t { num: 1, den: TIMEBASE_DEN };
    cfg.g_pass = ffi::VPX_RC_ONE_PASS;
    cfg.g_lag_in_frames = 0;
    cfg.rc_target_bitrate = kbps(config.bitrate_bps);
    cfg.rc_dropframe_thresh = 0;
    cfg.kf_mode = ffi::VPX_KF_AUTO;
    cfg.kf_min_dist = 0;
    cfg.kf_max_dist = keyframe_frames(config);
    match config.mode {
      EncodeMode::Recording => {
        cfg.rc_end_usage = ffi::VPX_VBR;
        cfg.g_error_resilient = 0;
      }
      EncodeMode::Live => {
        cfg.rc_end_usage = ffi::VPX_CBR;
        cfg.g_error_resilient = 1;
      }
    }

    let mut ctx: Box<ffi::vpx_codec_ctx_t> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: ctx and cfg are valid; on failure the context needs no
    // destroy.
    let err = unsafe { ffi::vpx_codec_enc_init_ver(&mut *ctx, iface, &*cfg, 0, ffi::VPX_ENCODER_ABI_VERSION) };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("init libvpx VP9 encoder: {}", err_string(err)));
    }
    let frame_duration_us = (TIMEBASE_DEN as u64 / config.frame_rate as u64).max(1) as c_ulong;
    let mut encoder = Vp9Encoder { ctx, cfg, width: config.width, height: config.height, frame_duration_us };

    let tile_columns = (config.width / MIN_TILE_WIDTH).max(1).ilog2() as c_int;
    let controls: [(&str, c_int, c_int); 7] = [
      ("cpu-used", ffi::VP8E_SET_CPUUSED, CPU_USED),
      ("row-mt", ffi::VP9E_SET_ROW_MT, 1),
      ("tile-columns", ffi::VP9E_SET_TILE_COLUMNS, tile_columns),
      ("auto-alt-ref", ffi::VP8E_SET_ENABLEAUTOALTREF, 0),
      ("aq-mode", ffi::VP9E_SET_AQ_MODE, AQ_MODE_CYCLIC_REFRESH),
      ("color-space", ffi::VP9E_SET_COLOR_SPACE, ffi::VPX_CS_BT_601),
      ("color-range", ffi::VP9E_SET_COLOR_RANGE, ffi::VPX_CR_STUDIO_RANGE),
    ];
    for (name, id, value) in controls {
      encoder.control(name, id, value)?;
    }
    Ok(encoder)
  }

  fn control(&mut self, name: &str, id: c_int, value: c_int) -> Result<(), String> {
    // SAFETY: the context is initialized; every id here takes one int.
    let err = unsafe { ffi::vpx_codec_control_(&mut *self.ctx, id, value) };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("libvpx VP9 encoder {name}: {}{}", err_string(err), error_detail(&self.ctx)));
    }
    Ok(())
  }

  /// Run one encode call (a frame, or the end of the stream for `None`)
  /// and collect the packets it produced.
  fn encode_into(
    &mut self,
    img: Option<&ffi::vpx_image_t>,
    pts_us: i64,
    flags: c_long,
  ) -> Result<Vec<EncodedFrame>, String> {
    let img_ptr = img.map_or(ptr::null(), |img| img as *const ffi::vpx_image_t);
    // SAFETY: the context is initialized; `img` (when given) describes a
    // live slice for the duration of the call, which libvpx reads and
    // does not keep.
    let err = unsafe {
      ffi::vpx_codec_encode(&mut *self.ctx, img_ptr, pts_us, self.frame_duration_us, flags, ffi::VPX_DL_REALTIME)
    };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("{}{}", err_string(err), error_detail(&self.ctx)));
    }
    let mut packets = Vec::new();
    let mut iter: ffi::vpx_codec_iter_t = ptr::null();
    loop {
      // SAFETY: the iterator protocol of vpx_codec_get_cx_data; a packet
      // stays valid until the next encode on this context, and its bytes
      // are copied out before that.
      let pkt = unsafe { ffi::vpx_codec_get_cx_data(&mut *self.ctx, &mut iter) };
      if pkt.is_null() {
        break;
      }
      let pkt = unsafe { &*pkt };
      if pkt.kind != ffi::VPX_CODEC_CX_FRAME_PKT {
        continue;
      }
      // SAFETY: the kind says the union holds a frame.
      let frame = unsafe { pkt.data.frame };
      // SAFETY: buf/sz describe the packet's bytes, owned by libvpx until
      // the next encode.
      let data = unsafe { std::slice::from_raw_parts(frame.buf as *const u8, frame.sz) }.to_vec();
      packets.push(EncodedFrame { pts_us: frame.pts, keyframe: frame.flags & ffi::VPX_FRAME_IS_KEY != 0, data });
    }
    Ok(packets)
  }
}

impl VideoEncoder for Vp9Encoder {
  fn encode(&mut self, i420: &[u8], pts_us: i64, force_keyframe: bool) -> Result<Vec<EncodedFrame>, String> {
    let (w, h) = (self.width, self.height);
    let expected = PixelLayout::I420.frame_size(w, h);
    if i420.len() != expected {
      return Err(format!("frame of {} bytes is not {w}x{h} I420 ({expected} bytes)", i420.len()));
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let y_len = w as usize * h as usize;
    let c_len = cw as usize * ch as usize;
    // libvpx reads the planes through a const image; the pointers are
    // `*mut` only because the struct is shared with the decoder's output.
    let base = i420.as_ptr() as *mut u8;
    // SAFETY: a zeroed vpx_image_t is a valid "no planes" image; every
    // field the encoder reads is set below.
    let mut img: ffi::vpx_image_t = unsafe { std::mem::zeroed() };
    img.fmt = ffi::VPX_IMG_FMT_I420;
    img.cs = ffi::VPX_CS_BT_601;
    img.range = ffi::VPX_CR_STUDIO_RANGE;
    img.w = w;
    img.h = h;
    img.bit_depth = 8;
    img.d_w = w;
    img.d_h = h;
    img.r_w = w;
    img.r_h = h;
    img.x_chroma_shift = 1;
    img.y_chroma_shift = 1;
    // SAFETY: the offsets stay inside the slice, whose length was checked
    // against the layout above.
    img.planes = [base, unsafe { base.add(y_len) }, unsafe { base.add(y_len + c_len) }, ptr::null_mut()];
    img.stride = [w as c_int, cw as c_int, cw as c_int, 0];
    img.bps = 12;
    img.img_data = base;
    let flags = if force_keyframe { ffi::VPX_EFLAG_FORCE_KF } else { 0 };
    self.encode_into(Some(&img), pts_us, flags)
  }

  fn flush(&mut self) -> Result<Vec<EncodedFrame>, String> {
    self.encode_into(None, 0, 0)
  }

  fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<(), String> {
    self.cfg.rc_target_bitrate = kbps(bitrate_bps);
    // SAFETY: both structs are initialized and owned by this encoder.
    let err = unsafe { ffi::vpx_codec_enc_config_set(&mut *self.ctx, &*self.cfg) };
    if err != ffi::VPX_CODEC_OK {
      return Err(format!("libvpx VP9 encoder bitrate: {}{}", err_string(err), error_detail(&self.ctx)));
    }
    Ok(())
  }
}

impl Drop for Vp9Encoder {
  fn drop(&mut self) {
    // SAFETY: initialized in `new`, destroyed exactly once here.
    unsafe { ffi::vpx_codec_destroy(&mut *self.ctx) };
  }
}

fn kbps(bitrate_bps: u32) -> c_uint {
  (bitrate_bps / 1000).max(MIN_BITRATE_KBPS)
}

/// The keyframe interval in frames at the nominal rate.
fn keyframe_frames(config: &EncoderConfig) -> c_uint {
  let frames = config.keyframe_interval_us.max(0) as u64 * config.frame_rate as u64 / TIMEBASE_DEN as u64;
  frames.clamp(1, c_uint::MAX as u64) as c_uint
}
