// Hand-written bindings to the libvpx VP9 decode API, the subset forge uses,
// against the vendored libvpx (forge/vendor/libvpx, pinned by the
// submodule; built and linked by build.rs). Declared by hand rather than
// generated: the surface is eight functions and three structs whose layout
// has not changed since these ABI versions were set, and one block is the
// whole thing a reviewer needs to read. `vpx_codec_dec_init_ver` checks the
// ABI version against the library it was built into, so a layout drift
// fails at open, never silently.
//
// Nothing here is for the rest of forge to call: the safe surface is
// `super::Vp9Decoder`, and under `video-encode` `super::Vp9Encoder` over
// the encoder block at the end (vpx_codec_vp9_cx and the vpx_encoder.h
// entry points, which build.rs then configures into the library).

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_long, c_uint, c_void};

/// VPX_DECODER_ABI_VERSION of the pinned libvpx: 3 + VPX_CODEC_ABI_VERSION,
/// which is 4 + VPX_IMAGE_ABI_VERSION (5). Bump with the submodule when the
/// headers change it; a mismatch makes init fail with VPX_CODEC_ABI_MISMATCH.
pub const VPX_DECODER_ABI_VERSION: c_int = 3 + 4 + 5;

/// vpx_codec_err_t: the one value that is not an error.
pub const VPX_CODEC_OK: c_int = 0;

/// vpx_img_fmt_t for 8-bit planar 4:2:0: VPX_IMG_FMT_PLANAR (0x100) | 2.
pub const VPX_IMG_FMT_I420: c_int = 0x100 | 2;

/// Opaque: the VP9 decoder interface returned by `vpx_codec_vp9_dx`.
#[repr(C)]
pub struct vpx_codec_iface_t {
  _private: [u8; 0],
}

/// Opaque: the decoder's private state behind the context.
#[repr(C)]
pub struct vpx_codec_priv_t {
  _private: [u8; 0],
}

#[repr(C)]
pub struct vpx_codec_dec_cfg_t {
  pub threads: c_uint,
  pub w: c_uint,
  pub h: c_uint,
}

/// vpx_codec_ctx_t. `config` is a union of three const pointers in the
/// header, so one pointer-wide field here.
#[repr(C)]
pub struct vpx_codec_ctx_t {
  pub name: *const c_char,
  pub iface: *mut vpx_codec_iface_t,
  pub err: c_int,
  pub err_detail: *const c_char,
  pub init_flags: c_long,
  pub config: *const c_void,
  pub priv_: *mut vpx_codec_priv_t,
}

/// vpx_image_t, one decoded picture. `fmt`, `cs` and `range` are C enums
/// (int-sized); the planes point into decoder-owned memory that is valid
/// until the next `vpx_codec_decode` on the same context.
#[repr(C)]
pub struct vpx_image_t {
  pub fmt: c_int,
  pub cs: c_int,
  pub range: c_int,
  pub w: c_uint,
  pub h: c_uint,
  pub bit_depth: c_uint,
  pub d_w: c_uint,
  pub d_h: c_uint,
  pub r_w: c_uint,
  pub r_h: c_uint,
  pub x_chroma_shift: c_uint,
  pub y_chroma_shift: c_uint,
  pub planes: [*mut u8; 4],
  pub stride: [c_int; 4],
  pub bps: c_int,
  pub user_priv: *mut c_void,
  pub img_data: *mut u8,
  pub img_data_owner: c_int,
  pub self_allocd: c_int,
  pub fb_priv: *mut c_void,
}

pub type vpx_codec_iter_t = *const c_void;

extern "C" {
  pub fn vpx_codec_vp9_dx() -> *mut vpx_codec_iface_t;
  pub fn vpx_codec_dec_init_ver(
    ctx: *mut vpx_codec_ctx_t,
    iface: *mut vpx_codec_iface_t,
    cfg: *const vpx_codec_dec_cfg_t,
    flags: c_long,
    ver: c_int,
  ) -> c_int;
  pub fn vpx_codec_decode(
    ctx: *mut vpx_codec_ctx_t,
    data: *const u8,
    data_sz: c_uint,
    user_priv: *mut c_void,
    deadline: c_long,
  ) -> c_int;
  pub fn vpx_codec_get_frame(ctx: *mut vpx_codec_ctx_t, iter: *mut vpx_codec_iter_t) -> *mut vpx_image_t;
  pub fn vpx_codec_destroy(ctx: *mut vpx_codec_ctx_t) -> c_int;
  pub fn vpx_codec_err_to_string(err: c_int) -> *const c_char;
  pub fn vpx_codec_error_detail(ctx: *const vpx_codec_ctx_t) -> *const c_char;
}

/// VPX_ENCODER_ABI_VERSION of the pinned libvpx: 18 + VPX_CODEC_ABI_VERSION
/// (4 + VPX_IMAGE_ABI_VERSION 5) + VPX_EXT_RATECTRL_ABI_VERSION (7 +
/// VPX_TPL_ABI_VERSION 5). Bump with the submodule; a mismatch makes
/// `vpx_codec_enc_init_ver` fail with VPX_CODEC_ABI_MISMATCH.
#[cfg(feature = "video-encode")]
pub const VPX_ENCODER_ABI_VERSION: c_int = 18 + (4 + 5) + (7 + 5);

#[cfg(feature = "video-encode")]
pub use encoder::*;

/// The encoder half of the API (vpx_encoder.h, vp8cx.h): the configuration
/// struct the encoder is initialized from, its output packet, the entry
/// points, and the control ids the realtime settings go through. The
/// control ids are the enum's values in vp8cx.h of the pinned libvpx,
/// checked against the header when they were written (2026-10-16); a wrong
/// id fails the control call at open, never silently.
#[cfg(feature = "video-encode")]
mod encoder {
  use super::*;
  use std::ffi::c_ulong;

  pub const VPX_SS_MAX_LAYERS: usize = 5;
  pub const VPX_TS_MAX_LAYERS: usize = 5;
  pub const VPX_TS_MAX_PERIODICITY: usize = 16;
  pub const VPX_MAX_LAYERS: usize = 12;
  /// The vizier rate-control factors at the end of the config: fifteen
  /// rationals the encoder fills with defaults and this crate never sets.
  pub const VIZIER_FACTORS: usize = 15;

  /// enum vpx_enc_pass.
  pub const VPX_RC_ONE_PASS: c_int = 0;
  /// enum vpx_rc_mode.
  pub const VPX_VBR: c_int = 0;
  pub const VPX_CBR: c_int = 1;
  /// enum vpx_kf_mode.
  pub const VPX_KF_AUTO: c_int = 1;
  /// vpx_codec_encode flags and deadline.
  pub const VPX_EFLAG_FORCE_KF: c_long = 1;
  pub const VPX_DL_REALTIME: c_ulong = 1;
  /// enum vpx_codec_cx_pkt_kind: a compressed frame.
  pub const VPX_CODEC_CX_FRAME_PKT: c_int = 0;
  /// vpx_codec_frame_flags_t: the packet is a keyframe.
  pub const VPX_FRAME_IS_KEY: u32 = 1;
  /// vpx_color_space_t and vpx_color_range_t (vpx_image.h).
  pub const VPX_CS_BT_601: c_int = 1;
  pub const VPX_CR_STUDIO_RANGE: c_int = 0;

  /// enum vp8e_enc_control_id (vp8cx.h).
  pub const VP8E_SET_CPUUSED: c_int = 13;
  pub const VP8E_SET_ENABLEAUTOALTREF: c_int = 14;
  pub const VP9E_SET_TILE_COLUMNS: c_int = 33;
  pub const VP9E_SET_AQ_MODE: c_int = 36;
  pub const VP9E_SET_COLOR_SPACE: c_int = 46;
  pub const VP9E_SET_COLOR_RANGE: c_int = 51;
  pub const VP9E_SET_ROW_MT: c_int = 55;

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct vpx_rational_t {
    pub num: c_int,
    pub den: c_int,
  }

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct vpx_fixed_buf_t {
    pub buf: *mut c_void,
    pub sz: usize,
  }

  /// vpx_codec_enc_cfg_t, every field, in the header's order: the encoder
  /// fills it (`vpx_codec_enc_config_default`) and reads it back
  /// (`vpx_codec_enc_init_ver`, `vpx_codec_enc_config_set`), so the layout
  /// must be the library's. Enums are int-sized.
  #[repr(C)]
  pub struct vpx_codec_enc_cfg_t {
    pub g_usage: c_uint,
    pub g_threads: c_uint,
    pub g_profile: c_uint,
    pub g_w: c_uint,
    pub g_h: c_uint,
    pub g_bit_depth: c_int,
    pub g_input_bit_depth: c_uint,
    pub g_timebase: vpx_rational_t,
    pub g_error_resilient: u32,
    pub g_pass: c_int,
    pub g_lag_in_frames: c_uint,
    pub rc_dropframe_thresh: c_uint,
    pub rc_resize_allowed: c_uint,
    pub rc_scaled_width: c_uint,
    pub rc_scaled_height: c_uint,
    pub rc_resize_up_thresh: c_uint,
    pub rc_resize_down_thresh: c_uint,
    pub rc_end_usage: c_int,
    pub rc_twopass_stats_in: vpx_fixed_buf_t,
    pub rc_firstpass_mb_stats_in: vpx_fixed_buf_t,
    pub rc_target_bitrate: c_uint,
    pub rc_min_quantizer: c_uint,
    pub rc_max_quantizer: c_uint,
    pub rc_undershoot_pct: c_uint,
    pub rc_overshoot_pct: c_uint,
    pub rc_buf_sz: c_uint,
    pub rc_buf_initial_sz: c_uint,
    pub rc_buf_optimal_sz: c_uint,
    pub rc_2pass_vbr_bias_pct: c_uint,
    pub rc_2pass_vbr_minsection_pct: c_uint,
    pub rc_2pass_vbr_maxsection_pct: c_uint,
    pub rc_2pass_vbr_corpus_complexity: c_uint,
    pub kf_mode: c_int,
    pub kf_min_dist: c_uint,
    pub kf_max_dist: c_uint,
    pub ss_number_layers: c_uint,
    pub ss_enable_auto_alt_ref: [c_int; VPX_SS_MAX_LAYERS],
    pub ss_target_bitrate: [c_uint; VPX_SS_MAX_LAYERS],
    pub ts_number_layers: c_uint,
    pub ts_target_bitrate: [c_uint; VPX_TS_MAX_LAYERS],
    pub ts_rate_decimator: [c_uint; VPX_TS_MAX_LAYERS],
    pub ts_periodicity: c_uint,
    pub ts_layer_id: [c_uint; VPX_TS_MAX_PERIODICITY],
    pub layer_target_bitrate: [c_uint; VPX_MAX_LAYERS],
    pub temporal_layering_mode: c_int,
    pub use_vizier_rc_params: c_int,
    pub vizier: [vpx_rational_t; VIZIER_FACTORS],
  }

  /// The `frame` member of vpx_codec_cx_pkt's data union: what a
  /// VPX_CODEC_CX_FRAME_PKT carries. `duration` is `unsigned long`, so the
  /// layout follows the platform's C long as Rust's does.
  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct vpx_codec_cx_pkt_frame {
    pub buf: *mut c_void,
    pub sz: usize,
    pub pts: i64,
    pub duration: c_ulong,
    pub flags: u32,
    pub partition_id: c_int,
    pub width: [c_uint; VPX_SS_MAX_LAYERS],
    pub height: [c_uint; VPX_SS_MAX_LAYERS],
    pub spatial_layer_encoded: [u8; VPX_SS_MAX_LAYERS],
  }

  /// The union is padded to 128 bytes less the kind by the header's `pad`
  /// member, which makes it 128 bytes at pointer alignment.
  #[repr(C)]
  #[derive(Clone, Copy)]
  pub union vpx_codec_cx_pkt_data {
    pub frame: vpx_codec_cx_pkt_frame,
    pub pad: [u8; 128 - std::mem::size_of::<c_int>()],
  }

  #[repr(C)]
  pub struct vpx_codec_cx_pkt_t {
    pub kind: c_int,
    pub data: vpx_codec_cx_pkt_data,
  }

  // The two structs the encoder reads back, pinned to the sizes a C
  // compiler gives them against the vendored headers on a 64-bit target
  // (measured 2026-10-16): a field added or reordered upstream fails here,
  // not inside libvpx.
  #[cfg(target_pointer_width = "64")]
  const _: () = assert!(std::mem::size_of::<vpx_codec_enc_cfg_t>() == 504);
  #[cfg(target_pointer_width = "64")]
  const _: () = assert!(std::mem::size_of::<vpx_codec_cx_pkt_t>() == 136);

  extern "C" {
    pub fn vpx_codec_vp9_cx() -> *mut vpx_codec_iface_t;
    pub fn vpx_codec_enc_config_default(
      iface: *mut vpx_codec_iface_t,
      cfg: *mut vpx_codec_enc_cfg_t,
      usage: c_uint,
    ) -> c_int;
    pub fn vpx_codec_enc_init_ver(
      ctx: *mut vpx_codec_ctx_t,
      iface: *mut vpx_codec_iface_t,
      cfg: *const vpx_codec_enc_cfg_t,
      flags: c_long,
      ver: c_int,
    ) -> c_int;
    pub fn vpx_codec_enc_config_set(ctx: *mut vpx_codec_ctx_t, cfg: *const vpx_codec_enc_cfg_t) -> c_int;
    pub fn vpx_codec_encode(
      ctx: *mut vpx_codec_ctx_t,
      img: *const vpx_image_t,
      pts: i64,
      duration: c_ulong,
      flags: c_long,
      deadline: c_ulong,
    ) -> c_int;
    pub fn vpx_codec_get_cx_data(ctx: *mut vpx_codec_ctx_t, iter: *mut vpx_codec_iter_t) -> *const vpx_codec_cx_pkt_t;
    /// Variadic control; every id used here takes one int.
    pub fn vpx_codec_control_(ctx: *mut vpx_codec_ctx_t, ctrl_id: c_int, ...) -> c_int;
  }
}
