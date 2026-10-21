// The runtime-free splat entry (src/splat-data.ts, published as
// @solidrt/3d/splat) under sol test: the three capture parsers against
// hand-built inputs, the bake against closed-form covariances through the
// reference decoder, the y-up flip (rotation and SH), the chunk
// quantization's error bounds, the k-d chunking (the runs, the in-chunk
// importance order and its ramp, the precision splits), the SH byte
// packing and the .sol3s round trip - so a bake
// regression fails here rather than as a wrong-looking cloud.
import { expect, test } from "flux:test"
import * as entry from "../src/splat-data.ts"
import { decodeRecord, decodeSh, decodeSplat, encodeSplat, parseSplat, SPLAT_CHUNK_BYTES, SPLAT_CHUNK_MAX, SPLAT_CHUNK_MIN, SPLAT_POSITION_BITS, SPLAT_RAMP_SAMPLES, SPLAT_RECORD_BYTES, SPLAT_SCALE_BITS, SPLAT_SH_LABEL_BYTES, SPLAT_SH_PALETTE_SIZE, SPLAT_SH_SUBSPACES, SPLAT_SH_TEXELS, SPLAT_STEP_VISIBLE, SPLAT_TEXEL_BYTES } from "../src/splat-data.ts"
import type { SplatData, SplatRecord } from "../src/splat-data.ts"

// color = 0.5 + SH_C0 * dc, the DC-band mapping (splat-data's constant).
const SH_C0 = 0.28209479177387814

// The 3D covariance a decoded record stands for, upper triangle
// [xx, xy, xz, yy, yz, zz]: M = R(q) diag(s), sigma = M Mt.
function covariance(r: SplatRecord): number[] {
  let [w, x, y, z] = r.rotation
  let [sx, sy, sz] = r.scale
  let m = [
    (1 - 2 * (y * y + z * z)) * sx, 2 * (x * y - w * z) * sy, 2 * (x * z + w * y) * sz,
    2 * (x * y + w * z) * sx, (1 - 2 * (x * x + z * z)) * sy, 2 * (y * z - w * x) * sz,
    2 * (x * z - w * y) * sx, 2 * (y * z + w * x) * sy, (1 - 2 * (x * x + y * y)) * sz,
  ]
  let row = (i: number, j: number) => m[i * 3]! * m[j * 3]! + m[i * 3 + 1]! * m[j * 3 + 1]! + m[i * 3 + 2]! * m[j * 3 + 2]!
  return [row(0, 0), row(0, 1), row(0, 2), row(1, 1), row(1, 2), row(2, 2)]
}

// Chunk `c`'s twelve floats.
function chunk(data: SplatData, c: number): number[] {
  let dv = new DataView(data.chunks.buffer, data.chunks.byteOffset + c * SPLAT_CHUNK_BYTES, SPLAT_CHUNK_BYTES)
  return Array.from({ length: 12 }, (_, f) => dv.getFloat32(f * 4, true))
}

// The chunk index record `slot` carries (the high half of its second
// word).
function chunkOf(data: SplatData, slot: number): number {
  return new DataView(data.records.buffer, data.records.byteOffset + slot * SPLAT_RECORD_BYTES, SPLAT_RECORD_BYTES).getUint32(4, true) >>> SPLAT_POSITION_BITS
}

// Chunk `c`'s run as [first record, past-the-end).
function run(data: SplatData, c: number): [number, number] {
  return [data.runs[c]!, c + 1 < data.runs.length ? data.runs[c + 1]! : data.count]
}

// A record's importance as the bake ranks it: the two largest scales
// times the opacity.
function importanceOf(r: SplatRecord): number {
  let s = [...r.scale].sort((a, b) => a - b)
  return s[1]! * s[2]! * (r.color[3]! / 255)
}

// Within the scale quantization's relative error (11/10/11 bits over a
// chunk's log-scale box: well under a percent).
const SCALE_QUANTIZATION_TOLERANCE = 0.01
function expectNear(got: number, want: number): void {
  expect(Math.abs(got - want)).toBeLessThanOrEqual(SCALE_QUANTIZATION_TOLERANCE * Math.max(Math.abs(want), 1e-12))
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

// The splat's index in its color bytes (r low, g, b high), so a record
// found in any order names its source.
let tagged = (i: number): number[] => [i & 255, (i >> 8) & 255, (i >> 16) & 255, 255]
let tagOf = (r: SplatRecord): number => r.color[0] + (r.color[1] << 8) + (r.color[2] << 16)

// A deterministic generator (a 32-bit LCG) for synthetic clouds.
function lcg(seed: number): () => number {
  let s = seed >>> 0
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0
    return s / 4294967296
  }
}

test("the splat entry loads without the runtime", async () => {
  let published = await import("@solidrt/3d/splat")
  expect(published.parseSplat).toBe(entry.parseSplat)
  expect(published.encodeSplat).toBe(entry.encodeSplat)
  expect(published.decodeRecord).toBe(entry.decodeRecord)
})

test("a .splat bakes flipped to y-up and decodes to the closed-form covariance", () => {
  // One axis-aligned splat: identity rotation, scales (2, 3, 4) - the
  // covariance is diag(4, 9, 16) however the frame flips. A one-splat
  // chunk has a degenerate box, so position and scale decode exactly.
  let bytes = dotSplat([{ p: [1, 2, 3], s: [2, 3, 4], c: [10, 20, 30, 255], q: [1, 0, 0, 0] }])
  let data = parseSplat(bytes)
  expect(data.count).toBe(1)
  expect(data.shDegree).toBe(0)
  expect(data.antialiased).toBe(false)
  expect(Array.from(data.runs)).toEqual([0])
  expect(data.ramp.length).toBe(SPLAT_RAMP_SAMPLES)
  expect(data.chunks.byteLength).toBe(SPLAT_CHUNK_BYTES)
  expect(chunkOf(data, 0)).toBe(0)
  let r = decodeRecord(data, 0)
  // The half-turn about x: (x, -y, -z).
  expect(r.position).toEqual([1, -2, -3])
  expect(r.scale[0]).toBeCloseTo(2)
  expect(r.scale[1]).toBeCloseTo(3)
  expect(r.scale[2]).toBeCloseTo(4)
  let cov = covariance(r)
  ;[4, 0, 0, 9, 0, 16].forEach((v, k) => expect(cov[k]).toBeCloseTo(v))
  expect(r.color).toEqual([10, 20, 30, 255])
  expect(data.sh).toBeNull()
  expect(Array.from(data.bounds)).toEqual([1, -2, -3, 1, -2, -3])
  // The parse reports its loss; nothing was lost here.
  expect(data.report!.chunks).toBe(1)
  expect(data.report!.runMin).toBe(1)
  expect(data.report!.capped).toBe(false)
  expect(data.report!.positionStep).toEqual({ median: 0, p99: 0, visibleShare: 0 })
  expect(data.report!.shClamped).toBe(0)
})

test("a rotated covariance decodes as R S2 Rt and the flip negates xy and xz", () => {
  // A quarter turn about z with scales (2, 1, 1) swaps the x and y
  // variances: sigma = diag(1, 4, 1).
  let q = [Math.SQRT1_2, 0, 0, Math.SQRT1_2]
  let straight = covariance(decodeRecord(parseSplat(dotSplat([{ p: [0, 0, 0], s: [2, 1, 1], c: [0, 0, 0, 255], q }]), { keepOrientation: true }), 0))
  expect(straight[0]).toBeCloseTo(1)
  expect(straight[3]).toBeCloseTo(4, 1)
  expect(straight[5]).toBeCloseTo(1)

  // An eighth turn about z leaves a pure xy correlation, which the y-up
  // flip negates (conjugation by diag(1, -1, -1)); yz stays.
  let eighth = [Math.cos(Math.PI / 8), 0, 0, Math.sin(Math.PI / 8)]
  let kept = covariance(decodeRecord(parseSplat(dotSplat([{ p: [0, 0, 0], s: [2, 1, 1], c: [0, 0, 0, 255], q: eighth }]), { keepOrientation: true }), 0))
  let flipped = covariance(decodeRecord(parseSplat(dotSplat([{ p: [0, 0, 0], s: [2, 1, 1], c: [0, 0, 0, 255], q: eighth }])), 0))
  expect(kept[1]!).toBeGreaterThan(0.5)
  expect(flipped[1]).toBeCloseTo(-kept[1]!, 2)
  expect(flipped[0]).toBeCloseTo(kept[0]!, 2)
  expect(flipped[3]).toBeCloseTo(kept[3]!, 2)
})

test("a chunk's run is ordered by importance, projected area times opacity descending, and its ramp samples it", () => {
  // A flat disc (two large axes) outranks a needle of the same volume:
  // the area is what a splat covers.
  let bytes = dotSplat([
    { p: [1, 0, 0], s: [1, 1, 1], c: [1, 0, 0, 128], q: [1, 0, 0, 0] },
    { p: [2, 0, 0], s: [2, 2, 2], c: [2, 0, 0, 255], q: [1, 0, 0, 0] },
    { p: [3, 0, 0], s: [1, 1, 1], c: [3, 0, 0, 255], q: [1, 0, 0, 0] },
    { p: [4, 0, 0], s: [3, 3, 0.01], c: [4, 0, 0, 255], q: [1, 0, 0, 0] },
    { p: [5, 0, 0], s: [0.1, 0.1, 9], c: [5, 0, 0, 255], q: [1, 0, 0, 0] },
  ])
  let data = parseSplat(bytes)
  // Five records are one chunk, stored by importance: the disc (9), the
  // 2-cube (4), the unit cube (1), the needle (0.9), the half-opaque
  // unit cube (0.5).
  expect([0, 1, 2, 3, 4].map(s => decodeRecord(data, s).color[0])).toEqual([4, 2, 3, 5, 1])
  expect(Array.from(data.runs)).toEqual([0])
  // The ramp: sample i is the importance at rank floor(i x 5 / 16),
  // the source's (the decoded record's scales are quantized).
  for (let i = 0; i < SPLAT_RAMP_SAMPLES; i++) expectNear(data.ramp[i]!, importanceOf(decodeRecord(data, Math.floor((i * 5) / SPLAT_RAMP_SAMPLES))))
})

test("quantization stays within a step of each chunk's box", () => {
  // 3000 splats spread over a box, random log scales over four octaves
  // and random rotations, every splat tagged with its index.
  let rand = lcg(7)
  let src: { p: number[]; s: number[]; c: number[]; q: number[] }[] = []
  for (let i = 0; i < 3000; i++) {
    let q = [rand() * 2 - 1, rand() * 2 - 1, rand() * 2 - 1, rand() * 2 - 1]
    let n = Math.hypot(...q)
    src.push({ p: [rand() * 10, rand() * 10, rand() * 10], s: [0.05, 0.1, 0.2].map(v => v * 2 ** (rand() * 4)), c: tagged(i), q: q.map(v => v / n) })
  }
  let data = parseSplat(dotSplat(src), { keepOrientation: true })
  expect(data.count).toBe(3000)
  expect(data.chunks.byteLength).toBe(Math.ceil(3000 / SPLAT_CHUNK_MAX) * SPLAT_CHUNK_BYTES)
  let seen = new Set<number>()
  for (let slot = 0; slot < data.count; slot++) {
    let r = decodeRecord(data, slot)
    let g = src[tagOf(r)]!
    seen.add(tagOf(r))
    // The record names its chunk, and the chunk's run holds the slot.
    let c = chunkOf(data, slot)
    let [lo, hi] = run(data, c)
    expect(slot).toBeGreaterThanOrEqual(lo)
    expect(slot).toBeLessThan(hi)
    let box = chunk(data, c)
    for (let k = 0; k < 3; k++) {
      // Half a unorm step of the chunk's extent on that axis, plus float
      // slack; and the decoded value lies inside the box.
      let step = (box[3 + k]! - box[k]!) / ((1 << SPLAT_POSITION_BITS) - 1)
      expect(Math.abs(r.position[k]! - g.p[k]!)).toBeLessThanOrEqual(step / 2 + 1e-5)
      expect(r.position[k]!).toBeGreaterThanOrEqual(box[k]! - 1e-5)
      expect(r.position[k]!).toBeLessThanOrEqual(box[3 + k]! + 1e-5)
      let logStep = (box[9 + k]! - box[6 + k]!) / ((1 << SPLAT_SCALE_BITS[k]!) - 1)
      expect(Math.abs(Math.log(r.scale[k]!) - Math.log(g.s[k]!))).toBeLessThanOrEqual(logStep / 2 + 1e-5)
    }
    // The input quaternion was itself quantized to the .splat's bytes (a
    // component of 1 wraps the byte, as a writer that does not clamp
    // would); compare against that, as the same rotation up to sign.
    let qIn = g.q.map(v => ((Math.round(v * 128 + 128) & 255) - 128) / 128)
    let nIn = Math.hypot(...qIn)
    let dot = r.rotation.reduce((acc, v, k) => acc + (v * qIn[k]!) / nIn, 0)
    expect(Math.abs(dot)).toBeGreaterThan(1 - 1e-5)
  }
  expect(seen.size).toBe(3000)
  let step = data.report!.positionStep
  expect(step.median).toBeGreaterThan(0)
  expect(step.p99).toBeGreaterThanOrEqual(step.median)
  expect(step.visibleShare).toBeGreaterThanOrEqual(0)
  expect(step.visibleShare).toBeLessThanOrEqual(1)
})

test("k-d chunks are compact boxes", () => {
  // Four blobs of one chunk each at the corners of a 100-unit square,
  // one unit across: every chunk must be one blob (a box under two
  // units), never a slab across two.
  let rand = lcg(3)
  let src: { p: number[]; s: number[]; c: number[]; q: number[] }[] = []
  let corners = [
    [0, 0],
    [0, 100],
    [100, 0],
    [100, 100],
  ]
  for (let i = 0; i < 4 * SPLAT_CHUNK_MAX; i++) {
    let corner = corners[i % 4]!
    src.push({ p: [corner[0]! + rand(), corner[1]! + rand(), rand()], s: [1, 1, 1], c: tagged(i), q: [1, 0, 0, 0] })
  }
  let data = parseSplat(dotSplat(src), { keepOrientation: true })
  let boxes = [0, 1, 2, 3].map(c => chunk(data, c))
  for (let box of boxes) for (let k = 0; k < 3; k++) expect(box[3 + k]! - box[k]!).toBeLessThan(2)
  // And a partial last chunk is just that: 1030 splats make five chunks,
  // the fifth of six records.
  let more = parseSplat(dotSplat([...src, ...src.slice(0, 6)]), { keepOrientation: true })
  expect(more.report!.chunks).toBe(5)
  expect(more.report!.runMin).toBe(6)
  expect(more.chunks.byteLength).toBe(5 * SPLAT_CHUNK_BYTES)
  expect(Array.from(more.runs)).toEqual([0, 256, 512, 768, 1024])
})

test("every chunk's run falls in importance and its ramp samples the run", () => {
  // Three importance levels a decade apart, shuffled in the input and
  // scattered in space: inside every chunk the records never ascend in
  // importance, and the ramp is the run's importance at each sixteenth.
  let rand = lcg(11)
  let src: { p: number[]; s: number[]; c: number[]; q: number[] }[] = []
  for (let i = 0; i < 3000; i++) {
    let scale = [10, 1, 0.1][i % 3]!
    src.push({ p: [rand() * 50, rand() * 50, rand() * 50], s: [scale, scale, scale], c: tagged(i), q: [1, 0, 0, 0] })
  }
  let data = parseSplat(dotSplat(src), { keepOrientation: true })
  expect(data.runs.length).toBe(data.report!.chunks)
  expect(data.ramp.length).toBe(data.runs.length * SPLAT_RAMP_SAMPLES)
  let seen = new Set<number>()
  for (let c = 0; c < data.runs.length; c++) {
    let [lo, hi] = run(data, c)
    expect(hi - lo).toBeLessThanOrEqual(SPLAT_CHUNK_MAX)
    let last = Infinity
    for (let slot = lo; slot < hi; slot++) {
      let r = decodeRecord(data, slot)
      seen.add(tagOf(r))
      expect(chunkOf(data, slot)).toBe(c)
      let imp = importanceOf(r)
      expect(imp).toBeLessThanOrEqual(last * (1 + 1e-5))
      last = imp
    }
    for (let i = 0; i < SPLAT_RAMP_SAMPLES; i++) {
      let sample = data.ramp[c * SPLAT_RAMP_SAMPLES + i]!
      expectNear(sample, importanceOf(decodeRecord(data, lo + Math.floor((i * (hi - lo)) / SPLAT_RAMP_SAMPLES))))
      if (i > 0) expect(sample).toBeLessThanOrEqual(data.ramp[c * SPLAT_RAMP_SAMPLES + i - 1]!)
    }
  }
  expect(seen.size).toBe(3000)
})

test("a sparse chunk splits down to SPLAT_CHUNK_MIN until its box stops moving its smallest splat", () => {
  // 256 micro-splats spread over 100 units: one chunk's box would step
  // 1.5e-3 units, fifteen times a 1e-4 splat, so the leaf splits until
  // the runs hit the floor; every record still names its chunk.
  let rand = lcg(5)
  let src: { p: number[]; s: number[]; c: number[]; q: number[] }[] = []
  for (let i = 0; i < SPLAT_CHUNK_MAX; i++) src.push({ p: [rand() * 100, rand() * 100, rand() * 100], s: [1e-4, 1e-4, 1e-4], c: tagged(i), q: [1, 0, 0, 0] })
  let data = parseSplat(dotSplat(src), { keepOrientation: true })
  expect(data.report!.chunks).toBe(SPLAT_CHUNK_MAX / SPLAT_CHUNK_MIN)
  expect(data.report!.runMin).toBe(SPLAT_CHUNK_MIN)
  expect(data.report!.capped).toBe(false)
  for (let c = 0; c < data.runs.length; c++) {
    let [lo, hi] = run(data, c)
    expect(hi - lo).toBe(SPLAT_CHUNK_MIN)
    for (let slot = lo; slot < hi; slot++) expect(chunkOf(data, slot)).toBe(c)
  }
  // The same splats at unit scale are one chunk: its step is nothing
  // against them, and no splat moves visibly.
  let coarse = parseSplat(dotSplat(src.map(g => ({ ...g, s: [1, 1, 1] }))), { keepOrientation: true })
  expect(coarse.report!.chunks).toBe(1)
  expect(coarse.report!.positionStep.visibleShare).toBe(0)
  // The split bake's step against the micro-splats is the coarse bake's
  // step (against the unit splats) times at most 1e4 / 16 chunks across:
  // every leaf halved the longest axis four times.
  expect(data.report!.positionStep.p99).toBeLessThan(coarse.report!.positionStep.p99 * 1e4)
  expect(data.report!.positionStep.visibleShare).toBeGreaterThan(0)
  expect(SPLAT_STEP_VISIBLE).toBe(0.1)
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
  // second is bigger, so it heads the draw order.
  // dc such that 0.5 + SH_C0 * dc = 1 exactly; opacity 0 sigmoids to 0.5.
  let dcWhite = 0.5 / SH_C0
  let bytes = ply(PLY_FIELDS, [
    [1, 2, 3, 0, 0, 0, dcWhite, 0, -0.5 / SH_C0, 0, Math.log(1), Math.log(2), Math.log(3), 1, 0, 0, 0],
    [4, 5, 6, 0, 0, 0, 0, 0, 0, 10, Math.log(4), Math.log(4), Math.log(4), 1, 0, 0, 0],
  ])

  let data = parseSplat(bytes)
  expect(data.count).toBe(2)
  // The two share a chunk, stored by importance: the big one first.
  expect(Array.from(data.runs)).toEqual([0])
  let big = decodeRecord(data, 0)
  let small = decodeRecord(data, 1)
  // Two splats share a chunk: positions quantize over the chunk's box
  // (within a step, here under 1/65535 of 3 units).
  big.position.forEach((v, k) => expect(v).toBeCloseTo([4, -5, -6][k]!, 4))
  small.position.forEach((v, k) => expect(v).toBeCloseTo([1, -2, -3][k]!, 4))
  // White DC band, zeroed band, clamped-negative band; sigmoid opacity.
  expect(small.color).toEqual([255, 128, 0, 128])
  expect(small.scale[0]).toBeCloseTo(1, 2)
  expect(small.scale[1]).toBeCloseTo(2, 2)
  expect(small.scale[2]).toBeCloseTo(3, 2)
  expect(big.scale[0]).toBeCloseTo(4, 2)
})

test("a .ply's SH bands bake channel-major in, coefficient-major out, flipped with the frame", () => {
  // Degree-2 file (24 f_rest, channel-major: coefficient k of channel c at
  // f_rest_[c * 8 + k]); coefficient k of channel c = (k + 1) * 0.01 * (c + 1)
  // so every byte is recognizable within the byte's [-1, 1) range.
  let rest = Array.from({ length: 24 }, (_, j) => {
    let c = Math.floor(j / 8)
    let k = j % 8
    return (k + 1) * 0.01 * (c + 1)
  })
  let fields = [...PLY_FIELDS, ...Array.from({ length: 24 }, (_, j) => "f_rest_" + j)]
  let row = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, ...rest]

  // Degree 1 out of a degree-2 file: three coefficients, one texel.
  let d1 = parseSplat(ply(fields, [row]), { shDegree: 1, shEncoding: "u8" })
  expect(d1.shDegree).toBe(1)
  expect(d1.shEncoding).toBe("u8")
  expect(d1.sh!.byteLength).toBe(1 * SPLAT_TEXEL_BYTES)
  let sh = decodeSh(d1, 0)
  expect(sh.length).toBe(9)
  // The y-up flip negates the y (k = 0) and z (k = 1) bases, keeps x (k = 2).
  let expected = [-0.01, -0.02, -0.03, -0.02, -0.04, -0.06, 0.03, 0.06, 0.09]
  expected.forEach((v, h) => expect(sh[h]).toBeCloseTo(v, 2))
  for (let b = 9; b < 16; b++) expect(d1.sh![b]).toBe(0)

  // Kept orientation: no signs change. Degree 2: eight coefficients, two texels.
  let d2 = parseSplat(ply(fields, [row]), { shDegree: 2, shEncoding: "u8", keepOrientation: true })
  expect(d2.sh!.byteLength).toBe(2 * SPLAT_TEXEL_BYTES)
  let kept = decodeSh(d2, 0)
  for (let k = 0; k < 8; k++) for (let c = 0; c < 3; c++) expect(kept[k * 3 + c]).toBeCloseTo((k + 1) * 0.01 * (c + 1), 2)
  // Under the flip at degree 2 the xy (k = 3) and xz (k = 6) bases negate,
  // yz (k = 4), 2zz-xx-yy (k = 5) and xx-yy (k = 7) keep.
  let flipped = decodeSh(parseSplat(ply(fields, [row]), { shDegree: 2 }), 0)
  let signs = [-1, -1, 1, -1, 1, 1, -1, 1]
  for (let k = 0; k < 8; k++) expect(flipped[k * 3]).toBeCloseTo(signs[k]! * (k + 1) * 0.01, 2)

  // The degree caps at the file's, and degree 0 bakes no block at all.
  expect(parseSplat(ply(fields, [row]), { shDegree: 3 }).shDegree).toBe(2)
  expect(parseSplat(ply(fields, [row])).sh).toBeNull()
  expect(() => parseSplat(ply(fields, [row]), { shDegree: 4 })).toThrow("0..3")

  // A coefficient past the byte's range clamps, and the report counts it.
  let loud = [...row]
  loud[17] = 3
  loud[18] = -3
  let clamped = parseSplat(ply(fields, [loud]), { shDegree: 1, shEncoding: "u8", keepOrientation: true })
  expect(clamped.report!.shClamped).toBe(2)
  expect(decodeSh(clamped, 0)[0]).toBeCloseTo(127 / 128, 3)
  expect(decodeSh(clamped, 0)[3]).toBe(-1)
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
  let r = decodeRecord(data, 0)
  // spz is y-up already: no flip.
  expect(r.position[0]).toBeCloseTo(1.5)
  expect(r.position[1]).toBeCloseTo(-2.25)
  expect(r.position[2]).toBeCloseTo(3)
  // The u8 log scale quantizes to sixteenths: compare against the
  // quantized scale, not the input's.
  let quantized = (s: number) => Math.exp(Math.round((Math.log(s) + 10) * 16) / 16 - 10)
  expect(r.scale[0]).toBeCloseTo(quantized(2), 2)
  expect(r.scale[1]).toBeCloseTo(quantized(3), 2)
  expect(r.scale[2]).toBeCloseTo(quantized(4), 2)
  // dc 0 is mid gray; the alpha byte passes through.
  expect(r.color).toEqual([128, 128, 128, 200])
})

test("a .spz SH block reads coefficient-major and never flips", () => {
  // Degree 1: nine bytes per splat, [c0r, c0g, c0b, c1r, ...].
  let sh = [0.25, -0.5, 0.75, 0.125, 0, -0.125, 1 / 128, 2 / 128, 3 / 128]
  let data = parseSplat(spz(2, [{ p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh }], 1), { shDegree: 1, shEncoding: "u8" })
  expect(data.shDegree).toBe(1)
  // Bytes in, bytes out: an spz's SH round-trips exactly under u8.
  let decoded = decodeSh(data, 0)
  sh.forEach((v, h) => expect(decoded[h]).toBe(v))
  // A file at degree 1 caps a degree-3 request; the default reads none.
  expect(parseSplat(spz(2, [{ p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh }], 1), { shDegree: 3 }).shDegree).toBe(1)
  expect(parseSplat(spz(2, [{ p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh }], 1)).sh).toBeNull()
})

test("a .spz version 3 smallest-three rotation matches the version 2 bake", () => {
  // An eighth turn about z: distinct diagonal AND correlation terms.
  let rot = [Math.cos(Math.PI / 8), 0, 0, Math.sin(Math.PI / 8)]
  let g = { p: [0, 0, 0], s: [2, 1, 1], a: 255, rot }
  let v2 = covariance(decodeRecord(parseSplat(spz(2, [g])), 0))
  let v3 = covariance(decodeRecord(parseSplat(spz(3, [g])), 0))
  for (let k = 0; k < 6; k++) expect(v3[k]).toBeCloseTo(v2[k]!, 1)
  expect(v2[1]!).toBeGreaterThan(0.5)
})

test("the .sol3s container round-trips and views its bytes", () => {
  let data = parseSplat(
    dotSplat([
      { p: [1, 2, 3], s: [2, 3, 4], c: [10, 20, 30, 200], q: [1, 0, 0, 0] },
      { p: [-1, 0, 5], s: [1, 1, 1], c: [1, 2, 3, 40], q: [0.5, 0.5, 0.5, 0.5] },
    ]),
    { antialiased: true },
  )
  let file = encodeSplat(data)
  let back = decodeSplat(file)
  expect(back.count).toBe(data.count)
  expect(back.shDegree).toBe(0)
  expect(back.sh).toBeNull()
  expect(back.antialiased).toBe(true)
  expect(back.runs).toEqual(data.runs)
  expect(back.ramp).toEqual(data.ramp)
  expect(Array.from(back.bounds)).toEqual(Array.from(data.bounds))
  expect(back.records).toEqual(data.records)
  expect(back.colors).toEqual(data.colors)
  expect(back.chunks).toEqual(data.chunks)
  // Zero copy: the blocks view the container's buffer; the report is the
  // bake's, not the file's.
  expect(back.records.buffer).toBe(file.buffer)
  expect(back.chunks.buffer).toBe(file.buffer)
  expect(back.runs.buffer).toBe(file.buffer)
  expect(back.ramp.buffer).toBe(file.buffer)
  expect(back.report).toBeUndefined()
  expect(decodeRecord(back, 1)).toEqual(decodeRecord(data, 1))
  // The empty cloud is a valid file.
  let empty = decodeSplat(encodeSplat(parseSplat(new Uint8Array(0), { format: "splat" })))
  expect(empty.count).toBe(0)
  expect(empty.runs.length).toBe(0)
  expect(empty.ramp.length).toBe(0)
  expect(empty.chunks.byteLength).toBe(0)
})

test("the .sol3s container carries the SH block and checks its size", () => {
  let sh = [0.25, -0.5, 0.75, 0.125, 0, -0.125, 0.5, 0.5, 0.5]
  let data = parseSplat(
    spz(2, [
      { p: [0, 0, 0], s: [1, 1, 1], a: 255, rot: [1, 0, 0, 0], sh },
      { p: [1, 0, 0], s: [2, 2, 2], a: 255, rot: [1, 0, 0, 0], sh: sh.map(v => -v) },
    ], 1),
    { shDegree: 1, shEncoding: "u8" },
  )
  let file = encodeSplat(data)
  let back = decodeSplat(file)
  expect(back.shDegree).toBe(1)
  expect(back.shEncoding).toBe("u8")
  expect(back.shPalette).toBeNull()
  expect(back.sh).toEqual(data.sh)
  expect(back.sh!.buffer).toBe(file.buffer)
  // The bigger splat heads its chunk's run, and its SH texel sits at
  // its record.
  expect(Array.from(back.runs)).toEqual([0])
  expect(decodeSh(back, 0)[0]).toBe(-0.25)
  expect(decodeSh(back, 1)[0]).toBe(0.25)
  expect(back.sh!.byteLength).toBe(2 * SPLAT_SH_TEXELS[1]! * SPLAT_TEXEL_BYTES)
  // A block of the wrong size for the degree is refused both ways, and
  // so is a chunk table of the wrong size.
  expect(() => encodeSplat({ ...data, sh: data.sh!.subarray(0, 8) })).toThrow("SH block")
  expect(() => encodeSplat({ ...data, shDegree: 0 })).toThrow("SH block")
  expect(() => encodeSplat({ ...data, chunks: data.chunks.subarray(0, 8) })).toThrow("chunk table")
  expect(() => encodeSplat({ ...data, colors: data.colors.subarray(0, 4) })).toThrow("color block")
  expect(() => encodeSplat({ ...data, runs: data.runs.subarray(0, 0) })).toThrow("run block")
  expect(() => encodeSplat({ ...data, ramp: data.ramp.subarray(0, 4) })).toThrow("ramp block")
})

// A degree-3 .ply (45 f_rest, channel-major) of `rows` splats at the
// origin whose SH coefficients come from `vector(i)` (45 values,
// coefficient-major rgb interleaved as decodeSh returns them).
function shPly(rows: number, vector: (i: number) => number[]): Uint8Array {
  let fields = [...PLY_FIELDS, ...Array.from({ length: 45 }, (_, j) => "f_rest_" + j)]
  return ply(
    fields,
    Array.from({ length: rows }, (_, i) => {
      let v = vector(i)
      let rest = Array.from({ length: 45 }, (_, j) => v[(j % 15) * 3 + Math.floor(j / 15)]!)
      return [i, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, ...rest]
    }),
  )
}

test("a pq bake palettes the SH bands per subspace, exactly when the codes suffice, and round-trips", () => {
  // Six distinct SH vectors over 300 splats: every subspace has six
  // distinct values for 256 codes, so the codebook holds them exactly
  // and the only error is the byte domain's, already applied to the u8
  // reference the vectors are measured against (the luma/chroma planes
  // are differences of bytes, exact on the lattice).
  const PATTERNS = 6
  let pattern = (p: number) => Array.from({ length: 45 }, (_, j) => Math.sin(p * 1.7 + j * 0.31) * 0.6)
  let bytes = shPly(300, i => pattern(i % PATTERNS))
  let pq = parseSplat(bytes, { shDegree: 3, keepOrientation: true })
  let u8 = parseSplat(bytes, { shDegree: 3, keepOrientation: true, shEncoding: "u8" })
  expect(pq.shEncoding).toBe("pq")
  expect(pq.sh!.byteLength).toBe(300 * SPLAT_SH_LABEL_BYTES[3]!)
  expect(pq.shPalette!.byteLength).toBe(SPLAT_SH_PALETTE_SIZE * SPLAT_SH_LABEL_BYTES[3]! * SPLAT_TEXEL_BYTES)
  for (let slot = 0; slot < 300; slot++) expect(Array.from(decodeSh(pq, slot))).toEqual(Array.from(decodeSh(u8, slot)))
  // At most PATTERNS labels in use per subspace.
  let subspaces = SPLAT_SH_LABEL_BYTES[3]!
  for (let j = 0; j < subspaces; j++) {
    let used = new Set<number>()
    for (let slot = 0; slot < 300; slot++) used.add(pq.sh![slot * subspaces + j]!)
    expect(used.size).toBe(PATTERNS)
  }
  expect(pq.report!.shPalette!.rms).toEqual(new Array(subspaces).fill(0))
  expect(pq.report!.shPalette!.p99).toEqual(new Array(subspaces).fill(0))
  expect(pq.report!.shPalette!.clamped).toBe(0)
  // Each plane's subspaces tile the 15 coefficients, and degree d's
  // prefix tiles its bands.
  for (let plane of ["luma", "chroma"] as const) {
    let runs = SPLAT_SH_SUBSPACES.filter(s => s.plane === plane).sort((a, b) => a.first - b.first)
    expect(runs[0]!.first).toBe(0)
    for (let i = 1; i < runs.length; i++) expect(runs[i]!.first).toBe(runs[i - 1]!.first + runs[i - 1]!.count)
    expect(runs[runs.length - 1]!.first + runs[runs.length - 1]!.count).toBe(15)
    for (let degree = 1; degree <= 3; degree++) {
      let within = SPLAT_SH_SUBSPACES.slice(0, SPLAT_SH_LABEL_BYTES[degree]!).filter(s => s.plane === plane)
      expect(within.reduce((n, s) => n + s.count, 0)).toBe([0, 3, 8, 15][degree])
    }
  }

  // The container carries labels and palette, 4-aligned, and views them.
  let file = encodeSplat(pq)
  let back = decodeSplat(file)
  expect(back.shEncoding).toBe("pq")
  expect(back.sh).toEqual(pq.sh)
  expect(back.shPalette).toEqual(pq.shPalette)
  expect(back.shPalette!.buffer).toBe(file.buffer)
  expect(back.shPalette!.byteOffset % 4).toBe(0)
  for (let slot = 0; slot < 300; slot += 37) expect(Array.from(decodeSh(back, slot))).toEqual(Array.from(decodeSh(pq, slot)))
  // A degree-1 cloud of three splats has six label bytes: the palette
  // block behind it still lands 4-aligned.
  let three = decodeSplat(encodeSplat(parseSplat(shPly(3, i => pattern(i)), { shDegree: 1, keepOrientation: true })))
  expect(three.sh!.byteLength).toBe(3 * SPLAT_SH_LABEL_BYTES[1]!)
  expect(three.shPalette!.byteOffset % 4).toBe(0)
  expect(three.shPalette!.byteLength).toBe(SPLAT_SH_PALETTE_SIZE * SPLAT_SH_LABEL_BYTES[1]! * SPLAT_TEXEL_BYTES)
  expect(Array.from(decodeSh(three, 2)).slice(0, 9).map(v => Math.round(v * 128))).toEqual(pattern(2).slice(0, 9).map(v => Math.round(v * 128)))
  // The shapes are checked both ways, and the encoding is validated.
  expect(() => encodeSplat({ ...pq, shPalette: null })).toThrow("SH palette")
  expect(() => encodeSplat({ ...pq, sh: pq.sh!.subarray(0, 8) })).toThrow("SH block")
  expect(() => encodeSplat({ ...pq, shEncoding: "u8" })).toThrow("SH block")
  expect(() => parseSplat(bytes, { shDegree: 1, shEncoding: "u4" as "u8" })).toThrow("shEncoding")
  // Degree 0 carries no SH whatever the encoding asked.
  let none = parseSplat(bytes, { shEncoding: "pq" })
  expect(none.sh).toBeNull()
  expect(none.shPalette).toBeNull()
  expect(none.shEncoding).toBe("u8")
})

test("a pq bake of more vectors than codes clusters them, deterministically, better than no palette", () => {
  // 1000 degree-1 vectors in a ball: no exact codebook exists. (Chroma
  // differences of two uniform [-0.5, 0.5] values stay inside the byte
  // range, so nothing clamps.) The count is the test host's budget: the
  // bake is k-means in JS and runs twice here, and QuickJS does 3000
  // vectors in over the 30 s an engine gets.
  const VECTORS = 1000
  let rand = lcg(11)
  let vectors = Array.from({ length: VECTORS }, () => Array.from({ length: 45 }, () => (rand() * 2 - 1) * 0.5))
  let bytes = shPly(VECTORS, i => vectors[i]!)
  let a = parseSplat(bytes, { shDegree: 1, keepOrientation: true })
  let b = parseSplat(bytes, { shDegree: 1, keepOrientation: true })
  expect(a.sh).toEqual(b.sh)
  expect(a.shPalette).toEqual(b.shPalette)
  // The clustering error is well under the data's own spread (the error
  // of a one-entry palette at the mean): a uniform [-0.5, 0.5] luma
  // coordinate has rms 0.289 = 37 byte steps, a chroma difference of two
  // of them rms 0.408 = 52; 256 codes over 3 or 6 dims land far below.
  let report = a.report!.shPalette!
  expect(report.rms.length).toBe(SPLAT_SH_LABEL_BYTES[1])
  expect(report.clamped).toBe(0)
  for (let j = 0; j < report.rms.length; j++) {
    let spread = SPLAT_SH_SUBSPACES[j]!.plane === "luma" ? 37 : 52
    expect(report.rms[j]!).toBeLessThan(spread / 2)
    expect(report.rms[j]!).toBeGreaterThan(0)
    expect(report.p99[j]!).toBeGreaterThanOrEqual(report.rms[j]!)
  }
  // Every splat decodes to its own nearest code: no decoded vector is
  // farther from its source than the farthest code the report saw.
  let worst = 0
  for (let slot = 0; slot < VECTORS; slot++) {
    let decoded = decodeSh(a, slot)
    let source = Math.round(decodeRecord(a, slot).position[0])
    let v = vectors[source]!
    for (let h = 0; h < 9; h++) worst = Math.max(worst, Math.abs(decoded[h]! - v[h]!) * 128)
  }
  expect(worst).toBeLessThan(128)
  expect(worst).toBeGreaterThanOrEqual(Math.max(...report.p99))
})

test("a pq bake clamps a chroma difference past the byte range and counts it", () => {
  // Red at the top of the range, green and blue at the bottom: r - g = 2,
  // outside [-1, 1), so the stored difference saturates; b - g = 0 does
  // not.
  let v = new Array(45).fill(0)
  v[0] = 0.99
  v[1] = -1
  v[2] = -1
  let data = parseSplat(shPly(2, () => v), { shDegree: 1, keepOrientation: true })
  expect(data.report!.shPalette!.clamped).toBe(2)
  let sh = decodeSh(data, 0)
  expect(sh[1]).toBe(-1)
  expect(sh[2]).toBe(-1)
  expect(sh[0]).toBeCloseTo(-1 + 127 / 128, 6)
})

test("the container rejects foreign and stale bytes by name", () => {
  expect(() => decodeSplat(new Uint8Array(4))).toThrow("too short")
  expect(() => decodeSplat(new TextEncoder().encode("not a splat file at all"))).toThrow("bad magic")
  let file = encodeSplat(parseSplat(dotSplat([{ p: [0, 0, 0], s: [1, 1, 1], c: [0, 0, 0, 255], q: [1, 0, 0, 0] }])))
  new DataView(file.buffer).setUint32(4, 2, true)
  expect(() => decodeSplat(file)).toThrow("version 2")
  expect(() => decodeRecord(decodeSplat(encodeSplat(parseSplat(new Uint8Array(0), { format: "splat" }))), 0)).toThrow("outside")
})

test("gzip bytes are named as the mistake they are", () => {
  expect(() => parseSplat(new Uint8Array([0x1f, 0x8b, 8, 0]))).toThrow("gunzip")
})
