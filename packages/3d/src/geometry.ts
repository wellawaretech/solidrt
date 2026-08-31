// Geometry: one interleaved vertex buffer described by a layout - an
// ordered attribute list that always starts with the standard prefix
// (position vec3, normal vec3, uv vec2: 8 floats, what every generator
// emits) and may carry further channels after it. The layout is open data:
// `withAttribute` appends any named channel (Three's `setAttribute`), and
// "colored" names the one common case, the prefix plus an aColor vec4 (12
// floats) as the per-vertex data channel for custom materials (tint, baked
// AO, any four scalars). Materials read attributes by name and adapt to
// whatever layout their geometry carries (one pipeline per layout met),
// so a geometry may carry more channels than a material reads.
// Indices are uint16 or uint32 (the generators here emit uint16; hand-built
// geometry past 64k vertices uses a Uint32Array and the draw entry follows
// the array type). Winding is counter-clockwise seen from outside in the
// y-up world, which the standard camera rig (perspective() with its baked
// y flip) presents as the engine's displayed-CCW front faces: every
// generator here culls correctly with cull: "back". Normals ride along
// unused by the unlit materials so the layout is ready for lights without
// a geometry change (inactive attributes are skipped but keep the stride).
//
// Pure module by design - geometry is data, and every function here is
// array math (the check rig checks/geometry-check.ts runs it headless on
// flux). The GPU buffer step lives in geometry-gpu.ts.

import type { VertexAttribute } from "@solidrt/core/gpu"
import { add, compose, cross, mat4, normalize, normalMatrix, sub, updateRotation, updateScale } from "./math.ts"
import type { Quat, TransformUpdate, Vec2, Vec3 } from "./math.ts"

/** A vertex layout: the named presets, or an explicit attribute list that
 * must begin with the standard prefix (aPos vec3, aNormal vec3, aUV vec2).
 * Absent on a Geometry means "standard". */
export type VertexLayout = "standard" | "colored" | "skinned" | VertexAttribute[]

const STANDARD_ATTRIBUTES: VertexAttribute[] = [
  { name: "aPos", format: "vec3" },
  { name: "aNormal", format: "vec3" },
  { name: "aUV", format: "vec2" },
]

/** The attribute lists behind the named layouts. Every layout shares the
 * standard prefix, so one shader vocabulary serves all of them. */
export const VERTEX_LAYOUTS: Record<"standard" | "colored" | "skinned", VertexAttribute[]> = {
  standard: STANDARD_ATTRIBUTES,
  colored: [...STANDARD_ATTRIBUTES, { name: "aColor", format: "vec4" }],
  // The rigged-model layout: 4 joint indices (as floats - exact to 2^24)
  // and their weights per vertex, what a skinned vertex stage reads.
  skinned: [...STANDARD_ATTRIBUTES, { name: "aJoints", format: "vec4" }, { name: "aWeights", format: "vec4" }],
}

/** Floats per vertex in the "standard" layout (the generators' own write
 * format before packing). */
export const STANDARD_FLOATS = 8

const FORMAT_FLOATS: Record<VertexAttribute["format"], number> = { f32: 1, vec2: 2, vec3: 3, vec4: 4 }

/** The attribute list of a layout (a preset name resolves to its list). */
export function layoutAttributes(layout?: VertexLayout): VertexAttribute[] {
  if (layout === undefined || layout === "standard") return VERTEX_LAYOUTS.standard
  if (layout === "colored") return VERTEX_LAYOUTS.colored
  if (layout === "skinned") return VERTEX_LAYOUTS.skinned
  return layout
}

/** Floats per vertex of a layout - its interleave stride. */
export function layoutStride(layout?: VertexLayout): number {
  let stride = 0
  for (let attr of layoutAttributes(layout)) stride += FORMAT_FLOATS[attr.format]
  return stride
}

/** A layout's identity as a string (name:format per attribute, in order):
 * two layouts with equal keys interleave identically. */
export function layoutKey(layout?: VertexLayout): string {
  return layoutAttributes(layout)
    .map(a => a.name + ":" + a.format)
    .join(",")
}

/** Where an attribute sits in the interleave: its float offset and size.
 * Null when the layout does not carry that name. */
export function layoutSlot(layout: VertexLayout | undefined, name: string): { offset: number; size: number; format: VertexAttribute["format"] } | null {
  let offset = 0
  for (let attr of layoutAttributes(layout)) {
    let size = FORMAT_FLOATS[attr.format]
    if (attr.name === name) return { offset, size, format: attr.format }
    offset += size
  }
  return null
}

/** The prefix check every layout must pass: standard attributes first, in
 * order, and no duplicate names after them. */
function checkLayout(layout: VertexAttribute[], where: string): void {
  for (let i = 0; i < STANDARD_ATTRIBUTES.length; i++) {
    let want = STANDARD_ATTRIBUTES[i]!
    let got = layout[i]
    if (got === undefined || got.name !== want.name || got.format !== want.format) {
      throw new Error(where + ": a layout must start with the standard prefix (aPos vec3, aNormal vec3, aUV vec2)")
    }
  }
  let seen = new Set<string>()
  for (let attr of layout) {
    if (seen.has(attr.name)) throw new Error(where + ": duplicate attribute '" + attr.name + "'")
    seen.add(attr.name)
  }
}

/**
 * The structural check for geometry about to draw: the layout passes the
 * prefix rule, the vertex float count is a whole number of its stride,
 * and indices are present. Throws naming the geometry. The scene runs it
 * at add() so hand-built geometry (a bare `layout: "colored"` over a
 * miscounted array) fails there instead of drawing garbage triangles.
 * Deliberately no max-index scan: that is O(indices) per add, and the
 * generators and merge/transform keep indices in range by construction.
 */
export function validateGeometry(geometry: Geometry): void {
  let name = geometry.label ? "geometry '" + geometry.label + "'" : "geometry"
  let layout = geometry.layout
  if (layout !== undefined && typeof layout !== "string") checkLayout(layout, name)
  let stride = layoutStride(layout)
  if (geometry.vertices.length % stride !== 0) {
    throw new Error(
      name + ": " + geometry.vertices.length + " vertex floats is not a whole number of " + stride + "-float (" + layoutKey(layout) + ") vertices",
    )
  }
  if (geometry.indices.length === 0) throw new Error(name + ": no indices")
}

/** The options every generator shares (each generator's own options type
 * extends this with its dimensions, all optional with defaults): `label`
 * for the GPU buffers, and `layout` to emit vertices in a wider layout
 * directly (the standard channels written, the extra slots zeroed for
 * fillAttribute/fillColors to write in place), so colored or custom
 * channel geometry is built in one pass instead of generate-then-repack. */
export type GeometryOptions = { label?: string; layout?: VertexLayout }

/** The generator tail: pack standard-layout vertices (number[] of 8 per
 * vertex, or an already-written Float32Array) and indices into a Geometry
 * of the requested layout. A wider layout spreads the standard channels
 * to its stride, leaving the extra slots zero. */
export function packGeometry(
  verts: ArrayLike<number>,
  indices: number[] | Uint16Array | Uint32Array,
  options: GeometryOptions = {},
): Geometry {
  let { label, layout } = options
  if (verts.length % STANDARD_FLOATS !== 0) {
    throw new Error("packGeometry: vertex data is not a whole number of standard-layout vertices")
  }
  let count = verts.length / STANDARD_FLOATS
  let packedIndices = indices instanceof Uint16Array || indices instanceof Uint32Array ? indices : packIndices(indices, count)
  let attrs = layoutAttributes(layout)
  if (layout !== undefined && typeof layout !== "string") checkLayout(attrs, "packGeometry")
  let stride = layoutStride(attrs)
  if (stride === STANDARD_FLOATS) {
    let vertices = verts instanceof Float32Array ? verts : new Float32Array(verts)
    return layout === undefined ? { vertices, indices: packedIndices, label } : { vertices, indices: packedIndices, layout, label }
  }
  let vertices = new Float32Array(count * stride)
  for (let i = 0; i < count; i++) {
    let s = i * STANDARD_FLOATS
    let d = i * stride
    for (let k = 0; k < STANDARD_FLOATS; k++) vertices[d + k] = verts[s + k]!
  }
  return { vertices, indices: packedIndices, layout, label }
}

// The stride a generator writing a Float32Array directly must use for the
// requested layout, and the matching finish (no repack: the data is
// already laid out).
function generatorStride(options: GeometryOptions): number {
  let { layout } = options
  let attrs = layoutAttributes(layout)
  if (layout !== undefined && typeof layout !== "string") checkLayout(attrs, "generator layout")
  return layoutStride(attrs)
}

function finishGeometry(vertices: Float32Array, indices: number[], options: GeometryOptions): Geometry {
  let { label, layout } = options
  let packed = packIndices(indices, vertices.length / layoutStride(layout))
  return layout === undefined ? { vertices, indices: packed, label } : { vertices, indices: packed, layout, label }
}

/** Uint16 indices when they fit, Uint32Array past 64k vertices - the draw
 * entry follows the array type. The tail of every unbounded generator. */
export function packIndices(indices: number[], vertexCount: number): Uint16Array | Uint32Array {
  return vertexCount > 65535 ? new Uint32Array(indices) : new Uint16Array(indices)
}

export type Geometry = {
  /** Interleaved [pos.xyz, normal.xyz, uv.xy] per vertex, plus color.rgba
   * in the "colored" layout. */
  vertices: Float32Array
  /** The array type picks the draw's index format: Uint32Array past 64k
   * vertices. */
  indices: Uint16Array | Uint32Array
  /** Vertex layout; absent means "standard". Must match the material's
   * layout - the scene rejects a mismatched pair at add(). */
  layout?: VertexLayout
  /** Debug name for the lazily-created GPU buffers. */
  label?: string
  _bounds?: Float32Array
}

/**
 * The geometry's LOCAL axis-aligned bounds as [minX, minY, minZ, maxX,
 * maxY, maxZ], computed from the vertices on first use and cached (like
 * the GPU buffers, geometry is treated as immutable after creation).
 * Picking's narrowphase volume; a flat geometry legitimately has zero
 * extent on an axis. An empty geometry yields a zero box at the origin.
 */
export function geometryBounds(geometry: Geometry): Float32Array {
  let bounds = geometry._bounds
  if (bounds === undefined) {
    bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])
    let v = geometry.vertices
    let stride = layoutStride(geometry.layout)
    for (let i = 0; i + 2 < v.length; i += stride) {
      let x = v[i]!, y = v[i + 1]!, z = v[i + 2]!
      if (x < bounds[0]!) bounds[0] = x
      if (y < bounds[1]!) bounds[1] = y
      if (z < bounds[2]!) bounds[2] = z
      if (x > bounds[3]!) bounds[3] = x
      if (y > bounds[4]!) bounds[4] = y
      if (z > bounds[5]!) bounds[5] = z
    }
    if (bounds[0]! > bounds[3]!) bounds.fill(0)
    geometry._bounds = bounds
  }
  return bounds
}

/** Per-vertex values for withAttribute/fillAttribute: a flat array of the
 * attribute's size per vertex, or a callback deriving each vertex's value
 * from the standard channels (what a baker wants). */
export type AttributeFill = ArrayLike<number> | ((index: number, pos: Vec3, normal: Vec3, uv: Vec2) => ArrayLike<number>)
/** AttributeFill for the aColor vec4 channel (4 per vertex). */
export type ColorFill = AttributeFill

/**
 * Append a named channel to a geometry: a new geometry (the source is
 * untouched, its GPU buffers stay independent) whose layout is the
 * source's plus `attr`, every existing channel copied through and the new
 * slots written from `fill`. This is Three's `geometry.setAttribute` for
 * an interleaved buffer - the one generic primitive; `withColors` is its
 * aColor spelling. A material reads the channel by declaring the matching
 * `in` (name and format) in its vertex stage.
 */
export function withAttribute(geometry: Geometry, attr: VertexAttribute, fill: AttributeFill, label?: string): Geometry {
  let srcLayout = layoutAttributes(geometry.layout)
  if (layoutSlot(srcLayout, attr.name) !== null) {
    throw new Error("withAttribute: geometry already carries '" + attr.name + "'")
  }
  let srcStride = layoutStride(srcLayout)
  if (geometry.vertices.length % srcStride !== 0) {
    throw new Error("withAttribute: vertex data is not a whole number of " + layoutKey(srcLayout) + " vertices")
  }
  let layout = [...srcLayout, { name: attr.name, format: attr.format }]
  checkLayout(layout, "withAttribute")
  let stride = layoutStride(layout)
  let count = geometry.vertices.length / srcStride
  let src = geometry.vertices
  let out = new Float32Array(count * stride)
  for (let i = 0; i < count; i++) {
    let s = i * srcStride
    let d = i * stride
    for (let k = 0; k < srcStride; k++) out[d + k] = src[s + k]!
  }
  fillSlot(out, layout, attr.name, fill, 0)
  return {
    vertices: out,
    indices: geometry.indices,
    layout,
    label: label ?? (geometry.label ? geometry.label + "-" + attr.name : undefined),
  }
}

/**
 * Derive a "colored"-layout geometry from a standard one: the same
 * positions, normals, uvs and indices, plus an aColor vec4 per vertex -
 * the data channel for materials whose vertex stage reads `in vec4 aColor`
 * (a tint, baked ambient occlusion, any four scalars; the name is the
 * standard vocabulary, the contents are yours). `withAttribute` with the
 * aColor channel; the "colored" preset name is kept on the result.
 */
export function withColors(geometry: Geometry, fill: ColorFill, label?: string): Geometry {
  if (layoutSlot(geometry.layout, "aColor") !== null) {
    throw new Error("withColors: geometry already carries an aColor channel")
  }
  let out = withAttribute(geometry, { name: "aColor", format: "vec4" }, fill, label ?? (geometry.label ? geometry.label + "-colored" : undefined))
  if (layoutKey(out.layout) === layoutKey("colored")) out.layout = "colored"
  return out
}

/**
 * The in-place primitive under withAttribute: write one channel the
 * geometry's layout already carries (withAttribute ADDS a channel; this
 * overwrites an existing one). The pos/normal/uv the callback receives
 * are read from the buffer itself, so a builder baking transforms while
 * writing hands the baker world-space vertices. Fills vertices
 * [first, first + count) - count defaults to the rest of the buffer -
 * and `fill` indexes relative to `first`, so a per-part callback works
 * unchanged for both APIs. Returns `geometry.vertices`.
 */
export function fillAttribute(geometry: Geometry, name: string, fill: AttributeFill, first = 0, count?: number): Float32Array {
  return fillSlot(geometry.vertices, geometry.layout, name, fill, first, count)
}

/** The raw form behind fillAttribute (withAttribute writes its fresh
 * buffer through it, before the Geometry exists): a bare array carries no
 * layout tag, so the caller states the layout and only the arithmetic is
 * checked. */
function fillSlot(vertices: Float32Array, layout: VertexLayout | undefined, name: string, fill: AttributeFill, first: number, count?: number): Float32Array {
  let slot = layoutSlot(layout, name)
  if (slot === null) throw new Error("fillAttribute: layout has no '" + name + "' attribute")
  let stride = layoutStride(layout)
  if (vertices.length % stride !== 0) {
    throw new Error("fillAttribute: vertex data is not a whole number of " + layoutKey(layout) + " vertices")
  }
  let total = vertices.length / stride
  let n = count ?? total - first
  if (!Number.isInteger(first) || !Number.isInteger(n) || first < 0 || n < 0 || first + n > total) {
    throw new Error("fillAttribute: range [" + first + ", " + (first + n) + ") is outside the buffer's " + total + " vertices")
  }
  let size = slot.size
  let fn = typeof fill === "function" ? fill : null
  let flat = typeof fill === "function" ? null : fill
  if (flat !== null && flat.length !== n * size) {
    throw new Error("fillAttribute: fill has " + flat.length + " floats, expected " + size + " per vertex (" + n * size + ")")
  }
  for (let i = 0; i < n; i++) {
    let d = (first + i) * stride
    let value: ArrayLike<number>
    let s: number
    if (fn !== null) {
      value = fn(i, [vertices[d]!, vertices[d + 1]!, vertices[d + 2]!], [vertices[d + 3]!, vertices[d + 4]!, vertices[d + 5]!], [vertices[d + 6]!, vertices[d + 7]!])
      s = 0
      if (value.length !== size) {
        throw new Error("fillAttribute: fill callback returned " + value.length + " floats for '" + name + "', expected " + size)
      }
    } else {
      value = flat!
      s = i * size
    }
    for (let k = 0; k < size; k++) vertices[d + slot.offset + k] = value[s + k]!
  }
  return vertices
}

/** `fillAttribute` for the aColor channel of a color-carrying geometry
 * (fill is 4 per vertex). */
export function fillColors(geometry: Geometry, fill: ColorFill, first = 0, count?: number): Float32Array {
  return fillAttribute(geometry, "aColor", fill, first, count)
}

/**
 * Bake a placement (the setTransform shape: Euler XYZ radians or a
 * quaternion, not both; number = uniform scale; absent = identity) into a
 * geometry: a new geometry (the source is
 * untouched, its GPU buffers stay independent) whose positions are moved
 * by the transform and whose normals follow through the inverse-transpose,
 * renormalized - correct under non-uniform scale. UVs, colors, indices and
 * layout copy through. This is Three's `geometry.applyMatrix4`, the first
 * half of authoring a static scene as data: transform each part into place,
 * mergeGeometries the parts, draw one mesh.
 */
export function transformGeometry(geometry: Geometry, transform: TransformUpdate, label?: string): Geometry {
  let rot: Quat = [0, 0, 0, 1]
  updateRotation(rot, transform, "transformGeometry")
  let scl: Vec3 = [1, 1, 1]
  if (transform.scale !== undefined) updateScale(scl, transform.scale)
  let m = compose(mat4(), transform.position ?? [0, 0, 0], rot, scl)
  let n = normalMatrix(mat4(), m)
  let stride = layoutStride(geometry.layout)
  let src = geometry.vertices
  if (src.length % stride !== 0) {
    throw new Error("transformGeometry: vertex data is not a whole number of " + layoutKey(geometry.layout) + " vertices")
  }
  let out = new Float32Array(src)
  for (let i = 0; i < out.length; i += stride) {
    let x = src[i]!, y = src[i + 1]!, z = src[i + 2]!
    out[i] = m[0] * x + m[4] * y + m[8] * z + m[12]
    out[i + 1] = m[1] * x + m[5] * y + m[9] * z + m[13]
    out[i + 2] = m[2] * x + m[6] * y + m[10] * z + m[14]
    let nx = src[i + 3]!, ny = src[i + 4]!, nz = src[i + 5]!
    let tx = n[0] * nx + n[4] * ny + n[8] * nz
    let ty = n[1] * nx + n[5] * ny + n[9] * nz
    let tz = n[2] * nx + n[6] * ny + n[10] * nz
    let len = Math.hypot(tx, ty, tz) || 1
    out[i + 3] = tx / len
    out[i + 4] = ty / len
    out[i + 5] = tz / len
  }
  return {
    vertices: out,
    indices: geometry.indices,
    layout: geometry.layout,
    label: label ?? (geometry.label ? geometry.label + "-transformed" : undefined),
  }
}

/**
 * Concatenate geometries into one: vertices appended in order, indices
 * offset to match, uint32 indices past 64k vertices. Every part must share
 * one layout - a mixed list throws, because the strides differ and a merge
 * that picked one would draw garbage, not a mesh missing a channel. The
 * second half of authoring a static scene as data (Three's
 * `BufferGeometryUtils.mergeGeometries`): the result is one draw entry and
 * one uModel write however many parts went in, so only what actually moves
 * keeps a node of its own.
 */
export function mergeGeometries(parts: Geometry[], label?: string): Geometry {
  if (parts.length === 0) throw new Error("mergeGeometries: no parts")
  let layout = parts[0]!.layout
  let key = layoutKey(layout)
  let stride = layoutStride(layout)
  let floats = 0
  let indexCount = 0
  for (let part of parts) {
    if (layoutKey(part.layout) !== key) {
      throw new Error("mergeGeometries: mixed layouts (" + key + " and " + layoutKey(part.layout) + ")")
    }
    if (part.vertices.length % stride !== 0) {
      throw new Error("mergeGeometries: a part's vertex data is not a whole number of " + key + " vertices")
    }
    floats += part.vertices.length
    indexCount += part.indices.length
  }
  let vertexCount = floats / stride
  let vertices = new Float32Array(floats)
  let indices = vertexCount > 65535 ? new Uint32Array(indexCount) : new Uint16Array(indexCount)
  let vOffset = 0
  let iOffset = 0
  for (let part of parts) {
    vertices.set(part.vertices, vOffset)
    let base = vOffset / stride
    let src = part.indices
    for (let i = 0; i < src.length; i++) indices[iOffset + i] = src[i]! + base
    vOffset += part.vertices.length
    iOffset += src.length
  }
  return { vertices, indices, layout, label }
}

// Indices for a row-major (cellRows + 1) x (cellCols + 1) vertex grid: two
// CCW triangles per cell, split across the row0col0-row1col1 diagonal -
// the one quad pattern every grid generator here shares (rows run along
// the surface, columns around, same handedness everywhere). A collapsed
// first/last vertex row (sphere pole, cone apex) skips its zero-area
// triangle per cell.
function gridIndices(cellRows: number, cellCols: number, skipFirst = false, skipLast = false): number[] {
  let cols = cellCols + 1
  let out: number[] = []
  for (let r = 0; r < cellRows; r++) {
    for (let c = 0; c < cellCols; c++) {
      let r0 = r * cols + c
      let r1 = r0 + cols
      if (!skipFirst || r > 0) out.push(r0 + 1, r0, r1 + 1)
      if (!skipLast || r < cellRows - 1) out.push(r0, r1, r1 + 1)
    }
  }
  return out
}

/** An axis-aligned box centered on the origin: 24 vertices, 36 indices. */
export type BoxOptions = GeometryOptions & { width?: number; height?: number; depth?: number }

export function box(options: BoxOptions = {}): Geometry {
  let { width = 1, height = 1, depth = 1 } = options
  let x = width / 2
  let y = height / 2
  let z = depth / 2
  let verts: number[] = []
  let indices: number[] = []
  type P = [number, number, number]
  // Corners a (bottom-left) through d (top-left), CCW seen from outside.
  let quad = (a: P, b: P, c: P, d: P, n: P) => {
    let base = verts.length / STANDARD_FLOATS
    let uv = [[0, 1], [1, 1], [1, 0], [0, 0]]
    let corners = [a, b, c, d]
    for (let i = 0; i < 4; i++) {
      let p = corners[i]!
      let t = uv[i]!
      verts.push(p[0], p[1], p[2], n[0], n[1], n[2], t[0]!, t[1]!)
    }
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3)
  }
  quad([-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z], [0, 0, 1]) // front
  quad([x, -y, -z], [-x, -y, -z], [-x, y, -z], [x, y, -z], [0, 0, -1]) // back
  quad([x, -y, z], [x, -y, -z], [x, y, -z], [x, y, z], [1, 0, 0]) // right
  quad([-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z], [-1, 0, 0]) // left
  quad([-x, y, z], [x, y, z], [x, y, -z], [-x, y, -z], [0, 1, 0]) // top
  quad([-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z], [0, -1, 0]) // bottom
  return packGeometry(verts, indices, options)
}

/**
 * A rectangle in the XY plane facing +z, centered on the origin. For a
 * ground plane, rotate it flat: `rotation={[-Math.PI / 2, 0, 0]}`.
 */
export type PlaneOptions = GeometryOptions & { width?: number; height?: number }

export function plane(options: PlaneOptions = {}): Geometry {
  let { width = 1, height = 1 } = options
  let x = width / 2
  let y = height / 2
  // prettier-ignore
  let vertices = [
    -x, -y, 0, 0, 0, 1, 0, 1,
    x, -y, 0, 0, 0, 1, 1, 1,
    x, y, 0, 0, 0, 1, 1, 0,
    -x, y, 0, 0, 0, 1, 0, 0,
  ]
  return packGeometry(vertices, [0, 1, 2, 0, 2, 3], options)
}

/**
 * A (p,q) torus knot swept into a tube, centered on the origin and standing
 * y-up: the knot's disc lies in the XZ plane with the weave running
 * vertically - the orientation a y-up world with XZ floors wants (the
 * standard-vocabulary divergence: Three's equivalent stands on z). The
 * tube's (tubularSegments x radialSegments) grid stores each vertex once;
 * the seam row/column duplicate the first with u/v = 1 (distinct texture
 * coordinates, so genuinely distinct vertices). UVs: u 0..1 along the knot,
 * v 0..1 around the tube.
 */
export type TorusKnotOptions = GeometryOptions & {
  radius?: number
  tube?: number
  tubularSegments?: number
  radialSegments?: number
  p?: number
  q?: number
}

export function torusKnot(options: TorusKnotOptions = {}): Geometry {
  let { radius = 1, tube = 0.4, tubularSegments = 64, radialSegments = 8, p = 2, q = 3 } = options
  // A point on the knot curve at parameter t (0..2*PI*p).
  let point = (t: number): Vec3 => {
    let qp = (q / p) * t
    let r = radius * (2 + Math.cos(qp)) * 0.5
    return [r * Math.cos(t), radius * Math.sin(qp) * 0.5, r * Math.sin(t)]
  }

  let rows = tubularSegments + 1
  let cols = radialSegments + 1
  let stride = generatorStride(options)
  let vertices = new Float32Array(rows * cols * stride)
  let at = 0

  for (let i = 0; i < rows; i++) {
    let t = (i / tubularSegments) * Math.PI * 2 * p
    // A stable frame along the curve: tangent from a finite difference, and
    // a normal biased away from the axis (P1 + P2), which is well-defined
    // everywhere on a torus knot and needs no parallel transport.
    let p1 = point(t)
    let p2 = point(t + 0.01)
    let tangent = sub(p2, p1)
    let bitangent = normalize(cross(tangent, add(p2, p1)))
    let normal = normalize(cross(bitangent, tangent))

    for (let j = 0; j < cols; j++) {
      let v = (j / radialSegments) * Math.PI * 2
      let cv = -Math.cos(v) * tube
      let sv = Math.sin(v) * tube
      let x = p1[0] + cv * normal[0] + sv * bitangent[0]
      let y = p1[1] + cv * normal[1] + sv * bitangent[1]
      let z = p1[2] + cv * normal[2] + sv * bitangent[2]
      let n = normalize([x - p1[0], y - p1[1], z - p1[2]])
      vertices[at] = x
      vertices[at + 1] = y
      vertices[at + 2] = z
      vertices[at + 3] = n[0]
      vertices[at + 4] = n[1]
      vertices[at + 5] = n[2]
      vertices[at + 6] = i / tubularSegments
      vertices[at + 7] = j / radialSegments
      at += stride
    }
  }

  return finishGeometry(vertices, gridIndices(tubularSegments, radialSegments), options)
}

/**
 * A capped cylinder on the y axis, centered on the origin. Different top
 * and bottom radii make it a truncated cone (`cone()` is the zero-top
 * case); side normals tilt with the taper. Side UVs: u around the
 * circumference, v 0 at the top to 1 at the bottom; caps get a planar
 * disc map. A zero radius skips that cap and the degenerate side
 * triangles at the apex.
 */
export type CylinderOptions = GeometryOptions & {
  radiusTop?: number
  radiusBottom?: number
  height?: number
  radialSegments?: number
}

export function cylinder(options: CylinderOptions = {}): Geometry {
  let { radiusTop = 0.5, radiusBottom = 0.5, height = 1, radialSegments = 24 } = options
  let h = height / 2
  let cols = radialSegments + 1
  let verts: number[] = []
  // Side normal: perpendicular to the slant line in the (radial, y) plane.
  let slant = Math.hypot(height, radiusBottom - radiusTop) || 1
  let nr = height / slant
  let ny = (radiusBottom - radiusTop) / slant
  let rows = [
    { r: radiusTop, y: h, v: 0 },
    { r: radiusBottom, y: -h, v: 1 },
  ]
  for (let row of rows) {
    for (let ix = 0; ix < cols; ix++) {
      let u = ix / radialSegments
      let phi = u * Math.PI * 2
      let dx = -Math.cos(phi)
      let dz = Math.sin(phi)
      verts.push(row.r * dx, row.y, row.r * dz, nr * dx, ny, nr * dz, u, row.v)
    }
  }
  let indices = gridIndices(1, radialSegments, radiusTop <= 0, radiusBottom <= 0)
  // Caps fan around a center vertex; the planar UV map has no seam, so the
  // ring wraps with modulo instead of duplicating a column.
  let cap = (r: number, y: number, up: number) => {
    let base = verts.length / STANDARD_FLOATS
    verts.push(0, y, 0, 0, up, 0, 0.5, 0.5)
    for (let i = 0; i < radialSegments; i++) {
      let phi = (i / radialSegments) * Math.PI * 2
      let x = -Math.cos(phi) * r
      let z = Math.sin(phi) * r
      verts.push(x, y, z, 0, up, 0, 0.5 + x / (2 * r), 0.5 + (up > 0 ? z : -z) / (2 * r))
    }
    for (let i = 0; i < radialSegments; i++) {
      let j = (i + 1) % radialSegments
      if (up > 0) indices.push(base, base + 1 + i, base + 1 + j)
      else indices.push(base, base + 1 + j, base + 1 + i)
    }
  }
  if (radiusTop > 0) cap(radiusTop, h, 1)
  if (radiusBottom > 0) cap(radiusBottom, -h, -1)
  return packGeometry(verts, indices, options)
}

/** A capped cone on the y axis, centered on the origin: `cylinder()` with
 * a zero top radius (each apex vertex carries its column's side normal, so
 * the surface shades smoothly around). */
export type ConeOptions = GeometryOptions & { radius?: number; height?: number; radialSegments?: number }

export function cone(options: ConeOptions = {}): Geometry {
  let { radius = 0.5, height = 1, radialSegments = 24, ...rest } = options
  return cylinder({ ...rest, radiusTop: 0, radiusBottom: radius, height, radialSegments })
}

/**
 * A torus lying flat, centered on the origin: the ring lies in the XZ
 * plane with the hole on the y axis - the y-up orientation torusKnot also
 * uses (Three's equivalent stands in XY). Option names are Three's:
 * radialSegments subdivides the tube cross-section, tubularSegments the
 * ring. UVs: u 0..1 around the ring, v 0..1 around the tube, seam
 * row/column duplicated like torusKnot.
 */
export type TorusOptions = GeometryOptions & {
  radius?: number
  tube?: number
  radialSegments?: number
  tubularSegments?: number
}

export function torus(options: TorusOptions = {}): Geometry {
  let { radius = 0.5, tube = 0.2, radialSegments = 12, tubularSegments = 32 } = options
  let rows = tubularSegments + 1
  let cols = radialSegments + 1
  let stride = generatorStride(options)
  let vertices = new Float32Array(rows * cols * stride)
  let at = 0
  for (let i = 0; i < rows; i++) {
    let phi = (i / tubularSegments) * Math.PI * 2
    let dx = -Math.cos(phi)
    let dz = Math.sin(phi)
    for (let j = 0; j < cols; j++) {
      let psi = (j / radialSegments) * Math.PI * 2
      let cp = Math.cos(psi)
      let sp = Math.sin(psi)
      let r = radius + tube * cp
      vertices[at] = r * dx
      vertices[at + 1] = tube * sp
      vertices[at + 2] = r * dz
      vertices[at + 3] = cp * dx
      vertices[at + 4] = sp
      vertices[at + 5] = cp * dz
      vertices[at + 6] = i / tubularSegments
      vertices[at + 7] = j / radialSegments
      at += stride
    }
  }
  return finishGeometry(vertices, gridIndices(tubularSegments, radialSegments), options)
}

/**
 * A disc in the XY plane facing +z, centered on the origin (rotate flat
 * like plane()). UVs are the planar map of the disc inscribed in the unit
 * square.
 */
export type CircleOptions = GeometryOptions & { radius?: number; segments?: number }

export function circle(options: CircleOptions = {}): Geometry {
  let { radius = 0.5, segments = 32 } = options
  let verts: number[] = [0, 0, 0, 0, 0, 1, 0.5, 0.5]
  let indices: number[] = []
  for (let i = 0; i < segments; i++) {
    let a = (i / segments) * Math.PI * 2
    let c = Math.cos(a)
    let s = Math.sin(a)
    verts.push(radius * c, radius * s, 0, 0, 0, 1, 0.5 + c * 0.5, 0.5 - s * 0.5)
  }
  for (let i = 0; i < segments; i++) {
    indices.push(0, 1 + i, 1 + ((i + 1) % segments))
  }
  return packGeometry(verts, indices, options)
}

/**
 * A flat annulus in the XY plane facing +z, centered on the origin. UVs
 * are the planar map of the OUTER disc, so a ring textures like the
 * matching circle() with the middle cut out.
 */
export type RingOptions = GeometryOptions & { innerRadius?: number; outerRadius?: number; segments?: number }

export function ring(options: RingOptions = {}): Geometry {
  let { innerRadius = 0.25, outerRadius = 0.5, segments = 32 } = options
  let verts: number[] = []
  let indices: number[] = []
  for (let i = 0; i < segments; i++) {
    let a = (i / segments) * Math.PI * 2
    let c = Math.cos(a)
    let s = Math.sin(a)
    for (let r of [innerRadius, outerRadius]) {
      verts.push(r * c, r * s, 0, 0, 0, 1, 0.5 + (r * c) / (2 * outerRadius), 0.5 - (r * s) / (2 * outerRadius))
    }
  }
  for (let i = 0; i < segments; i++) {
    let j = (i + 1) % segments
    indices.push(i * 2, i * 2 + 1, j * 2 + 1, i * 2, j * 2 + 1, j * 2)
  }
  return packGeometry(verts, indices, options)
}

/** A UV sphere centered on the origin (poles on the y axis). */
export type SphereOptions = GeometryOptions & { radius?: number; widthSegments?: number; heightSegments?: number }

export function sphere(options: SphereOptions = {}): Geometry {
  let { radius = 0.5, widthSegments = 24, heightSegments = 16 } = options
  let verts: number[] = []
  for (let iy = 0; iy <= heightSegments; iy++) {
    let v = iy / heightSegments
    let theta = v * Math.PI
    let sinT = Math.sin(theta)
    let cosT = Math.cos(theta)
    for (let ix = 0; ix <= widthSegments; ix++) {
      let u = ix / widthSegments
      let phi = u * Math.PI * 2
      let nx = -Math.cos(phi) * sinT
      let ny = cosT
      let nz = Math.sin(phi) * sinT
      verts.push(radius * nx, radius * ny, radius * nz, nx, ny, nz, u, v)
    }
  }
  // Both pole rows are collapsed to the pole point.
  let indices = gridIndices(heightSegments, widthSegments, true, true)
  return packGeometry(verts, indices, options)
}
