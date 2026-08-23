// Column-major 4x4 math, the exact formulas the 3d package's math.ts uses
// (compose / multiply / normalMatrix), so a matrix computed here equals
// one computed in JS bit for bit.

use super::Mat4;

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Translation * rotation (unit quaternion, xyzw) * scale.
pub fn compose(position: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Mat4 {
  let [x, y, z, w] = rotation;
  let (x2, y2, z2) = (x + x, y + y, z + z);
  let (xx, xy, xz) = (x * x2, x * y2, x * z2);
  let (yy, yz, zz) = (y * y2, y * z2, z * z2);
  let (wx, wy, wz) = (w * x2, w * y2, w * z2);
  let [sx, sy, sz] = scale;
  [
    (1.0 - (yy + zz)) * sx,
    (xy + wz) * sx,
    (xz - wy) * sx,
    0.0,
    (xy - wz) * sy,
    (1.0 - (xx + zz)) * sy,
    (yz + wx) * sy,
    0.0,
    (xz + wy) * sz,
    (yz - wx) * sz,
    (1.0 - (xx + yy)) * sz,
    0.0,
    position[0],
    position[1],
    position[2],
    1.0,
  ]
}

/// a * b (column vectors: b applies first).
pub fn multiply(a: Mat4, b: Mat4) -> Mat4 {
  let mut out = [0.0; 16];
  for col in 0..4 {
    let (b0, b1, b2, b3) = (b[col * 4], b[col * 4 + 1], b[col * 4 + 2], b[col * 4 + 3]);
    for row in 0..4 {
      out[col * 4 + row] = b0 * a[row] + b1 * a[4 + row] + b2 * a[8 + row] + b3 * a[12 + row];
    }
  }
  out
}

/// Inverse-transpose of the upper 3x3 packed into a mat4 (shaders take
/// `mat3(uNormal)`); a degenerate input yields the raw cofactors, not NaNs.
pub fn normal_matrix(m: &Mat4) -> Mat4 {
  let (a, b, c) = (m[0], m[4], m[8]);
  let (d, e, f) = (m[1], m[5], m[9]);
  let (g, h, i) = (m[2], m[6], m[10]);
  let c00 = e * i - f * h;
  let c01 = f * g - d * i;
  let c02 = d * h - e * g;
  let det = a * c00 + b * c01 + c * c02;
  let s = 1.0 / if det == 0.0 { 1.0 } else { det };
  [
    c00 * s,
    (c * h - b * i) * s,
    (b * f - c * e) * s,
    0.0,
    c01 * s,
    (a * i - c * g) * s,
    (c * d - a * f) * s,
    0.0,
    c02 * s,
    (b * g - a * h) * s,
    (a * e - b * d) * s,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
  ]
}

/// Invert an AFFINE matrix (bottom row 0,0,0,1): the upper 3x3 by
/// cofactors, the translation pulled back through it. A degenerate input
/// yields the raw cofactors instead of NaNs, like `normal_matrix`.
pub fn invert_affine(m: &Mat4) -> Mat4 {
  let (a, b, c) = (m[0], m[4], m[8]);
  let (d, e, f) = (m[1], m[5], m[9]);
  let (g, h, i) = (m[2], m[6], m[10]);
  let (tx, ty, tz) = (m[12], m[13], m[14]);
  let c00 = e * i - f * h;
  let c01 = f * g - d * i;
  let c02 = d * h - e * g;
  let det = a * c00 + b * c01 + c * c02;
  let s = 1.0 / if det == 0.0 { 1.0 } else { det };
  let (r00, r01, r02) = (c00 * s, (c * h - b * i) * s, (b * f - c * e) * s);
  let (r10, r11, r12) = (c01 * s, (a * i - c * g) * s, (c * d - a * f) * s);
  let (r20, r21, r22) = (c02 * s, (b * g - a * h) * s, (a * e - b * d) * s);
  [
    r00,
    r10,
    r20,
    0.0,
    r01,
    r11,
    r21,
    0.0,
    r02,
    r12,
    r22,
    0.0,
    -(r00 * tx + r01 * ty + r02 * tz),
    -(r10 * tx + r11 * ty + r12 * tz),
    -(r20 * tx + r21 * ty + r22 * tz),
    1.0,
  ]
}

/// m * [v, 1].
pub fn transform_point(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
  [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14],
  ]
}

/// m * [v, 0] (the upper 3x3 only).
pub fn transform_vector(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
  [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
  ]
}
