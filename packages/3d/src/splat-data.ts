// @solidrt/3d/splat - the splat DATA side of the package without the
// runtime: the capture parsers (.ply, .splat, gunzipped .spz), the bake
// and the .sol3s container, for `sol tool 3d/splat` and any app bake
// script under bun. The package root imports `flux:*` and so loads only on
// the runtime; this entry imports nothing that does (the model-data rule;
// tests/splat-data.test.ts imports it on the bare flux binary and fails
// the moment a gui or sol: import creeps into the chain).
//
//   import { encodeSplat, parseSplat } from "@solidrt/3d/splat"
//   writeFileSync("assets/scan.sol3s", encodeSplat(parseSplat(bytes)))
//
// loadSplat / createSplatMesh on the runtime read what it writes: a .sol3s
// record IS the texel the splat shader fetches (one rgba32ui texel per
// splat, SPLAT_ATTRIBUTES as a byte layout - the field's 16-byte chunked
// record, what Unity's importer and PlayCanvas's compressed ply converged
// on) and its position word is the key the core sorts by depth against
// the chunk table, so runtime loading is a header parse plus byte views.
// The bake quantizes against 256-splat chunks (positions and log scales
// as 11/10/11 bits over the chunk's box, the rotation as a smallest-three
// quaternion, color rgba8), stores the records in spatial order with the
// draw order (by importance) as an id block beside them, packs the
// spherical-harmonic bands beyond DC as a byte per coefficient, and
// stands a y-down capture up to y-up, deleting the orientation footgun
// every viewer pushes onto the app.

import type { VertexAttribute } from "@solidrt/core/gpu"

/** One baked splat record, 16 bytes = one rgba32ui texel, four words:
 * the position as 16-bit unorms over its chunk's position box (x in the
 * low half of word 0, y in its high half, z in the low half of word 1;
 * the high half of word 1 is reserved, 0), the rotation as a
 * smallest-three quaternion (the largest component's index in the top
 * two bits, the other three as 10-bit unorms over [-1/sqrt2, 1/sqrt2] in
 * 1022 steps, zero exact, in index order at bits 20, 10 and 0; the
 * largest is made positive, so it derives as sqrt(1 - the rest)), and
 * the scale packed 11/10/11 bits over the chunk's log-scale box. The
 * sRGB color + opacity sit beside the record as an rgba8 texel of the
 * color block (SPLAT_COLOR_BYTES). decodeRecord is the reference decoder
 * the vertex stage agrees with. 16 bits of position is what the fine
 * detail of a trained capture needs: the field's 11 bits over a 256-splat
 * box moved a lizard's sub-millimetre scales by multiples of their size
 * (the plan's Findings). */
export const SPLAT_RECORD_BYTES = 16
/** Bytes per splat of the color block: sRGB r, g, b and opacity. */
export const SPLAT_COLOR_BYTES = 4
/** Texels of the record texture per splat (SPLAT_RECORD_BYTES / 16). */
export const SPLAT_RECORD_TEXELS = 1
/** Bytes per rgba32ui texel. */
export const SPLAT_TEXEL_BYTES = 16
/** Splats per chunk: record id >> 8 is the chunk, whose box the record's
 * position and scale words are unorms over. The field's size (Unity,
 * PlayCanvas): small enough that a chunk's box is a few splats across
 * where the cloud is dense, large enough that the table is 0.2 bytes per
 * splat. */
export const SPLAT_CHUNK_SIZE = 256
/** Floats per chunk record: position min xyz, position max xyz, log-scale
 * min xyz, log-scale max xyz - as three rgba32f texels (minX, minY, minZ,
 * maxX | maxY, maxZ, minSx, minSy | minSz, maxSx, maxSy, maxSz). */
export const SPLAT_CHUNK_FLOATS = 12
/** Bytes per chunk record (SPLAT_CHUNK_FLOATS x 4). */
export const SPLAT_CHUNK_BYTES = 48
/** Texels of the chunk texture per chunk. */
export const SPLAT_CHUNK_TEXELS = 3
/** Unorm bits of a position component (65536 steps over the chunk's
 * box per axis). */
export const SPLAT_POSITION_BITS = 16
/** Unorm bits of the packed scale word per axis (x at the top, then y, z
 * at the bottom): 11/10/11 fills the word. */
export const SPLAT_SCALE_BITS = [11, 10, 11]
/** Unorm bits per stored quaternion component. */
export const SPLAT_ROTATION_BITS = 10
/** Texels of the SH texture per splat for each degree 0..3: the
 * 3 x ((d + 1)^2 - 1) coefficients beyond DC as a byte each, sixteen
 * per texel (degree 1 = 9 bytes in 1 texel, 2 = 24 in 2, 3 = 45 in 3). */
export const SPLAT_SH_TEXELS = [0, 1, 2, 3]
/** An SH coefficient byte decodes as (byte - SPLAT_SH_BYTE_ZERO) /
 * SPLAT_SH_BYTE_SCALE, spz's encoding: [-1, 1) in 1/128 steps. */
export const SPLAT_SH_BYTE_ZERO = 128
export const SPLAT_SH_BYTE_SCALE = 128

/** How a bake stores the SH bands beyond DC. "u8": a byte per coefficient
 * in SPLAT_SH_TEXELS texels per splat (exact for an spz, 16/32/48 bytes
 * per splat). "pq": product-quantized - each coefficient's rgb is first
 * split into a luma plane and a chroma plane (SPLAT_SH_SUBSPACES), the
 * planes are cut into SPLAT_SH_SUBSPACES by band, each subspace is
 * clustered at bake into a codebook of SPLAT_SH_PALETTE_SIZE entries by
 * k-means, and a splat stores one byte label per subspace
 * (SPLAT_SH_LABEL_BYTES per splat: 2/4/8 at degree 1/2/3) beside one
 * shared palette block; 6x less texture at degree 3, at the price of the
 * clustering error the bake reports. */
export type SplatShEncoding = "u8" | "pq"
/** One pq subspace: a run of `count` coefficients from `first` (3DGS
 * order) of one plane of the per-coefficient color transform the palette
 * stores - "luma" is the green channel, "chroma" the red and blue
 * channels as differences from green (r - g, b - g; exact on the byte
 * lattice, clamped to the byte range when a difference exceeds it, which
 * the report counts). */
export type SplatShSubspace = { plane: "luma" | "chroma"; first: number; count: number }
/** The pq subspaces, in the order degree d uses the first
 * SPLAT_SH_LABEL_BYTES[d] of: band 1 luma and chroma, band 2 luma and
 * chroma, band 3 luma in two halves, band 3 chroma in two halves. The real
 * SH basis is orthonormal, so squared coefficient error is the mean
 * squared color error over all view directions, and a band is the natural
 * unit of that error; luma and chroma are separate subspaces because a
 * capture's SH is mostly achromatic (rgb nearly equal) and one codebook
 * over rgb spends its codes along the gray axis and averages the rare hue
 * shifts away - measured on the lizard: a band split kept 0.3 of the
 * chroma norm at the median, this split keeps it all at the same luma
 * error; band 3 splits in two because it carries most of the luma error
 * and 256 codes over its 7 coefficients were measured at twice the error
 * of two subspaces. Every subspace's bytes (3, 6, 5, 10, 3, 4, 6, 8) fit
 * one 16-byte palette texel. */
export const SPLAT_SH_SUBSPACES: SplatShSubspace[] = [
  { plane: "luma", first: 0, count: 3 },
  { plane: "chroma", first: 0, count: 3 },
  { plane: "luma", first: 3, count: 5 },
  { plane: "chroma", first: 3, count: 5 },
  { plane: "luma", first: 8, count: 3 },
  { plane: "luma", first: 11, count: 4 },
  { plane: "chroma", first: 8, count: 3 },
  { plane: "chroma", first: 11, count: 4 },
]
/** Subspaces in use (= label bytes per splat and palette texels per entry)
 * at SH degree 0..3. */
export const SPLAT_SH_LABEL_BYTES = [0, 2, 4, 8]
/** Label texels per splat at SH degree 0..3: the labels ride an integer
 * texture of up to four bytes per texel (rg8ui at degree 1, rgba8ui
 * above), two texels at degree 3. */
export const SPLAT_SH_LABEL_TEXELS = [0, 1, 1, 2]
/** Bytes per label texel at SH degree 0..3. */
export const SPLAT_SH_LABEL_TEXEL_BYTES = [0, 2, 4, 4]
/** Codebook entries per subspace: what one label byte addresses. */
export const SPLAT_SH_PALETTE_SIZE = 256

/** The record as a byte layout, in word order. The core's depth sort keys
 * on iPosition as packed unorm16x3 words against the chunk table (the
 * hand-off declares that column and the engine keeps those 8 bytes per
 * splat beside its id, nothing else of the record); the shader reads the
 * same bytes as one uvec4 texel. */
export const SPLAT_ATTRIBUTES: VertexAttribute[] = [
  { name: "iPosition", format: "uint32x2" },
  { name: "iRotation", format: "uint32" },
  { name: "iScale", format: "uint32" },
]

/** What the bake lost, for the tool to print. `positionStep` is the
 * chunk's position step over the splat's largest axis (0.01: the step is
 * 1% of the splat), as the median and 99th percentile over the cloud and
 * the share of splats above SPLAT_STEP_VISIBLE; `shClamped` how many SH
 * coefficients fell outside the byte's [-1, 1) and were clamped;
 * `shPalette` (the "pq" encoding) what the codebooks lost, per subspace in
 * use: `rms` the root mean square error per coefficient and `p99` the 99th
 * percentile over splats of the largest coefficient error, both in byte
 * steps (1/SPLAT_SH_BYTE_SCALE; the u8 encoding's own rounding is 0.29 rms,
 * 0.5 max), and `clamped` how many chroma differences fell outside the
 * byte range. */
export type BakeReport = {
  chunks: number
  positionStep: { median: number; p99: number; visibleShare: number }
  shClamped: number
  shPalette?: { rms: number[]; p99: number[]; clamped: number }
}
/** A position step above this fraction of the splat's largest axis is
 * where quantization starts to show; the report counts those splats. */
export const SPLAT_STEP_VISIBLE = 0.1

/** A parsed or loaded splat cloud: `records` is the record block
 * (SPLAT_ATTRIBUTES, SPLAT_RECORD_BYTES per splat, `count` of them) in
 * SPATIAL order (a k-d partition, so every chunk is a compact box),
 * `colors` the color block (SPLAT_COLOR_BYTES per splat, in record
 * order), `chunks` the chunk table (SPLAT_CHUNK_BYTES per chunk of
 * SPLAT_CHUNK_SIZE records, the last chunk partial), `order` the draw
 * population (record ids by importance, size times opacity descending,
 * so its first n are a usable scene at any n), `sh` the matching SH
 * texel block (SPLAT_SH_TEXELS per splat, in record order) or null at
 * degree 0. From decodeSplat the blocks VIEW the container's bytes. */
export type SplatData = {
  count: number
  records: Uint8Array
  colors: Uint8Array
  chunks: Uint8Array
  /** Record ids in draw order: by importance, descending; `count` of
   * them, a permutation of the records. */
  order: Uint32Array
  /** [minX, minY, minZ, maxX, maxY, maxZ] of the splat centers. */
  bounds: Float32Array
  /** Spherical-harmonic bands baked beyond the base color (0..3). */
  shDegree: number
  /** How `sh` (and `shPalette`) hold the bands; "u8" at degree 0. */
  shEncoding: SplatShEncoding
  /** The per-splat SH block, in record order; null at degree 0. Under
   * "u8": count x SPLAT_SH_TEXELS[shDegree] texels of SPLAT_TEXEL_BYTES,
   * each splat's coefficients beyond DC as bytes, coefficient-major with
   * rgb interleaved (coefficient k of channel c at byte 3k + c),
   * zero-padded to its texels. Under "pq": SPLAT_SH_LABEL_BYTES[shDegree]
   * bytes per splat, the label of subspace j at byte j. */
  sh: Uint8Array | null
  /** Under "pq", the palette: SPLAT_SH_PALETTE_SIZE entries of
   * SPLAT_SH_LABEL_BYTES[shDegree] rgba32ui texels (entry e's subspace j
   * at texel e x subspaces + j), a texel holding the subspace's plane
   * values as bytes - a luma subspace its green coefficients in order, a
   * chroma subspace its red-minus-green coefficients then its
   * blue-minus-green ones - zero-padded. null otherwise. */
  shPalette: Uint8Array | null
  /** The capture was trained with the antialiasing compensation
   * (Mip-Splatting, nerfstudio's antialiased mode): the renderer scales
   * opacity by the dilation's determinant ratio. */
  antialiased: boolean
  /** What parseSplat's bake lost; not stored in the container. */
  report?: BakeReport
}

/** One record decoded: the reference the shader's decode must match. */
export type SplatRecord = {
  position: [number, number, number]
  /** Linear scales per axis. */
  scale: [number, number, number]
  /** Unit quaternion (w, x, y, z). */
  rotation: [number, number, number, number]
  /** sRGB color and opacity bytes. */
  color: [number, number, number, number]
}

export type SplatFormat = "ply" | "splat" | "spz"

export type ParseSplatOptions = {
  /** The input format; sniffed from the bytes when omitted (.ply and .spz
   * carry magic; anything else must be whole 32-byte .splat records). */
  format?: SplatFormat
  /** Keep the capture's own orientation. By default a y-down capture
   * (.ply and .splat are COLMAP-framed) is stood up to y-up at bake -
   * exact and free, positions, rotations and SH coefficients rotated
   * together - so a baked splat drops into a scene like any model; .spz
   * is y-up already and never flips. */
  keepOrientation?: boolean
  /** Spherical-harmonic bands to bake beyond the base color, 0..3,
   * capped at what the capture holds (.splat has none). Default 0: the
   * view-dependent bands cost texture (1/2/4 bytes per splat palette-coded,
   * 16/32/48 as bytes), so a bake opts in. */
  shDegree?: number
  /** How the bands are stored (SplatShEncoding). Default "pq": the
   * k-means palette, 12x less texture at degree 3 for a clustering error
   * the report states. "u8" keeps every coefficient as a byte - exact for
   * an spz, and the reference when judging a palette bake. */
  shEncoding?: SplatShEncoding
  /** The capture was trained antialiased (Mip-Splatting, nerfstudio's
   * `antialiased` rasterization); a per-capture fact the trainer knows
   * and the file carries. Default false (vanilla 3DGS). */
  antialiased?: boolean
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
// spz packs the DC band as u8 around 0.5 with this scale (Niantic's
// load-spz.cc colorScale): dc = (byte / 255 - 0.5) / SPZ_COLOR_SCALE.
const SPZ_COLOR_SCALE = 0.15
// spz packs log scales as u8 sixteenths offset by -10: exp(byte / 16 - 10).
const SPZ_SCALE_OFFSET = 10
const SPZ_SCALE_STEP = 16

/** "SOLS" read as a little-endian u32. */
const MAGIC = 0x534c4f53
// Version 3: the 16-byte chunk-quantized record and the chunk table in
// spatial order, the draw order as an id block, y-up unless the bake kept
// orientation, the SH block as bytes; the header carries count,
// chunkSize, shDegree, antialiased, bounds and the block ranges.
// (Version 2 was the 32-byte two-texel record with the covariance baked
// as halves and half-float SH; version 1 the 28-byte instance-attribute
// record of the first viewer.)
const VERSION = 3

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

// Log scales are clamped into this range before quantization: a trainer
// can drive a scale to zero (log of minus infinity), and nothing on screen
// is smaller than e^-12 or larger than e^10 scene units.
const SPLAT_LOG_SCALE_MIN = -12
const SPLAT_LOG_SCALE_MAX = 10

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
 * the inflated bytes - `sol tool 3d/splat` does) - into baked SplatData:
 * chunk-quantized records in spatial order, the draw order by importance
 * (size times opacity), y-up by default, bounds measured, the loss
 * reported. Runs anywhere (bun, the runtime); for a capture of real
 * size run it at pack time and ship encodeSplat's .sol3s.
 */
export function parseSplat(bytes: Uint8Array, opts?: ParseSplatOptions): SplatData {
  let format = opts?.format ?? sniffFormat(bytes)
  let degree = opts?.shDegree ?? 0
  if (!(Number.isInteger(degree) && degree >= 0 && degree <= 3)) throw new Error("parseSplat: shDegree must be an integer 0..3, got " + degree)
  let encoding = opts?.shEncoding ?? "pq"
  if (encoding !== "u8" && encoding !== "pq") throw new Error("parseSplat: shEncoding must be \"u8\" or \"pq\", got " + String(encoding))
  let raw = format === "ply" ? parsePly(bytes, degree) : format === "spz" ? parseSpz(bytes, degree) : parseDotSplat(bytes)
  if (opts?.keepOrientation === true) raw.flip = false
  return bake(raw, opts?.antialiased === true, encoding)
}

function sniffFormat(bytes: Uint8Array): SplatFormat {
  if (bytes.length >= 4 && bytes[0] === 0x70 && bytes[1] === 0x6c && bytes[2] === 0x79 && (bytes[3] === 0x0a || bytes[3] === 0x0d)) return "ply"
  if (bytes.length >= 4) {
    let dv = new DataView(bytes.buffer, bytes.byteOffset, 4)
    if (dv.getUint32(0, true) === SPZ_MAGIC) return "spz"
  }
  if (bytes.length >= 2 && bytes[0] === 0x1f && bytes[1] === 0x8b) {
    throw new Error("parseSplat: gzip-compressed input (.spz files are); gunzip the bytes first - sol tool 3d/splat does")
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
// A position component's unorm range.
const POSITION_MAX = (1 << SPLAT_POSITION_BITS) - 1
// The scale word's unorm range and shift per axis (x at the top).
const SCALE_MAX = SPLAT_SCALE_BITS.map(b => (1 << b) - 1)
const SCALE_SHIFT = [SPLAT_SCALE_BITS[1]! + SPLAT_SCALE_BITS[2]!, SPLAT_SCALE_BITS[2]!, 0]
// The stored quaternion components span an even number of steps, so the
// midpoint (a zero component: every axis-aligned splat) is a code of its
// own and decodes exactly; the one unused code costs a tenth of a percent
// of range.
const ROTATION_MAX = (1 << SPLAT_ROTATION_BITS) - 2
const ROTATION_MASK = (1 << SPLAT_ROTATION_BITS) - 1

// A log scale inside the quantizable range; NaN (a negative scale) and
// minus infinity (a zero scale) land on the floor.
let clampLog = (v: number): number => (v > SPLAT_LOG_SCALE_MAX ? SPLAT_LOG_SCALE_MAX : v >= SPLAT_LOG_SCALE_MIN ? v : SPLAT_LOG_SCALE_MIN)

// `v` as a unorm of `max` steps over [lo, hi]; a degenerate range (every
// record of the chunk at the same value) packs as 0 and decodes to lo.
function packUnorm(v: number, lo: number, hi: number, max: number): number {
  if (!(hi > lo)) return 0
  let q = Math.round(((v - lo) / (hi - lo)) * max)
  return q < 0 ? 0 : q > max ? max : q
}

// Three scale unorms into one word, x at the top.
let packScale = (q: number[]): number => ((q[0]! << SCALE_SHIFT[0]!) | (q[1]! << SCALE_SHIFT[1]!) | q[2]!) >>> 0

// Axis `k` of a packed scale word back to [lo, hi].
function unpackScale(word: number, k: number, lo: number, hi: number): number {
  let q = (word >>> SCALE_SHIFT[k]!) & SCALE_MAX[k]!
  return lo + (q / SCALE_MAX[k]!) * (hi - lo)
}

// A 16-bit position unorm back to [lo, hi].
let unpackPosition = (q: number, lo: number, hi: number): number => lo + (q / POSITION_MAX) * (hi - lo)

// A unit quaternion (w, x, y, z) as smallest-three: the largest
// component's index in the top two bits (made positive by negating the
// quaternion, which is the same rotation), the other three in index order
// at bits 20, 10, 0 as 10-bit unorms over [-1/sqrt2, 1/sqrt2] (ROTATION_MAX
// steps).
function packRotation(q: number[]): number {
  let largest = 0
  for (let k = 1; k < 4; k++) if (Math.abs(q[k]!) > Math.abs(q[largest]!)) largest = k
  let sign = q[largest]! < 0 ? -1 : 1
  let word = largest * 2 ** 30
  let shift = 2 * SPLAT_ROTATION_BITS
  for (let k = 0; k < 4; k++) {
    if (k === largest) continue
    let u = Math.round(((q[k]! * sign * Math.SQRT2 + 1) / 2) * ROTATION_MAX)
    word += (u < 0 ? 0 : u > ROTATION_MAX ? ROTATION_MAX : u) * 2 ** shift
    shift -= SPLAT_ROTATION_BITS
  }
  return word >>> 0
}

function unpackRotation(word: number): [number, number, number, number] {
  let q: [number, number, number, number] = [0, 0, 0, 0]
  let largest = word >>> 30
  let shift = 2 * SPLAT_ROTATION_BITS
  let sum = 0
  for (let k = 0; k < 4; k++) {
    if (k === largest) continue
    let u = (word >>> shift) & ROTATION_MASK
    let v = ((u / ROTATION_MAX) * 2 - 1) / Math.SQRT2
    q[k] = v
    sum += v * v
    shift -= SPLAT_ROTATION_BITS
  }
  q[largest] = Math.sqrt(Math.max(0, 1 - sum))
  return q
}

// The bake: stand a y-down capture up (positions (x, -y, -z); the
// quaternion composed with the half turn about x, q' = (-x, w, -z, y);
// the SH coefficients odd in (y, z) negated), partition the records
// spatially (kdOrder) into chunks of SPLAT_CHUNK_SIZE consecutive
// records and quantize each against its chunk, rank the records by
// importance (size times opacity, descending - the .splat convention)
// into the draw order, measure bounds, pack the SH bands (packSh).
function bake(g: Gaussians, antialiased: boolean, encoding: SplatShEncoding): SplatData {
  let n = g.count
  let D = SH_COEFFICIENTS[g.shDegree]!
  let sy = g.flip ? -1 : 1
  let pos = new Float32Array(n * 3)
  let logs = new Float32Array(n * 3)
  let quat = new Float32Array(n * 4)
  let importance = new Float32Array(n)
  for (let i = 0; i < n; i++) {
    // The + 0 folds a flip's -0 to 0, so the header JSON round-trips.
    pos[i * 3] = g.positions[i * 3]! + 0
    pos[i * 3 + 1] = g.positions[i * 3 + 1]! * sy + 0
    pos[i * 3 + 2] = g.positions[i * 3 + 2]! * sy + 0
    for (let k = 0; k < 3; k++) logs[i * 3 + k] = clampLog(Math.log(g.scales[i * 3 + k]!))
    importance[i] = g.scales[i * 3]! * g.scales[i * 3 + 1]! * g.scales[i * 3 + 2]! * g.colors[i * 4 + 3]!
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
    if (g.flip) {
      quat[i * 4] = -x
      quat[i * 4 + 1] = w
      quat[i * 4 + 2] = -z
      quat[i * 4 + 3] = y
    } else {
      quat[i * 4] = w
      quat[i * 4 + 1] = x
      quat[i * 4 + 2] = y
      quat[i * 4 + 3] = z
    }
  }

  // The storage order: order[slot] = the source splat at that record.
  let order = new Uint32Array(n)
  for (let i = 0; i < n; i++) order[i] = i
  kdOrder(order, 0, n, pos)
  // The draw order: record ids ranked by importance.
  let ranks = new Uint32Array(n)
  for (let i = 0; i < n; i++) ranks[i] = i
  ranks.sort((a, b) => importance[b]! - importance[a]!)
  let slotOf = new Uint32Array(n)
  for (let slot = 0; slot < n; slot++) slotOf[order[slot]!] = slot
  let drawOrder = new Uint32Array(n)
  for (let r = 0; r < n; r++) drawOrder[r] = slotOf[ranks[r]!]!

  let chunkCount = Math.ceil(n / SPLAT_CHUNK_SIZE)
  let records = new Uint8Array(n * SPLAT_RECORD_BYTES)
  let rdv = new DataView(records.buffer)
  let colors = new Uint8Array(n * SPLAT_COLOR_BYTES)
  let chunks = new Uint8Array(chunkCount * SPLAT_CHUNK_BYTES)
  let cdv = new DataView(chunks.buffer)
  // The coefficients beyond DC as clamped bytes, compact (3D per slot, in
  // record order), for packSh to lay out or cluster.
  let coeffs = D > 0 ? new Uint8Array(n * D * 3) : null
  let bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])
  let stepRatio = new Float32Array(n)
  let shClamped = 0
  let pmin = [0, 0, 0]
  let pmax = [0, 0, 0]
  let smin = [0, 0, 0]
  let smax = [0, 0, 0]
  let q = [0, 0, 0]
  let rotation = [0, 0, 0, 0]
  for (let c = 0; c < chunkCount; c++) {
    let lo = c * SPLAT_CHUNK_SIZE
    let hi = Math.min(n, lo + SPLAT_CHUNK_SIZE)
    for (let k = 0; k < 3; k++) {
      pmin[k] = Infinity
      pmax[k] = -Infinity
      smin[k] = Infinity
      smax[k] = -Infinity
    }
    for (let s = lo; s < hi; s++) {
      let i = order[s]!
      for (let k = 0; k < 3; k++) {
        let p = pos[i * 3 + k]!
        let l = logs[i * 3 + k]!
        if (p < pmin[k]!) pmin[k] = p
        if (p > pmax[k]!) pmax[k] = p
        if (l < smin[k]!) smin[k] = l
        if (l > smax[k]!) smax[k] = l
      }
    }
    let at = c * SPLAT_CHUNK_BYTES
    for (let k = 0; k < 3; k++) {
      cdv.setFloat32(at + k * 4, pmin[k]!, true)
      cdv.setFloat32(at + 12 + k * 4, pmax[k]!, true)
      cdv.setFloat32(at + 24 + k * 4, smin[k]!, true)
      cdv.setFloat32(at + 36 + k * 4, smax[k]!, true)
    }
    let step = 0
    for (let k = 0; k < 3; k++) step = Math.max(step, (pmax[k]! - pmin[k]!) / POSITION_MAX)
    for (let slot = lo; slot < hi; slot++) {
      let i = order[slot]!
      stepRatio[slot] = step / Math.exp(Math.max(logs[i * 3]!, logs[i * 3 + 1]!, logs[i * 3 + 2]!))
      for (let k = 0; k < 3; k++) {
        let p = pos[i * 3 + k]!
        if (p < bounds[k]!) bounds[k] = p
        if (p > bounds[k + 3]!) bounds[k + 3] = p
        q[k] = packUnorm(p, pmin[k]!, pmax[k]!, POSITION_MAX)
      }
      let rat = slot * SPLAT_RECORD_BYTES
      rdv.setUint32(rat, (q[0]! | (q[1]! << SPLAT_POSITION_BITS)) >>> 0, true)
      rdv.setUint32(rat + 4, q[2]!, true)
      for (let k = 0; k < 4; k++) rotation[k] = quat[i * 4 + k]!
      rdv.setUint32(rat + 8, packRotation(rotation), true)
      for (let k = 0; k < 3; k++) q[k] = packUnorm(logs[i * 3 + k]!, smin[k]!, smax[k]!, SCALE_MAX[k]!)
      rdv.setUint32(rat + 12, packScale(q), true)
      for (let k = 0; k < 4; k++) colors[slot * SPLAT_COLOR_BYTES + k] = g.colors[i * 4 + k]!
      if (coeffs !== null) {
        let base = slot * D * 3
        for (let k = 0; k < D; k++) {
          let sign = g.flip ? SH_FLIP_SIGN[k]! : 1
          for (let ch = 0; ch < 3; ch++) {
            let b = Math.round(g.sh![(i * D + k) * 3 + ch]! * sign * SPLAT_SH_BYTE_SCALE + SPLAT_SH_BYTE_ZERO)
            if (b < 0 || b > 255) {
              shClamped++
              b = b < 0 ? 0 : 255
            }
            coeffs[base + k * 3 + ch] = b
          }
        }
      }
    }
  }
  if (n === 0) bounds.fill(0)
  let visible = 0
  for (let slot = 0; slot < n; slot++) if (stepRatio[slot]! > SPLAT_STEP_VISIBLE) visible++
  stepRatio.sort()
  let at = (p: number) => (n === 0 ? 0 : stepRatio[Math.min(n - 1, Math.floor(p * n))]!)
  let report: BakeReport = { chunks: chunkCount, positionStep: { median: at(0.5), p99: at(0.99), visibleShare: n === 0 ? 0 : visible / n }, shClamped }
  let packed = packSh(coeffs, n, g.shDegree, encoding)
  if (packed.report !== undefined) report.shPalette = packed.report
  return {
    count: n,
    records,
    colors,
    chunks,
    order: drawOrder,
    bounds,
    shDegree: g.shDegree,
    shEncoding: D > 0 ? encoding : "u8",
    sh: packed.sh,
    shPalette: packed.palette,
    antialiased,
    report,
  }
}

// The pq bake's tuning. A codebook is seeded (k-means++) and iterated
// (Lloyd) on a random sample of this many vectors - 256 per centroid at
// SPLAT_SH_PALETTE_SIZE, enough for the centroids to settle - then
// iterated over the whole cloud until it converges; a pass stops when
// fewer than this share of vectors change label.
const SPLAT_PQ_SAMPLE = 65536
const SPLAT_PQ_SAMPLE_ITERATIONS = 32
const SPLAT_PQ_FULL_ITERATIONS = 8
const SPLAT_PQ_CONVERGED = 0.001
// The seed of the bake's generator: a bake is a function of its input.
const SPLAT_PQ_SEED = 0x53482d53

// A deterministic generator (a 32-bit LCG) in [0, 1).
function lcg(seed: number): () => number {
  let s = seed >>> 0
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0
    return s / 4294967296
  }
}

// A clamped coefficient byte to its value, and back.
let shByteValue = (b: number): number => (b - SPLAT_SH_BYTE_ZERO) / SPLAT_SH_BYTE_SCALE
let shValueByte = (v: number): number => Math.min(255, Math.max(0, Math.round(v * SPLAT_SH_BYTE_SCALE + SPLAT_SH_BYTE_ZERO)))

// Lay the compact coefficient bytes (3D per slot) out as the SH block of
// `encoding`: under u8 the per-splat texels; under pq a codebook per
// subspace trained on the cloud (trainCodebook), the labels per splat and
// the palette texels, with the clustering error for the report.
function packSh(
  coeffs: Uint8Array | null,
  n: number,
  degree: number,
  encoding: SplatShEncoding,
): { sh: Uint8Array | null; palette: Uint8Array | null; report?: BakeReport["shPalette"] } {
  if (coeffs === null) return { sh: null, palette: null }
  let D = SH_COEFFICIENTS[degree]!
  if (encoding === "u8") {
    let shBytes = SPLAT_SH_TEXELS[degree]! * SPLAT_TEXEL_BYTES
    let sh = new Uint8Array(n * shBytes)
    for (let slot = 0; slot < n; slot++) sh.set(coeffs.subarray(slot * D * 3, (slot + 1) * D * 3), slot * shBytes)
    return { sh, palette: null }
  }
  let subspaces = SPLAT_SH_LABEL_BYTES[degree]!
  let sh = new Uint8Array(n * subspaces)
  let palette = new Uint8Array(SPLAT_SH_PALETTE_SIZE * subspaces * SPLAT_TEXEL_BYTES)
  let rms: number[] = []
  let p99: number[] = []
  let clamped = 0
  let rand = lcg(SPLAT_PQ_SEED)
  for (let j = 0; j < subspaces; j++) {
    let { plane, first, count } = SPLAT_SH_SUBSPACES[j]!
    let dims = subspaceDims(SPLAT_SH_SUBSPACES[j]!)
    // The subspace's vectors, one row per slot: the green bytes, or the
    // red and blue differences from green clamped into the byte range.
    let vectors = new Float32Array(n * dims)
    for (let slot = 0; slot < n; slot++) {
      let base = slot * D * 3 + first * 3
      for (let k = 0; k < count; k++) {
        let g = coeffs[base + k * 3 + 1]!
        if (plane === "luma") {
          vectors[slot * dims + k] = shByteValue(g)
          continue
        }
        for (let c = 0; c < 2; c++) {
          let diff = coeffs[base + k * 3 + (c === 0 ? 0 : 2)]! - g + SPLAT_SH_BYTE_ZERO
          if (diff < 0 || diff > 255) {
            clamped++
            diff = diff < 0 ? 0 : 255
          }
          vectors[slot * dims + c * count + k] = shByteValue(diff)
        }
      }
    }
    let { codebook, labels, maxError, sse } = trainCodebook(vectors, n, dims, rand)
    for (let slot = 0; slot < n; slot++) sh[slot * subspaces + j] = labels[slot]!
    for (let e = 0; e < SPLAT_SH_PALETTE_SIZE; e++) {
      let at = (e * subspaces + j) * SPLAT_TEXEL_BYTES
      for (let d = 0; d < dims; d++) palette[at + d] = shValueByte(codebook[e * dims + d]!)
    }
    rms.push(n === 0 ? 0 : Math.sqrt(sse / (n * dims)) * SPLAT_SH_BYTE_SCALE)
    maxError.sort()
    p99.push(n === 0 ? 0 : maxError[Math.min(n - 1, Math.floor(0.99 * n))]! * SPLAT_SH_BYTE_SCALE)
  }
  return { sh, palette, report: { rms, p99, clamped } }
}

// A subspace's vector length: its coefficients for a luma plane, twice
// them for the chroma plane's two differences.
let subspaceDims = (sub: SplatShSubspace): number => (sub.plane === "luma" ? sub.count : 2 * sub.count)

// Cluster `n` vectors of `dims` into SPLAT_SH_PALETTE_SIZE centroids:
// k-means++ seeding and Lloyd iterations on a sample (the whole cloud when
// it is no bigger), then Lloyd over the whole cloud to convergence, the
// centroids rounded to the byte domain they are stored in, and a last
// assignment against the stored centroids - so `labels`, `maxError` (per
// vector, the largest coefficient error) and `sse` describe exactly what
// the palette will decode to.
function trainCodebook(
  vectors: Float32Array,
  n: number,
  dims: number,
  rand: () => number,
): { codebook: Float32Array; labels: Uint8Array; maxError: Float32Array; sse: number } {
  let K = SPLAT_SH_PALETTE_SIZE
  let codebook = new Float32Array(K * dims)
  let labels = new Uint8Array(n)
  let maxError = new Float32Array(n)
  if (n === 0) return { codebook, labels, maxError, sse: 0 }
  let sampleCount = Math.min(n, SPLAT_PQ_SAMPLE)
  let sample = vectors
  if (sampleCount < n) {
    sample = new Float32Array(sampleCount * dims)
    for (let s = 0; s < sampleCount; s++) {
      let i = Math.floor(rand() * n)
      sample.set(vectors.subarray(i * dims, (i + 1) * dims), s * dims)
    }
  }
  seedCodebook(sample, sampleCount, dims, codebook, rand)
  let sampleLabels = new Uint8Array(sampleCount)
  lloyd(sample, sampleCount, dims, codebook, sampleLabels, SPLAT_PQ_SAMPLE_ITERATIONS)
  if (sampleCount < n) lloyd(vectors, n, dims, codebook, labels, SPLAT_PQ_FULL_ITERATIONS)
  for (let c = 0; c < K * dims; c++) codebook[c] = shByteValue(shValueByte(codebook[c]!))
  let sse = 0
  for (let i = 0; i < n; i++) {
    let k = nearest(vectors, i, dims, codebook)
    labels[i] = k
    let worst = 0
    for (let d = 0; d < dims; d++) {
      let e = vectors[i * dims + d]! - codebook[k * dims + d]!
      sse += e * e
      if (Math.abs(e) > worst) worst = Math.abs(e)
    }
    maxError[i] = worst
  }
  return { codebook, labels, maxError, sse }
}

// k-means++: the first centroid a random vector, each next one drawn with
// probability proportional to its squared distance from the nearest
// centroid so far. A cloud with fewer distinct vectors than centroids runs
// out of distance: the remaining centroids stay zero and attract nothing.
function seedCodebook(vectors: Float32Array, n: number, dims: number, codebook: Float32Array, rand: () => number): void {
  let K = SPLAT_SH_PALETTE_SIZE
  let minDist = new Float32Array(n).fill(Infinity)
  let pick = Math.floor(rand() * n)
  codebook.set(vectors.subarray(pick * dims, (pick + 1) * dims), 0)
  for (let k = 1; k < K; k++) {
    let total = 0
    let prev = (k - 1) * dims
    for (let i = 0; i < n; i++) {
      let d = 0
      for (let c = 0; c < dims; c++) {
        let e = vectors[i * dims + c]! - codebook[prev + c]!
        d += e * e
      }
      if (d < minDist[i]!) minDist[i] = d
      total += minDist[i]!
    }
    if (total === 0) return
    let r = rand() * total
    let chosen = n - 1
    for (let i = 0; i < n; i++) {
      r -= minDist[i]!
      if (r <= 0) {
        chosen = i
        break
      }
    }
    codebook.set(vectors.subarray(chosen * dims, (chosen + 1) * dims), k * dims)
  }
}

// Lloyd iterations over `vectors`: assign each to its nearest centroid,
// move each centroid to its members' mean, until fewer than
// SPLAT_PQ_CONVERGED of the vectors change label or `maxIterations` pass.
// A centroid left without members takes over the vector farthest from its
// own centroid among the clusters not yet raided this iteration (the
// cluster that spreads most splits; an empty code is a wasted byte value).
function lloyd(vectors: Float32Array, n: number, dims: number, codebook: Float32Array, labels: Uint8Array, maxIterations: number): void {
  let K = SPLAT_SH_PALETTE_SIZE
  let sums = new Float64Array(K * dims)
  let counts = new Uint32Array(K)
  let farthestDist = new Float32Array(K)
  let farthestIndex = new Uint32Array(K)
  labels.fill(255)
  for (let iteration = 0; iteration < maxIterations; iteration++) {
    sums.fill(0)
    counts.fill(0)
    farthestDist.fill(0)
    let changed = 0
    for (let i = 0; i < n; i++) {
      let k = nearest(vectors, i, dims, codebook)
      if (labels[i] !== k) {
        labels[i] = k
        changed++
      }
      counts[k]!++
      let d = 0
      for (let c = 0; c < dims; c++) {
        let v = vectors[i * dims + c]!
        sums[k * dims + c]! += v
        let e = v - codebook[k * dims + c]!
        d += e * e
      }
      if (d > farthestDist[k]!) {
        farthestDist[k] = d
        farthestIndex[k] = i
      }
    }
    for (let k = 0; k < K; k++) {
      if (counts[k]! > 0) {
        for (let c = 0; c < dims; c++) codebook[k * dims + c] = sums[k * dims + c]! / counts[k]!
        continue
      }
      let source = 0
      for (let s = 1; s < K; s++) if (farthestDist[s]! > farthestDist[source]!) source = s
      if (farthestDist[source]! === 0) {
        // Every vector sits on its centroid: fewer distinct vectors than
        // codes. Nothing to split; leave the code unused.
        continue
      }
      let i = farthestIndex[source]!
      codebook.set(vectors.subarray(i * dims, (i + 1) * dims), k * dims)
      farthestDist[source] = 0
      changed++
    }
    if (changed < SPLAT_PQ_CONVERGED * n) return
  }
}

// The nearest centroid of vector `i`, by squared distance with an early
// exit once a partial sum passes the best so far.
function nearest(vectors: Float32Array, i: number, dims: number, codebook: Float32Array): number {
  let K = SPLAT_SH_PALETTE_SIZE
  let base = i * dims
  let best = 0
  let bestDist = Infinity
  for (let k = 0; k < K; k++) {
    let at = k * dims
    let d = 0
    for (let c = 0; c < dims && d < bestDist; c++) {
      let e = vectors[base + c]! - codebook[at + c]!
      d += e * e
    }
    if (d < bestDist) {
      bestDist = d
      best = k
    }
  }
  return best
}

// Partition the records at order[lo..hi) spatially for chunking, in
// place: a k-d split at the median of the longest axis, the left size
// snapped to a whole number of chunks, down to leaves of one chunk, so
// each chunk is a compact box (no Morton seams). The remainder below a
// chunk is always the rightmost leaf, the cloud's last partial chunk.
function kdOrder(order: Uint32Array, lo: number, hi: number, pos: Float32Array): void {
  let n = hi - lo
  if (n <= SPLAT_CHUNK_SIZE) return
  let left = Math.max(SPLAT_CHUNK_SIZE, Math.floor((n + SPLAT_CHUNK_SIZE) / (2 * SPLAT_CHUNK_SIZE)) * SPLAT_CHUNK_SIZE)
  selectAxis(order, lo, hi, lo + left - 1, longestAxis(order, lo, hi, pos), pos)
  kdOrder(order, lo, lo + left, pos)
  kdOrder(order, lo + left, hi, pos)
}

// The axis with the largest extent over the records at order[lo..hi).
function longestAxis(order: Uint32Array, lo: number, hi: number, pos: Float32Array): number {
  let min = [Infinity, Infinity, Infinity]
  let max = [-Infinity, -Infinity, -Infinity]
  for (let s = lo; s < hi; s++) {
    let i = order[s]! * 3
    for (let k = 0; k < 3; k++) {
      let p = pos[i + k]!
      if (p < min[k]!) min[k] = p
      if (p > max[k]!) max[k] = p
    }
  }
  let axis = 0
  for (let k = 1; k < 3; k++) if (max[k]! - min[k]! > max[axis]! - min[axis]!) axis = k
  return axis
}

// Quickselect over order[lo..hi) on `axis`: afterwards the record at
// `nth` is where a full sort would put it, everything before it no
// larger, everything after no smaller (Hoare partitions, in place).
function selectAxis(order: Uint32Array, lo: number, hi: number, nth: number, axis: number, pos: Float32Array): void {
  let key = (s: number) => pos[order[s]! * 3 + axis]!
  let left = lo
  let right = hi - 1
  while (right > left) {
    let mid = (left + right) >> 1
    let a = key(left)
    let b = key(mid)
    let c = key(right)
    let pivot = a < b ? (b < c ? b : a < c ? c : a) : a < c ? a : b < c ? c : b
    let i = left
    let j = right
    while (i <= j) {
      while (key(i) < pivot) i++
      while (key(j) > pivot) j--
      if (i <= j) {
        let t = order[i]!
        order[i] = order[j]!
        order[j] = t
        i++
        j--
      }
    }
    if (nth <= j) right = j
    else if (nth >= i) left = i
    else return
  }
}

/**
 * Decode record `slot` of a cloud against its chunk: the reference the
 * shader's decode agrees with, for tests, tools and any app reading a
 * bake back (per-record work, so not for whole clouds at runtime).
 */
export function decodeRecord(data: SplatData, slot: number): SplatRecord {
  if (!(Number.isInteger(slot) && slot >= 0 && slot < data.count)) throw new Error("decodeRecord: slot " + slot + " is outside the cloud's " + data.count + " records")
  let rdv = new DataView(data.records.buffer, data.records.byteOffset + slot * SPLAT_RECORD_BYTES, SPLAT_RECORD_BYTES)
  let cdv = new DataView(data.chunks.buffer, data.chunks.byteOffset + Math.floor(slot / SPLAT_CHUNK_SIZE) * SPLAT_CHUNK_BYTES, SPLAT_CHUNK_BYTES)
  let chunk = (f: number) => cdv.getFloat32(f * 4, true)
  let xy = rdv.getUint32(0, true)
  let z = rdv.getUint32(4, true) & POSITION_MAX
  let scale = rdv.getUint32(12, true)
  let at = slot * SPLAT_COLOR_BYTES
  return {
    position: [unpackPosition(xy & POSITION_MAX, chunk(0), chunk(3)), unpackPosition(xy >>> SPLAT_POSITION_BITS, chunk(1), chunk(4)), unpackPosition(z, chunk(2), chunk(5))],
    scale: [Math.exp(unpackScale(scale, 0, chunk(6), chunk(9))), Math.exp(unpackScale(scale, 1, chunk(7), chunk(10))), Math.exp(unpackScale(scale, 2, chunk(8), chunk(11)))],
    rotation: unpackRotation(rdv.getUint32(8, true)),
    color: [data.colors[at]!, data.colors[at + 1]!, data.colors[at + 2]!, data.colors[at + 3]!],
  }
}

/**
 * Decode record `slot`'s SH coefficients beyond DC (coefficient-major,
 * rgb interleaved: coefficient k of channel c at 3k + c), 3 x ((d + 1)^2
 * - 1) of them; empty at degree 0.
 */
export function decodeSh(data: SplatData, slot: number): Float32Array {
  if (!(Number.isInteger(slot) && slot >= 0 && slot < data.count)) throw new Error("decodeSh: slot " + slot + " is outside the cloud's " + data.count + " records")
  let D = SH_COEFFICIENTS[data.shDegree]!
  let out = new Float32Array(D * 3)
  if (data.sh === null) return out
  if (data.shEncoding === "u8") {
    let base = slot * SPLAT_SH_TEXELS[data.shDegree]! * SPLAT_TEXEL_BYTES
    for (let j = 0; j < D * 3; j++) out[j] = shByteValue(data.sh[base + j]!)
    return out
  }
  if (data.shPalette === null) throw new Error("decodeSh: a pq-encoded cloud without its palette")
  let subspaces = SPLAT_SH_LABEL_BYTES[data.shDegree]!
  // Luma first (green), then the chroma differences added to it.
  for (let pass of ["luma", "chroma"] as const) {
    for (let j = 0; j < subspaces; j++) {
      let { plane, first, count } = SPLAT_SH_SUBSPACES[j]!
      if (plane !== pass) continue
      let at = (data.sh[slot * subspaces + j]! * subspaces + j) * SPLAT_TEXEL_BYTES
      for (let k = 0; k < count; k++) {
        let g = (first + k) * 3 + 1
        if (plane === "luma") {
          out[g] = shByteValue(data.shPalette[at + k]!)
        } else {
          out[g - 1] = out[g]! + shByteValue(data.shPalette[at + k]!)
          out[g + 1] = out[g]! + shByteValue(data.shPalette[at + count + k]!)
        }
      }
    }
  }
  return out
}

/**
 * The chunk table's position boxes as the engine's packed key takes them
 * (`transferRecords`' `packed.bounds`): six floats per chunk, min xyz
 * then max xyz, in chunk order - the first half of each chunk record.
 */
export function splatChunkBounds(data: SplatData): Float32Array {
  let chunks = data.chunks.byteLength / SPLAT_CHUNK_BYTES
  let out = new Float32Array(chunks * 6)
  let dv = new DataView(data.chunks.buffer, data.chunks.byteOffset, data.chunks.byteLength)
  for (let c = 0; c < chunks; c++) {
    for (let f = 0; f < 6; f++) out[c * 6 + f] = dv.getFloat32(c * SPLAT_CHUNK_BYTES + f * 4, true)
  }
  return out
}

type Block = { offset: number; bytes: number }

type Header = {
  count: number
  chunkSize: number
  shDegree: number
  antialiased: boolean
  bounds: number[]
  records: Block
  colors: Block
  chunks: Block
  order: Block
  sh?: Block & { encoding: SplatShEncoding; palette?: Block }
}

// The per-splat SH block's byte length for a cloud under `encoding`: the
// coefficient texels or the label bytes per splat times the count; 0 at
// degree 0.
function shBytesOf(count: number, shDegree: number, encoding: SplatShEncoding): number {
  if (encoding === "pq") return count * SPLAT_SH_LABEL_BYTES[shDegree]!
  return count * SPLAT_SH_TEXELS[shDegree]! * SPLAT_TEXEL_BYTES
}

// The palette block's byte length: the codebook texels under pq, 0 under u8
// or at degree 0.
function paletteBytesOf(shDegree: number, encoding: SplatShEncoding): number {
  if (encoding !== "pq" || shDegree === 0) return 0
  return SPLAT_SH_PALETTE_SIZE * SPLAT_SH_LABEL_BYTES[shDegree]! * SPLAT_TEXEL_BYTES
}

// The chunk table's byte length for a cloud.
function chunkBytesOf(count: number): number {
  return Math.ceil(count / SPLAT_CHUNK_SIZE) * SPLAT_CHUNK_BYTES
}

// The shape checks encode and decode share, naming `site`.
function checkShape(site: string, count: number, shDegree: number, encoding: SplatShEncoding, bounds: ArrayLike<number>, records: number, colors: number, chunks: number, order: number, sh: number, palette: number): void {
  if (!(Number.isInteger(count) && count >= 0)) throw new Error(site + ": count must be a non-negative integer, got " + count)
  if (records !== count * SPLAT_RECORD_BYTES) throw new Error(site + ": the record block holds " + records + " bytes, not " + count + " whole splat records")
  if (colors !== count * SPLAT_COLOR_BYTES) throw new Error(site + ": the color block holds " + colors + " bytes, not " + count + " rgba8 colors")
  if (chunks !== chunkBytesOf(count)) throw new Error(site + ": the chunk table holds " + chunks + " bytes; " + count + " splats need " + chunkBytesOf(count))
  if (order !== count * 4) throw new Error(site + ": the order block holds " + order + " bytes, not " + count + " record ids")
  if (bounds.length !== 6) throw new Error(site + ": bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
  if (!(Number.isInteger(shDegree) && shDegree >= 0 && shDegree <= 3)) throw new Error(site + ": shDegree must be 0..3, got " + shDegree)
  if (encoding !== "u8" && encoding !== "pq") throw new Error(site + ": SH encoding '" + String(encoding) + "' is not supported (u8, pq)")
  if (sh !== shBytesOf(count, shDegree, encoding)) throw new Error(site + ": the SH block holds " + sh + " bytes; degree " + shDegree + " as " + encoding + " over " + count + " splats needs " + shBytesOf(count, shDegree, encoding))
  if (palette !== paletteBytesOf(shDegree, encoding)) throw new Error(site + ": the SH palette holds " + palette + " bytes; degree " + shDegree + " as " + encoding + " needs " + paletteBytesOf(shDegree, encoding))
}

/**
 * Serialize a splat cloud into the .sol3s container, all little-endian:
 * "SOLS" u32 | version u32 | jsonLength u32 | json (padded to 4) |
 * payload. The JSON header carries count, chunkSize, shDegree,
 * antialiased, bounds and the byte range of the record block, the color
 * block, the chunk table, the order block and (above degree 0) the SH
 * block with its encoding and, under pq, its palette block; the payload is
 * the blocks verbatim, each starting 4-aligned.
 */
export function encodeSplat(data: SplatData): Uint8Array {
  checkShape("encodeSplat", data.count, data.shDegree, data.shEncoding, data.bounds, data.records.byteLength, data.colors.byteLength, data.chunks.byteLength, data.order.byteLength, data.sh?.byteLength ?? 0, data.shPalette?.byteLength ?? 0)
  let shBytes = shBytesOf(data.count, data.shDegree, data.shEncoding)
  let paletteBytes = paletteBytesOf(data.shDegree, data.shEncoding)
  // Blocks start 4-aligned (decodeSplat views them as typed arrays); a
  // byte-sized block (the pq labels) is followed by zero padding.
  let at = 0
  let next = (bytes: number): Block => {
    at += (4 - (at % 4)) % 4
    let block = { offset: at, bytes }
    at += bytes
    return block
  }
  let header: Header = {
    count: data.count,
    chunkSize: SPLAT_CHUNK_SIZE,
    shDegree: data.shDegree,
    antialiased: data.antialiased === true,
    bounds: Array.from(data.bounds),
    records: next(data.records.byteLength),
    colors: next(data.colors.byteLength),
    chunks: next(data.chunks.byteLength),
    order: next(data.order.byteLength),
  }
  if (shBytes > 0) {
    header.sh = { ...next(shBytes), encoding: data.shEncoding }
    if (paletteBytes > 0) header.sh.palette = next(paletteBytes)
  }
  let json = new TextEncoder().encode(JSON.stringify(header))
  let jsonPadded = json.byteLength + ((4 - (json.byteLength % 4)) % 4)
  let out = new Uint8Array(12 + jsonPadded + at)
  let dv = new DataView(out.buffer)
  dv.setUint32(0, MAGIC, true)
  dv.setUint32(4, VERSION, true)
  dv.setUint32(8, json.byteLength, true)
  out.set(json, 12)
  let payload = 12 + jsonPadded
  out.set(data.records, payload + header.records.offset)
  out.set(data.colors, payload + header.colors.offset)
  out.set(data.chunks, payload + header.chunks.offset)
  out.set(new Uint8Array(data.order.buffer, data.order.byteOffset, data.order.byteLength), payload + header.order.offset)
  if (data.sh !== null && shBytes > 0) out.set(data.sh, payload + header.sh!.offset)
  if (data.shPalette !== null && paletteBytes > 0) out.set(data.shPalette, payload + header.sh!.palette!.offset)
  return out
}

/**
 * Read a .sol3s container back into SplatData. The blocks are VIEWS onto
 * `bytes` (copied once only when the input is not 4-aligned), so the
 * bytes must outlive the data - which is the point: loading a bake is a
 * header parse, and the views upload as-is.
 */
export function decodeSplat(bytes: Uint8Array): SplatData {
  if (bytes.byteOffset % 4 !== 0) bytes = new Uint8Array(bytes)
  if (bytes.byteLength < 12) throw new Error("decodeSplat: not a splat file (too short)")
  let dv = new DataView(bytes.buffer, bytes.byteOffset, 12)
  if (dv.getUint32(0, true) !== MAGIC) throw new Error("decodeSplat: not a splat file (bad magic)")
  let version = dv.getUint32(4, true)
  if (version !== VERSION) throw new Error("decodeSplat: version " + version + ", expected " + VERSION + " - re-bake with `sol tool 3d/splat`")
  let jsonLength = dv.getUint32(8, true)
  if (bytes.byteLength < 12 + jsonLength) throw new Error("decodeSplat: shorter than its header")
  let header: Header = JSON.parse(new TextDecoder().decode(bytes.subarray(12, 12 + jsonLength)))
  let payload = 12 + jsonLength + ((4 - (jsonLength % 4)) % 4)
  if (header.chunkSize !== SPLAT_CHUNK_SIZE) throw new Error("decodeSplat: chunk size " + header.chunkSize + ", expected " + SPLAT_CHUNK_SIZE)
  let encoding: SplatShEncoding = header.sh?.encoding ?? "u8"
  checkShape("decodeSplat", header.count, header.shDegree, encoding, header.bounds, header.records?.bytes ?? -1, header.colors?.bytes ?? -1, header.chunks?.bytes ?? -1, header.order?.bytes ?? -1, header.sh?.bytes ?? 0, header.sh?.palette?.bytes ?? 0)
  let block = (b: Block, name: string): Uint8Array => {
    if (!(Number.isInteger(b.offset) && b.offset >= 0 && b.offset % 4 === 0) || bytes.byteLength < payload + b.offset + b.bytes) throw new Error("decodeSplat: shorter than its " + name + " block (or the block is not 4-aligned)")
    return bytes.subarray(payload + b.offset, payload + b.offset + b.bytes)
  }
  let order = block(header.order, "order")
  let sh: Uint8Array | null = null
  let shPalette: Uint8Array | null = null
  if (header.sh !== undefined && header.sh.bytes > 0) {
    sh = block(header.sh, "SH")
    if (header.sh.palette !== undefined) shPalette = block(header.sh.palette, "SH palette")
  }
  return {
    count: header.count,
    records: block(header.records, "record"),
    colors: block(header.colors, "color"),
    chunks: block(header.chunks, "chunk"),
    order: new Uint32Array(order.buffer, order.byteOffset, header.count),
    bounds: Float32Array.from(header.bounds),
    shDegree: header.shDegree,
    shEncoding: header.shDegree > 0 ? encoding : "u8",
    sh,
    shPalette,
    antialiased: header.antialiased === true,
  }
}
