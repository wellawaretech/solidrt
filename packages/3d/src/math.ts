// 3D math over plain number arrays: column-major mat4 (the layout mat4
// uniforms expect) in a right-handed, y-up world with the camera looking
// down -z. perspective() bakes the engine's y-down clip flip into the
// projection, so scene code never sees the flip and standard CCW-outward
// meshes cull correctly with cull: "back" (see the pixel contract in
// @solidrt/core/gpu). Every function writes into a caller-owned `out`
// matrix and allocates nothing: the hot path - a moved node is one compose,
// one or two multiplies, one param write - must stay allocation-free on an
// interpreter. Mat4 is a 16-tuple so constant-index access stays plain
// `number` under noUncheckedIndexedAccess.

export type Vec2 = [number, number]
export type Vec3 = [number, number, number]
export type Vec4 = [number, number, number, number]
// prettier-ignore
export type Mat4 = [
  number, number, number, number,
  number, number, number, number,
  number, number, number, number,
  number, number, number, number,
]

/** A fresh identity matrix. */
export function mat4(): Mat4 {
  return [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
}

export function identity(out: Mat4): Mat4 {
  out[0] = 1; out[1] = 0; out[2] = 0; out[3] = 0
  out[4] = 0; out[5] = 1; out[6] = 0; out[7] = 0
  out[8] = 0; out[9] = 0; out[10] = 1; out[11] = 0
  out[12] = 0; out[13] = 0; out[14] = 0; out[15] = 1
  return out
}

export function copy(out: Mat4, m: Mat4): Mat4 {
  out[0] = m[0]; out[1] = m[1]; out[2] = m[2]; out[3] = m[3]
  out[4] = m[4]; out[5] = m[5]; out[6] = m[6]; out[7] = m[7]
  out[8] = m[8]; out[9] = m[9]; out[10] = m[10]; out[11] = m[11]
  out[12] = m[12]; out[13] = m[13]; out[14] = m[14]; out[15] = m[15]
  return out
}

/**
 * Transform a point by m with w = 1, keeping the homogeneous result: the
 * clip-space building block (scene.project, picking). The caller owns the
 * perspective divide and the w <= 0 behind-the-camera test.
 */
export function transformPoint(out: Vec4, m: Mat4, p: Vec3): Vec4 {
  let x = p[0], y = p[1], z = p[2]
  out[0] = m[0] * x + m[4] * y + m[8] * z + m[12]
  out[1] = m[1] * x + m[5] * y + m[9] * z + m[13]
  out[2] = m[2] * x + m[6] * y + m[10] * z + m[14]
  out[3] = m[3] * x + m[7] * y + m[11] * z + m[15]
  return out
}

/** out = a * b (column vectors: b applies first). out may alias a or b. */
export function multiply(out: Mat4, a: Mat4, b: Mat4): Mat4 {
  let a00 = a[0], a01 = a[1], a02 = a[2], a03 = a[3]
  let a10 = a[4], a11 = a[5], a12 = a[6], a13 = a[7]
  let a20 = a[8], a21 = a[9], a22 = a[10], a23 = a[11]
  let a30 = a[12], a31 = a[13], a32 = a[14], a33 = a[15]
  let b0 = b[0], b1 = b[1], b2 = b[2], b3 = b[3]
  out[0] = b0 * a00 + b1 * a10 + b2 * a20 + b3 * a30
  out[1] = b0 * a01 + b1 * a11 + b2 * a21 + b3 * a31
  out[2] = b0 * a02 + b1 * a12 + b2 * a22 + b3 * a32
  out[3] = b0 * a03 + b1 * a13 + b2 * a23 + b3 * a33
  b0 = b[4]; b1 = b[5]; b2 = b[6]; b3 = b[7]
  out[4] = b0 * a00 + b1 * a10 + b2 * a20 + b3 * a30
  out[5] = b0 * a01 + b1 * a11 + b2 * a21 + b3 * a31
  out[6] = b0 * a02 + b1 * a12 + b2 * a22 + b3 * a32
  out[7] = b0 * a03 + b1 * a13 + b2 * a23 + b3 * a33
  b0 = b[8]; b1 = b[9]; b2 = b[10]; b3 = b[11]
  out[8] = b0 * a00 + b1 * a10 + b2 * a20 + b3 * a30
  out[9] = b0 * a01 + b1 * a11 + b2 * a21 + b3 * a31
  out[10] = b0 * a02 + b1 * a12 + b2 * a22 + b3 * a32
  out[11] = b0 * a03 + b1 * a13 + b2 * a23 + b3 * a33
  b0 = b[12]; b1 = b[13]; b2 = b[14]; b3 = b[15]
  out[12] = b0 * a00 + b1 * a10 + b2 * a20 + b3 * a30
  out[13] = b0 * a01 + b1 * a11 + b2 * a21 + b3 * a31
  out[14] = b0 * a02 + b1 * a12 + b2 * a22 + b3 * a32
  out[15] = b0 * a03 + b1 * a13 + b2 * a23 + b3 * a33
  return out
}

/**
 * Compose translation + rotation + scale into a local matrix. Rotation is
 * Euler angles in radians applied x, then y, then z (R = Rz * Ry * Rx on
 * column vectors) - the common "XYZ" order.
 */
export function compose(out: Mat4, position: Vec3, rotation: Vec3, scale: Vec3): Mat4 {
  let cx = Math.cos(rotation[0]), sx = Math.sin(rotation[0])
  let cy = Math.cos(rotation[1]), sy = Math.sin(rotation[1])
  let cz = Math.cos(rotation[2]), sz = Math.sin(rotation[2])
  let r00 = cz * cy
  let r01 = cz * sy * sx - sz * cx
  let r02 = cz * sy * cx + sz * sx
  let r10 = sz * cy
  let r11 = sz * sy * sx + cz * cx
  let r12 = sz * sy * cx - cz * sx
  let r20 = -sy
  let r21 = cy * sx
  let r22 = cy * cx
  out[0] = r00 * scale[0]; out[1] = r10 * scale[0]; out[2] = r20 * scale[0]; out[3] = 0
  out[4] = r01 * scale[1]; out[5] = r11 * scale[1]; out[6] = r21 * scale[1]; out[7] = 0
  out[8] = r02 * scale[2]; out[9] = r12 * scale[2]; out[10] = r22 * scale[2]; out[11] = 0
  out[12] = position[0]; out[13] = position[1]; out[14] = position[2]; out[15] = 1
  return out
}

/**
 * Right-handed perspective projection with the engine's y-down clip flip
 * BAKED IN (row two is negated): geometry authored y-up displays y-up, and
 * the flip this projection applies is exactly what makes displayed-CCW
 * front faces line up with CCW-outward meshes, so cull: "back" works.
 * `fovy` is the vertical field of view in RADIANS.
 */
export function perspective(out: Mat4, fovy: number, aspect: number, near: number, far: number): Mat4 {
  let f = 1 / Math.tan(fovy / 2)
  let nf = 1 / (near - far)
  out[0] = f / aspect; out[1] = 0; out[2] = 0; out[3] = 0
  out[4] = 0; out[5] = -f; out[6] = 0; out[7] = 0
  out[8] = 0; out[9] = 0; out[10] = (far + near) * nf; out[11] = -1
  out[12] = 0; out[13] = 0; out[14] = 2 * far * near * nf; out[15] = 0
  return out
}

// Vec3 helpers for geometry construction. These allocate (unlike the matrix
// functions above): they serve generation-time code - curve frames, normals -
// not the per-frame path. Exposed on the /math subpath only, so `add` does
// not collide with the scene's add() on the package root.

export function add(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

export function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

export function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

export function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

export function scale(v: Vec3, s: number): Vec3 {
  return [v[0] * s, v[1] * s, v[2] * s]
}

/** Unit vector; a zero-length input comes back unchanged. */
export function normalize(v: Vec3): Vec3 {
  let len = Math.hypot(v[0], v[1], v[2]) || 1
  return [v[0] / len, v[1] / len, v[2] / len]
}

/**
 * View matrix (world -> camera) for a camera at `eye` looking at `target`
 * with the given `up`. Degenerate inputs (eye == target, up parallel to the
 * view direction) fall back to axis defaults instead of producing NaNs.
 */
export function lookAt(out: Mat4, eye: Vec3, target: Vec3, up: Vec3): Mat4 {
  let zx = eye[0] - target[0]
  let zy = eye[1] - target[1]
  let zz = eye[2] - target[2]
  let len = Math.hypot(zx, zy, zz)
  if (len === 0) {
    zx = 0; zy = 0; zz = 1
  } else {
    zx /= len; zy /= len; zz /= len
  }
  let xx = up[1] * zz - up[2] * zy
  let xy = up[2] * zx - up[0] * zz
  let xz = up[0] * zy - up[1] * zx
  len = Math.hypot(xx, xy, xz)
  if (len === 0) {
    xx = 1; xy = 0; xz = 0
  } else {
    xx /= len; xy /= len; xz /= len
  }
  let yx = zy * xz - zz * xy
  let yy = zz * xx - zx * xz
  let yz = zx * xy - zy * xx
  out[0] = xx; out[1] = yx; out[2] = zx; out[3] = 0
  out[4] = xy; out[5] = yy; out[6] = zy; out[7] = 0
  out[8] = xz; out[9] = yz; out[10] = zz; out[11] = 0
  out[12] = -(xx * eye[0] + xy * eye[1] + xz * eye[2])
  out[13] = -(yx * eye[0] + yy * eye[1] + yz * eye[2])
  out[14] = -(zx * eye[0] + zy * eye[1] + zz * eye[2])
  out[15] = 1
  return out
}
