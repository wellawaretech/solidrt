// Hand-written bindings to the C shim over msdfgen (alloy/csrc/msdf_shim.cpp,
// compiled with the vendored core by build.rs). The surface is a dozen
// functions over one opaque handle, so one block is the whole thing a
// reviewer needs to read, as forge's Basis bindings are. Coordinates are
// the shim's (y up, inside on the right of travel); the rules that make an
// outline fit them live in `super::msdf`, the only caller.

use std::ffi::{c_double, c_float, c_int};

/// An msdfgen shape under construction, owned by the shim.
#[repr(C)]
pub struct MsdfShape {
  _private: [u8; 0],
}

extern "C" {
  pub fn msdf_shape_new() -> *mut MsdfShape;
  pub fn msdf_shape_free(shape: *mut MsdfShape);
  pub fn msdf_shape_contour(shape: *mut MsdfShape);
  pub fn msdf_shape_line(shape: *mut MsdfShape, x0: c_double, y0: c_double, x1: c_double, y1: c_double);
  pub fn msdf_shape_quad(
    shape: *mut MsdfShape,
    x0: c_double,
    y0: c_double,
    cx: c_double,
    cy: c_double,
    x1: c_double,
    y1: c_double,
  );
  pub fn msdf_shape_cubic(
    shape: *mut MsdfShape,
    x0: c_double,
    y0: c_double,
    c0x: c_double,
    c0y: c_double,
    c1x: c_double,
    c1y: c_double,
    x1: c_double,
    y1: c_double,
  );
  pub fn msdf_shape_reverse(shape: *mut MsdfShape);
  pub fn msdf_shape_normalize(shape: *mut MsdfShape);
  pub fn msdf_shape_bounds(
    shape: *mut MsdfShape,
    left: *mut c_double,
    bottom: *mut c_double,
    right: *mut c_double,
    top: *mut c_double,
  ) -> c_int;
  pub fn msdf_generate_mtsdf(
    shape: *mut MsdfShape,
    angle: c_double,
    pixels: *mut c_float,
    width: c_int,
    height: c_int,
    scale: c_double,
    translate_x: c_double,
    translate_y: c_double,
    range: c_double,
  ) -> c_int;
}
