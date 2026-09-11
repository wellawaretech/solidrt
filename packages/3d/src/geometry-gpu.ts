// Geometry on the GPU: the lazy buffer step for geometry.ts's data. Buffers
// (and the shape - the spatial core's own copy of positions, UVs and, for
// a triangle list, indices, one per geometry however many meshes share
// it: the box of every node drawing the geometry and the triangle
// narrowphase) are created on first acquire and shared by every mesh
// and scene drawing the geometry; each draw entry holds one reference, and
// the buffers are
// freed when the last reference is released - deferred to a microtask, so
// a same-tick entry rebuild (a material swap, a geometry that comes right
// back) keeps its upload. The handles and the reference count live in a
// map private to this module, keeping Geometry itself plain data.
// disposeGeometry frees immediately, the explicit override; either way the
// geometry stays usable - fresh buffers are created on next acquire.

import { createBuffer, destroyBuffer, writeBuffer } from "@solidrt/core/gpu"
import type { BufferId, IndexFormat } from "@solidrt/core/gpu"
import { createShape, destroyShape, updateShape } from "flux:spatial"
import type { ShapeId } from "flux:spatial"
import { geometryStreams, geometryTopology, geometryVertexCount, layoutSlot, layoutStride, vertexBytes } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"

/** An acquired reference to a geometry's GPU buffers: what a draw entry
 * binds, and the token releaseGeometryBuffers takes - releasing the exact
 * acquisition keeps the pairing correct however the caller's geometry
 * fields have moved since. */
export type GeometryBuffers = {
  /** One vertex buffer per stream, in stream order: what the entry binds
   * before the material's instance buffers. */
  buffers: BufferId[]
  index: BufferId
  indexFormat: IndexFormat
  /** The core's shape: positions (and, when the layout carries a
   * float32x2 aUV, uvs) for every topology, the box of each node set to
   * it; the triangle narrowphase too for a triangle list. */
  shape: ShapeId
}

type GpuEntry = GeometryBuffers & { geometry: Geometry; streams: ArrayBufferView[]; refs: number }

let entries = new WeakMap<Geometry, GpuEntry>()

/** Vertex uploads keyed by the array itself: geometries sharing one
 * vertex array (a wireframe or edges geometry over its source's, a
 * stream shared through withAttribute) share one GPU buffer, each entry
 * holding a reference to it - which is also what makes an in-place
 * update (updateVertices) reach every sharer. */
let vertexUploads = new WeakMap<ArrayBufferView, { buffer: BufferId; refs: number }>()

function acquireVertices(vertices: ArrayBufferView, label: string | undefined): BufferId {
  let upload = vertexUploads.get(vertices)
  if (upload === undefined) {
    upload = { buffer: createBuffer(vertices, { autoFree: false, label }), refs: 0 }
    vertexUploads.set(vertices, upload)
  }
  upload.refs++
  return upload.buffer
}

function releaseVertices(vertices: ArrayBufferView): void {
  let upload = vertexUploads.get(vertices)
  if (upload === undefined) return
  upload.refs--
  if (upload.refs > 0) return
  vertexUploads.delete(vertices)
  destroyBuffer(upload.buffer)
}

// The shape reads positions (and uvs, when they are plain floats in
// stream 0) through a Float32Array view over the main buffer's bytes:
// every stride and offset is a multiple of 4, so the view is exact
// whatever the layout packs elsewhere. A uv in a packed format or in
// another stream is left out (hits carry no uv); positions are float32x3
// in stream 0 by the layout rule.
type ShapeView = { floats: Float32Array; stride: number; uvAt: number }

function shapeView(geometry: Geometry): ShapeView {
  let v = geometry.vertices
  let floats = new Float32Array(v.buffer, v.byteOffset, v.byteLength / Float32Array.BYTES_PER_ELEMENT)
  let uv = layoutSlot(geometry.layout, "aUV")
  let uvAt = uv !== null && uv.format === "float32x2" ? uv.offset / Float32Array.BYTES_PER_ELEMENT : -1
  return { floats, stride: layoutStride(geometry.layout) / Float32Array.BYTES_PER_ELEMENT, uvAt }
}

// Only a triangle list hands the core its indices: any other topology is
// box-only, its shape there for the box alone.
function createGeometryShape(geometry: Geometry): ShapeId {
  let { floats, stride, uvAt } = shapeView(geometry)
  return createShape(floats, stride, 0, uvAt, geometryTopology(geometry) === "triangles" ? geometry.indices : undefined)
}

/** The geometry's GPU buffers, created on first use, plus the index format
 * the draw entry must bind them with. Takes a reference - pair every
 * acquire with a releaseGeometryBuffers of the returned token when the
 * entry built from it goes. */
export function acquireGeometryBuffers(geometry: Geometry): GeometryBuffers {
  let entry = entries.get(geometry)
  if (entry === undefined) {
    let streams = geometryStreams(geometry).map(s => s.vertices)
    entry = {
      geometry,
      streams,
      buffers: streams.map((vertices, i) => acquireVertices(vertices, geometry.label ? geometry.label + (i === 0 ? "-verts" : "-stream" + i) : undefined)),
      index: createBuffer(geometry.indices, {
        autoFree: false,
        label: geometry.label ? geometry.label + "-indices" : undefined,
      }),
      indexFormat: geometry.indices instanceof Uint32Array ? "uint32" : "uint16",
      shape: createGeometryShape(geometry),
      refs: 0,
    }
    entries.set(geometry, entry)
  }
  entry.refs++
  return entry
}

/** Release one acquire. At zero references the buffers are freed at the
 * end of the microtask; an acquire before then keeps them, so a detach and
 * re-attach in one tick never re-uploads. A token orphaned by an explicit
 * disposeGeometry releases against the orphan, never against a successor's
 * fresh buffers. */
export function releaseGeometryBuffers(acquired: GeometryBuffers): void {
  let entry = acquired as GpuEntry
  if (entry.refs === 0) return
  entry.refs--
  if (entry.refs > 0) return
  queueMicrotask(() => {
    if (entries.get(entry.geometry) !== entry || entry.refs > 0) return
    entries.delete(entry.geometry)
    for (let vertices of entry.streams) releaseVertices(vertices)
    destroyBuffer(entry.index)
    destroyShape(entry.shape)
  })
}

/**
 * Free the geometry's GPU buffers now, held references or not - the
 * explicit override for geometry an app is done with for good. Draw
 * entries created from them hold their own reference, so destruction order
 * is safe; the geometry can be used again afterwards (fresh buffers are
 * created on next use).
 */
export function disposeGeometry(geometry: Geometry): void {
  let entry = entries.get(geometry)
  if (entry === undefined) return
  entries.delete(geometry)
  for (let vertices of entry.streams) releaseVertices(vertices)
  destroyBuffer(entry.index)
  destroyShape(entry.shape)
}

/** Options of `updateVertices`: the stream (default 0, the main buffer)
 * and the vertex range (default the whole stream). */
export type UpdateVerticesOptions = { stream?: number; first?: number; count?: number }

/**
 * Re-upload vertices `[first, first + count)` of one stream from the
 * geometry's own array, in place: Three's `attribute.needsUpdate` with an
 * update range. Write the array first (through geometryAttribute or
 * fillAttribute), then call this; every mesh, view and wireframe over the
 * geometry sees the new bytes, and the other streams are untouched -
 * which is what a per-frame channel in a stream of its own buys. A
 * stream-0 update drops the cached bounds and rewrites the same range of
 * the core's shape, so every node drawing the geometry picks and culls
 * where it is now drawn: the box follows at the next flush, the triangle
 * index is rebuilt by the next query (Three's raycast reads the live
 * attribute the same way; Unity's and Godot's colliders never follow). A
 * geometry not yet on the GPU has nothing to update: its first acquire
 * uploads the array as it is then.
 */
export function updateVertices(geometry: Geometry, options: UpdateVerticesOptions = {}): void {
  let streams = geometryStreams(geometry)
  let index = options.stream ?? 0
  if (!Number.isInteger(index) || index < 0 || index >= streams.length) {
    throw new Error("updateVertices: stream " + index + " is out of range; the geometry has streams 0.." + (streams.length - 1))
  }
  let total = geometryVertexCount(geometry, "updateVertices")
  let first = options.first ?? 0
  let count = options.count ?? total - first
  if (!Number.isInteger(first) || !Number.isInteger(count) || first < 0 || count < 0 || first + count > total) {
    throw new Error("updateVertices: range [" + first + ", " + (first + count) + ") is outside the geometry's " + total + " vertices")
  }
  if (index === 0) geometry._bounds = undefined
  let stream = streams[index]!
  let upload = vertexUploads.get(stream.vertices)
  if (upload === undefined || count === 0) return
  let stride = layoutStride(stream.layout)
  writeBuffer(upload.buffer, vertexBytes(stream.vertices).subarray(first * stride, (first + count) * stride), first * stride)
  let entry = entries.get(geometry)
  if (index === 0 && entry !== undefined) {
    let view = shapeView(geometry)
    updateShape(entry.shape, view.floats.subarray(first * view.stride, (first + count) * view.stride), view.stride, 0, view.uvAt, first)
  }
}
