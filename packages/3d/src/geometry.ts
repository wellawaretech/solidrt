// Geometry: interleaved vertex data in the one layout every scene material
// shares - position vec3, normal vec3, uv vec2 (8 floats per vertex) - plus
// uint16 indices. Winding is counter-clockwise seen from outside in the
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
import type { BufferId, VertexAttribute } from "@solidrt/core/gpu"
import { add, cross, normalize, sub } from "./math.ts"
import type { Vec3 } from "./math.ts"

export const VERTEX_LAYOUT: VertexAttribute[] = [
  { name: "aPos", format: "vec3" },
  { name: "aNormal", format: "vec3" },
  { name: "aUV", format: "vec2" },
]
export const FLOATS_PER_VERTEX = 8

export type Geometry = {
  /** Interleaved [pos.xyz, normal.xyz, uv.xy] per vertex. */
  vertices: Float32Array
  indices: Uint16Array
  /** Debug name for the lazily-created GPU buffers. */
  label?: string
  _buffer?: BufferId
  _index?: BufferId
}

/** The geometry's GPU buffers, created on first use and cached on it. */
export function geometryBuffers(geometry: Geometry): { buffer: BufferId; index: BufferId } {
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
  return { buffer, index }
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

  let indices = new Uint16Array(tubularSegments * radialSegments * 6)
  let n = 0
  for (let i = 0; i < tubularSegments; i++) {
    for (let j = 0; j < radialSegments; j++) {
      let a = i * cols + j
      let b = (i + 1) * cols + j
      let c = (i + 1) * cols + j + 1
      let d = i * cols + j + 1
      indices[n++] = a
      indices[n++] = b
      indices[n++] = c
      indices[n++] = a
      indices[n++] = c
      indices[n++] = d
    }
  }

  return { vertices, indices, label }
}

/** A UV sphere centered on the origin (poles on the y axis). */
export function sphere(radius = 0.5, widthSegments = 24, heightSegments = 16, label?: string): Geometry {
  let verts: number[] = []
  let indices: number[] = []
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
  let cols = widthSegments + 1
  for (let iy = 0; iy < heightSegments; iy++) {
    for (let ix = 0; ix < widthSegments; ix++) {
      let a = iy * cols + ix + 1
      let b = iy * cols + ix
      let c = (iy + 1) * cols + ix
      let d = (iy + 1) * cols + ix + 1
      if (iy !== 0) indices.push(a, b, d)
      if (iy !== heightSegments - 1) indices.push(b, c, d)
    }
  }
  return { vertices: new Float32Array(verts), indices: new Uint16Array(indices), label }
}
