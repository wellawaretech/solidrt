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
      manual: true,
      label: geometry.label ? geometry.label + "-verts" : undefined,
    })
    index = createBuffer(geometry.indices, {
      manual: true,
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
