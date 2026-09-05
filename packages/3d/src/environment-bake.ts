// Environment baking: the pure pipeline behind `srt tool 3d/environment`
// and its tests. A Radiance .hdr panorama is decoded to linear float, laid
// onto the six faces of a cube, convolved level by level with the GGX lobe
// `standard`'s split sum samples (Karis's importance sampling, the source
// lod picked from each sample's solid angle - the Filament and Unity
// method; Godot and Three convolve the same way at runtime), and packed
// into the .srte container loadEnvironment uploads as an explicit rgba16f
// chain. Runs under bun and on the runtime alike: no GPU, no flux imports.
//
// Cube faces hold what a GL lookup of a world direction returns (GL's own
// convention, no library flip): cubeDirection is the texel-to-direction
// table and cubeLookup its inverse.

/** Face edge of the mip level a roughness of 1 samples (ENVIRONMENT in
 * glsl.ts maps roughness onto the chain by it): the chain is convolved
 * down to this edge, and the levels below repeat roughness 1 - never
 * read by a material, present because a chain must reach 1x1. */
export const ENV_ROUGH_FACE = 4
// GGX samples per texel of a convolved level.
const PREFILTER_SAMPLES = 512
// Bias on the source lod a sample reads (Karis: +1 smooths the estimate
// without visible extra blur).
const SOURCE_LOD_BIAS = 1
// A sample's normal-lobe cosine below this contributes nothing (the lobe
// mirrored below the horizon).
const MIN_NOL = 1e-6
// The mip level count of a chain from `size` down to 1x1 (GL's rule).
export function mipLevels(size: number): number {
  return 32 - Math.clz32(size)
}

/** A decoded panorama: linear rgb floats, row-major from the top row. */
export type Panorama = { width: number; height: number; data: Float32Array }
/** Six rgba float faces in GL order (+X, -X, +Y, -Y, +Z, -Z). */
export type CubeFaces = Float32Array[]

// The Radiance format tag a .hdr carries (or omits, meaning the same).
const RGBE_FORMAT = "32-bit_rle_rgbe"
// New-style RLE scanlines exist only in this width range (the format's rule).
const RLE_MIN_WIDTH = 8
const RLE_MAX_WIDTH = 0x7fff
// A run byte above this is a repeat of the next byte (count - 128 times).
const RUN_FLAG = 128
// The RGBE exponent bias: a mantissa byte scales by 2^(e - 128) / 255.
const RGBE_EXPONENT_BIAS = 128

/**
 * Decode a Radiance .hdr (RGBE) file into linear rgb floats: the header
 * (`#?` signature, `FORMAT=32-bit_rle_rgbe`), the standard `-Y h +X w`
 * orientation, flat or new-style run-length scanlines - what every HDRI
 * source ships (Three's RGBELoader reads the same subset). Throws on a
 * truncated file, another orientation or the pre-1990 old-style RLE.
 */
export function decodeHdr(bytes: Uint8Array): Panorama {
  let pos = 0
  let readLine = (): string => {
    let s = ""
    while (pos < bytes.length) {
      let c = bytes[pos++]!
      if (c === 0x0a) break
      s += String.fromCharCode(c)
    }
    return s
  }
  if (!readLine().startsWith("#?")) throw new Error("decodeHdr: not a Radiance .hdr file (no #? signature)")
  let format = RGBE_FORMAT
  for (;;) {
    if (pos >= bytes.length) throw new Error("decodeHdr: truncated header")
    let line = readLine()
    if (line === "") break
    if (line.startsWith("FORMAT=")) format = line.slice("FORMAT=".length)
  }
  if (format !== RGBE_FORMAT) throw new Error("decodeHdr: unsupported FORMAT " + format + " (only " + RGBE_FORMAT + ")")
  let res = readLine().trim().split(/\s+/)
  if (res.length !== 4 || res[0] !== "-Y" || res[2] !== "+X") {
    throw new Error("decodeHdr: unsupported orientation " + res.join(" ") + " (only -Y <height> +X <width>)")
  }
  let height = Number(res[1])
  let width = Number(res[3])
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1) {
    throw new Error("decodeHdr: bad resolution " + res.join(" "))
  }
  let data = new Float32Array(width * height * 3)
  let rgbe = new Uint8Array(width * 4)
  let rle = width >= RLE_MIN_WIDTH && width <= RLE_MAX_WIDTH
  let byte = (): number => {
    if (pos >= bytes.length) throw new Error("decodeHdr: truncated pixel data")
    return bytes[pos++]!
  }
  for (let y = 0; y < height; y++) {
    if (rle && bytes[pos] === 2 && bytes[pos + 1] === 2 && ((bytes[pos + 2]! << 8) | bytes[pos + 3]!) === width) {
      pos += 4
      for (let ch = 0; ch < 4; ch++) {
        let x = 0
        while (x < width) {
          let count = byte()
          if (count > RUN_FLAG) {
            count -= RUN_FLAG
            if (x + count > width) throw new Error("decodeHdr: run overflows scanline " + y)
            let v = byte()
            while (count-- > 0) rgbe[x++ * 4 + ch] = v
          } else {
            if (count === 0 || x + count > width) throw new Error("decodeHdr: run overflows scanline " + y)
            while (count-- > 0) rgbe[x++ * 4 + ch] = byte()
          }
        }
      }
    } else {
      for (let x = 0; x < width; x++) {
        let r = byte()
        let g = byte()
        let b = byte()
        let e = byte()
        if (r === 1 && g === 1 && b === 1) throw new Error("decodeHdr: old-style RLE is not supported")
        rgbe[x * 4] = r
        rgbe[x * 4 + 1] = g
        rgbe[x * 4 + 2] = b
        rgbe[x * 4 + 3] = e
      }
    }
    let row = y * width * 3
    for (let x = 0; x < width; x++) {
      let e = rgbe[x * 4 + 3]!
      let scale = e === 0 ? 0 : Math.pow(2, e - RGBE_EXPONENT_BIAS) / 255
      data[row + x * 3] = rgbe[x * 4]! * scale
      data[row + x * 3 + 1] = rgbe[x * 4 + 1]! * scale
      data[row + x * 3 + 2] = rgbe[x * 4 + 2]! * scale
    }
  }
  return { width, height, data }
}

/** The world direction a lookup must have to land on face `face` at
 * (s, t) in 0..1 (t = 0 the first row): the GL cube-map table, written
 * into `out` (unnormalized). */
export function cubeDirection(face: number, s: number, t: number, out: Float64Array): void {
  let a = 2 * s - 1
  let b = 2 * t - 1
  let x: number
  let y: number
  let z: number
  if (face === 0) (x = 1), (y = -b), (z = -a)
  else if (face === 1) (x = -1), (y = -b), (z = a)
  else if (face === 2) (x = a), (y = 1), (z = b)
  else if (face === 3) (x = a), (y = -1), (z = -b)
  else if (face === 4) (x = a), (y = -b), (z = 1)
  else (x = -a), (y = -b), (z = -1)
  out[0] = x
  out[1] = y
  out[2] = z
}

/** The inverse of cubeDirection: the face and (s, t) a world direction
 * lands on, written into `out` as [face, s, t]. */
export function cubeLookup(wx: number, wy: number, wz: number, out: Float64Array): void {
  let dx = wx
  let dy = wy
  let dz = wz
  let ax = Math.abs(dx)
  let ay = Math.abs(dy)
  let az = Math.abs(dz)
  let face: number
  let a: number
  let b: number
  if (ax >= ay && ax >= az) {
    if (dx > 0) (face = 0), (a = -dz / dx), (b = -dy / dx)
    else (face = 1), (a = -dz / dx), (b = dy / dx)
  } else if (ay >= az) {
    if (dy > 0) (face = 2), (a = dx / dy), (b = dz / dy)
    else (face = 3), (a = -dx / dy), (b = dz / dy)
  } else {
    if (dz > 0) (face = 4), (a = dx / dz), (b = -dy / dz)
    else (face = 5), (a = dx / dz), (b = dy / dz)
  }
  out[0] = face
  out[1] = (a + 1) / 2
  out[2] = (b + 1) / 2
}

// Bilinear panorama sample along a direction (the GLSL equirect mapping:
// the center column faces -Z, the top row +Y), wrapping horizontally and
// clamping at the poles; accumulated into `out` at weight `w`.
function samplePanorama(p: Panorama, dx: number, dy: number, dz: number, w: number, out: Float64Array): void {
  let u = Math.atan2(dx, -dz) / (2 * Math.PI) + 0.5
  let v = Math.acos(Math.max(-1, Math.min(1, dy))) / Math.PI
  let fx = u * p.width - 0.5
  let fy = v * p.height - 0.5
  let x0 = Math.floor(fx)
  let y0 = Math.floor(fy)
  let tx = fx - x0
  let ty = fy - y0
  let x1 = (((x0 + 1) % p.width) + p.width) % p.width
  x0 = ((x0 % p.width) + p.width) % p.width
  let y1 = Math.min(p.height - 1, Math.max(0, y0 + 1))
  y0 = Math.min(p.height - 1, Math.max(0, y0))
  let d = p.data
  let i00 = (y0 * p.width + x0) * 3
  let i10 = (y0 * p.width + x1) * 3
  let i01 = (y1 * p.width + x0) * 3
  let i11 = (y1 * p.width + x1) * 3
  let w00 = (1 - tx) * (1 - ty) * w
  let w10 = tx * (1 - ty) * w
  let w01 = (1 - tx) * ty * w
  let w11 = tx * ty * w
  for (let c = 0; c < 3; c++) {
    out[c] = out[c]! + d[i00 + c]! * w00 + d[i10 + c]! * w10 + d[i01 + c]! * w01 + d[i11 + c]! * w11
  }
}

/**
 * Lay a panorama onto the six faces of a `size` cube (rgba floats, alpha
 * 1), supersampling each texel enough to average the panorama's finer
 * texels rather than skip them. The CPU form of equirectToCube.
 */
export function panoramaToCube(p: Panorama, size: number): CubeFaces {
  if (!Number.isInteger(size) || size < 1) throw new Error("panoramaToCube: size must be a positive integer, got " + size)
  // Sub-samples per axis: the panorama's texels along the equator per cube
  // texel (a face spans a quarter turn).
  let ss = Math.max(1, Math.ceil(p.width / (4 * size)))
  let weight = 1 / (ss * ss)
  let dir = new Float64Array(3)
  let acc = new Float64Array(3)
  let faces: CubeFaces = []
  for (let face = 0; face < 6; face++) {
    let px = new Float32Array(size * size * 4)
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        acc[0] = acc[1] = acc[2] = 0
        for (let j = 0; j < ss; j++) {
          for (let i = 0; i < ss; i++) {
            cubeDirection(face, (x + (i + 0.5) / ss) / size, (y + (j + 0.5) / ss) / size, dir)
            let n = 1 / Math.hypot(dir[0]!, dir[1]!, dir[2]!)
            samplePanorama(p, dir[0]! * n, dir[1]! * n, dir[2]! * n, weight, acc)
          }
        }
        let o = (y * size + x) * 4
        px[o] = acc[0]!
        px[o + 1] = acc[1]!
        px[o + 2] = acc[2]!
        px[o + 3] = 1
      }
    }
    faces.push(px)
  }
  return faces
}

// The 2x2 box chain of a cube down to 1x1 (within each face; the source
// the convolution reads at a per-sample lod).
function boxChain(base: CubeFaces, size: number): CubeFaces[] {
  let chain = [base]
  let edge = size
  while (edge > 1) {
    let prev = chain[chain.length - 1]!
    let next = Math.max(1, edge >> 1)
    let level: CubeFaces = []
    for (let face = 0; face < 6; face++) {
      let src = prev[face]!
      let dst = new Float32Array(next * next * 4)
      for (let y = 0; y < next; y++) {
        for (let x = 0; x < next; x++) {
          let x0 = Math.min(edge - 1, x * 2)
          let x1 = Math.min(edge - 1, x * 2 + 1)
          let y0 = Math.min(edge - 1, y * 2)
          let y1 = Math.min(edge - 1, y * 2 + 1)
          let o = (y * next + x) * 4
          for (let c = 0; c < 4; c++) {
            dst[o + c] =
              (src[(y0 * edge + x0) * 4 + c]! + src[(y0 * edge + x1) * 4 + c]! + src[(y1 * edge + x0) * 4 + c]! + src[(y1 * edge + x1) * 4 + c]!) / 4
          }
        }
      }
      level.push(dst)
    }
    chain.push(level)
    edge = next
  }
  return chain
}

// Bilinear cube fetch within the face a direction lands on (no cross-face
// blend; the hardware's seamless filtering is not needed for a bake),
// accumulated into `out` at weight `w`.
function fetchCube(level: CubeFaces, edge: number, dx: number, dy: number, dz: number, w: number, lk: Float64Array, out: Float64Array): void {
  cubeLookup(dx, dy, dz, lk)
  let px = level[lk[0]!]!
  let fx = Math.min(edge - 1, Math.max(0, lk[1]! * edge - 0.5))
  let fy = Math.min(edge - 1, Math.max(0, lk[2]! * edge - 0.5))
  let x0 = Math.floor(fx)
  let y0 = Math.floor(fy)
  let tx = fx - x0
  let ty = fy - y0
  let x1 = Math.min(edge - 1, x0 + 1)
  let y1 = Math.min(edge - 1, y0 + 1)
  let i00 = (y0 * edge + x0) * 4
  let i10 = (y0 * edge + x1) * 4
  let i01 = (y1 * edge + x0) * 4
  let i11 = (y1 * edge + x1) * 4
  let w00 = (1 - tx) * (1 - ty) * w
  let w10 = tx * (1 - ty) * w
  let w01 = (1 - tx) * ty * w
  let w11 = tx * ty * w
  for (let c = 0; c < 3; c++) {
    out[c] = out[c]! + px[i00 + c]! * w00 + px[i10 + c]! * w10 + px[i01 + c]! * w01 + px[i11 + c]! * w11
  }
}

// Van der Corput radical inverse: the second Hammersley coordinate.
function radicalInverse(i: number): number {
  let bits = i >>> 0
  bits = ((bits << 16) | (bits >>> 16)) >>> 0
  bits = (((bits & 0x55555555) << 1) | ((bits & 0xaaaaaaaa) >>> 1)) >>> 0
  bits = (((bits & 0x33333333) << 2) | ((bits & 0xcccccccc) >>> 2)) >>> 0
  bits = (((bits & 0x0f0f0f0f) << 4) | ((bits & 0xf0f0f0f0) >>> 4)) >>> 0
  bits = (((bits & 0x00ff00ff) << 8) | ((bits & 0xff00ff00) >>> 8)) >>> 0
  return bits / 4294967296
}

/** The perceptual roughness mip level `level` of a `size` chain holds:
 * linear from 0 at the base to 1 at the ENV_ROUGH_FACE level, 1 below it. */
export function levelRoughness(size: number, level: number): number {
  let roughLevels = Math.log2(size) - Math.log2(ENV_ROUGH_FACE)
  return roughLevels <= 0 ? 1 : Math.min(1, level / roughLevels)
}

/**
 * Convolve a cube into its full mip chain: level 0 the faces as given, each
 * level below prefiltered with the GGX lobe at levelRoughness (the normal,
 * view and reflection directions taken equal, as every engine's bake
 * does), by importance sampling with the source read at the lod that
 * matches each sample's solid angle. Returns the levels, base first, each
 * six rgba float faces.
 */
export function prefilterCube(base: CubeFaces, size: number): CubeFaces[] {
  if (base.length !== 6) throw new Error("prefilterCube: six faces expected, got " + base.length)
  let source = boxChain(base, size)
  let levels: CubeFaces[] = [base.map(f => Float32Array.from(f))]
  let count = mipLevels(size)
  // The solid angle of one base-level source texel.
  let texelAngle = (4 * Math.PI) / (6 * size * size)
  let dir = new Float64Array(3)
  let lk = new Float64Array(3)
  let acc = new Float64Array(3)
  // Per level, the sample set in the normal's tangent frame: it depends on
  // roughness only, so it is built once and reused for every texel.
  let sx = new Float64Array(PREFILTER_SAMPLES)
  let sy = new Float64Array(PREFILTER_SAMPLES)
  let sz = new Float64Array(PREFILTER_SAMPLES)
  let sLod = new Int32Array(PREFILTER_SAMPLES)
  let sNol = new Float64Array(PREFILTER_SAMPLES)
  for (let level = 1; level < count; level++) {
    let edge = Math.max(1, size >> level)
    let rough = levelRoughness(size, level)
    let alpha = rough * rough
    let a2 = alpha * alpha
    let samples = 0
    for (let i = 0; i < PREFILTER_SAMPLES; i++) {
      let u1 = (i + 0.5) / PREFILTER_SAMPLES
      let u2 = radicalInverse(i)
      let phi = 2 * Math.PI * u1
      let cosT = Math.sqrt((1 - u2) / (1 + (a2 - 1) * u2))
      let sinT = Math.sqrt(Math.max(0, 1 - cosT * cosT))
      // N = V, so the reflected sample's cosine is 2 cos^2 - 1.
      let nol = 2 * cosT * cosT - 1
      if (nol <= MIN_NOL) continue
      let d = cosT * cosT * (a2 - 1) + 1
      let ggx = a2 / (Math.PI * d * d)
      let pdf = ggx / 4
      let sampleAngle = 1 / (PREFILTER_SAMPLES * pdf)
      let lod = Math.round(Math.max(0, 0.5 * Math.log2(sampleAngle / texelAngle) + SOURCE_LOD_BIAS))
      sx[samples] = sinT * Math.cos(phi)
      sy[samples] = sinT * Math.sin(phi)
      sz[samples] = cosT
      sLod[samples] = Math.min(source.length - 1, lod)
      sNol[samples] = nol
      samples++
    }
    let faces: CubeFaces = []
    for (let face = 0; face < 6; face++) {
      let px = new Float32Array(edge * edge * 4)
      for (let y = 0; y < edge; y++) {
        for (let x = 0; x < edge; x++) {
          cubeDirection(face, (x + 0.5) / edge, (y + 0.5) / edge, dir)
          let inv = 1 / Math.hypot(dir[0]!, dir[1]!, dir[2]!)
          let nx = dir[0]! * inv
          let ny = dir[1]! * inv
          let nz = dir[2]! * inv
          // A tangent frame around the normal.
          let ux = 0
          let uy = 0
          let uz = 1
          if (Math.abs(nz) > 0.999) (ux = 1), (uz = 0)
          let tx = uy * nz - uz * ny
          let ty = uz * nx - ux * nz
          let tz = ux * ny - uy * nx
          let tn = 1 / Math.hypot(tx, ty, tz)
          tx *= tn
          ty *= tn
          tz *= tn
          let bx = ny * tz - nz * ty
          let by = nz * tx - nx * tz
          let bz = nx * ty - ny * tx
          acc[0] = acc[1] = acc[2] = 0
          let weight = 0
          for (let i = 0; i < samples; i++) {
            let hx = tx * sx[i]! + bx * sy[i]! + nx * sz[i]!
            let hy = ty * sx[i]! + by * sy[i]! + ny * sz[i]!
            let hz = tz * sx[i]! + bz * sy[i]! + nz * sz[i]!
            let k = 2 * sz[i]!
            let lx = k * hx - nx
            let ly = k * hy - ny
            let lz = k * hz - nz
            let lod = sLod[i]!
            fetchCube(source[lod]!, Math.max(1, size >> lod), lx, ly, lz, sNol[i]!, lk, acc)
            weight += sNol[i]!
          }
          let o = (y * edge + x) * 4
          px[o] = acc[0]! / weight
          px[o + 1] = acc[1]! / weight
          px[o + 2] = acc[2]! / weight
          px[o + 3] = 1
        }
      }
      faces.push(px)
    }
    levels.push(faces)
  }
  return levels
}

// The .srte container: "SRTE" u32 | version u32 | size u32 | levels u32,
// then the faces as float32 rgba, level-major, face-major - the exact
// buffers createCubeTexture's explicit chain takes, viewed in place.
/** "SRTE" read as a little-endian u32. */
const MAGIC = 0x45545253
const VERSION = 1
const HEADER_BYTES = 16

/** Serialize a prefiltered chain (prefilterCube's result) into the .srte container. */
export function encodeEnvironment(levels: CubeFaces[], size: number): Uint8Array {
  if (levels.length !== mipLevels(size)) {
    throw new Error("encodeEnvironment: a " + size + " chain has " + mipLevels(size) + " levels, got " + levels.length)
  }
  let total = HEADER_BYTES
  for (let faces of levels) for (let f of faces) total += f.byteLength
  let out = new Uint8Array(total)
  let view = new DataView(out.buffer)
  view.setUint32(0, MAGIC, true)
  view.setUint32(4, VERSION, true)
  view.setUint32(8, size, true)
  view.setUint32(12, levels.length, true)
  let pos = HEADER_BYTES
  for (let faces of levels) {
    for (let f of faces) {
      out.set(new Uint8Array(f.buffer, f.byteOffset, f.byteLength), pos)
      pos += f.byteLength
    }
  }
  return out
}

/** Read a .srte container back into its size and chain, the faces viewing
 * the bytes in place (copied only if the buffer is not float-aligned). */
export function decodeEnvironment(bytes: Uint8Array): { size: number; levels: CubeFaces[] } {
  if (bytes.byteLength < HEADER_BYTES) throw new Error("decodeEnvironment: not an environment file (too short)")
  let view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  if (view.getUint32(0, true) !== MAGIC) throw new Error("decodeEnvironment: not an environment file (bad magic)")
  let version = view.getUint32(4, true)
  if (version !== VERSION) throw new Error("decodeEnvironment: version " + version + ", expected " + VERSION + " - re-bake with srt tool 3d/environment")
  let size = view.getUint32(8, true)
  let count = view.getUint32(12, true)
  if (size < 1 || count !== mipLevels(size)) throw new Error("decodeEnvironment: a " + size + " chain has " + mipLevels(size) + " levels, file says " + count)
  let aligned = (bytes.byteOffset + HEADER_BYTES) % 4 === 0
  let pos = HEADER_BYTES
  let levels: CubeFaces[] = []
  for (let level = 0; level < count; level++) {
    let edge = Math.max(1, size >> level)
    let floats = edge * edge * 4
    let faces: CubeFaces = []
    for (let face = 0; face < 6; face++) {
      if (pos + floats * 4 > bytes.byteLength) throw new Error("decodeEnvironment: truncated at level " + level + " face " + face)
      faces.push(
        aligned
          ? new Float32Array(bytes.buffer, bytes.byteOffset + pos, floats)
          : new Float32Array(bytes.slice(pos, pos + floats * 4).buffer),
      )
      pos += floats * 4
    }
    levels.push(faces)
  }
  return { size, levels }
}
