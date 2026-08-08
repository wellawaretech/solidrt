// Geometry: interleaved vertex data in one of two named layouts - the
// "standard" position vec3, normal vec3, uv vec2 (8 floats per vertex)
// every generator emits, and "colored", which appends an aColor vec4
// (12 floats, derived with withColors) as the per-vertex data channel for
// custom materials (tint, baked AO, any four scalars) - plus
// indices, uint16 or uint32 (the generators here emit uint16; hand-built
// geometry past 64k vertices uses a Uint32Array and the draw entry follows
// the array type). Winding is counter-clockwise seen from outside in the
// y-up world, which the standard camera rig (perspective() with its baked
// y flip) presents as the engine's displayed-CCW front faces: every
// generator here culls correctly with cull: "back". Normals ride along
// unused by the unlit materials so the layout is ready for lights without
// a geometry change (inactive attributes are skipped but keep the stride).
//
// GPU buffers are created lazily on first use and shared by every mesh and
// scene drawing the geometry. They are app-lifetime by design - one
// geometry commonly outlives the component that first drew it, so
// owner-scoped auto-free would free a buffer other scenes still draw from.
// disposeGeometry frees them when an app is done with a geometry for good.

import { createBuffer, destroyBuffer } from "@solidrt/core/gpu"
import type { BufferId, IndexFormat, VertexAttribute } from "@solidrt/core/gpu"
import { add, cross, normalize, sub } from "./math.ts"
import type { Vec2, Vec3, Vec4 } from "./math.ts"

export type VertexLayout = "standard" | "colored"

const STANDARD_ATTRIBUTES: VertexAttribute[] = [
  { name: "aPos", format: "vec3" },
  { name: "aNormal", format: "vec3" },
  { name: "aUV", format: "vec2" },
]

/** The pipeline attribute list for each named layout. A deliberately small
 * set (not an open per-geometry model): every layout shares the standard
 * prefix, so one shader vocabulary serves all of them. */
export const VERTEX_LAYOUTS: Record<VertexLayout, VertexAttribute[]> = {
  standard: STANDARD_ATTRIBUTES,
  colored: [...STANDARD_ATTRIBUTES, { name: "aColor", format: "vec4" }],
}

/** Floats per vertex in the "standard" layout (what every generator emits). */
export const FLOATS_PER_VERTEX = 8
const COLORED_FLOATS = 12

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
  _buffer?: BufferId
  _index?: BufferId
}

/** The geometry's GPU buffers, created on first use and cached on it,
 * plus the index format the draw entry must bind them with. */
export function geometryBuffers(geometry: Geometry): {
  buffer: BufferId
  index: BufferId
  indexFormat: IndexFormat
} {
  let buffer = geometry._buffer
  let index = geometry._index
  if (buffer === undefined || index === undefined) {
    buffer = createBuffer(geometry.vertices, {
      autoFree: false,
      label: geometry.label ? geometry.label + "-verts" : undefined,
    })
    index = createBuffer(geometry.indices, {
      autoFree: false,
      label: geometry.label ? geometry.label + "-indices" : undefined,
    })
    geometry._buffer = buffer
    geometry._index = index
  }
  return { buffer, index, indexFormat: geometry.indices instanceof Uint32Array ? "uint32" : "uint16" }
}

/**
 * Free the geometry's GPU buffers. Draw entries created from them hold
 * their own reference, so destruction order is safe; the geometry can be
 * used again afterwards (fresh buffers are created on next use).
 */
export function disposeGeometry(geometry: Geometry): void {
  if (geometry._buffer !== undefined) destroyBuffer(geometry._buffer)
  if (geometry._index !== undefined) destroyBuffer(geometry._index)
  geometry._buffer = undefined
  geometry._index = undefined
}

/** Per-vertex aColor values for withColors/fillColors: a flat 4-per-vertex
 * array, or a callback deriving each vertex's vec4 from the vertex data. */
export type ColorFill = ArrayLike<number> | ((index: number, pos: Vec3, normal: Vec3, uv: Vec2) => Vec4)

/**
 * Derive a "colored"-layout geometry from a standard one: the same
 * positions, normals, uvs and indices, plus an aColor vec4 per vertex -
 * the data channel for materials whose vertex stage reads `in vec4 aColor`
 * (a tint, baked ambient occlusion, any four scalars; the name is the
 * standard vocabulary, the contents are yours). The callback form receives
 * each vertex's position, normal and uv - what a baker wants. The source
 * geometry is untouched and its GPU buffers stay independent.
 */
export function withColors(geometry: Geometry, fill: ColorFill, label?: string): Geometry {
  if (geometry.layout === "colored") {
    throw new Error("withColors: geometry already carries an aColor channel")
  }
  if (geometry.vertices.length % FLOATS_PER_VERTEX !== 0) {
    throw new Error("withColors: vertex data is not a whole number of standard-layout vertices")
  }
  let count = geometry.vertices.length / FLOATS_PER_VERTEX
  if (typeof fill !== "function" && fill.length !== count * 4) {
    throw new Error("withColors: fill has " + fill.length + " floats, expected 4 per vertex (" + count * 4 + ")")
  }
  let src = geometry.vertices
  let out = new Float32Array(count * COLORED_FLOATS)
  for (let i = 0; i < count; i++) {
    let s = i * FLOATS_PER_VERTEX
    let d = i * COLORED_FLOATS
    for (let k = 0; k < FLOATS_PER_VERTEX; k++) out[d + k] = src[s + k]!
  }
  fillColors(out, fill)
  return {
    vertices: out,
    indices: geometry.indices,
    layout: "colored",
    label: label ?? (geometry.label ? geometry.label + "-colored" : undefined),
  }
}

/**
 * The in-place primitive under withColors: write the aColor slots of a
 * colored-layout interleave you already own - the hook for a merging
 * builder baking colors over its packed buffer (the pos/normal/uv the
 * callback receives are read from the buffer itself, so a packer that
 * bakes transforms while writing hands the baker world-space vertices).
 * Fills vertices [first, first + count) - count defaults to the rest of
 * the buffer - and `fill` indexes relative to `first`, so a per-part
 * callback works unchanged for both APIs. Returns `vertices`.
 *
 * This trusts the buffer to BE colored-layout data - a bare array carries
 * no layout tag, so only the arithmetic is checked. The Geometry-level
 * withColors stays the checked path.
 */
export function fillColors(vertices: Float32Array, fill: ColorFill, first = 0, count?: number): Float32Array {
  if (vertices.length % COLORED_FLOATS !== 0) {
    throw new Error("fillColors: vertex data is not a whole number of colored-layout vertices")
  }
  let total = vertices.length / COLORED_FLOATS
  let n = count ?? total - first
  if (!Number.isInteger(first) || !Number.isInteger(n) || first < 0 || n < 0 || first + n > total) {
    throw new Error("fillColors: range [" + first + ", " + (first + n) + ") is outside the buffer's " + total + " vertices")
  }
  let fn = typeof fill === "function" ? fill : null
  if (!fn && fill.length !== n * 4) {
    throw new Error("fillColors: fill has " + fill.length + " floats, expected 4 per vertex (" + n * 4 + ")")
  }
  for (let i = 0; i < n; i++) {
    let d = (first + i) * COLORED_FLOATS
    let c: Vec4 = fn
      ? fn(i, [vertices[d]!, vertices[d + 1]!, vertices[d + 2]!], [vertices[d + 3]!, vertices[d + 4]!, vertices[d + 5]!], [vertices[d + 6]!, vertices[d + 7]!])
      : [(fill as ArrayLike<number>)[i * 4]!, (fill as ArrayLike<number>)[i * 4 + 1]!, (fill as ArrayLike<number>)[i * 4 + 2]!, (fill as ArrayLike<number>)[i * 4 + 3]!]
    vertices[d + 8] = c[0]
    vertices[d + 9] = c[1]
    vertices[d + 10] = c[2]
    vertices[d + 11] = c[3]
  }
  return vertices
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
export function box(width = 1, height = 1, depth = 1, label?: string): Geometry {
  let x = width / 2
  let y = height / 2
  let z = depth / 2
  let verts: number[] = []
  let indices: number[] = []
  type P = [number, number, number]
  // Corners a (bottom-left) through d (top-left), CCW seen from outside.
  let quad = (a: P, b: P, c: P, d: P, n: P) => {
    let base = verts.length / FLOATS_PER_VERTEX
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
  return { vertices: new Float32Array(verts), indices: new Uint16Array(indices), label }
}

/**
 * A rectangle in the XY plane facing +z, centered on the origin. For a
 * ground plane, rotate it flat: `rotation={[-Math.PI / 2, 0, 0]}`.
 */
export function plane(width = 1, height = 1, label?: string): Geometry {
  let x = width / 2
  let y = height / 2
  // prettier-ignore
  let vertices = new Float32Array([
    -x, -y, 0, 0, 0, 1, 0, 1,
    x, -y, 0, 0, 0, 1, 1, 1,
    x, y, 0, 0, 0, 1, 1, 0,
    -x, y, 0, 0, 0, 1, 0, 0,
  ])
  return { vertices, indices: new Uint16Array([0, 1, 2, 0, 2, 3]), label }
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
export function torusKnot(
  radius = 1,
  tube = 0.4,
  tubularSegments = 64,
  radialSegments = 8,
  p = 2,
  q = 3,
  label?: string,
): Geometry {
  // A point on the knot curve at parameter t (0..2*PI*p).
  let point = (t: number): Vec3 => {
    let qp = (q / p) * t
    let r = radius * (2 + Math.cos(qp)) * 0.5
    return [r * Math.cos(t), radius * Math.sin(qp) * 0.5, r * Math.sin(t)]
  }

  let rows = tubularSegments + 1
  let cols = radialSegments + 1
  let vertices = new Float32Array(rows * cols * FLOATS_PER_VERTEX)
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
      at += FLOATS_PER_VERTEX
    }
  }

  let indices = new Uint16Array(gridIndices(tubularSegments, radialSegments))

  return { vertices, indices, label }
}

/**
 * A capped cylinder on the y axis, centered on the origin. Different top
 * and bottom radii make it a truncated cone (`cone()` is the zero-top
 * case); side normals tilt with the taper. Side UVs: u around the
 * circumference, v 0 at the top to 1 at the bottom; caps get a planar
 * disc map. A zero radius skips that cap and the degenerate side
 * triangles at the apex.
 */
export function cylinder(
  radiusTop = 0.5,
  radiusBottom = 0.5,
  height = 1,
  radialSegments = 24,
  label?: string,
): Geometry {
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
    let base = verts.length / FLOATS_PER_VERTEX
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
  return { vertices: new Float32Array(verts), indices: new Uint16Array(indices), label }
}

/** A capped cone on the y axis, centered on the origin: `cylinder()` with
 * a zero top radius (each apex vertex carries its column's side normal, so
 * the surface shades smoothly around). */
export function cone(radius = 0.5, height = 1, radialSegments = 24, label?: string): Geometry {
  return cylinder(0, radius, height, radialSegments, label)
}

/**
 * A torus lying flat, centered on the origin: the ring lies in the XZ
 * plane with the hole on the y axis - the y-up orientation torusKnot also
 * uses (Three's equivalent stands in XY). Signature order is Three's:
 * radialSegments subdivides the tube cross-section, tubularSegments the
 * ring. UVs: u 0..1 around the ring, v 0..1 around the tube, seam
 * row/column duplicated like torusKnot.
 */
export function torus(
  radius = 0.5,
  tube = 0.2,
  radialSegments = 12,
  tubularSegments = 32,
  label?: string,
): Geometry {
  let rows = tubularSegments + 1
  let cols = radialSegments + 1
  let vertices = new Float32Array(rows * cols * FLOATS_PER_VERTEX)
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
      at += FLOATS_PER_VERTEX
    }
  }
  let indices = new Uint16Array(gridIndices(tubularSegments, radialSegments))
  return { vertices, indices, label }
}

/**
 * A disc in the XY plane facing +z, centered on the origin (rotate flat
 * like plane()). UVs are the planar map of the disc inscribed in the unit
 * square.
 */
export function circle(radius = 0.5, segments = 32, label?: string): Geometry {
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
  return { vertices: new Float32Array(verts), indices: new Uint16Array(indices), label }
}

/**
 * A flat annulus in the XY plane facing +z, centered on the origin. UVs
 * are the planar map of the OUTER disc, so a ring textures like the
 * matching circle() with the middle cut out.
 */
export function ring(innerRadius = 0.25, outerRadius = 0.5, segments = 32, label?: string): Geometry {
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
  return { vertices: new Float32Array(verts), indices: new Uint16Array(indices), label }
}

/** A UV sphere centered on the origin (poles on the y axis). */
export function sphere(radius = 0.5, widthSegments = 24, heightSegments = 16, label?: string): Geometry {
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
  return { vertices: new Float32Array(verts), indices: new Uint16Array(indices), label }
}
