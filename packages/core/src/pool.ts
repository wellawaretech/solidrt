// The resource pool: GPU ids freed by one consumer and taken again by the
// next instead of destroyed and created. A chunk allocation measured 1-2 ms
// on a desktop GPU; a region of models is many of those, so a handler of
// the residency gate (a tile layer's chunks, a 3d region's geometry) gives
// what a leave frees to the pool and takes it back on the next enter. The
// pool is a JS free list over bare ids, since that is where ids live; it
// knows nothing of what an id is. The CONSUMER builds the class string
// that says which ids are interchangeable (a shader target's pipeline,
// since setTargetSize resizes id-stable; a buffer's byte size) and gives
// the destroyer for an id that leaves the pool, so one pool can hold
// buffers and targets together and be shared by several consumers. No
// consumer keeps a free list of its own. Pure JS (no GPU imports), so the
// headless check (tests/pool.test.ts) drives it directly.
import { getOwner, onCleanup } from "@solidjs/signals"

export type ResourcePoolOptions = {
  /** How many ids the pool keeps, every class counted together: giving
   * one past the cap frees the least recently given id. */
  cap: number
  /** Destroy an id the pool lets go of (past the cap, or at drain): the
   * consumer's `destroyBuffer` or `destroyTexture`, chosen by the class
   * it built. */
  free: (id: number, cls: string) => void
  /** Skip the owner-scoped auto-drain. */
  autoFree?: boolean
}

export type ResourcePool = {
  /** The most recently given id of `cls`, or undefined on a miss (the
   * consumer then creates). A taken id is the consumer's again. */
  take(cls: string): number | undefined
  /** Keep a freed id for the next take of its class. Past `cap` the oldest
   * id in the pool, of whatever class, is freed. */
  give(cls: string, id: number): void
  /** Free every id held, oldest first; the pool stays usable. */
  drain(): void
  /** How many ids the pool holds. */
  readonly size: number
}

/**
 * Create a resource pool of at most `cap` GPU ids (ResourcePoolOptions):
 * `take(cls)` hands back the id most recently given under that class
 * (warm first), `give(cls, id)` keeps a freed id and frees the oldest held
 * past the cap, `drain()` frees them all. Drained with the owning reactive
 * scope (opt out with `{ autoFree: false }` and drain yourself).
 */
export function createResourcePool(opts: ResourcePoolOptions): ResourcePool {
  if (!(Number.isInteger(opts.cap) && opts.cap >= 0)) throw new Error(`createResourcePool: cap must be a non-negative integer, got ${opts.cap}`)
  if (typeof opts.free !== "function") throw new Error("createResourcePool: free must be a function")
  let cap = opts.cap
  let free = opts.free
  // Held ids in give order, oldest first: the cap evicts from the front,
  // a take searches from the back so the warmest id of a class comes out.
  let held: { cls: string; id: number }[] = []
  let pool: ResourcePool = {
    take(cls) {
      for (let i = held.length - 1; i >= 0; i--) {
        if (held[i]!.cls !== cls) continue
        let id = held[i]!.id
        held.splice(i, 1)
        return id
      }
      return undefined
    },
    give(cls, id) {
      if (cap === 0) {
        free(id, cls)
        return
      }
      held.push({ cls, id })
      while (held.length > cap) {
        let oldest = held.shift()!
        free(oldest.id, oldest.cls)
      }
    },
    drain() {
      let all = held
      held = []
      for (let entry of all) free(entry.id, entry.cls)
    },
    get size() {
      return held.length
    },
  }
  if (opts.autoFree !== false && getOwner()) onCleanup(() => pool.drain())
  return pool
}
