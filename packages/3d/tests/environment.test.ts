// The environment bake's pure pieces (environment-bake.ts): the RGBE
// decoder against a synthetic encoder, the cube direction table and its
// inverse, the panorama-to-cube and GGX prefilter on maps whose answer is
// known, and the .sol3e round trip. `sol test packages/3d`.
import { expect, test } from "flux:test"
import {
  cubeDirection,
  cubeLookup,
  decodeEnvironment,
  decodeHdr,
  encodeEnvironment,
  ENV_ROUGH_FACE,
  levelRoughness,
  mipLevels,
  panoramaToCube,
  prefilterCube,
} from "../src/environment-bake.ts"
import type { CubeFaces, Panorama } from "../src/environment-bake.ts"

// A float to RGBE (shared exponent, Ward's rule: the largest channel's
// mantissa in 128..255 at the exponent frexp gives it).
function rgbe(r: number, g: number, b: number): [number, number, number, number] {
  let m = Math.max(r, g, b)
  if (m < 1e-32) return [0, 0, 0, 0]
  let e = Math.floor(Math.log2(m)) + 1
  let scale = 256 / Math.pow(2, e)
  return [Math.floor(r * scale), Math.floor(g * scale), Math.floor(b * scale), e + 128]
}

// Encode a panorama as .hdr, flat or with new-style run-length scanlines
// (runs of equal bytes as repeats, the rest as literal runs).
function encodeHdr(p: Panorama, rle: boolean): Uint8Array {
  let out: number[] = []
  let text = (s: string) => {
    for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i))
  }
  text("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y " + p.height + " +X " + p.width + "\n")
  for (let y = 0; y < p.height; y++) {
    let row: number[][] = []
    for (let x = 0; x < p.width; x++) {
      let i = (y * p.width + x) * 3
      row.push(rgbe(p.data[i]!, p.data[i + 1]!, p.data[i + 2]!))
    }
    if (!rle) {
      for (let px of row) out.push(...px)
      continue
    }
    out.push(2, 2, (p.width >> 8) & 0xff, p.width & 0xff)
    for (let ch = 0; ch < 4; ch++) {
      let x = 0
      while (x < p.width) {
        let v = row[x]![ch]!
        let run = 1
        while (x + run < p.width && row[x + run]![ch] === v && run < 127) run++
        if (run >= 3) {
          out.push(128 + run, v)
          x += run
        } else {
          let lit = 0
          while (x + lit < p.width && lit < 128 && (x + lit + 2 >= p.width || row[x + lit]![ch] !== row[x + lit + 1]![ch] || row[x + lit]![ch] !== row[x + lit + 2]![ch])) lit++
          if (lit === 0) lit = 1
          out.push(lit)
          for (let i = 0; i < lit; i++) out.push(row[x + i]![ch]!)
          x += lit
        }
      }
    }
  }
  return Uint8Array.from(out)
}

function panorama(width: number, height: number, f: (x: number, y: number) => [number, number, number]): Panorama {
  let data = new Float32Array(width * height * 3)
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      let [r, g, b] = f(x, y)
      data[(y * width + x) * 3] = r
      data[(y * width + x) * 3 + 1] = g
      data[(y * width + x) * 3 + 2] = b
    }
  }
  return { width, height, data }
}

let constant = (v: number) => panorama(32, 16, () => [v, v, v])
// A gradient with a bright sun: HDR values, distinct per texel.
let scene = panorama(64, 32, (x, y) => [x / 64 + (x === 40 && y === 8 ? 50 : 0), y / 32, 0.25 + ((x * 7 + y * 3) % 11) / 20])

test("decodeHdr: flat scanlines round trip within the RGBE mantissa", () => {
  let p = decodeHdr(encodeHdr(scene, false))
  expect(p.width).toBe(64)
  expect(p.height).toBe(32)
  // RGBE shares one exponent per texel, so a channel's step is the
  // texel's largest channel over 128 (plus the 256/255 decode scale).
  for (let i = 0; i < p.data.length; i++) {
    let t = i - (i % 3)
    let m = Math.max(scene.data[t]!, scene.data[t + 1]!, scene.data[t + 2]!)
    expect(Math.abs(p.data[i]! - scene.data[i]!)).toBeLessThanOrEqual(m * 0.02)
  }
})
test("decodeHdr: run-length scanlines decode identically to flat", () => {
  let flat = decodeHdr(encodeHdr(scene, false))
  let rle = decodeHdr(encodeHdr(scene, true))
  expect(rle.data).toEqual(flat.data)
})
test("decodeHdr: rejects a non-hdr file, another orientation and a truncated one", () => {
  expect(() => decodeHdr(Uint8Array.from([0x89, 0x50, 0x4e, 0x47]))).toThrow("no #? signature")
  let bytes = encodeHdr(constant(1), false)
  let text = new TextDecoder().decode(bytes).replace("-Y 16 +X 32", "+Y 16 +X 32")
  expect(() => decodeHdr(new TextEncoder().encode(text))).toThrow("orientation")
  expect(() => decodeHdr(bytes.slice(0, bytes.length - 10))).toThrow("truncated")
})

test("cube table: cubeLookup inverts cubeDirection on every texel of every face", () => {
  let dir = new Float64Array(3)
  let lk = new Float64Array(3)
  for (let face = 0; face < 6; face++) {
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        let s = (x + 0.5) / 8
        let t = (y + 0.5) / 8
        cubeDirection(face, s, t, dir)
        cubeLookup(dir[0]!, dir[1]!, dir[2]!, lk)
        expect(lk[0]).toBe(face)
        expect(lk[1]).toBeCloseTo(s, 9)
        expect(lk[2]).toBeCloseTo(t, 9)
      }
    }
  }
})
test("cube table: the +Y face looks up, +Z forward, and the mip chain counts", () => {
  let dir = new Float64Array(3)
  cubeDirection(2, 0.5, 0.5, dir)
  expect(Array.from(dir)).toEqual([0, 1, 0])
  cubeDirection(4, 0.5, 0.5, dir)
  expect(Array.from(dir)).toEqual([0, -0, 1])
  // +X lands on face 0 at its center: GL's own table, no flip.
  let lk = new Float64Array(3)
  cubeLookup(1, 0, 0, lk)
  expect(Array.from(lk)).toEqual([0, 0.5, 0.5])
  expect(mipLevels(1)).toBe(1)
  expect(mipLevels(4)).toBe(3)
  expect(mipLevels(128)).toBe(8)
  expect(levelRoughness(128, 0)).toBe(0)
  expect(levelRoughness(128, Math.log2(128 / ENV_ROUGH_FACE))).toBe(1)
  expect(levelRoughness(128, 7)).toBe(1)
})

// Every texel of every face of every level near `v`.
function expectLevelsNear(levels: CubeFaces[], v: number, tolerance: number) {
  for (let faces of levels) {
    for (let px of faces) {
      for (let i = 0; i < px.length; i += 4) {
        expect(Math.abs(px[i]! - v)).toBeLessThanOrEqual(tolerance)
        expect(px[i + 3]).toBe(1)
      }
    }
  }
}

test("bake: a constant panorama is constant at every level of the chain", () => {
  let base = panoramaToCube(constant(0.75), 16)
  let levels = prefilterCube(base, 16)
  expect(levels.length).toBe(5)
  expectLevelsNear(levels, 0.75, 1e-5)
})
test("bake: the sky's hemisphere lands on the +Y face and the convolution keeps its energy", () => {
  // Top half bright, bottom half dark: the +Y face reads bright, -Y
  // dark; every convolved level still tells up from down, and averages
  // to the same half-lit sphere (the lobe only redistributes).
  let sky = panorama(64, 32, (_, y) => (y < 16 ? [1, 1, 1] : [0, 0, 0]))
  let base = panoramaToCube(sky, 16)
  expect(base[2]![0]).toBeCloseTo(1, 5)
  expect(base[3]![0]).toBeCloseTo(0, 5)
  let levels = prefilterCube(base, 16)
  let mean = (faces: CubeFaces) => {
    let sum = 0
    let n = 0
    for (let px of faces) for (let i = 0; i < px.length; i += 4) (sum += px[i]!), n++
    return sum / n
  }
  for (let level = 1; level < levels.length; level++) {
    let faces = levels[level]!
    // Straight up stays mostly lit, straight down mostly dark: the
    // source-lod blur bleeds the horizon, a fully rough lobe most.
    expect(faces[2]![0]!).toBeGreaterThan(0.7)
    expect(faces[3]![0]!).toBeLessThan(0.3)
    expect(Math.abs(mean(faces) - mean(base))).toBeLessThan(0.05)
  }
})
test("bake: the .sol3e container round trips", () => {
  let levels = prefilterCube(panoramaToCube(scene, 8), 8)
  let bytes = encodeEnvironment(levels, 8)
  let back = decodeEnvironment(bytes)
  expect(back.size).toBe(8)
  expect(back.levels.length).toBe(4)
  for (let l = 0; l < levels.length; l++) for (let f = 0; f < 6; f++) expect(back.levels[l]![f]).toEqual(levels[l]![f])
  // An unaligned view (a byte offset into a larger buffer) still decodes.
  let shifted = new Uint8Array(bytes.length + 1)
  shifted.set(bytes, 1)
  let odd = decodeEnvironment(shifted.subarray(1))
  expect(odd.levels[1]![4]).toEqual(levels[1]![4])
  expect(() => decodeEnvironment(bytes.slice(0, 100))).toThrow("truncated")
  expect(() => encodeEnvironment(levels.slice(1), 8)).toThrow("levels")
})
