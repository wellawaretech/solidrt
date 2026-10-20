// The package's resource pool: the GPU buffers a geometry's release frees
// and the decoded textures a model's dispose frees wait here for the next
// acquire of the same class instead of being destroyed and created - the
// rule the residency gate's handlers live by (okf/done/spatial-residency-gate.md:
// a buffer create is a blocking call to the raster thread, 1-2 ms on a
// desktop GPU, and a region of models is many of those). One pool for the
// package, shared with the 2d tile layers through setResourcePool, made
// lazily with a constant cap when a region first needs it; until then
// (an app with no streaming) nothing is pooled and a free is a destroy,
// exactly as before. The pool hands out only ids the GPU is done with
// (createResourcePool's `retire`), so a take is never a write into what
// a frame in flight reads. Block-compressed textures are create-once by
// the GPU contract and never pooled.
import { createResourcePool, destroyBuffer, destroyTexture } from "@solidrt/core/gpu"
import type { BufferId, ResourcePool, TextureFormat, TextureId } from "@solidrt/core/gpu"

// Ids the package's default pool keeps, every class together: a few
// regions' worth of geometry buffers and textures on the low-end
// baseline, LRU beyond it.
const DEFAULT_POOL_CAP = 512

// The kinds this package registers on whatever pool it uses (see
// ResourcePool.destroyWith): a consumer's own names, so a shared pool's
// other consumers never collide.
const BUFFER_KIND = "3d-buffer"
const TEXTURE_KIND = "3d-texture"

let pool: ResourcePool | null = null
let installed = new WeakSet<ResourcePool>()

function register(p: ResourcePool): void {
  if (installed.has(p)) return
  installed.add(p)
  p.destroyWith(BUFFER_KIND, id => destroyBuffer(id as BufferId))
  p.destroyWith(TEXTURE_KIND, id => destroyTexture(id as TextureId))
}

/**
 * The resource pool the package's geometry buffers and decoded model
 * textures go through (createResourcePool in @solidrt/core/gpu): pass
 * one to share it with the 2d tile layers and other handlers, null to
 * stop pooling (what is held stays in that pool, which its maker
 * drains). The package registers its own kinds on the pool it is given.
 * Without a call, the first region installs a pool of its own.
 */
export function setResourcePool(next: ResourcePool | null): void {
  if (next !== null) register(next)
  pool = next
}

/** The pool in force, or null while nothing is pooled. */
export function resourcePool(): ResourcePool | null {
  return pool
}

/** The pool, made if none was set: what a region installs. */
export function ensureResourcePool(): ResourcePool {
  if (pool === null) setResourcePool(createResourcePool({ cap: DEFAULT_POOL_CAP, autoFree: false }))
  return pool!
}

/** The pool class of a buffer: interchangeable by byte size. */
export function bufferClass(bytes: number): string {
  return BUFFER_KIND + ":" + bytes
}

/** The pool class of a texture: interchangeable by format, size and
 * whether it carries a mip chain. */
export function textureClass(format: TextureFormat, width: number, height: number, mipmap: boolean): string {
  return TEXTURE_KIND + ":" + format + ":" + width + "x" + height + (mipmap ? ":mip" : "")
}
