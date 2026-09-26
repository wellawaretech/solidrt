// The runtime-free splat entry (src/splat-data.ts, published as
// @solidrt/3d/splat) under bun test: the three capture parsers against
// hand-built inputs, the covariance bake against the closed-form answers,
// the y-up flip, the importance order and the .srts round trip - so a
// bake regression fails here rather than as a wrong-looking cloud.
import { expect, test } from "bun:test"
import * as entry from "../src/splat-data.ts"
import { parseSplat, encodeSplat, decodeSplat, SPLAT_RECORD_BYTES } from "../src/splat-data.ts"
import type { SplatData } from "../src/splat-data.ts"

// color = 0.5 + SH_C0 * dc, the DC-band mapping (splat-data's constant).
const SH_C0 = 0.28209479177387814

// One decoded record, halfs and colors back to numbers.
function record(data: SplatData, slot: number) {
  let at = data.records.byteOffset + slot * SPLAT_RECORD_BYTES
  let dv = new DataView(data.records.buffer, at, SPLAT_RECORD_BYTES)
  return {
    center: [dv.getFloat32(0, true), dv.getFloat32(4, true), dv.getFloat32(8, true)],
    cov: [12, 14, 16, 18, 20, 22].map(o => dv.getFloat16(o, true)),
    color: [...data.records.subarray(slot * SPLAT_RECORD_BYTES + 24, slot * SPLAT_RECORD_BYTES + 28)],
  }
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

test("a 3DGS .ply parses its training activations", () => {
  // Two splats with an nx/ny/nz block the parser must step over. The
  // second is bigger, so it bakes to slot 0.
  let fields = ["x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]
  let header = "ply\nformat binary_little_endian 1.0\ncomment made by a test\nelement vertex 2\n" + fields.map(f => "property float " + f + "\n").join("") + "end_header\n"
  let head = new TextEncoder().encode(header)
  // dc such that 0.5 + SH_C0 * dc = 1 exactly; opacity 0 sigmoids to 0.5.
  let dcWhite = 0.5 / SH_C0
  let rows = [
    [1, 2, 3, 0, 0, 0, dcWhite, 0, -0.5 / SH_C0, 0, Math.log(1), Math.log(2), Math.log(3), 1, 0, 0, 0],
    [4, 5, 6, 0, 0, 0, 0, 0, 0, 10, Math.log(4), Math.log(4), Math.log(4), 1, 0, 0, 0],
  ]
  let body = new Float32Array(rows.flat())
  let bytes = new Uint8Array(head.byteLength + body.byteLength)
  bytes.set(head, 0)
  bytes.set(new Uint8Array(body.buffer), head.byteLength)

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

// A gunzipped spz: the 16-byte header, then positions (24-bit fixed),
// alphas, colors (DC around 0.5), scales (log sixteenths), rotations.
function spz(version: number, splats: { p: number[]; s: number[]; a: number; rot: number[] }[]): Uint8Array {
  let n = splats.length
  let rotBytes = version === 2 ? 3 : 4
  let out = new Uint8Array(16 + n * (9 + 1 + 3 + 3 + rotBytes))
  let dv = new DataView(out.buffer)
  dv.setUint32(0, 0x5053474e, true)
  dv.setUint32(4, version, true)
  dv.setUint32(8, n, true)
  out[12] = 0 // shDegree
  out[13] = 12 // fractionalBits
  let positions = 16
  let alphas = positions + n * 9
  let colors = alphas + n
  let scales = colors + n * 3
  let rotations = scales + n * 3
  splats.forEach((g, i) => {
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
  expect(back.shDegree).toBe(data.shDegree)
  expect(Array.from(back.bounds)).toEqual(Array.from(data.bounds))
  expect(back.records).toEqual(data.records)
  // Zero copy: the records view the container's buffer.
  expect(back.records.buffer).toBe(file.buffer)
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
