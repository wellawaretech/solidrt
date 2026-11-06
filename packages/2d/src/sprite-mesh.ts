// Atlas frame meshes, pure: a sheet's coverage mask, and per frame the
// convex hull of its covered texels, simplified outward to the hull
// layer's vertex budget and packed into the record's mesh field. No GPU or
// GUI imports, so its test (tests/sprite-mesh.test.ts) runs headless on
// the flux binary.
//
// What the mesh hugs is the covered texels as squares, padded half a texel
// for the bilinear reach, clipped to the frame. Everything that depends on
// the screen, not the texture, is the vertex stage's push (glsl.ts,
// spriteCorner): the antialiasing margin, the outline, and half a lattice
// cell diagonal, which keeps the rounding below conservative - a packed
// point sits within that distance of the true one, so the pushed polygon
// contains the true one.

/** Points every hull-layer instance draws: the mesh's vertex budget. An
 * 8-gon around a circle, the worst case for a convex hull, covers 5.5%
 * more than the circle (a 16-gon 1.3%), so a larger budget buys little. */
export const MESH_VERTICES = 8
/** Steps of the mesh lattice per axis over the frame: a packed coordinate
 * is a byte b at b / MESH_LATTICE of the frame. */
export const MESH_LATTICE = 255
/** Floats of a record's mesh field: the 16 bytes of 8 points (x then y,
 * ring order) and the ring's point count (byte MESH_COUNT_BYTE) as
 * integer-valued floats of 3 bytes each, so a plain float copy of a
 * record keeps it (no bit pattern to canonicalize). All zero means no
 * mesh: the sprite draws its full frame quad. */
export const MESH_FLOATS = 6
/** The byte of the mesh field holding the ring's distinct point count
 * (3 to MESH_VERTICES), after the points: a vertex reads its own point
 * and its two ring neighbours by it, not the whole ring. */
export const MESH_COUNT_BYTE = MESH_VERTICES * 2
// Bytes one mesh float holds: integers below 2^24 are exact in float32.
const BYTES_PER_FLOAT = 3
const BYTE = 256

/** The triangle-strip order of a mesh's ring points: a zigzag (0, 1, 7, 2,
 * 6, 3, 5, 4) that fans any convex 8-point ring; padding points repeat
 * the last one and only make degenerate triangles. What a hull layer's
 * per-vertex corner buffer holds. */
export const MESH_STRIP: readonly number[] = [0, 1, 7, 2, 6, 3, 5, 4]

// The lowest alpha byte a texel of a distance-field sheet counts as covered
// at: the true field at 0.5 is the edge (the field is nonzero almost to the
// frame's border, so alpha above zero would cover nearly everything).
const SDF_EDGE_ALPHA = 128
// The lowest alpha byte a texel of a colour sheet counts as covered at.
const COLOUR_ALPHA = 1
// Float slack, in frame fractions: a corner the simplification adds may sit
// this far outside the frame, and a cross product this close to zero is a
// collinear point.
const FRAME_EPSILON = 1e-9
// A texel word's alpha byte as a value: the top byte of a little-endian
// RGBA8 word.
const ALPHA_SHIFT = 2 ** 24

/** A sheet's mesh source, kept on the atlas so frames cut later fill
 * their mesh: the coverage of its texels, one bit per texel, row-major
 * (bit `y * width + x`). `kind` is the mesh a frame gets from it. */
export type AtlasMesh = { kind: "hull"; coverage: Uint32Array }

/**
 * A frame's mesh: `points` is the polygon as packed, in frame fractions
 * (0..1 over the unflipped frame rect, u right, v down), its ring order
 * the one every hull is wound in; `packed` the MESH_FLOATS record field
 * (writeMesh). The vertex stage pushes the polygon out on screen and
 * mirrors it with the sprite's flips, so the mesh itself is
 * flip-independent.
 */
export type FrameMesh = { points: Float32Array; packed: Float32Array }

// A polygon as parallel coordinate lists: the hull build allocates no
// object per point (QuickJS pays for every one, and a 2048-square sheet
// cuts thousands of frames).
type Polygon = { x: number[]; y: number[] }

/** The coverage mask of premultiplied RGBA8 pixels: a texel is covered
 * when its alpha is above zero, or for a distance field (`sdf`) when the
 * true field in alpha reaches the edge. Reads a texel as one
 * little-endian word (alpha its top byte) and stores 32 texels at once. */
export function coverageMask(data: Uint8Array, width: number, height: number, sdf: boolean): Uint32Array {
  let count = width * height
  let bits = new Uint32Array((count + 31) >>> 5)
  let min = sdf ? SDF_EDGE_ALPHA : COLOUR_ALPHA
  if (data.byteOffset % 4 !== 0) {
    for (let i = 0, a = 3; i < count; i++, a += 4) if (data[a]! >= min) bits[i >>> 5] = bits[i >>> 5]! | (1 << (i & 31))
    return bits
  }
  let texels = new Uint32Array(data.buffer, data.byteOffset, count)
  // The lowest texel word whose alpha byte reaches `min`.
  let threshold = min * ALPHA_SHIFT
  for (let w = 0, i = 0; w < bits.length; w++) {
    let word = 0
    let end = Math.min(count, i + 32)
    for (let b = 0; i < end; i++, b++) if (texels[i]! >= threshold) word |= 1 << b
    bits[w] = word
  }
  return bits
}

/** The first covered texel index in [from, to), or -1: an empty word is
 * skipped whole. */
function firstSet(bits: Uint32Array, from: number, to: number): number {
  let i = from
  while (i < to) {
    let word = bits[i >>> 5]! >>> (i & 31)
    if (word !== 0) {
      let at = i + 31 - Math.clz32(word & -word)
      return at < to ? at : -1
    }
    i = (i | 31) + 1
  }
  return -1
}

/** The last covered texel index in [from, to), or -1. */
function lastSet(bits: Uint32Array, from: number, to: number): number {
  let i = to - 1
  while (i >= from) {
    let word = (bits[i >>> 5]! << (31 - (i & 31))) >>> 0
    if (word !== 0) {
      let at = i - Math.clz32(word)
      return at >= from ? at : -1
    }
    i = (i & ~31) - 1
  }
  return -1
}

/**
 * The hull mesh of the pixel rect [x, x + w) x [y, y + h) of a sheet
 * `width` x `height` with coverage `bits`: the convex hull of the covered
 * texels (padded half a texel), clipped to the frame, simplified outward
 * to MESH_VERTICES and packed. A row scan from both ends stops at the
 * first covered texel, since a convex hull needs only each row's extremes.
 * Undefined for a frame with no covered texel (it draws as its quad).
 */
export function hullMesh(bits: Uint32Array, width: number, height: number, x: number, y: number, w: number, h: number): FrameMesh | undefined {
  let tx0 = Math.max(0, Math.floor(x))
  let tx1 = Math.min(width, Math.ceil(x + w))
  let ty0 = Math.max(0, Math.floor(y))
  let ty1 = Math.min(height, Math.ceil(y + h))
  let rows = Math.max(0, ty1 - ty0)
  let lo = new Int32Array(rows)
  let hi = new Int32Array(rows)
  let any = false
  for (let r = 0; r < rows; r++) {
    let row = (ty0 + r) * width
    let first = firstSet(bits, row + tx0, row + tx1)
    lo[r] = first < 0 ? -1 : first - row
    hi[r] = first < 0 ? -1 : lastSet(bits, first, row + tx1) - row
    any ||= first >= 0
  }
  if (!any) return undefined
  // The padded squares [t - 0.5, t + 1.5] have corners on the lines
  // y = k - 0.5, each line meeting the tops of row k and the bottoms of
  // row k - 2: only each line's leftmost and rightmost corner can be on
  // the hull. In line order, left before right, the points are sorted
  // by (y, x), the monotone chain's input.
  let points: Polygon = { x: [], y: [] }
  for (let k = 0; k <= rows + 1; k++) {
    let top = k < rows && lo[k]! >= 0
    let bottom = k >= 2 && k - 2 < rows && lo[k - 2]! >= 0
    if (!top && !bottom) continue
    let left = top ? (bottom ? Math.min(lo[k]!, lo[k - 2]!) : lo[k]!) : lo[k - 2]!
    let right = top ? (bottom ? Math.max(hi[k]!, hi[k - 2]!) : hi[k]!) : hi[k - 2]!
    let py = (ty0 + k - 0.5 - y) / h
    points.x.push((left - 0.5 - x) / w, (right + 1.5 - x) / w)
    points.y.push(py, py)
  }
  // A chain over (y, x) order winds the other way round: reversed, every
  // turn's cross product is positive.
  let hull = monotoneChain(points)
  hull.x.reverse()
  hull.y.reverse()
  return packMesh(simplify(clipToFrame(hull)))
}

function cross(ox: number, oy: number, ax: number, ay: number, bx: number, by: number): number {
  return (ax - ox) * (by - oy) - (ay - oy) * (bx - ox)
}

/** The convex hull of points sorted lexicographically (Andrew's monotone
 * chain), collinear points dropped. Sorted by (x, y), every turn of the
 * result has a positive cross product - the winding the vertex stage's
 * outward normals assume. */
function monotoneChain(points: Polygon): Polygon {
  let n = points.x.length
  if (n < 3) return { x: points.x.slice(), y: points.y.slice() }
  let hx: number[] = []
  let hy: number[] = []
  let push = (i: number, floor: number) => {
    let px = points.x[i]!
    let py = points.y[i]!
    while (hx.length >= floor && cross(hx[hx.length - 2]!, hy[hy.length - 2]!, hx[hx.length - 1]!, hy[hy.length - 1]!, px, py) <= 0) {
      hx.pop()
      hy.pop()
    }
    hx.push(px)
    hy.push(py)
  }
  for (let i = 0; i < n; i++) push(i, 2)
  let lower = hx.length + 1
  for (let i = n - 2; i >= 0; i--) push(i, lower)
  hx.pop()
  hy.pop()
  return { x: hx, y: hy }
}

/** The convex hull of unsorted points, wound like monotoneChain's. */
function convexHull(points: Polygon): Polygon {
  let order = points.x.map((_, i) => i).sort((a, b) => points.x[a]! - points.x[b]! || points.y[a]! - points.y[b]!)
  return monotoneChain({ x: order.map(i => points.x[i]!), y: order.map(i => points.y[i]!) })
}

/** A convex polygon clipped to the frame [0, 1]^2 (Sutherland-Hodgman):
 * still convex, same winding, with the collinear points the clip leaves
 * dropped. A polygon inside the frame passes untouched. */
function clipToFrame(polygon: Polygon): Polygon {
  if (polygon.x.every(v => v >= 0 && v <= 1) && polygon.y.every(v => v >= 0 && v <= 1)) return polygon
  let edge = (input: Polygon, inside: (px: number, py: number) => number): Polygon => {
    let out: Polygon = { x: [], y: [] }
    let n = input.x.length
    for (let i = 0; i < n; i++) {
      let ax = input.x[i]!
      let ay = input.y[i]!
      let bx = input.x[(i + 1) % n]!
      let by = input.y[(i + 1) % n]!
      let da = inside(ax, ay)
      let db = inside(bx, by)
      if (da >= 0) {
        out.x.push(ax)
        out.y.push(ay)
      }
      if (da >= 0 !== db >= 0) {
        let t = da / (da - db)
        out.x.push(ax + (bx - ax) * t)
        out.y.push(ay + (by - ay) * t)
      }
    }
    return out
  }
  let out = polygon
  out = edge(out, px => px)
  out = edge(out, px => 1 - px)
  out = edge(out, (_, py) => py)
  out = edge(out, (_, py) => 1 - py)
  return dropCollinear(out)
}

function dropCollinear(polygon: Polygon): Polygon {
  let { x, y } = polygon
  for (let i = 0; i < x.length && x.length > 3; ) {
    let n = x.length
    let p = (i + n - 1) % n
    let q = (i + 1) % n
    if (Math.abs(cross(x[p]!, y[p]!, x[i]!, y[i]!, x[q]!, y[q]!)) <= FRAME_EPSILON) {
      x.splice(i, 1)
      y.splice(i, 1)
      i = Math.max(0, i - 1)
    } else {
      i++
    }
  }
  return polygon
}

/**
 * A convex polygon simplified OUTWARD to at most MESH_VERTICES points,
 * never cutting into it: each step removes the edge whose two neighbours,
 * extended until they meet, add the least area, and only where they meet
 * inside the frame (the lattice spans the frame alone). With more than
 * four points some adjacent pair of turns sums below a half turn, so a
 * candidate exists until the frame bound rules them all out; then the
 * polygon's bounding box, four points, is the answer. A merge changes
 * only the candidates of the edges around it, so only those recompute.
 */
function simplify(polygon: Polygon): Polygon {
  let { x, y } = polygon
  if (x.length <= MESH_VERTICES) return polygon
  // Per edge i (points i to i + 1): the area its removal adds (Infinity
  // where it may not go) and the point that replaces it.
  let area: number[] = []
  let qx: number[] = []
  let qy: number[] = []
  let candidate = (i: number) => {
    let n = x.length
    let a = (i + n - 1) % n
    let p1 = i
    let p2 = (i + 1) % n
    let b = (i + 2) % n
    // a + t (p1 - a) = b + s (p2 - b): the extended neighbours meet.
    let d1x = x[p1]! - x[a]!
    let d1y = y[p1]! - y[a]!
    let d2x = x[p2]! - x[b]!
    let d2y = y[p2]! - y[b]!
    let denom = d1x * d2y - d1y * d2x
    area[i] = Infinity
    if (Math.abs(denom) <= FRAME_EPSILON) return
    let abx = x[b]! - x[a]!
    let aby = y[b]! - y[a]!
    let t = (abx * d2y - aby * d2x) / denom
    let s = (abx * d1y - aby * d1x) / denom
    // Both past the removed edge's ends: the lines converge outward.
    if (!(t >= 1 && s >= 1)) return
    let px = x[a]! + d1x * t
    let py = y[a]! + d1y * t
    if (px < -FRAME_EPSILON || px > 1 + FRAME_EPSILON || py < -FRAME_EPSILON || py > 1 + FRAME_EPSILON) return
    area[i] = Math.abs(cross(x[p1]!, y[p1]!, px, py, x[p2]!, y[p2]!)) / 2
    qx[i] = px
    qy[i] = py
  }
  for (let i = 0; i < x.length; i++) candidate(i)
  while (x.length > MESH_VERTICES) {
    let best = 0
    for (let i = 1; i < x.length; i++) if (area[i]! < area[best]!) best = i
    if (area[best] === Infinity) return boundingBox(polygon)
    x[best] = qx[best]!
    y[best] = qy[best]!
    let gone = (best + 1) % x.length
    x.splice(gone, 1)
    y.splice(gone, 1)
    area.splice(gone, 1)
    qx.splice(gone, 1)
    qy.splice(gone, 1)
    if (gone < best) best--
    let n = x.length
    for (let d = -2; d <= 1; d++) candidate((best + d + n) % n)
  }
  return polygon
}

function boundingBox(polygon: Polygon): Polygon {
  let x0 = Math.min(...polygon.x)
  let y0 = Math.min(...polygon.y)
  let x1 = Math.max(...polygon.x)
  let y1 = Math.max(...polygon.y)
  return { x: [x0, x1, x1, x0], y: [y0, y0, y1, y1] }
}

/** A convex polygon in frame fractions (at most MESH_VERTICES points) to
 * its mesh: each point rounded to the lattice, the hull of the rounded
 * points taken again (rounding may leave a point reflex, and the strip
 * fans a convex ring only), padded to the budget by repeating the last
 * point, and packed 3 bytes per float with the point count after the
 * points. Undefined when the points pack to all zeros, which reads as no
 * mesh. */
function packMesh(polygon: Polygon): FrameMesh | undefined {
  let lattice = (v: number) => Math.min(MESH_LATTICE, Math.max(0, Math.round(v * MESH_LATTICE)))
  let ring = convexHull({ x: polygon.x.map(lattice), y: polygon.y.map(lattice) })
  if (ring.x.length < 3) {
    // A shape thinner than a lattice cell rounds to a segment or a point,
    // which the push cannot open (a segment has no outward side): its
    // bounding box on the lattice instead, rounded outward and at least
    // one cell wide.
    let box = boundingBox(polygon)
    let lo = (v: number) => Math.min(MESH_LATTICE - 1, Math.max(0, Math.floor(v * MESH_LATTICE)))
    let hi = (v: number, low: number) => Math.min(MESH_LATTICE, Math.max(low + 1, Math.ceil(v * MESH_LATTICE)))
    let bx0 = lo(box.x[0]!)
    let by0 = lo(box.y[0]!)
    let bx1 = hi(box.x[1]!, bx0)
    let by1 = hi(box.y[2]!, by0)
    ring = { x: [bx0, bx1, bx1, bx0], y: [by0, by0, by1, by1] }
  }
  let packed = new Float32Array(MESH_FLOATS)
  for (let i = 0; i < MESH_VERTICES * 2; i++) {
    let point = Math.min(i >> 1, ring.x.length - 1)
    let byte = i % 2 === 0 ? ring.x[point]! : ring.y[point]!
    let k = Math.floor(i / BYTES_PER_FLOAT)
    packed[k] = packed[k]! + byte * BYTE ** (i % BYTES_PER_FLOAT)
  }
  if (packed.every(f => f === 0)) return undefined
  let at = Math.floor(MESH_COUNT_BYTE / BYTES_PER_FLOAT)
  packed[at] = packed[at]! + ring.x.length * BYTE ** (MESH_COUNT_BYTE % BYTES_PER_FLOAT)
  return { points: unpackPoints(packed), packed }
}

/** The distinct ring points of a packed mesh field, in frame fractions
 * (its point count's worth, the padding left out): what
 * `FrameMesh.points` holds. */
export function unpackPoints(packed: ArrayLike<number>): Float32Array {
  let count = Math.min(MESH_VERTICES, meshByte(packed, MESH_COUNT_BYTE))
  let out = new Float32Array(count * 2)
  for (let i = 0; i < count * 2; i++) out[i] = meshByte(packed, i) / MESH_LATTICE
  return out
}

function meshByte(packed: ArrayLike<number>, i: number): number {
  let word = packed[Math.floor(i / BYTES_PER_FLOAT)]!
  return Math.floor(word / BYTE ** (i % BYTES_PER_FLOAT)) % BYTE
}

/** The mesh field of a record at `at`, or undefined where it is all zero
 * (no mesh: the full quad). What a layer's getSprite reads back. */
export function readMesh(data: Float32Array, at: number): FrameMesh | undefined {
  let packed = data.slice(at, at + MESH_FLOATS)
  if (packed.every(f => f === 0)) return undefined
  return { points: unpackPoints(packed), packed }
}
