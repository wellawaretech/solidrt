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
// `super::Vp9Decoder`. The encoder bindings (vpx_codec_vp9_cx and the
// vpx_encoder.h entry points) join this block when encoding lands.

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
