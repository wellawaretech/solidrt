// Hand-written bindings to Basis Universal's C API, the subset forge uses,
// against the vendored sources (forge/vendor/basis_universal, pinned by the
// submodule to a release tag; built and linked by build.rs). The API is
// upstream's own (`encoder/basisu_wasm_transcoder_api.h` and
// `encoder/basisu_wasm_api.h`, the surface its `example_capi` builds
// against): plain C functions over opaque handles. "wasm" in the names is
// historical - in a native build every `u64` "offset" is a pointer cast
// (`wasm_ptr` / `wasm_offset` in the sources), and every `wasm_bool_t` is a
// `u32` that is zero for false. Declared by hand rather than generated: the
// surface is two dozen functions and no structs, so one block is the whole
// thing a reviewer needs to read.
//
// Nothing here is for the rest of forge to call: the safe surface is
// `super::transcode` and `super::encode`.

use std::ffi::c_int;

/// basist::transcoder_texture_format values (transcoder/basisu_transcoder.h).
pub const TF_ETC2_RGBA: u32 = 1;
pub const TF_BC7_RGBA: u32 = 6;
pub const TF_RGBA32: u32 = 13;

/// basist::basis_tex_format values (BTF_* in basisu_wasm_api_common.h).
pub const BTF_ETC1S: u32 = 0;
pub const BTF_UASTC_LDR_4X4: u32 = 1;

/// BU_COMP_FLAGS_* (basisu_wasm_api_common.h).
pub const COMP_FLAGS_THREADED: u64 = 1 << 9;
pub const COMP_FLAGS_KTX2_OUTPUT: u64 = 1 << 11;
pub const COMP_FLAGS_KTX2_UASTC_ZSTD: u64 = 1 << 12;
pub const COMP_FLAGS_SRGB: u64 = 1 << 13;
pub const COMP_FLAGS_GEN_MIPS_CLAMP: u64 = 1 << 14;
pub const COMP_FLAGS_GEN_MIPS_WRAP: u64 = 1 << 15;

/// BU_QUALITY_MIN / BU_QUALITY_MAX: the unified quality scale.
pub const QUALITY_MIN: c_int = 1;
pub const QUALITY_MAX: c_int = 100;
/// BU_EFFORT_DEFAULT: the encoder's own default effort, what its command
/// line tool runs at.
pub const EFFORT_DEFAULT: c_int = 2;
/// The low-level quality argument (a UASTC RDO lambda or a DCT quality)
/// when the unified quality scale is in use: upstream takes one or the
/// other and zeroes this one itself if both are given.
pub const LOW_LEVEL_QUALITY_UNUSED: f32 = 0.0;

extern "C" {
  // Transcoder.
  pub fn bt_init();
  pub fn bt_ktx2_open(data_mem_ofs: u64, data_len: u32) -> u64;
  pub fn bt_ktx2_close(handle: u64);
  pub fn bt_ktx2_get_width(handle: u64) -> u32;
  pub fn bt_ktx2_get_height(handle: u64) -> u32;
  pub fn bt_ktx2_get_levels(handle: u64) -> u32;
  pub fn bt_ktx2_get_faces(handle: u64) -> u32;
  pub fn bt_ktx2_get_layers(handle: u64) -> u32;
  pub fn bt_ktx2_is_etc1s(handle: u64) -> u32;
  pub fn bt_ktx2_is_uastc_ldr_4x4(handle: u64) -> u32;
  pub fn bt_ktx2_is_srgb(handle: u64) -> u32;
  pub fn bt_ktx2_is_video(handle: u64) -> u32;
  pub fn bt_ktx2_get_level_orig_width(handle: u64, level_index: u32, layer_index: u32, face_index: u32) -> u32;
  pub fn bt_ktx2_get_level_orig_height(handle: u64, level_index: u32, layer_index: u32, face_index: u32) -> u32;
  pub fn bt_ktx2_start_transcoding(handle: u64) -> u32;
  pub fn bt_ktx2_transcode_image_level(
    ktx2_handle: u64,
    level_index: u32,
    layer_index: u32,
    face_index: u32,
    output_block_mem_ofs: u64,
    output_blocks_buf_size_in_blocks_or_pixels: u32,
    transcoder_texture_format: u32,
    decode_flags: u32,
    output_row_pitch_in_blocks_or_pixels: u32,
    output_rows_in_pixels: u32,
    channel0: c_int,
    channel1: c_int,
    state_handle: u64,
  ) -> u32;

  // Encoder.
  pub fn bu_init();
  pub fn bu_new_comp_params() -> u64;
  pub fn bu_delete_comp_params(params_ofs: u64) -> u32;
  pub fn bu_comp_params_set_image_rgba32(
    params_ofs: u64,
    image_index: u32,
    img_data_ofs: u64,
    width: u32,
    height: u32,
    pitch_in_bytes: u32,
  ) -> u32;
  pub fn bu_compress_texture(
    params_ofs: u64,
    desired_basis_tex_format: u32,
    quality_level: c_int,
    effort_level: c_int,
    flags_and_quality: u64,
    low_level_uastc_rdo_or_dct_quality: f32,
  ) -> u32;
  pub fn bu_comp_params_get_comp_data_size(params_ofs: u64) -> u64;
  pub fn bu_comp_params_get_comp_data_ofs(params_ofs: u64) -> u64;
}
