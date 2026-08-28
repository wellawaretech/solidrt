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
/** A rotation as [x, y, z, w] - glTF's and Three's component order. */
export type Quat = [number, number, number, number]
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

/**
 * Transform a DIRECTION by m's upper 3x3 (w = 0: rotation and scale apply,
 * translation does not). The ray-direction counterpart of transformPoint.
 */
export function transformVector(out: Vec3, m: Mat4, v: Vec3): Vec3 {
  let x = v[0], y = v[1], z = v[2]
  out[0] = m[0] * x + m[4] * y + m[8] * z
  out[1] = m[1] * x + m[5] * y + m[9] * z
  out[2] = m[2] * x + m[6] * y + m[10] * z
  return out
}

/**
 * Invert an AFFINE matrix - a world or local matrix whose bottom row is
 * 0,0,0,1, NOT a projection: the upper 3x3 inverts by cofactors and the
 * translation is pulled back through it. Picking's world-to-local step.
 * `out` may alias `m`. A degenerate (zero-scale) matrix yields the raw
 * cofactors instead of NaNs, the same policy as normalMatrix.
 */
export function invertAffine(out: Mat4, m: Mat4): Mat4 {
  let a = m[0], b = m[4], c = m[8]
  let d = m[1], e = m[5], f = m[9]
  let g = m[2], h = m[6], i = m[10]
  let tx = m[12], ty = m[13], tz = m[14]
  let c00 = e * i - f * h
  let c01 = f * g - d * i
  let c02 = d * h - e * g
  let det = a * c00 + b * c01 + c * c02
  let s = 1 / (det || 1)
  let r00 = c00 * s, r01 = (c * h - b * i) * s, r02 = (b * f - c * e) * s
  let r10 = c01 * s, r11 = (a * i - c * g) * s, r12 = (c * d - a * f) * s
  let r20 = c02 * s, r21 = (b * g - a * h) * s, r22 = (a * e - b * d) * s
  out[0] = r00; out[1] = r10; out[2] = r20; out[3] = 0
  out[4] = r01; out[5] = r11; out[6] = r21; out[7] = 0
  out[8] = r02; out[9] = r12; out[10] = r22; out[11] = 0
  out[12] = -(r00 * tx + r01 * ty + r02 * tz)
  out[13] = -(r10 * tx + r11 * ty + r12 * tz)
  out[14] = -(r20 * tx + r21 * ty + r22 * tz)
  out[15] = 1
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
 * Compose translation + rotation + scale into a local matrix - Three's
 * `Matrix4.compose` signature, rotation as a quaternion.
 *
 * `rotation` must be a UNIT quaternion: a non-unit one scales the geometry
 * by |q|^2, silently. The scene closes that trap by normalizing on write
 * (setTransform) rather than paying for a check on every compose.
 */
export function compose(out: Mat4, position: Vec3, rotation: Quat, scale: Vec3): Mat4 {
  let x = rotation[0], y = rotation[1], z = rotation[2], w = rotation[3]
  let x2 = x + x, y2 = y + y, z2 = z + z
  let xx = x * x2, xy = x * y2, xz = x * z2
  let yy = y * y2, yz = y * z2, zz = z * z2
  let wx = w * x2, wy = w * y2, wz = w * z2
  let sx = scale[0], sy = scale[1], sz = scale[2]
  out[0] = (1 - (yy + zz)) * sx; out[1] = (xy + wz) * sx; out[2] = (xz - wy) * sx; out[3] = 0
  out[4] = (xy - wz) * sy; out[5] = (1 - (xx + zz)) * sy; out[6] = (yz + wx) * sy; out[7] = 0
  out[8] = (xz + wy) * sz; out[9] = (yz - wx) * sz; out[10] = (1 - (xx + yy)) * sz; out[11] = 0
  out[12] = position[0]; out[13] = position[1]; out[14] = position[2]; out[15] = 1
  return out
}

/**
 * The normal matrix for a world matrix: the inverse-transpose of its upper
 * 3x3 (the cofactor matrix over the determinant), packed into a mat4 - the
 * engine's settable uniform set has no mat3, so shaders take `mat3(uNormal)`.
 * Correct under any transform including non-uniform scale, where
 * `mat3(uModel)` would bend normals off the surface. A degenerate
 * (zero-scale) input yields the raw cofactors instead of NaNs.
 */
export function normalMatrix(out: Mat4, m: Mat4): Mat4 {
  let a = m[0], b = m[4], c = m[8]
  let d = m[1], e = m[5], f = m[9]
  let g = m[2], h = m[6], i = m[10]
  let c00 = e * i - f * h
  let c01 = f * g - d * i
  let c02 = d * h - e * g
  let det = a * c00 + b * c01 + c * c02
  let s = 1 / (det || 1)
  out[0] = c00 * s; out[1] = (c * h - b * i) * s; out[2] = (b * f - c * e) * s; out[3] = 0
  out[4] = c01 * s; out[5] = (a * i - c * g) * s; out[6] = (c * d - a * f) * s; out[7] = 0
  out[8] = c02 * s; out[9] = (b * g - a * h) * s; out[10] = (a * e - b * d) * s; out[11] = 0
  out[12] = 0; out[13] = 0; out[14] = 0; out[15] = 1
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

/**
 * Orthographic projection with the same y-down clip flip BAKED IN as
 * perspective() (row two negated): view-space x in [left, right] and y in
 * [bottom, top] fill the target at any depth, [near, far] maps to depth
 * like perspective. The camera's `ortho` option; the flip lives here for
 * the reason given at perspective().
 */
export function orthographic(out: Mat4, left: number, right: number, top: number, bottom: number, near: number, far: number): Mat4 {
  let lr = 1 / (left - right)
  let bt = 1 / (bottom - top)
  let nf = 1 / (near - far)
  out[0] = -2 * lr; out[1] = 0; out[2] = 0; out[3] = 0
  out[4] = 0; out[5] = 2 * bt; out[6] = 0; out[7] = 0
  out[8] = 0; out[9] = 0; out[10] = 2 * nf; out[11] = 0
  out[12] = (left + right) * lr; out[13] = -(top + bottom) * bt; out[14] = (far + near) * nf; out[15] = 1
  return out
}

// Quaternions, the rotation the scene actually stores. Euler triples are a
// boundary format only - authoring (setTransform's `rotation`, the
// components' `rotation` prop) and reading back (getRotation) - so the order
// convention and gimbal lock live at that boundary and nowhere else.

/** A fresh identity rotation. */
export function quat(): Quat {
  return [0, 0, 0, 1]
}

/** Unit quaternion; a zero-length input comes back as the identity. `out`
 * may alias `q`. */
export function quatNormalize(out: Quat, q: Quat): Quat {
  let len = Math.hypot(q[0], q[1], q[2], q[3])
  if (len === 0) {
    out[0] = 0; out[1] = 0; out[2] = 0; out[3] = 1
    return out
  }
  out[0] = q[0] / len; out[1] = q[1] / len; out[2] = q[2] / len; out[3] = q[3] / len
  return out
}

/**
 * A transform update - the shape setTransform writes and transformGeometry
 * bakes. Absent keys mean "keep" (nodes) or identity (geometry).
 */
export type TransformUpdate = {
  position?: Vec3
  /** Euler radians in XYZ order (x first), Three's `Euler` default -
   * converted to a quaternion on use. */
  rotation?: Vec3
  /** The rotation itself. Normalized on use, so a hand-built or drifted
   * quaternion cannot silently scale the geometry. Passing this together
   * with `rotation` is an error, not a precedence question. */
  quaternion?: Quat
  /** A number is uniform scale. */
  scale?: Vec3 | number
}

/**
 * Resolve an update's rotation into `out`: euler converted, quaternion
 * normalized. Returns false (out untouched) when the update carries
 * neither; throws when it carries both. `caller` names the verb in the
 * error.
 */
export function updateRotation(out: Quat, update: TransformUpdate, caller: string): boolean {
  let r = update.rotation
  let q = update.quaternion
  if (r !== undefined && q !== undefined) {
    throw new Error("Pass rotation or quaternion to " + caller + ", not both")
  }
  if (r !== undefined) quatFromEuler(out, r)
  else if (q !== undefined) quatNormalize(out, q)
  else return false
  return true
}

/** Expand an update's scale (number = uniform) into `out`. */
export function updateScale(out: Vec3, scale: Vec3 | number): Vec3 {
  if (typeof scale === "number") {
    out[0] = scale; out[1] = scale; out[2] = scale
  } else {
    out[0] = scale[0]; out[1] = scale[1]; out[2] = scale[2]
  }
  return out
}

/**
 * Euler radians to a quaternion, in XYZ order: x applied first, then y,
 * then z (R = Rx * Ry * Rz on column vectors), Three's `Euler` default - a
 * triple copied from a Three scene means the same thing here.
 *
 * ONE order exists, deliberately: a per-call order argument is how the same
 * triple ends up meaning two different things in two places, and the
 * quaternion is right there for anything an order was going to express.
 */
export function quatFromEuler(out: Quat, euler: Vec3): Quat {
  let c1 = Math.cos(euler[0] / 2), s1 = Math.sin(euler[0] / 2)
  let c2 = Math.cos(euler[1] / 2), s2 = Math.sin(euler[1] / 2)
  let c3 = Math.cos(euler[2] / 2), s3 = Math.sin(euler[2] / 2)
  out[0] = s1 * c2 * c3 + c1 * s2 * s3
  out[1] = c1 * s2 * c3 - s1 * c2 * s3
  out[2] = c1 * c2 * s3 + s1 * s2 * c3
  out[3] = c1 * c2 * c3 - s1 * s2 * s3
  return out
}

/**
 * A quaternion back to Euler radians in the same XYZ order - the inverse of
 * quatFromEuler, a convenience for reading and debugging rather than a peer
 * of the quaternion: the mapping is many-to-one (a triple and that triple
 * plus a full turn agree), and at the poles (local +z straight up or down)
 * only the sum of x and z is determined, so this pins z to 0 and folds the
 * roll into x. Round-tripping the result reproduces the rotation exactly;
 * it need not reproduce the triple you started from.
 */
export function eulerFromQuat(out: Vec3, q: Quat): Vec3 {
  let x = q[0], y = q[1], z = q[2], w = q[3]
  let x2 = x + x, y2 = y + y, z2 = z + z
  let xx = x * x2, xy = x * y2, xz = x * z2
  let yy = y * y2, yz = y * z2, zz = z * z2
  let wx = w * x2, wy = w * y2, wz = w * z2
  // The same matrix entries compose() writes: m02 = sin(y) alone, and the
  // x/z pair reads off the rest unless cos(y) is 0 (the pole).
  let m00 = 1 - (yy + zz)
  let m01 = xy - wz
  let m02 = xz + wy
  // cos(y), which both remaining pairs scale with. atan2 against it beats
  // asin(m02) - Three's form - near the poles, where asin's derivative
  // blows up and a 1e-16 error in m02 becomes 1e-8 in the angle. It also
  // lets the pole branch start three orders of magnitude later: the pairs
  // stay well-conditioned until cos(y) approaches the noise floor.
  let cy = Math.hypot(m00, m01)
  out[1] = Math.atan2(m02, cy)
  if (cy > 1e-7) {
    out[0] = Math.atan2(wx - yz, 1 - (xx + yy))
    out[2] = Math.atan2(-m01, m00)
  } else {
    // Only x + z (at +y) or x - z (at -y) is determined; pin z and fold
    // the whole roll into x.
    out[0] = Math.atan2(yz + wx, 1 - (xx + zz))
    out[2] = 0
  }
  return out
}

/**
 * The rotation of `angle` RADIANS about `axis` - Three's `setFromAxisAngle`,
 * Unity's `AngleAxis` (which takes degrees; this takes radians like
 * everything else here). The axis need not be normalized - the named
 * engines all require a unit axis and silently corrupt the rotation
 * otherwise, the same precondition trap `quatFromTo` closes. A zero axis
 * yields the identity.
 */
export function quatFromAxisAngle(out: Quat, axis: Vec3, angle: number): Quat {
  let x = axis[0], y = axis[1], z = axis[2]
  let len = Math.hypot(x, y, z)
  if (len === 0) {
    out[0] = 0; out[1] = 0; out[2] = 0; out[3] = 1
    return out
  }
  let s = Math.sin(angle / 2) / len
  out[0] = x * s
  out[1] = y * s
  out[2] = z * s
  out[3] = Math.cos(angle / 2)
  return out
}

/**
 * out = a * b - the same order contract as the mat4 `multiply` above: on
 * column vectors b applies first, so `quatMultiply(q, spin, q)` composes a
 * further world-frame spin onto q while `quatMultiply(q, q, spin)` spins
 * about q's own local frame. `out` may alias `a` or `b`.
 *
 * The product of unit quaternions is unit up to float drift, so this does
 * not renormalize; an accumulator composed every frame drifts slowly, and
 * the scene's setTransform renormalizes on write anyway.
 */
export function quatMultiply(out: Quat, a: Quat, b: Quat): Quat {
  let ax = a[0], ay = a[1], az = a[2], aw = a[3]
  let bx = b[0], by = b[1], bz = b[2], bw = b[3]
  out[0] = aw * bx + ax * bw + ay * bz - az * by
  out[1] = aw * by - ax * bz + ay * bw + az * bx
  out[2] = aw * bz + ax * by - ay * bx + az * bw
  out[3] = aw * bw - ax * bx - ay * by - az * bz
  return out
}

/**
 * Spherical interpolation from `a` to `b`: constant angular velocity along
 * the shortest path (the sign of `b` is flipped when the pair straddles the
 * quaternion double cover, so it never takes the long way round). t = 0 is
 * `a`, t = 1 is `b`'s rotation; inputs must be unit and the result is unit.
 * `out` may alias `a` or `b`.
 *
 * The canonical damped follow is
 * `quatSlerp(q, q, target, 1 - Math.exp(-k * dt))` - frame-rate
 * independent, k is the tracking speed.
 */
export function quatSlerp(out: Quat, a: Quat, b: Quat, t: number): Quat {
  let ax = a[0], ay = a[1], az = a[2], aw = a[3]
  let bx = b[0], by = b[1], bz = b[2], bw = b[3]
  let cos = ax * bx + ay * by + az * bz + aw * bw
  if (cos < 0) {
    cos = -cos
    bx = -bx; by = -by; bz = -bz; bw = -bw
  }
  let wa: number
  let wb: number
  if (cos < 0.9995) {
    let theta = Math.acos(cos > 1 ? 1 : cos)
    let sin = Math.sin(theta)
    wa = Math.sin((1 - t) * theta) / sin
    wb = Math.sin(t * theta) / sin
  } else {
    // Nearly identical: sin(theta) is noise, and a straight lerp is within
    // float precision of the arc - normalized below like any other result.
    wa = 1 - t
    wb = t
  }
  out[0] = wa * ax + wb * bx
  out[1] = wa * ay + wb * by
  out[2] = wa * az + wb * bz
  out[3] = wa * aw + wb * bw
  return quatNormalize(out, out)
}

/**
 * The shortest-arc rotation taking `from` to `to`: Unity's
 * `Quaternion.FromToRotation`, glam's `Quat::from_rotation_arc`. Three
 * calls this `setFromUnitVectors`; renamed because that name states a
 * precondition instead of the operation, and this one has no such
 * precondition - neither input need be normalized.
 *
 * This is how a y-axis solid gets aimed - `quatFromTo(q, [0, 1, 0], dir)`
 * for a cylinder or cone - where lookAt's +z convention would need a
 * correction. Opposite vectors have no shortest arc (every half turn is
 * equally short); a stable perpendicular axis is picked. A zero-length
 * input yields the identity.
 */
export function quatFromTo(out: Quat, from: Vec3, to: Vec3): Quat {
  let ax = from[0], ay = from[1], az = from[2]
  let bx = to[0], by = to[1], bz = to[2]
  let la = Math.hypot(ax, ay, az)
  let lb = Math.hypot(bx, by, bz)
  if (la === 0 || lb === 0) {
    out[0] = 0; out[1] = 0; out[2] = 0; out[3] = 1
    return out
  }
  ax /= la; ay /= la; az /= la
  bx /= lb; by /= lb; bz /= lb
  let r = ax * bx + ay * by + az * bz + 1
  if (r < 1e-6) {
    // Antiparallel: the cross product vanishes, so take any perpendicular
    // axis - crossing with the smaller of from's x/z components cannot
    // vanish too, and picking off from alone keeps the choice stable.
    r = 0
    if (Math.abs(ax) > Math.abs(az)) {
      out[0] = -ay; out[1] = ax; out[2] = 0
    } else {
      out[0] = 0; out[1] = -az; out[2] = ay
    }
  } else {
    out[0] = ay * bz - az * by
    out[1] = az * bx - ax * bz
    out[2] = ax * by - ay * bx
  }
  out[3] = r
  return quatNormalize(out, out)
}

/**
 * The rotation that points the local +z axis along `forward`, with `up`
 * choosing the roll about it - the object-aiming counterpart of lookAt(),
 * which builds the camera's inverse frame. Neither input need be
 * normalized. Degenerate inputs (zero forward, up parallel to forward) fall
 * back to a stable perpendicular instead of producing NaNs.
 */
export function quatFromFrame(out: Quat, forward: Vec3, up: Vec3): Quat {
  let zx = forward[0], zy = forward[1], zz = forward[2]
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
    // up is parallel to forward: cross with a world axis that cannot be,
    // picked off z's own components so the choice is stable per direction.
    let ax = Math.abs(zx) < 0.9 ? 1 : 0
    let ay = ax === 1 ? 0 : 1
    xx = ay * zz
    xy = -ax * zz
    xz = ax * zy - ay * zx
    len = Math.hypot(xx, xy, xz)
  }
  xx /= len; xy /= len; xz /= len
  let yx = zy * xz - zz * xy
  let yy = zz * xx - zx * xz
  let yz = zx * xy - zy * xx
  // X | Y | Z are the rotation's columns, so its diagonal is xx, yy, zz.
  // Branching on the largest diagonal entry keeps the divisor away from
  // zero; the basis is orthonormal, so the result is already unit.
  let trace = xx + yy + zz
  if (trace > 0) {
    let s = 0.5 / Math.sqrt(trace + 1)
    out[0] = (yz - zy) * s
    out[1] = (zx - xz) * s
    out[2] = (xy - yx) * s
    out[3] = 0.25 / s
  } else if (xx > yy && xx > zz) {
    let s = 2 * Math.sqrt(1 + xx - yy - zz)
    out[0] = 0.25 * s
    out[1] = (yx + xy) / s
    out[2] = (zx + xz) / s
    out[3] = (yz - zy) / s
  } else if (yy > zz) {
    let s = 2 * Math.sqrt(1 + yy - xx - zz)
    out[0] = (yx + xy) / s
    out[1] = 0.25 * s
    out[2] = (zy + yz) / s
    out[3] = (zx - xz) / s
  } else {
    let s = 2 * Math.sqrt(1 + zz - xx - yy)
    out[0] = (zx + xz) / s
    out[1] = (zy + yz) / s
    out[2] = 0.25 * s
    out[3] = (xy - yx) / s
  }
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
 *
 * On the /math subpath ONLY - the package root's `lookAt` is the scene verb
 * that aims a node (the Matrix4/Object3D split Three makes under the same
 * name), the same collision rule the Vec3 helpers follow.
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

/**
 * Entry distance of a ray against a box: the smallest t >= 0 with
 * origin + t * direction inside [min, max] (0 when the origin starts
 * inside), or -1 for a miss. The direction need not be normalized - t is
 * in units of its length, which is what keeps a ray transformed into a
 * mesh's local space reporting world distances.
 */
export function rayBoxDistance(
  ox: number, oy: number, oz: number,
  dx: number, dy: number, dz: number,
  minX: number, minY: number, minZ: number,
  maxX: number, maxY: number, maxZ: number,
): number {
  let tNear = 0
  let tFar = Infinity
  // Per axis: a zero direction component never crosses the slab, so the
  // origin must already be inside it (the multiply-by-inverse shortcut
  // turns that case into NaN, hence the explicit branch).
  if (dx === 0) {
    if (ox < minX || ox > maxX) return -1
  } else {
    let inv = 1 / dx
    let t1 = (minX - ox) * inv
    let t2 = (maxX - ox) * inv
    if (t1 > t2) { let t = t1; t1 = t2; t2 = t }
    if (t1 > tNear) tNear = t1
    if (t2 < tFar) tFar = t2
  }
  if (dy === 0) {
    if (oy < minY || oy > maxY) return -1
  } else {
    let inv = 1 / dy
    let t1 = (minY - oy) * inv
    let t2 = (maxY - oy) * inv
    if (t1 > t2) { let t = t1; t1 = t2; t2 = t }
    if (t1 > tNear) tNear = t1
    if (t2 < tFar) tFar = t2
  }
  if (dz === 0) {
    if (oz < minZ || oz > maxZ) return -1
  } else {
    let inv = 1 / dz
    let t1 = (minZ - oz) * inv
    let t2 = (maxZ - oz) * inv
    if (t1 > t2) { let t = t1; t1 = t2; t2 = t }
    if (t1 > tNear) tNear = t1
    if (t2 < tFar) tFar = t2
  }
  return tFar >= tNear ? tNear : -1
}

/**
 * The far bound of slice `index` of `count` when a range near..far is
 * split for shadow cascades: `lambda` 0 slices it uniformly, 1
 * logarithmically (equal texel density per unit of view depth, which
 * starves the far slices), between the two in between. The last slice
 * ends at `far`; a near of 0 has no logarithm and slices uniformly.
 */
export function cascadeSplit(near: number, far: number, index: number, count: number, lambda: number): number {
  if (index >= count - 1) return far
  let t = (index + 1) / count
  let uniform = near + (far - near) * t
  if (!(near > 0)) return uniform
  let log = near * Math.pow(far / near, t)
  return uniform + (log - uniform) * lambda
}

/** The camera facts a frustum slice depends on: its view matrix (rows are
 * its right, up and back axes), eye, vertical fov in degrees and, for an
 * orthographic camera, the extents (fov ignored then). */
export type FrustumSpec = { view: Mat4; eye: Vec3; fov: number; ortho: { left: number; right: number; top: number; bottom: number } | null }

/**
 * The bounding sphere of the slice zn..zf of a camera's view frustum
 * (`aspect` = width / height): writes the centre to `out`, returns the
 * radius. Perspective: the centre sits on the view axis where the near
 * and far corner rings are equidistant, clamped into the slice, so it
 * is the tightest sphere on the axis; orthographic: the slice box's
 * centre and half-diagonal. A sphere rather than the slice's own corners
 * so a shadow box fitted to it keeps its size while the camera turns.
 */
export function frustumSliceSphere(out: Vec3, cam: FrustumSpec, aspect: number, zn: number, zf: number): number {
  let v = cam.view
  let fx = -v[2]
  let fy = -v[6]
  let fz = -v[10]
  let o = cam.ortho
  if (o === null) {
    // Corner distance from the axis per unit of depth.
    let k = Math.tan((cam.fov * Math.PI) / 360) * Math.hypot(1, aspect)
    let rn = zn * k
    let rf = zf * k
    let zc = zf > zn ? Math.min(zf, Math.max(zn, (zf * zf + rf * rf - zn * zn - rn * rn) / (2 * (zf - zn)))) : zn
    out[0] = cam.eye[0] + fx * zc
    out[1] = cam.eye[1] + fy * zc
    out[2] = cam.eye[2] + fz * zc
    return Math.hypot(zf - zc, rf)
  }
  let zc = 0.5 * (zn + zf)
  let cx = 0.5 * (o.left + o.right)
  let cy = 0.5 * (o.top + o.bottom)
  out[0] = cam.eye[0] + fx * zc + v[0] * cx + v[1] * cy
  out[1] = cam.eye[1] + fy * zc + v[4] * cx + v[5] * cy
  out[2] = cam.eye[2] + fz * zc + v[8] * cx + v[9] * cy
  return Math.hypot(0.5 * (o.right - o.left), 0.5 * (o.top - o.bottom), 0.5 * (zf - zn))
}

/**
 * Snap `p`'s coordinates along the first two axes of `basis` (a rotation
 * matrix whose rows are the frame's axes - `lookAt([0, 0, 0], dir, up)`
 * for a light) to multiples of `step`, leaving the third as it is;
 * writes to `out`, which may be `p`. A shadow box centred on the result
 * moves by whole texels only, so its shadows do not swim as the camera
 * creeps.
 */
export function snapToGrid(out: Vec3, p: Vec3, basis: Mat4, step: number): Vec3 {
  let x = basis[0] * p[0] + basis[4] * p[1] + basis[8] * p[2]
  let y = basis[1] * p[0] + basis[5] * p[1] + basis[9] * p[2]
  let z = basis[2] * p[0] + basis[6] * p[1] + basis[10] * p[2]
  x = Math.round(x / step) * step
  y = Math.round(y / step) * step
  out[0] = basis[0] * x + basis[1] * y + basis[2] * z
  out[1] = basis[4] * x + basis[5] * y + basis[6] * z
  out[2] = basis[8] * x + basis[9] * y + basis[10] * z
  return out
}
