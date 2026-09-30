// @solidrt/3d/splat - the splat DATA side of the package without the
// runtime: the capture parsers (.ply, .splat, gunzipped .spz), the
// covariance bake and the .srts container, for `srt tool 3d/splat` and any
// app bake script under bun. The package root imports `flux:*` and so
// loads only on the runtime; this entry imports nothing that does (the
// model-data rule; tests/splat-data.test.ts imports it on the bare flux
// binary and fails the moment a gui or srt: import creeps into the chain).
//
//   import { encodeSplat, parseSplat } from "@solidrt/3d/splat"
//   writeFileSync("assets/scan.srts", encodeSplat(parseSplat(bytes)))
//
// loadSplat / createSplatMesh on the runtime read what it writes: a .srts
// record IS the texel data the splat shader fetches (two rgba32ui texels
// per splat, SPLAT_ATTRIBUTES as a byte layout) and the record the core
// sorts by depth, so runtime loading is a header parse plus byte views -
// the 3D covariance is precomputed here, deleting the quaternion decode,
// the rotation build and the M*Mt from the vertex stage (the measured
// per-splat wall no bake-less viewer can avoid), the spherical-harmonic
// bands beyond DC pack into a second texel block, and the y-up bake
// deletes the orientation footgun every viewer pushes onto the app.

import type { VertexAttribute } from "@solidrt/core/gpu"

/** One baked splat record, 32 bytes = two rgba32ui texels: center
 * float32x3 (12) + sRGB color and opacity unorm8x4 (4) | 3D covariance
 * upper triangle as six float16 (12) + a spare u32 (4). Centers stay
 * float32 deliberately - half positions are a known precision mistake on
 * large scenes; covariance quantizes to fp16 robustly. */
export const SPLAT_RECORD_BYTES = 32
/** Texels of the record texture per splat (SPLAT_RECORD_BYTES / 16). */
export const SPLAT_RECORD_TEXELS = 2
/** Bytes per rgba32ui texel. */
export const SPLAT_TEXEL_BYTES = 16
/** Texels of the SH texture per splat for each degree 0..3: the
 * 3 x ((d + 1)^2 - 1) halves beyond DC packed pairwise into u32s, eight
 * halves per texel (degree 1 = 9 halves in 2 texels, 2 = 24 in 3,
 * 3 = 45 in 6). */
export const SPLAT_SH_TEXELS = [0, 2, 3, 6]

/** The record as a byte layout, in texel order: the center and packed
 * color (texel A), then the covariance halves - iCovA (xx, xy, xz, yy),
 * iCovB (yz, zz) - and the spare word (texel B). The core's depth sort
 * keys on iCenter at this stride; the shader reads the same bytes as two
 * uvec4 texels. */
export const SPLAT_ATTRIBUTES: VertexAttribute[] = [
  { name: "iCenter", format: "float32x3" },
  { name: "iColor", format: "unorm8x4" },
  { name: "iCovA", format: "float16x4" },
  { name: "iCovB", format: "float16x2" },
  { name: "iSpare", format: "uint32" },
]

/** A parsed or loaded splat cloud: `records` is the interleaved record
 * block (SPLAT_ATTRIBUTES, SPLAT_RECORD_BYTES per splat, `count` of
 * them), importance-sorted at bake so the first n records are a usable
 * scene at any n; `sh` the matching SH texel block (SPLAT_SH_TEXELS per
 * splat, in the same order) or null at degree 0. From decodeSplat both
 * VIEW the container's bytes. */
export type SplatData = {
  count: number
  records: Uint8Array
  /** [minX, minY, minZ, maxX, maxY, maxZ] of the splat centers. */
  bounds: Float32Array
  /** Spherical-harmonic bands baked beyond the base color (0..3). */
  shDegree: number
  /** The SH texel block: count x SPLAT_SH_TEXELS[shDegree] texels of
   * SPLAT_TEXEL_BYTES, each splat's halves beyond DC coefficient-major
   * with rgb interleaved (coefficient k of channel c at half 3k + c),
   * zero-padded to its texels. null at degree 0. */
  sh: Uint8Array | null
}

export type SplatFormat = "ply" | "splat" | "spz"

export type ParseSplatOptions = {
  /** The input format; sniffed from the bytes when omitted (.ply and .spz
   * carry magic; anything else must be whole 32-byte .splat records). */
  format?: SplatFormat
  /** Keep the capture's own orientation. By default a y-down capture
   * (.ply and .splat are COLMAP-framed) is stood up to y-up at bake -
   * exact and free, positions, covariances and SH coefficients rotated
   * together - so a baked splat drops into a scene like any model; .spz
   * is y-up already and never flips. */
  keepOrientation?: boolean
  /** Spherical-harmonic bands to bake beyond the base color, 0..3,
   * capped at what the capture holds (.splat has none). Default 0: the
   * view-dependent bands cost 32/48/96 bytes per splat of texture at
   * degree 1/2/3, so a bake opts in. */
  shDegree?: number
}

// The zeroth spherical harmonic: color = 0.5 + SH_C0 * f_dc, the 3DGS
// DC-band to base-color mapping every viewer applies.
const SH_C0 = 0.28209479177387814
// Coefficients per color channel beyond DC at SH degree 0..3:
// (d + 1)^2 - 1.
const SH_COEFFICIENTS = [0, 3, 8, 15]
// The y-up flip (a half turn about x: y and z negate) on the 15 basis
// functions beyond DC, in the 3DGS order (y, z, x | xy, yz, 2zz-xx-yy, xz,
// xx-yy | y(3xx-yy), xyz, y(4zz-xx-yy), z(2zz-3xx-3yy), x(4zz-xx-yy),
// z(xx-yy), x(xx-3yy)): a basis odd in (y, z) together flips sign, so its
// coefficient negates to keep the color at the rotated direction.
const SH_FLIP_SIGN = [-1, -1, 1, -1, 1, 1, -1, 1, -1, 1, -1, -1, 1, -1, 1]
// spz stores an SH coefficient as a byte around 128: (byte - 128) / 128.
const SPZ_SH_SCALE = 128
// Largest finite float16; covariance entries are clamped into this range
// (a monster background splat past it would otherwise bake to infinity).
const F16_MAX = 65504
// spz packs the DC band as u8 around 0.5 with this scale (Niantic's
// load-spz.cc colorScale): dc = (byte / 255 - 0.5) / SPZ_COLOR_SCALE.
const SPZ_COLOR_SCALE = 0.15
// spz packs log scales as u8 sixteenths offset by -10: exp(byte / 16 - 10).
const SPZ_SCALE_OFFSET = 10
const SPZ_SCALE_STEP = 16

/** "SRTS" read as a little-endian u32. */
const MAGIC = 0x53545253
// Version 2: the 32-byte two-texel record, importance-sorted, y-up unless
// the bake kept orientation, plus the optional SH texel block; the header
// carries count, shDegree, bounds and both block ranges. (Version 1 was
// the 28-byte instance-attribute record of the first viewer.)
const VERSION = 2

/** The de-facto .splat interchange record (antimatter15): position
 * float32x3, scale float32x3, sRGB color + opacity unorm8x4, rotation
 * quaternion (w, x, y, z) as unorm8 around 128. */
const SPLAT_IN_BYTES = 32

// spz (gunzipped) header: magic, version, count as u32, then shDegree,
// fractionalBits, flags, reserved as bytes.
const SPZ_MAGIC = 0x5053474e
const SPZ_HEADER_BYTES = 16
// spz version 2 packs rotations as the quaternion's first three (x, y, z
// around 127.5, w derived); version 3 as smallest-three in four bytes.
// Version 1 (float16 positions) was never released and version 4 moved to
// zstd streams; both are rejected by name.
const SPZ_VERSIONS = [2, 3]
// Smallest-three fields: 9-bit magnitudes scaled by 1/sqrt(2), a sign bit
// each, the largest component's index in the top two bits.
const SPZ_ROT_MAG_BITS = 9
// Coefficients per color channel for SH degree 1..3 (bands beyond DC).
const SPZ_SH_DIM = [0, 3, 8, 15]

// The parsers' common product, before the bake: linear scales, (w,x,y,z)
// rotations (normalized at bake), sRGB color + opacity bytes, and the SH
// coefficients beyond DC at `shDegree` (coefficient-major, rgb
// interleaved: sh[(i * D + k) * 3 + c], D = SH_COEFFICIENTS[shDegree]).
// `flip` marks a y-down source the bake stands up.
type Gaussians = {
  count: number
  positions: Float32Array
  scales: Float32Array
  rotations: Float32Array
  colors: Uint8Array
  shDegree: number
  sh: Float32Array | null
  flip: boolean
}

let sigmoid = (x: number) => 1 / (1 + Math.exp(-x))
let colorByte = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255)

/**
 * Parse a captured splat cloud - a trainer's .ply, the de-facto .splat
 * interchange, or a gunzipped .spz (Niantic; the container is
 * gzip-compressed, and this entry stays decompression-free, so hand it
 * the inflated bytes - `srt tool 3d/splat` does) - into baked SplatData:
 * covariance precomputed, y-up by default, importance-sorted (size times
 * opacity), bounds measured. Runs anywhere (bun, the runtime); for a
 * capture of real size run it at pack time and ship encodeSplat's .srts.
 */
export function parseSplat(bytes: Uint8Array, opts?: ParseSplatOptions): SplatData {
  let format = opts?.format ?? sniffFormat(bytes)
  let degree = opts?.shDegree ?? 0
  if (!(Number.isInteger(degree) && degree >= 0 && degree <= 3)) throw new Error("parseSplat: shDegree must be an integer 0..3, got " + degree)
  let raw = format === "ply" ? parsePly(bytes, degree) : format === "spz" ? parseSpz(bytes, degree) : parseDotSplat(bytes)
  if (opts?.keepOrientation === true) raw.flip = false
  return bake(raw)
}

function sniffFormat(bytes: Uint8Array): SplatFormat {
  if (bytes.length >= 4 && bytes[0] === 0x70 && bytes[1] === 0x6c && bytes[2] === 0x79 && (bytes[3] === 0x0a || bytes[3] === 0x0d)) return "ply"
  if (bytes.length >= 4) {
    let dv = new DataView(bytes.buffer, bytes.byteOffset, 4)
    if (dv.getUint32(0, true) === SPZ_MAGIC) return "spz"
  }
  if (bytes.length >= 2 && bytes[0] === 0x1f && bytes[1] === 0x8b) {
    throw new Error("parseSplat: gzip-compressed input (.spz files are); gunzip the bytes first - srt tool 3d/splat does")
  }
  if (bytes.length > 0 && bytes.length % SPLAT_IN_BYTES === 0) return "splat"
  throw new Error("parseSplat: unrecognized input - not a .ply, not a gunzipped .spz, and not whole 32-byte .splat records")
}

// One property of a .ply vertex element: its byte offset in a record and
// whether it is a 32-bit float (the only type the trainer fields use).
type PlyProperty = { offset: number; float: boolean }

// A 3DGS trainer's .ply: binary little-endian, a leading `vertex` element
// whose properties carry raw training values - log scales, pre-sigmoid
// opacity, the DC band as SH coefficients, the quaternion (w, x, y, z)
// in rot_0..3, and the higher bands as f_rest_0..(3D - 1) laid out
// channel-major (coefficient k of channel c at f_rest_[c * D + k], D the
// file's coefficients per channel). `degree` caps the bands read.
function parsePly(bytes: Uint8Array, degree: number): Gaussians {
  // The header is ASCII up to "end_header" and its newline.
  let probe = new TextDecoder().decode(bytes.subarray(0, Math.min(bytes.length, 65536)))
  let mark = probe.indexOf("end_header")
  if (mark < 0) throw new Error("parseSplat: .ply header end not found (or a header past 64 KiB)")
  let headerEnd = probe.indexOf("\n", mark)
  if (headerEnd < 0) throw new Error("parseSplat: .ply header end not terminated")
  let lines = probe
    .slice(0, headerEnd)
    .split("\n")
    .map(l => l.trim())
  if (lines[0] !== "ply") throw new Error("parseSplat: not a .ply file")
  let format = lines.find(l => l.startsWith("format "))
  if (format !== "format binary_little_endian 1.0") {
    throw new Error("parseSplat: .ply format must be binary_little_endian 1.0, got '" + (format ?? "none") + "'")
  }

  const SIZES: Record<string, number> = { char: 1, int8: 1, uchar: 1, uint8: 1, short: 2, int16: 2, ushort: 2, uint16: 2, int: 4, int32: 4, uint: 4, uint32: 4, float: 4, float32: 4, double: 8, float64: 8 }
  let count = -1
  let stride = 0
  let properties = new Map<string, PlyProperty>()
  let element: string | null = null
  for (let line of lines) {
    let words = line.split(/\s+/)
    if (words[0] === "element") {
      if (element === "vertex") break
      element = words[1] ?? null
      if (element === "vertex") count = Number(words[2])
    } else if (words[0] === "property" && element === "vertex") {
      if (words[1] === "list") throw new Error("parseSplat: .ply vertex element has a list property; records must be fixed-size")
      let size = SIZES[words[1]!]
      let name = words[2]
      if (size === undefined || name === undefined) throw new Error("parseSplat: malformed .ply property '" + line + "'")
      properties.set(name, { offset: stride, float: words[1] === "float" || words[1] === "float32" })
      stride += size
    } else if (words[0] === "property" && element !== null && element !== "vertex") {
      // Data of an element BEFORE vertex would shift the vertex block by
      // an amount only its data knows; trainers write vertex first.
      throw new Error("parseSplat: .ply has element '" + element + "' before vertex; only vertex-first files are supported")
    }
  }
  if (count < 0) throw new Error("parseSplat: .ply has no vertex element")

  const FIELDS = ["x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]
  let at: number[] = FIELDS.map(name => {
    let p = properties.get(name)
    if (p === undefined) throw new Error("parseSplat: .ply vertex element is missing '" + name + "' - not a 3DGS capture?")
    if (!p.float) throw new Error("parseSplat: .ply property '" + name + "' must be float")
    return p.offset
  })

  // The higher bands: however many f_rest_* the file carries (0, 9, 24
  // or 45), read down to the requested degree.
  let restCount = 0
  while (properties.has("f_rest_" + restCount)) restCount++
  if (restCount % 3 !== 0 || !SH_COEFFICIENTS.includes(restCount / 3)) {
    throw new Error("parseSplat: .ply carries " + restCount + " f_rest properties; expected 0, 9, 24 or 45")
  }
  let fileD = restCount / 3
  let shDegree = Math.min(degree, SH_COEFFICIENTS.indexOf(fileD))
  let D = SH_COEFFICIENTS[shDegree]!
  let restAt: number[] = []
  for (let c = 0; c < 3; c++) {
    for (let k = 0; k < D; k++) {
      let p = properties.get("f_rest_" + (c * fileD + k))!
      if (!p.float) throw new Error("parseSplat: .ply property 'f_rest_" + (c * fileD + k) + "' must be float")
      restAt.push(p.offset)
    }
  }

  let base = headerEnd + 1
  if (bytes.length < base + count * stride) throw new Error("parseSplat: .ply shorter than its " + count + " vertices")
  let dv = new DataView(bytes.buffer, bytes.byteOffset + base, count * stride)
  let out = emptyGaussians(count, true, shDegree)
  let rec = 0
  let read = (f: number) => dv.getFloat32(rec + at[f]!, true)
  for (let i = 0; i < count; i++) {
    rec = i * stride
    if (out.sh !== null) {
      for (let c = 0; c < 3; c++) {
        for (let k = 0; k < D; k++) out.sh[(i * D + k) * 3 + c] = dv.getFloat32(rec + restAt[c * D + k]!, true)
      }
    }
    out.positions[i * 3] = read(0)
    out.positions[i * 3 + 1] = read(1)
    out.positions[i * 3 + 2] = read(2)
    out.colors[i * 4] = colorByte(0.5 + SH_C0 * read(3))
    out.colors[i * 4 + 1] = colorByte(0.5 + SH_C0 * read(4))
    out.colors[i * 4 + 2] = colorByte(0.5 + SH_C0 * read(5))
    out.colors[i * 4 + 3] = colorByte(sigmoid(read(6)))
    out.scales[i * 3] = Math.exp(read(7))
    out.scales[i * 3 + 1] = Math.exp(read(8))
    out.scales[i * 3 + 2] = Math.exp(read(9))
    out.rotations[i * 4] = read(10)
    out.rotations[i * 4 + 1] = read(11)
    out.rotations[i * 4 + 2] = read(12)
    out.rotations[i * 4 + 3] = read(13)
  }
  return out
}

// The .splat record: everything already activated (linear scales, sRGB
// color + opacity bytes), the quaternion (w, x, y, z) as unorm8 around 128.
function parseDotSplat(bytes: Uint8Array): Gaussians {
  if (bytes.length % SPLAT_IN_BYTES !== 0) throw new Error("parseSplat: .splat length is not whole 32-byte records")
  let count = bytes.length / SPLAT_IN_BYTES
  let dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let out = emptyGaussians(count, true, 0)
  for (let i = 0; i < count; i++) {
    let rec = i * SPLAT_IN_BYTES
    for (let k = 0; k < 3; k++) {
      out.positions[i * 3 + k] = dv.getFloat32(rec + k * 4, true)
      out.scales[i * 3 + k] = dv.getFloat32(rec + 12 + k * 4, true)
    }
    for (let k = 0; k < 4; k++) {
      out.colors[i * 4 + k] = bytes[rec + 24 + k]!
      out.rotations[i * 4 + k] = (bytes[rec + 28 + k]! - 128) / 128
    }
  }
  return out
}

// A gunzipped .spz: 16-byte header, then per-attribute blocks in file
// order - positions (24-bit fixed point), alphas (sigmoid as u8), colors
// (DC band around 0.5), scales (log sixteenths), rotations (version 2:
// first-three; version 3: smallest-three), SH (a byte per coefficient
// around 128, coefficient-major with rgb interleaved: coefficient k of
// channel c of splat i at (i * shDim + k) * 3 + c). spz stores y-up (RUB)
// already, so it never flips. `degree` caps the bands read.
function parseSpz(bytes: Uint8Array, degree: number): Gaussians {
  if (bytes.length < SPZ_HEADER_BYTES) throw new Error("parseSplat: .spz too short for its header")
  let dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  if (dv.getUint32(0, true) !== SPZ_MAGIC) throw new Error("parseSplat: bad .spz magic")
  let version = dv.getUint32(4, true)
  if (!SPZ_VERSIONS.includes(version)) {
    throw new Error("parseSplat: .spz version " + version + " is not supported (version 2 or 3; 4+ moved to zstd streams)")
  }
  let count = dv.getUint32(8, true)
  let shDegree = bytes[12]!
  let fractionalBits = bytes[13]!
  if (shDegree > 3) throw new Error("parseSplat: .spz SH degree " + shDegree + " out of range")
  let rotBytes = version === 2 ? 3 : 4
  let positionsAt = SPZ_HEADER_BYTES
  let alphasAt = positionsAt + count * 9
  let colorsAt = alphasAt + count
  let scalesAt = colorsAt + count * 3
  let rotationsAt = scalesAt + count * 3
  let shAt = rotationsAt + count * rotBytes
  let end = shAt + count * SPZ_SH_DIM[shDegree]! * 3
  if (bytes.length < end) throw new Error("parseSplat: .spz shorter than its " + count + " splats")

  let scale = 1 / (1 << fractionalBits)
  let bakedDegree = Math.min(degree, shDegree)
  let fileD = SPZ_SH_DIM[shDegree]!
  let D = SH_COEFFICIENTS[bakedDegree]!
  let out = emptyGaussians(count, false, bakedDegree)
  let xyzw = [0, 0, 0, 0]
  for (let i = 0; i < count; i++) {
    if (out.sh !== null) {
      for (let k = 0; k < D; k++) {
        for (let c = 0; c < 3; c++) out.sh[(i * D + k) * 3 + c] = (bytes[shAt + (i * fileD + k) * 3 + c]! - SPZ_SH_SCALE) / SPZ_SH_SCALE
      }
    }
    for (let k = 0; k < 3; k++) {
      let at = positionsAt + (i * 3 + k) * 3
      let fixed = bytes[at]! | (bytes[at + 1]! << 8) | (bytes[at + 2]! << 16)
      if (fixed & 0x800000) fixed |= -0x1000000
      out.positions[i * 3 + k] = fixed * scale
      out.scales[i * 3 + k] = Math.exp(bytes[scalesAt + i * 3 + k]! / SPZ_SCALE_STEP - SPZ_SCALE_OFFSET)
      let dc = (bytes[colorsAt + i * 3 + k]! / 255 - 0.5) / SPZ_COLOR_SCALE
      out.colors[i * 4 + k] = colorByte(0.5 + SH_C0 * dc)
    }
    out.colors[i * 4 + 3] = bytes[alphasAt + i]!
    let q = out.rotations
    if (version === 2) {
      let x = bytes[rotationsAt + i * 3]! / 127.5 - 1
      let y = bytes[rotationsAt + i * 3 + 1]! / 127.5 - 1
      let z = bytes[rotationsAt + i * 3 + 2]! / 127.5 - 1
      q[i * 4] = Math.sqrt(Math.max(0, 1 - x * x - y * y - z * z))
      q[i * 4 + 1] = x
      q[i * 4 + 2] = y
      q[i * 4 + 3] = z
    } else {
      // Smallest-three, unpacked into (x, y, z, w) order then stored
      // (w, x, y, z): top two bits index the largest component, then
      // three sign + 9-bit magnitudes, highest component index first.
      let comp = dv.getUint32(rotationsAt + i * 4, true)
      let mask = (1 << SPZ_ROT_MAG_BITS) - 1
      let largest = comp >>> 30
      let sum = 0
      for (let k = 3; k >= 0; k--) {
        if (k === largest) continue
        let mag = comp & mask
        let neg = (comp >>> SPZ_ROT_MAG_BITS) & 1
        comp = comp >>> (SPZ_ROT_MAG_BITS + 1)
        let v = (Math.SQRT1_2 * mag) / mask
        xyzw[k] = neg === 1 ? -v : v
        sum += v * v
      }
      xyzw[largest] = Math.sqrt(Math.max(0, 1 - sum))
      q[i * 4] = xyzw[3]!
      q[i * 4 + 1] = xyzw[0]!
      q[i * 4 + 2] = xyzw[1]!
      q[i * 4 + 3] = xyzw[2]!
    }
  }
  return out
}

function emptyGaussians(count: number, flip: boolean, shDegree: number): Gaussians {
  let D = SH_COEFFICIENTS[shDegree]!
  return {
    count,
    positions: new Float32Array(count * 3),
    scales: new Float32Array(count * 3),
    rotations: new Float32Array(count * 4),
    colors: new Uint8Array(count * 4),
    shDegree,
    sh: D > 0 ? new Float32Array(count * D * 3) : null,
    flip,
  }
}

// The bake: importance-sort (size times opacity, descending - the .splat
// convention, so a count prefix is a usable scene at any n), compute each
// splat's 3D covariance R S St Rt once, stand a y-down capture up
// (positions (x, -y, -z); the covariance conjugated by the same half-turn
// negates its xy and xz entries; the SH coefficients odd in (y, z) negate),
// measure bounds, write the records and the SH texels.
function bake(g: Gaussians): SplatData {
  let n = g.count
  let importance = new Float32Array(n)
  for (let i = 0; i < n; i++) {
    importance[i] = g.scales[i * 3]! * g.scales[i * 3 + 1]! * g.scales[i * 3 + 2]! * g.colors[i * 4 + 3]!
  }
  let order = new Uint32Array(n)
  for (let i = 0; i < n; i++) order[i] = i
  order.sort((a, b) => importance[b]! - importance[a]!)

  let sy = g.flip ? -1 : 1
  let records = new Uint8Array(n * SPLAT_RECORD_BYTES)
  let dv = new DataView(records.buffer)
  let D = SH_COEFFICIENTS[g.shDegree]!
  let shBytes = SPLAT_SH_TEXELS[g.shDegree]! * SPLAT_TEXEL_BYTES
  let sh = D > 0 ? new Uint8Array(n * shBytes) : null
  let shdv = sh === null ? null : new DataView(sh.buffer)
  let bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])
  let clampHalf = (v: number) => Math.min(F16_MAX, Math.max(-F16_MAX, v))
  let half = (at: number, v: number) => dv.setFloat16(at, clampHalf(v), true)
  for (let slot = 0; slot < n; slot++) {
    let i = order[slot]!
    let w = g.rotations[i * 4]!
    let x = g.rotations[i * 4 + 1]!
    let y = g.rotations[i * 4 + 2]!
    let z = g.rotations[i * 4 + 3]!
    let norm = Math.hypot(w, x, y, z)
    if (norm > 0) {
      w /= norm
      x /= norm
      y /= norm
      z /= norm
    } else {
      w = 1
    }
    let sx = g.scales[i * 3]!
    let syc = g.scales[i * 3 + 1]!
    let sz = g.scales[i * 3 + 2]!
    // M = R * diag(s); sigma = M * Mt, upper triangle.
    let m00 = (1 - 2 * (y * y + z * z)) * sx
    let m01 = 2 * (x * y - w * z) * syc
    let m02 = 2 * (x * z + w * y) * sz
    let m10 = 2 * (x * y + w * z) * sx
    let m11 = (1 - 2 * (x * x + z * z)) * syc
    let m12 = 2 * (y * z - w * x) * sz
    let m20 = 2 * (x * z - w * y) * sx
    let m21 = 2 * (y * z + w * x) * syc
    let m22 = (1 - 2 * (x * x + y * y)) * sz
    let xx = m00 * m00 + m01 * m01 + m02 * m02
    let xy = (m00 * m10 + m01 * m11 + m02 * m12) * sy
    let xz = (m00 * m20 + m01 * m21 + m02 * m22) * sy
    let yy = m10 * m10 + m11 * m11 + m12 * m12
    let yz = m10 * m20 + m11 * m21 + m12 * m22
    let zz = m20 * m20 + m21 * m21 + m22 * m22

    // The + 0 folds a flip's -0 to 0, so the header JSON round-trips.
    let px = g.positions[i * 3]! + 0
    let py = g.positions[i * 3 + 1]! * sy + 0
    let pz = g.positions[i * 3 + 2]! * sy + 0
    if (px < bounds[0]!) bounds[0] = px
    if (py < bounds[1]!) bounds[1] = py
    if (pz < bounds[2]!) bounds[2] = pz
    if (px > bounds[3]!) bounds[3] = px
    if (py > bounds[4]!) bounds[4] = py
    if (pz > bounds[5]!) bounds[5] = pz

    // Texel A: the center and the packed color; texel B: the six
    // covariance halves and the spare word (left 0).
    let at = slot * SPLAT_RECORD_BYTES
    dv.setFloat32(at, px, true)
    dv.setFloat32(at + 4, py, true)
    dv.setFloat32(at + 8, pz, true)
    records[at + 12] = g.colors[i * 4]!
    records[at + 13] = g.colors[i * 4 + 1]!
    records[at + 14] = g.colors[i * 4 + 2]!
    records[at + 15] = g.colors[i * 4 + 3]!
    half(at + 16, xx)
    half(at + 18, xy)
    half(at + 20, xz)
    half(at + 22, yy)
    half(at + 24, yz)
    half(at + 26, zz)
    // The SH halves, consecutive = packed pairwise into the texels' u32s
    // (the low half first, as unpackHalf2x16 reads them).
    if (shdv !== null) {
      let base = slot * shBytes
      for (let k = 0; k < D; k++) {
        let sign = g.flip ? SH_FLIP_SIGN[k]! : 1
        for (let c = 0; c < 3; c++) shdv.setFloat16(base + (k * 3 + c) * 2, clampHalf(g.sh![(i * D + k) * 3 + c]! * sign), true)
      }
    }
  }
  if (n === 0) bounds.fill(0)
  return { count: n, records, bounds, shDegree: g.shDegree, sh }
}

type Block = { offset: number; bytes: number }

type Header = { count: number; shDegree: number; bounds: number[]; records: Block; sh?: Block }

// The SH block's byte length for a cloud: its texels per splat times the
// count; 0 at degree 0.
function shBytesOf(count: number, shDegree: number): number {
  return count * SPLAT_SH_TEXELS[shDegree]! * SPLAT_TEXEL_BYTES
}

/**
 * Serialize a splat cloud into the .srts container, all little-endian:
 * "SRTS" u32 | version u32 | jsonLength u32 | json (padded to 4) |
 * payload. The JSON header carries count, shDegree, bounds and the byte
 * range of the record block and (above degree 0) the SH block; the
 * payload is both blocks verbatim.
 */
export function encodeSplat(data: SplatData): Uint8Array {
  if (data.records.byteLength !== data.count * SPLAT_RECORD_BYTES) {
    throw new Error("encodeSplat: records hold " + data.records.byteLength + " bytes, not " + data.count + " whole splat records")
  }
  if (data.bounds.length !== 6) throw new Error("encodeSplat: bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
  if (!(Number.isInteger(data.shDegree) && data.shDegree >= 0 && data.shDegree <= 3)) throw new Error("encodeSplat: shDegree must be 0..3, got " + data.shDegree)
  let shBytes = shBytesOf(data.count, data.shDegree)
  if ((data.sh?.byteLength ?? 0) !== shBytes) {
    throw new Error("encodeSplat: the SH block holds " + (data.sh?.byteLength ?? 0) + " bytes; degree " + data.shDegree + " over " + data.count + " splats needs " + shBytes)
  }
  let header: Header = {
    count: data.count,
    shDegree: data.shDegree,
    bounds: Array.from(data.bounds),
    records: { offset: 0, bytes: data.records.byteLength },
  }
  if (shBytes > 0) header.sh = { offset: data.records.byteLength, bytes: shBytes }
  let json = new TextEncoder().encode(JSON.stringify(header))
  let jsonPadded = json.byteLength + ((4 - (json.byteLength % 4)) % 4)
  let out = new Uint8Array(12 + jsonPadded + data.records.byteLength + shBytes)
  let dv = new DataView(out.buffer)
  dv.setUint32(0, MAGIC, true)
  dv.setUint32(4, VERSION, true)
  dv.setUint32(8, json.byteLength, true)
  out.set(json, 12)
  out.set(data.records, 12 + jsonPadded)
  if (data.sh !== null && shBytes > 0) out.set(data.sh, 12 + jsonPadded + data.records.byteLength)
  return out
}

/**
 * Read a .srts container back into SplatData. The records and the SH
 * block are VIEWS onto `bytes` (copied once only when the input is not
 * 4-aligned), so the bytes must outlive the data - which is the point:
 * loading a bake is a header parse, and the views upload as-is.
 */
export function decodeSplat(bytes: Uint8Array): SplatData {
  if (bytes.byteOffset % 4 !== 0) bytes = new Uint8Array(bytes)
  if (bytes.byteLength < 12) throw new Error("decodeSplat: not a splat file (too short)")
  let dv = new DataView(bytes.buffer, bytes.byteOffset, 12)
  if (dv.getUint32(0, true) !== MAGIC) throw new Error("decodeSplat: not a splat file (bad magic)")
  let version = dv.getUint32(4, true)
  if (version !== VERSION) throw new Error("decodeSplat: version " + version + ", expected " + VERSION + " - re-bake with `srt tool 3d/splat`")
  let jsonLength = dv.getUint32(8, true)
  let header: Header = JSON.parse(new TextDecoder().decode(bytes.subarray(12, 12 + jsonLength)))
  let payload = 12 + jsonLength + ((4 - (jsonLength % 4)) % 4)
  if (header.records.bytes !== header.count * SPLAT_RECORD_BYTES) {
    throw new Error("decodeSplat: record block holds " + header.records.bytes + " bytes, not " + header.count + " whole splat records")
  }
  if (bytes.byteLength < payload + header.records.offset + header.records.bytes) {
    throw new Error("decodeSplat: shorter than its " + header.count + " records")
  }
  if (!(Number.isInteger(header.shDegree) && header.shDegree >= 0 && header.shDegree <= 3)) {
    throw new Error("decodeSplat: shDegree " + header.shDegree + " out of range")
  }
  let shBytes = shBytesOf(header.count, header.shDegree)
  let sh: Uint8Array | null = null
  if (shBytes > 0) {
    if (header.sh === undefined || header.sh.bytes !== shBytes) {
      throw new Error("decodeSplat: SH block holds " + (header.sh?.bytes ?? 0) + " bytes; degree " + header.shDegree + " over " + header.count + " splats needs " + shBytes)
    }
    if (bytes.byteLength < payload + header.sh.offset + shBytes) throw new Error("decodeSplat: shorter than its SH block")
    sh = bytes.subarray(payload + header.sh.offset, payload + header.sh.offset + shBytes)
  }
  return {
    count: header.count,
    records: bytes.subarray(payload + header.records.offset, payload + header.records.offset + header.records.bytes),
    bounds: Float32Array.from(header.bounds),
    shDegree: header.shDegree,
    sh,
  }
}
