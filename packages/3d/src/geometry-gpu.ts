// Geometry on the GPU: the lazy buffer step for geometry.ts's data. Buffers
// (and the picking shape - the spatial core's own copy of positions, UVs
// and indices for the triangle narrowphase, one per geometry however many
// meshes share it) are created on first acquire and shared by every mesh
// and scene drawing the geometry; each draw entry holds one reference, and
// the buffers are
// freed when the last reference is released - deferred to a microtask, so
// a same-tick entry rebuild (a material swap, a geometry that comes right
// back) keeps its upload. The handles and the reference count live in a
// map private to this module, keeping Geometry itself plain data.
// disposeGeometry frees immediately, the explicit override; either way the
// geometry stays usable - fresh buffers are created on next acquire.

import { createBuffer, destroyBuffer } from "@solidrt/core/gpu"
import type { BufferId, IndexFormat } from "@solidrt/core/gpu"
import { createShape, destroyShape } from "flux:spatial"
import type { ShapeId } from "flux:spatial"
import { layoutStride } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"

/** An acquired reference to a geometry's GPU buffers: what a draw entry
 * binds, and the token releaseGeometryBuffers takes - releasing the exact
 * acquisition keeps the pairing correct however the caller's geometry
 * fields have moved since. */
export type GeometryBuffers = {
  buffer: BufferId
  index: BufferId
  indexFormat: IndexFormat
  /** The picking shape (positions at 0, uv at 6 of every layout). */
  shape: ShapeId
}

type GpuEntry = GeometryBuffers & { geometry: Geometry; refs: number }

let entries = new WeakMap<Geometry, GpuEntry>()

/** The geometry's GPU buffers, created on first use, plus the index format
 * the draw entry must bind them with. Takes a reference - pair every
 * acquire with a releaseGeometryBuffers of the returned token when the
 * entry built from it goes. */
export function acquireGeometryBuffers(geometry: Geometry): GeometryBuffers {
  let entry = entries.get(geometry)
  if (entry === undefined) {
    entry = {
      geometry,
      buffer: createBuffer(geometry.vertices, {
        autoFree: false,
        label: geometry.label ? geometry.label + "-verts" : undefined,
      }),
      index: createBuffer(geometry.indices, {
        autoFree: false,
        label: geometry.label ? geometry.label + "-indices" : undefined,
      }),
      indexFormat: geometry.indices instanceof Uint32Array ? "uint32" : "uint16",
      shape: createShape(geometry.vertices, layoutStride(geometry.layout), 0, 6, geometry.indices),
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
    destroyBuffer(entry.buffer)
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
  destroyBuffer(entry.buffer)
  destroyBuffer(entry.index)
  destroyShape(entry.shape)
}
