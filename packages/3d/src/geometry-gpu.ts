// Geometry on the GPU: the lazy buffer step for geometry.ts's data. Buffers
// are created on first use and shared by every mesh and scene drawing the
// geometry. They are app-lifetime by design - one geometry commonly
// outlives the component that first drew it, so owner-scoped auto-free
// would free a buffer other scenes still draw from. disposeGeometry frees
// them when an app is done with a geometry for good.

import { createBuffer, destroyBuffer } from "@solidrt/core/gpu"
import type { BufferId, IndexFormat } from "@solidrt/core/gpu"
import type { Geometry } from "./geometry.ts"

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
