// Pure clip sampling - the mixer's core, split out so it runs with no
// scene or engine (checks under flux, custom drivers, a future bake-side
// resample). Everything here is plain array math over ModelChannel data.

import { quatNormalize, quatSlerp } from "./math.ts"
import type { Quat } from "./math.ts"
import type { ModelChannel } from "./gltf.ts"

/**
 * Sample one channel at `time` (seconds, clamped to the key range) into
 * `out` - 3 floats for position/scale, 4 for rotation. Pure array math:
 * step holds the earlier key, linear lerps (rotation slerps the shortest
 * path), cubic evaluates the glTF CUBICSPLINE Hermite (rotation
 * renormalized, per spec).
 */
export function sampleChannel(channel: ModelChannel, time: number, out: number[]): void {
  let times = channel.times
  let values = channel.values
  let keys = times.length
  let elements = channel.path === "rotation" ? 4 : 3
  let stride = channel.interpolation === "cubic" ? elements * 3 : elements
  // The value element offset within a key: cubic keys are [in, value, out].
  let mid = channel.interpolation === "cubic" ? elements : 0
  if (keys === 0) return
  if (time <= times[0]! || keys === 1) {
    for (let e = 0; e < elements; e++) out[e] = values[mid + e]!
    return
  }
  if (time >= times[keys - 1]!) {
    let at = (keys - 1) * stride + mid
    for (let e = 0; e < elements; e++) out[e] = values[at + e]!
    return
  }
  // The key pair around `time`: binary search for the last key at or
  // before it.
  let lo = 0
  let hi = keys - 1
  while (hi - lo > 1) {
    let m = (lo + hi) >> 1
    if (times[m]! <= time) lo = m
    else hi = m
  }
  let t0 = times[lo]!
  let t1 = times[hi]!
  let span = t1 - t0
  let s = span > 0 ? (time - t0) / span : 0
  let a = lo * stride + mid
  let b = hi * stride + mid
  switch (channel.interpolation) {
    case "step":
      for (let e = 0; e < elements; e++) out[e] = values[a + e]!
      return
    case "linear":
      if (channel.path === "rotation") {
        readQuat(SAMPLE_A, values, a)
        readQuat(SAMPLE_B, values, b)
        quatSlerp(SAMPLE_A, SAMPLE_A, SAMPLE_B, s)
        for (let e = 0; e < 4; e++) out[e] = SAMPLE_A[e]!
      } else {
        for (let e = 0; e < elements; e++) out[e] = values[a + e]! + (values[b + e]! - values[a + e]!) * s
      }
      return
    case "cubic": {
      // glTF's Hermite: p(s) = h00 v0 + h10 d b0 + h01 v1 + h11 d a1,
      // where b0 is key lo's OUT-tangent and a1 key hi's IN-tangent
      // (per-second, so they scale by the span d).
      let s2 = s * s
      let s3 = s2 * s
      let h00 = 2 * s3 - 3 * s2 + 1
      let h10 = s3 - 2 * s2 + s
      let h01 = -2 * s3 + 3 * s2
      let h11 = s3 - s2
      let outTan = lo * stride + elements * 2
      let inTan = hi * stride
      for (let e = 0; e < elements; e++) {
        out[e] = h00 * values[a + e]! + h10 * span * values[outTan + e]! + h01 * values[b + e]! + h11 * span * values[inTan + e]!
      }
      if (channel.path === "rotation") {
        readQuat(SAMPLE_A, out, 0)
        quatNormalize(SAMPLE_A, SAMPLE_A)
        for (let e = 0; e < 4; e++) out[e] = SAMPLE_A[e]!
      }
      return
    }
  }
}

const SAMPLE_A: Quat = [0, 0, 0, 1]
const SAMPLE_B: Quat = [0, 0, 0, 1]

function readQuat(out: Quat, values: ArrayLike<number>, at: number): void {
  out[0] = values[at]!
  out[1] = values[at + 1]!
  out[2] = values[at + 2]!
  out[3] = values[at + 3]!
}
