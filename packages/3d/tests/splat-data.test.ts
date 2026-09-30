// The runtime-free splat entry (src/splat-data.ts, published as
// @solidrt/3d/splat) under srt test: the three capture parsers against
// hand-built inputs, the covariance bake against the closed-form answers,
// the y-up flip (covariance and SH), the SH texel packing, the importance
// order and the .srts round trip - so a bake regression fails here rather
// than as a wrong-looking cloud.
import { expect, test } from "flux:test"
import * as entry from "../src/splat-data.ts"
import { parseSplat, encodeSplat, decodeSplat, SPLAT_RECORD_BYTES, SPLAT_SH_TEXELS, SPLAT_TEXEL_BYTES } from "../src/splat-data.ts"
import type { SplatData } from "../src/splat-data.ts"

// color = 0.5 + SH_C0 * dc, the DC-band mapping (splat-data's constant).
const SH_C0 = 0.28209479177387814

// One decoded record (texel A: center + color bytes, texel B: covariance
// halves + the spare word), halfs and colors back to numbers.
function record(data: SplatData, slot: number) {
  let at = data.records.byteOffset + slot * SPLAT_RECORD_BYTES
  let dv = new DataView(data.records.buffer, at, SPLAT_RECORD_BYTES)
  return {
    center: [dv.getFloat32(0, true), dv.getFloat32(4, true), dv.getFloat32(8, true)],
    color: [...data.records.subarray(slot * SPLAT_RECORD_BYTES + 12, slot * SPLAT_RECORD_BYTES + 16)],
    cov: [16, 18, 20, 22, 24, 26].map(o => dv.getFloat16(o, true)),
    spare: dv.getUint32(28, true),
  }
}

// One splat's SH texels as halves (every half of its texels, padding
// included).
function shHalves(data: SplatData, slot: number): number[] {
  let bytes = SPLAT_SH_TEXELS[data.shDegree]! * SPLAT_TEXEL_BYTES
  let dv = new DataView(data.sh!.buffer, data.sh!.byteOffset + slot * bytes, bytes)
  return Array.from({ length: bytes / 2 }, (_, h) => dv.getFloat16(h * 2, true))
}

// A .splat record: position, linear scale, color bytes, quaternion
// (w, x, y, z) quantized v * 128 + 128.
function dotSplat(splats: { p: number[]; s: number[]; c: number[]; q: number[] }[]): Uint8Array {
  let out = new Uint8Array(splats.length * 32)
  let dv = new DataView(out.buffer)
  splats.forEach((g, i) => {
    for (let k = 0; k < 3; k++) {
      dv.setFloat32(i * 32 + k * 4, g.p[k]!, true)
      dv.setFloat32(i * 32 + 12 + k * 4, g.s[k]!, true)
    }
    for (let k = 0; k < 4; k++) {
      out[i * 32 + 24 + k] = g.c[k]!
      out[i * 32 + 28 + k] = Math.round(g.q[k]! * 128 + 128)
    }
  })
  return out
}

test("the splat entry loads without the runtime", async () => {
  let published = await import("@solidrt/3d/splat")
  expect(published.parseSplat).toBe(entry.parseSplat)
  expect(published.encodeSplat).toBe(entry.encodeSplat)
})

test("a .splat bakes flipped to y-up with the covariance closed form", () => {
  // One axis-aligned splat: identity rotation, scales (2, 3, 4) - the
  // covariance is diag(4, 9, 16) however the frame flips.
  let bytes = dotSplat([{ p: [1, 2, 3], s: [2, 3, 4], c: [10, 20, 30, 255], q: [1, 0, 0, 0] }])
  let data = parseSplat(bytes)
  expect(data.count).toBe(1)
  expect(data.shDegree).toBe(0)
  let r = record(data, 0)
  // The half-turn about x: (x, -y, -z).
  expect(r.center).toEqual([1, -2, -3])
  expect(r.cov[0]).toBeCloseTo(4)
  expect(r.cov[1]).toBeCloseTo(0)
  expect(r.cov[2]).toBeCloseTo(0)
  expect(r.cov[3]).toBeCloseTo(9)
  expect(r.cov[4]).toBeCloseTo(0)
  expect(r.cov[5]).toBeCloseTo(16)
  expect(r.color).toEqual([10, 20, 30, 255])
  expect(r.spare).toBe(0)
  expect(data.sh).toBeNull()
  expect(Array.from(data.bounds)).toEqual([1, -2, -3, 1, -2, -3])
})

test("a rotated covariance bakes as R S2 Rt and the flip negates xy and xz", () => {
  // A quarter turn about z with scales (2, 1, 1) swaps the x and y
  // variances: sigma = diag(1, 4, 1).
  let q = [Math.SQRT1_2, 0, 0, Math.SQRT1_2]
  let straight = parseSplat(dotSplat([{ p: [0, 0, 0], s: [2, 1, 1], c: [0, 0, 0, 255], q }]), { keepOrientation: true })
  let r = record(straight, 0)
  expect(r.cov[0]).toBeCloseTo(1)
  expect(r.cov[3]).toBeCloseTo(4, 1)
  expect(r.cov[5]).toBeCloseTo(1)

  // An eighth turn about z leaves a pure xy correlation, which the y-up
  // flip negates (conjugation by diag(1, -1, -1)); yz stays.
  let eighth = [Math.cos(Math.PI / 8), 0, 0, Math.sin(Math.PI / 8)]
  let kept = record(parseSplat(dotSplat([{ p: [0, 0, 0], s: [2, 1, 1], c: [0, 0, 0, 255], q: eighth }]), { keepOrientation: true }), 0)
  let flipped = record(parseSplat(dotSplat([{ p: [0, 0, 0], s: [2, 1, 1], c: [0, 0, 0, 255], q: eighth }])), 0)
  expect(kept.cov[1]!).toBeGreaterThan(0.5)
  expect(flipped.cov[1]).toBeCloseTo(-kept.cov[1]!, 2)
  expect(flipped.cov[0]).toBeCloseTo(kept.cov[0]!, 2)
  expect(flipped.cov[3]).toBeCloseTo(kept.cov[3]!, 2)
})

test("records come out importance-sorted, size times opacity descending", () => {
  let bytes = dotSplat([
    { p: [1, 0, 0], s: [1, 1, 1], c: [1, 0, 0, 128], q: [1, 0, 0, 0] },
    { p: [2, 0, 0], s: [2, 2, 2], c: [2, 0, 0, 255], q: [1, 0, 0, 0] },
    { p: [3, 0, 0], s: [1, 1, 1], c: [3, 0, 0, 255], q: [1, 0, 0, 0] },
  ])
  let data = parseSplat(bytes)
  expect([record(data, 0).color[0], record(data, 1).color[0], record(data, 2).color[0]]).toEqual([2, 3, 1])
})

// A binary little-endian .ply of float properties `fields`, one row per
// vertex.
function ply(fields: string[], rows: number[][]): Uint8Array {
  let header = "ply\nformat binary_little_endian 1.0\ncomment made by a test\nelement vertex " + rows.length + "\n" + fields.map(f => "property float " + f + "\n").join("") + "end_header\n"
  let head = new TextEncoder().encode(header)
  let body = new Float32Array(rows.flat())
  let bytes = new Uint8Array(head.byteLength + body.byteLength)
  bytes.set(head, 0)
  bytes.set(new Uint8Array(body.buffer), head.byteLength)
  return bytes
}

const PLY_FIELDS = ["x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]

test("a 3DGS .ply parses its training activations", () => {
  // Two splats with an nx/ny/nz block the parser must step over. The
  // second is bigger, so it bakes to slot 0.
  // dc such that 0.5 + SH_C0 * dc = 1 exactly; opacity 0 sigmoids to 0.5.
  let dcWhite = 0.5 / SH_C0
  let bytes = ply(PLY_FIELDS, [
    [1, 2, 3, 0, 0, 0, dcWhite, 0, -0.5 / SH_C0, 0, Math.log(1), Math.log(2), Math.log(3), 1, 0, 0, 0],
    [4, 5, 6, 0, 0, 0, 0, 0, 0, 10, Math.log(4), Math.log(4), Math.log(4), 1, 0, 0, 0],
  ])

  let data = parseSplat(bytes)
  expect(data.count).toBe(2)
  let big = record(data, 0)
  let small = record(data, 1)
  expect(big.center).toEqual([4, -5, -6])
  expect(small.center).toEqual([1, -2, -3])
  // White DC band, zeroed band, clamped-negative band; sigmoid opacity.
  expect(small.color).toEqual([255, 128, 0, 128])
  expect(small.cov[0]).toBeCloseTo(1)
  expect(small.cov[3]).toBeCloseTo(4)
  expect(small.cov[5]).toBeCloseTo(9)
  expect(big.cov[0]).toBeCloseTo(16)
})

test("a .ply's SH bands bake channel-major in, coefficient-major out, flipped with the frame", () => {
  // Degree-2 file (24 f_rest, channel-major: coefficient k of channel c at
  // f_rest_[c * 8 + k]); coefficient k of channel c = (k + 1) * 0.1 * (c + 1)
  // so every half is recognizable.
  let rest = Array.from({ length: 24 }, (_, j) => {
    let c = Math.floor(j / 8)
    let k = j % 8
    return (k + 1) * 0.1 * (c + 1)
  })
  let fields = [...PLY_FIELDS, ...Array.from({ length: 24 }, (_, j) => "f_rest_" + j)]
  let row = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, ...rest]

  // Degree 1 out of a degree-2 file: three coefficients, two texels.
  let d1 = parseSplat(ply(fields, [row]), { shDegree: 1 })
  expect(d1.shDegree).toBe(1)
  expect(d1.sh!.byteLength).toBe(2 * SPLAT_TEXEL_BYTES)
  let halves = shHalves(d1, 0)
  expect(halves.length).toBe(16)
  // The y-up flip negates the y (k = 0) and z (k = 1) bases, keeps x (k = 2).
  let expected = [-0.1, -0.2, -0.3, -0.2, -0.4, -0.6, 0.3, 0.6, 0.9]
  expected.forEach((v, h) => expect(halves[h]).toBeCloseTo(v, 2))
  for (let h = 9; h < 16; h++) expect(halves[h]).toBe(0)

  // Kept orientation: no signs change. Degree 2: eight coefficients, three texels.
  let d2 = parseSplat(ply(fields, [row]), { shDegree: 2, keepOrientation: true })
  expect(d2.sh!.byteLength).toBe(3 * SPLAT_TEXEL_BYTES)
  let kept = shHalves(d2, 0)
  for (let k = 0; k < 8; k++) for (let c = 0; c < 3; c++) expect(kept[k * 3 + c]).toBeCloseTo((k + 1) * 0.1 * (c + 1), 2)
  // Under the flip at degree 2 the xy (k = 3) and xz (k = 6) bases negate,
  // yz (k = 4), 2zz-xx-yy (k = 5) and xx-yy (k = 7) keep.
  let flipped = shHalves(parseSplat(ply(fields, [row]), { shDegree: 2 }), 0)
  let signs = [-1, -1, 1, -1, 1, 1, -1, 1]
  for (let k = 0; k < 8; k++) expect(flipped[k * 3]).toBeCloseTo(signs[k]! * (k + 1) * 0.1, 2)

  // The degree caps at the file's, and degree 0 bakes no block at all.
  expect(parseSplat(ply(fields, [row]), { shDegree: 3 }).shDegree).toBe(2)
  expect(parseSplat(ply(fields, [row])).sh).toBeNull()
  expect(() => parseSplat(ply(fields, [row]), { shDegree: 4 })).toThrow("0..3")
})

// A gunzipped spz: the 16-byte header, then positions (24-bit fixed),
// alphas, colors (DC around 0.5), scales (log sixteenths), rotations, and
// the SH block at `shDegree` (a byte per coefficient around 128,
// coefficient-major with rgb interleaved) when `sh` is given.
function spz(version: number, splats: { p: number[]; s: number[]; a: number; rot: number[]; sh?: number[] }[], shDegree = 0): Uint8Array {
  let n = splats.length
  let rotBytes = version === 2 ? 3 : 4
  let shDim = [0, 3, 8, 15][shDegree]!
  let out = new Uint8Array(16 + n * (9 + 1 + 3 + 3 + rotBytes + shDim * 3))
  let dv = new DataView(out.buffer)
  dv.setUint32(0, 0x5053474e, true)
  dv.setUint32(4, version, true)
  dv.setUint32(8, n, true)
  out[12] = shDegree
  out[13] = 12 // fractionalBits
  let positions = 16
  let alphas = positions + n * 9
  let colors = alphas + n
  let scales = colors + n * 3
  let rotations = scales + n * 3
  let shAt = rotations + n * rotBytes
  splats.forEach((g, i) => {
    for (let j = 0; j < shDim * 3; j++) out[shAt + i * shDim * 3 + j] = Math.round((g.sh?.[j] ?? 0) * 128 + 128)
    for (let k = 0; k < 3; k++) {
      let fixed = Math.round(g.p[k]! * (1 << 12))
      out[positions + (i * 3 + k) * 3] = fixed & 0xff
      out[positions + (i * 3 + k) * 3 + 1] = (fixed >> 8) & 0xff
      out[positions + (i * 3 + k) * 3 + 2] = (fixed >> 16) & 0xff
      out[colors + i * 3 + k] = 128 // dc 0 in every channel
      out[scales + i * 3 + k] = Math.round((Math.log(g.s[k]!) + 10) * 16)
    }
    out[alphas + i] = g.a
    if (version === 2) {
      // First-three: x, y, z around 127.5, w derived non-negative.
      for (let k = 0; k < 3; k++) out[rotations + i * 3 + k] = Math.round((g.rot[k + 1]! + 1) * 127.5)
    } else {
      // Smallest-three over (x, y, z, w): the largest component's index
      // in the top two bits, then sign + 9-bit magnitudes, component 0
      // packed lowest (the decoder reads k = 3 down to 0).
      let xyzw = [g.rot[1]!, g.rot[2]!, g.rot[3]!, g.rot[0]!]
      let largest = xyzw.reduce((best, v, k) => (Math.abs(v) > Math.abs(xyzw[best]!) ? k : best), 0)
      let flip = xyzw[largest]! < 0 ? -1 : 1
      let comp = largest * 2 ** 30
      let shift = 0
      for (let k = 3; k >= 0; k--) {
        if (k === largest) continue
        let v = xyzw[k]! * flip
        let mag = Math.round((Math.abs(v) / Math.SQRT1_2) * 511)
        comp += (((v < 0 ? 1 : 0) << 9) + mag) * 2 ** shift
        shift += 10
      }
      dv.setUint32(rotations + i * 4, comp >>> 0, true)
    }
  })
  return out
}

test("a .spz version 2 parses fixed-point positions unflipped", () => {
  let data = parseSplat(spz(2, [{ p: [1.5, -2.25, 3], s: [2, 3, 4], a: 200, rot: [1, 0, 0, 0] }]))
  expect(data.count).toBe(1)
  let r = record(data, 0)
  // spz is y-up already: no flip.
  expect(r.center[0]).toBeCloseTo(1.5)
  expect(r.center[1]).toBeCloseTo(-2.25)
  expect(r.center[2]).toBeCloseTo(3)
  // The u8 log scale quantizes to sixteenths: compare against the
  // quantized variance, not the input's.
  let quantized = (s: number) => Math.exp(Math.round((Math.log(s) + 10) * 16) / 16 - 10) ** 2
  expect(r.cov[0]).toBeCloseTo(quantized(2), 1)
  expect(r.cov[3]).toBeCloseTo(quantized(3), 1)
  expect(r.cov[5]).toBeCloseTo(quantized(4), 1)
  // dc 0 is mid gray; the alpha byte passes through.
  expect(r.color).toEqual([128, 128, 128, 200])
})

test("a .spz SH block reads coefficient-major and never flips", () => {
  // Degree 1: nine bytes per splat, [c0r, c0g, c0b, c1r, ...].
  let sh = [0.25, -0.5, 0.75, 0.125, 0, -0.125, 1 / 128, 2 / 128, 3 / 128]
  let data = parseSplat(spz(2, [{ p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh }], 1), { shDegree: 1 })
  expect(data.shDegree).toBe(1)
  let halves = shHalves(data, 0)
  sh.forEach((v, h) => expect(halves[h]).toBeCloseTo(v, 2))
  // spz is y-up already: the SH signs stand as stored.
  expect(halves[0]).toBeCloseTo(0.25, 2)
  // A file at degree 1 caps a degree-3 request; the default reads none.
  expect(parseSplat(spz(2, [{ p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh }], 1), { shDegree: 3 }).shDegree).toBe(1)
  expect(parseSplat(spz(2, [{ p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh }], 1)).sh).toBeNull()
})

test("a .spz version 3 smallest-three rotation matches the version 2 bake", () => {
  // An eighth turn about z: distinct diagonal AND correlation terms.
  let rot = [Math.cos(Math.PI / 8), 0, 0, Math.sin(Math.PI / 8)]
  let g = { p: [0, 0, 0], s: [2, 1, 1], a: 255, rot }
  let v2 = record(parseSplat(spz(2, [g])), 0)
  let v3 = record(parseSplat(spz(3, [g])), 0)
  for (let k = 0; k < 6; k++) expect(v3.cov[k]).toBeCloseTo(v2.cov[k]!, 1)
  expect(v2.cov[1]!).toBeGreaterThan(0.5)
})

test("the .srts container round-trips and views its bytes", () => {
  let data = parseSplat(dotSplat([
    { p: [1, 2, 3], s: [2, 3, 4], c: [10, 20, 30, 200], q: [1, 0, 0, 0] },
    { p: [-1, 0, 5], s: [1, 1, 1], c: [1, 2, 3, 40], q: [0.5, 0.5, 0.5, 0.5] },
  ]))
  let file = encodeSplat(data)
  let back = decodeSplat(file)
  expect(back.count).toBe(data.count)
  expect(back.shDegree).toBe(0)
  expect(back.sh).toBeNull()
  expect(Array.from(back.bounds)).toEqual(Array.from(data.bounds))
  expect(back.records).toEqual(data.records)
  // Zero copy: the records view the container's buffer.
  expect(back.records.buffer).toBe(file.buffer)
})

test("the .srts container carries the SH block and checks its size", () => {
  let sh = [0.25, -0.5, 0.75, 0.125, 0, -0.125, 0.5, 0.5, 0.5]
  let data = parseSplat(spz(2, [
    { p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh },
    { p: [1, 0, 0], s: [2, 2, 2], a: 255, rot: [1, 0, 0, 0], sh: sh.map(v => -v) },
  ], 1), { shDegree: 1 })
  let file = encodeSplat(data)
  let back = decodeSplat(file)
  expect(back.shDegree).toBe(1)
  expect(back.sh).toEqual(data.sh)
  expect(back.sh!.buffer).toBe(file.buffer)
  // The bigger splat bakes first, and its SH texels travel with it.
  expect(shHalves(back, 0)[0]).toBeCloseTo(-0.25, 2)
  expect(shHalves(back, 1)[0]).toBeCloseTo(0.25, 2)
  // A block of the wrong size for the degree is refused both ways.
  expect(() => encodeSplat({ ...data, sh: data.sh!.subarray(0, 16) })).toThrow("SH block")
  expect(() => encodeSplat({ ...data, shDegree: 0 })).toThrow("SH block")
})

test("the container rejects foreign and stale bytes by name", () => {
  expect(() => decodeSplat(new Uint8Array(4))).toThrow("too short")
  expect(() => decodeSplat(new TextEncoder().encode("not a splat file at all"))).toThrow("bad magic")
  let file = encodeSplat(parseSplat(dotSplat([{ p: [0, 0, 0], s: [1, 1, 1], c: [0, 0, 0, 255], q: [1, 0, 0, 0] }])))
  new DataView(file.buffer).setUint32(4, 99, true)
  expect(() => decodeSplat(file)).toThrow("version 99")
})

test("gzip bytes are named as the mistake they are", () => {
  expect(() => parseSplat(new Uint8Array([0x1f, 0x8b, 8, 0]))).toThrow("gunzip")
})
