// The resource pool: GPU ids freed by one consumer and taken again by the
// next instead of destroyed and created. A chunk allocation measured 1-2 ms
// on a desktop GPU; a region of models is many of those, so a handler of
// the residency gate (a tile layer's chunks, a 3d region's geometry) gives
// what a leave frees to the pool and takes it back on the next enter. The
// pool is a JS free list over bare ids, since that is where ids live; it
// knows nothing of what an id is. The CONSUMER builds the class string
// `<kind>:<detail>` that says which ids are interchangeable (a shader
// target's pipeline, since setTargetSize resizes id-stable; a buffer's
// byte size) and registers the destroyer of each kind it uses on the pool
// it uses (destroyWith), so one pool can hold buffers and targets
// together and be shared by several consumers without the pool's maker
// knowing any of them. No consumer keeps a free list of its own. Pure JS
// (no GPU imports), so the headless check (tests/pool.test.ts) drives it
// directly.
import { getOwner, onCleanup } from "@solidjs/signals"

export type ResourcePoolOptions = {
  /** How many ids the pool keeps, every class counted together: giving
   * one past the cap frees the least recently given id. */
  cap: number
  /** Skip the owner-scoped auto-drain. */
  autoFree?: boolean
  /**
   * Hand a given id out again only once every frame submitted before its
   * give has retired on the GPU: a give is stamped with `submitted()`, the
   * count of frames handed to the GPU so far, and the id is takeable once
   * `retired()`, the count of frames whose GPU work has completed, has
   * reached that stamp. A frame in flight samples the textures and reads
   * the buffers it was drawn with, and a write into one of those (a bake,
   * an upload, a resize) makes a tiled driver drain the queue first - 27
   * ms per write measured on an Adreno 610
   * (okf/backlog/2d-tile-world-frame-hitches.md). A take that finds only
   * unretired ids of its class misses, and the consumer creates. The gpu
   * module's createResourcePool fills this with the runtime's two counts;
   * a pure pool (the headless test) passes its own or none, and none
   * means every given id is takeable at once.
   */
  retire?: { submitted: () => number; retired: () => number }
}

// Every pool's traffic, summed: what the stats report (gpu.ts registers
// them under poolHits, poolMisses and poolHeld), so a world's reuse rate
// is one number however many pools its handlers share.
let hits = 0
let misses = 0
let held = 0

/** The pools' cumulative takes that found an id (`hits`) and that did not
 * (`misses`, a create followed), and the ids every live pool holds right
 * now (`held`). */
export function poolStats(): { hits: number; misses: number; held: number } {
  return { hits, misses, held }
}

export type ResourcePool = {
  /** The most recently given id of `cls` the GPU is done with
   * (ResourcePoolOptions `retire`), or undefined on a miss (the consumer
   * then creates). A taken id is the consumer's again. */
  take(cls: string): number | undefined
  /** Keep a freed id for the next take of its class. Past `cap` the oldest
   * id in the pool, of whatever class, is freed. */
  give(cls: string, id: number): void
  /** Free every id held, oldest first; the pool stays usable. */
  drain(): void
  /** How many ids the pool holds. */
  readonly size: number
  /**
   * Register the destroyer of a kind's ids (the part of a class before
   * its colon: `destroyBuffer` for the consumer's buffer kind,
   * `destroyTexture` for its target kind). A consumer registers its kinds
   * on whatever pool it was given, so a shared pool needs nothing from
   * its maker; the same function again is a no-op, a different one for
   * a registered kind throws (kinds are a consumer's, name your own). A
   * give under a kind with no destroyer throws.
   */
  destroyWith(kind: string, destroy: (id: number) => void): void
}

/**
 * Create a resource pool of at most `cap` GPU ids (ResourcePoolOptions):
 * `take(cls)` hands back the id most recently given under that class
 * (warm first) among those the GPU is done with (`retire`),
 * `give(cls, id)` keeps a freed id and frees the oldest held past the cap,
 * `drain()` frees them all through the destroyer registered for the id's
 * kind (destroyWith). Drained with the owning reactive scope (opt out with
 * `{ autoFree: false }` and drain yourself).
 */
export function createResourcePool(opts: ResourcePoolOptions): ResourcePool {
  if (!(Number.isInteger(opts.cap) && opts.cap >= 0)) throw new Error(`createResourcePool: cap must be a non-negative integer, got ${opts.cap}`)
  let retire = opts.retire
  if (retire !== undefined && !(typeof retire.submitted === "function" && typeof retire.retired === "function")) {
    throw new Error("createResourcePool: retire needs a submitted() and a retired() frame count")
  }
  let cap = opts.cap
  let destroyers = new Map<string, (id: number) => void>()
  let kindOf = (cls: string): string => {
    let at = cls.indexOf(":")
    return at < 0 ? cls : cls.slice(0, at)
  }
  let free = (id: number, cls: string) => destroyers.get(kindOf(cls))!(id)
  // Held ids in give order, oldest first (so by give stamp too): the cap
  // evicts from the front, a take searches from the back so the warmest
  // retired id of a class comes out, skipping the ids a frame in flight
  // may still read.
  let entries: { cls: string; id: number; at: number }[] = []
  let pool: ResourcePool = {
    take(cls) {
      let retired = retire === undefined ? Infinity : retire.retired()
      for (let i = entries.length - 1; i >= 0; i--) {
        if (entries[i]!.cls !== cls || entries[i]!.at > retired) continue
        let id = entries[i]!.id
        entries.splice(i, 1)
        held--
        hits++
        return id
      }
      misses++
      return undefined
    },
    give(cls, id) {
      if (!destroyers.has(kindOf(cls))) throw new Error(`give: no destroyer registered for kind "${kindOf(cls)}" (class "${cls}"); call destroyWith first`)
      if (cap === 0) {
        free(id, cls)
        return
      }
      entries.push({ cls, id, at: retire === undefined ? 0 : retire.submitted() })
      held++
      while (entries.length > cap) {
        let oldest = entries.shift()!
        held--
        free(oldest.id, oldest.cls)
      }
    },
    drain() {
      let all = entries
      entries = []
      held -= all.length
      for (let entry of all) free(entry.id, entry.cls)
    },
    get size() {
      return entries.length
    },
    destroyWith(kind, destroy) {
      if (typeof destroy !== "function") throw new Error(`destroyWith: the destroyer of kind "${kind}" must be a function`)
      let prior = destroyers.get(kind)
      if (prior === undefined) destroyers.set(kind, destroy)
      else if (prior !== destroy) throw new Error(`destroyWith: kind "${kind}" already has a destroyer from another consumer; name a kind of your own`)
    },
  }
  if (opts.autoFree !== false && getOwner()) onCleanup(() => pool.drain())
  return pool
}
