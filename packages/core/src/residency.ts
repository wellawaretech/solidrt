// Region handlers over the spatial core's residency gate: the lifecycle a
// region's content goes through between the gate's enter and leave, built
// once here for every package (a 3d world's cells, a 2d sprite
// population's) and for an app streaming anything else by distance (a
// sound field, a simulation). The GATE decides which regions should exist
// (flux:spatial setRegion/createGrid: viewers, hysteresis, the paced
// enters, the weighted limits); a HANDLER here acts on its two events:
//
//   enter -> load (async data prep, abortable: a file, a decode, an
//            isolate call; skipped when the region's data was retained)
//         -> build (the creates, an iterator stepped under the frame
//            slice budget, `slice` in slice.ts; the content stays hidden)
//         -> shown (the package shows the content, hides the proxy)
//   leave -> abort a load in flight, cancel a build under way (its
//            iterator closes, so a generator's `finally` releases what it
//            made), unload (the app's own state, BEFORE the content goes,
//            so an editable world reads its cell back), teardown (the
//            package removes the content), retain the data (LRU).
//
// A load or build that throws is fail-soft: the region is "failed" and
// empty, the error on the handle and reported once; the next enter
// retries. The gate's own states (pending, stalled) show through the
// handle's `state` while the handler holds nothing. The package hooks
// (`spawn`, `shown`, `teardown`) are what a node form adds; an app with
// no node form passes none.
import * as spatial from "flux:spatial"
import type { GridId, NodeId, RegionSpec } from "flux:spatial"
import { on } from "sol:events"
import { registerStat } from "sol:dev"
import { slice } from "./slice"
import type { Sliced } from "./slice"
import { requestFrame } from "./window"

export type RegionCell = [number, number, number]

/**
 * The scene-wide cap on resident weight (bytes: every region's measured
 * content, or its estimate until measured), null for none (default);
 * Unreal's streaming pool size. Past it the most needed pending region
 * displaces the least needed resident ones, and one nothing can make
 * room for is stalled until a view moves. flux:spatial setResidencyLimit.
 */
export function setResidencyLimit(bytes: number | null): void {
  spatial.setResidencyLimit(bytes)
}

/**
 * The region enters delivered per frame scene-wide (default 16): every
 * grid and region spends the one budget, the highest priority then the
 * nearest first. flux:spatial setResidencyBudget.
 */
export function setResidencyBudget(enters: number): void {
  if (!(Number.isInteger(enters) && enters >= 0)) throw new Error(`setResidencyBudget: enters per frame must be a non-negative integer, got ${enters}`)
  spatial.setResidencyBudget(enters)
}

/** Where a region stands: the gate's "pending" (wanted, its enter not
 * yet delivered) and "stalled" (held back by a limit until a view moves),
 * then the handler's "loading" (`load` under way), "building" (`build`
 * stepped under the slice budget), "shown" (built and showing), "failed"
 * (a load or build threw; empty until the next enter); "unwanted" while
 * no viewer wants it. */
export type RegionState = "unwanted" | "pending" | "stalled" | "loading" | "building" | "shown" | "failed"

/**
 * What a region's content is and how it comes and goes: `load` prepares
 * the data (async, abortable through the signal: a file read, a decode,
 * an isolate call - whatever the thread must not wait on; absent, `build`
 * gets `undefined`), `build` makes the content from it (a generator
 * yielding between creates runs one step per slice of the frame budget;
 * a plain function runs as one step), `unload` runs on leave BEFORE the
 * content is torn down, with the data, for the app's own state.
 */
export type RegionHandler<T, R> = {
  load?: (region: R, signal: AbortSignal) => T | Promise<T>
  build: (region: R, data: T) => void | Iterator<void>
  unload?: (region: R, data: T) => void
}

/** The hooks a package's node form adds around the handler: `spawn` as
 * the enter is handled (make the region's node), `shown` when the build
 * finished (show the content, hide the proxy), `teardown` on leave after
 * `unload` (remove the content; for a grid cell, the node too). */
export type RegionHooks<R> = {
  spawn?: (region: R) => void
  shown?: (region: R) => void
  teardown?: (region: R) => void
}

export type Region<T> = {
  /** The region's state (RegionState). */
  readonly state: RegionState
  /** The loaded data while the region holds any (from `load` on, until
   * the leave's teardown). */
  readonly data: T | undefined
  /** What the last load or build threw, while "failed". */
  readonly error: unknown
  /** The cell index of a grid region; null for an explicit one. */
  readonly cell: RegionCell | null
  /** The handle the grid form was created by; null for an explicit region. */
  readonly grid: RegionGrid<T> | null
  readonly label: string | undefined
  /** The core node an explicit region is attached to (attach), or null. */
  readonly node: NodeId | null
  /** Report the weight the content measures (its GPU bytes, say): what
   * the gate's limits weigh the region by from here on, in place of the
   * declared estimate. Ignored while the region is not wanted. */
  measure(weight: number): void
  /** Explicit regions: declare the region on a core node with a box (the
   * gate tests it from the next flush); the node's handler is this
   * region until `detach`. Throws for a grid region. */
  attach(node: NodeId): void
  /** Explicit regions: end the declaration; content still held leaves
   * now (unload, teardown), since the gate's own leave would arrive too
   * late to name this handler. */
  detach(): void
  /** Assignable: called after every state change. */
  onStateChange?: (region: Region<T>, state: RegionState) => void
  /** Assignable: called with what a load or build threw (the region is
   * then "failed"; the error is reported to the console when no
   * listener is set). */
  onError?: (region: Region<T>, error: unknown) => void
  /** @internal */
  _lifecycle: Lifecycle<T>
}

export type RegionOptions<T> = RegionHandler<T, Region<T>> & {
  /** Wanted within this distance of a viewer's view volume. */
  enter: number
  /** Kept until beyond this distance (at least `enter`). */
  leave: number
  /** The estimated weight the gate's limits admit the region by until
   * `measure` reports the content's (default 0: free under every limit). */
  weight?: number
  /** An integer: a higher priority enters first whatever the distance
   * and is evicted last (default 0). */
  priority?: number
  /** Keep the data of a left region so the next enter skips `load`
   * (default false). */
  retain?: boolean
  label?: string
  hooks?: RegionHooks<Region<T>>
}

export type RegionGridOptions<T> = RegionHandler<T, Region<T>> & {
  /** The world corner of cell [0, 0, 0]. */
  origin: [number, number, number]
  /** The cell size per axis; 0 = unbounded along that axis. */
  cell: [number, number, number]
  enter: number
  leave: number
  /** An inclusive cell-index box outside which no cell exists. */
  bounds?: [number, number, number, number, number, number]
  /** A period in cells per axis (0 = none): a torus, the canonical cell
   * reported once. */
  wrap?: [number, number, number]
  /** The estimated weight of one cell until `measure` (default 1). */
  weight?: number
  /** The grid's own cap on resident weight. */
  limit?: number
  /** An integer priority among grids and regions (default 0). */
  priority?: number
  /** How many left cells' data to keep (LRU) so a re-enter skips `load`
   * (default 0). */
  retain?: number
  label?: string
  hooks?: RegionHooks<Region<T>>
}

export type RegionGrid<T> = {
  /** Declare the grid to the gate (cells enter from the next flush). */
  start(): void
  /** End the grid: every held cell leaves now (unload, teardown). */
  stop(): void
  /** The gate's id while started, else null. */
  readonly grid: GridId | null
  readonly label: string | undefined
  /** The cells the handler holds (from enter to leave), fresh array. */
  cells(): Region<T>[]
  /** The held cell at an index, or null. */
  cellAt(i: number, j: number, k: number): Region<T> | null
  /** Re-cap the grid's own limit (null = none) or its estimate per cell. */
  setLimit(limit: number | null): void
  setWeight(weight: number): void
  /** Drop the retained data. */
  forget(): void
  /** Assignable: called after every cell's state change. */
  onStateChange?: (region: Region<T>, state: RegionState) => void
  onError?: (region: Region<T>, error: unknown) => void
  /** Stop and release everything. */
  dispose(): void
}

// A retained-data store: the last `cap` entries by key, oldest dropped.
class Retainer<T> {
  private entries = new Map<string, T>()
  constructor(private cap: number) {}
  take(key: string): { data: T } | undefined {
    if (!this.entries.has(key)) return undefined
    let data = this.entries.get(key)!
    this.entries.delete(key)
    return { data }
  }
  keep(key: string, data: T): void {
    if (this.cap <= 0) return
    this.entries.delete(key)
    this.entries.set(key, data)
    while (this.entries.size > this.cap) {
      let oldest = this.entries.keys().next().value as string
      this.entries.delete(oldest)
    }
  }
  clear(): void {
    this.entries.clear()
  }
}

// What every region form shares: the enter-to-leave machine.
type Lifecycle<T> = {
  region: Region<T>
  handler: RegionHandler<T, Region<T>>
  hooks: RegionHooks<Region<T>> | undefined
  retainer: Retainer<T>
  key: string
  /** Bumped by every enter and leave: a settled load or build from an
   * earlier generation is dropped. */
  generation: number
  held: RegionState
  data: T | undefined
  hasData: boolean
  error: unknown
  abort: AbortController | null
  job: Sliced | null
  notify: ((region: Region<T>, state: RegionState) => void) | undefined
  report: ((region: Region<T>, error: unknown) => void) | undefined
}

// The handlers the gate's events reach: explicit regions by node, grids
// by id. One subscription, installed at the first region.
let byNode = new Map<NodeId, Lifecycle<unknown>>()
let byGrid = new Map<GridId, GridImpl<unknown>>()
let subscribed = false
type RegionEvent = { node?: NodeId; grid?: GridId; cell?: RegionCell }

function subscribe(): void {
  if (subscribed) return
  subscribed = true
  on("spatialRegionEnter", (event: RegionEvent) => {
    if (event.node !== undefined) {
      let life = byNode.get(event.node)
      if (life !== undefined) enter(life)
    } else if (event.grid !== undefined && event.cell !== undefined) byGrid.get(event.grid)?.enterCell(event.cell)
  })
  on("spatialRegionLeave", (event: RegionEvent) => {
    if (event.node !== undefined) {
      let life = byNode.get(event.node)
      if (life !== undefined) leave(life)
    } else if (event.grid !== undefined && event.cell !== undefined) byGrid.get(event.grid)?.leaveCell(event.cell)
  })
}

// The stats: regions the handlers hold by state.
let counts = { loading: 0, building: 0, shown: 0, failed: 0 }
registerStat("regionsLoading", () => counts.loading)
registerStat("regionsBuilding", () => counts.building)
registerStat("regionsShown", () => counts.shown)
registerStat("regionsFailed", () => counts.failed)

function setHeld<T>(life: Lifecycle<T>, state: RegionState): void {
  let was = life.held
  if (was in counts) counts[was as keyof typeof counts]--
  life.held = state
  if (state in counts) counts[state as keyof typeof counts]++
  life.notify?.(life.region, life.region.state)
}

function fail<T>(life: Lifecycle<T>, error: unknown): void {
  life.error = error
  life.abort = null
  life.job = null
  setHeld(life, "failed")
  life.report?.(life.region, error)
}

function enter<T>(life: Lifecycle<T>): void {
  if (life.held !== "unwanted") return
  life.generation++
  life.error = undefined
  life.hooks?.spawn?.(life.region)
  let retained = life.retainer.take(life.key)
  if (retained !== undefined) {
    build(life, retained.data)
    return
  }
  if (life.handler.load === undefined) {
    build(life, undefined as T)
    return
  }
  let generation = life.generation
  let abort = new AbortController()
  life.abort = abort
  setHeld(life, "loading")
  Promise.resolve()
    .then(() => life.handler.load!(life.region, abort.signal))
    .then(
      data => {
        if (generation !== life.generation) return
        life.abort = null
        build(life, data)
      },
      error => {
        if (generation !== life.generation) return
        fail(life, error)
      },
    )
}

function build<T>(life: Lifecycle<T>, data: T): void {
  let generation = life.generation
  life.data = data
  life.hasData = true
  setHeld(life, "building")
  let handler = life.handler
  let region = life.region
  // One job whatever the build returns: a plain build is one step, run
  // in the frame's slice slot in queue order like the rest.
  let steps = (function* () {
    let made = handler.build(region, data)
    if (made !== undefined && made !== null && typeof (made as Iterator<void>).next === "function") yield* made as Iterable<void> & Iterator<void>
  })()
  // Settled inside the frame's drain, so the content shows in the frame
  // its last step ran in.
  let job = slice(steps, {
    label: region.label,
    onSettle(state, error) {
      if (generation !== life.generation) return
      if (state === "done") {
        life.job = null
        setHeld(life, "shown")
        life.hooks?.shown?.(region)
      } else if (state === "failed") fail(life, error)
    },
  })
  life.job = job
  job.done.catch(() => {})
}

function leave<T>(life: Lifecycle<T>): void {
  if (life.held === "unwanted") return
  life.generation++
  life.abort?.abort()
  life.abort = null
  life.job?.cancel()
  life.job = null
  if (life.hasData) {
    try {
      life.handler.unload?.(life.region, life.data as T)
    } catch (error) {
      console.error("A region's unload threw" + (life.region.label !== undefined ? ` (${life.region.label})` : "") + ":", error)
    }
  }
  life.hooks?.teardown?.(life.region)
  if (life.hasData) life.retainer.keep(life.key, life.data as T)
  life.data = undefined
  life.hasData = false
  setHeld(life, "unwanted")
}

/** The state the handle reports: what the handler holds, else what the
 * gate says (pending or stalled), else unwanted. */
function gateState(read: () => spatial.RegionState | null): RegionState {
  let state: spatial.RegionState | null
  try {
    state = read()
  } catch {
    return "unwanted"
  }
  return state === "pending" || state === "stalled" ? state : "unwanted"
}

function makeLifecycle<T>(
  handler: RegionHandler<T, Region<T>>,
  hooks: RegionHooks<Region<T>> | undefined,
  retainer: Retainer<T>,
  key: string,
  shape: { cell: RegionCell | null; grid: RegionGrid<T> | null; label: string | undefined },
  gate: () => spatial.RegionState | null,
  measure: (weight: number) => void,
  attach: (node: NodeId) => void,
  detach: (node: NodeId) => void,
): Lifecycle<T> {
  let life: Lifecycle<T> = {
    region: null!,
    handler,
    hooks,
    retainer,
    key,
    generation: 0,
    held: "unwanted",
    data: undefined,
    hasData: false,
    error: undefined,
    abort: null,
    job: null,
    notify: undefined,
    report: undefined,
  }
  let attached: NodeId | null = null
  let region: Region<T> = {
    get state() {
      return life.held !== "unwanted" ? life.held : gateState(gate)
    },
    get data() {
      return life.data
    },
    get error() {
      return life.error
    },
    cell: shape.cell,
    grid: shape.grid,
    label: shape.label,
    get node() {
      return attached
    },
    measure(weight) {
      if (!(Number.isFinite(weight) && weight >= 0)) throw new Error(`measure: weight must be finite and not negative, got ${weight}`)
      if (life.held === "unwanted") return
      measure(weight)
    },
    attach(node) {
      if (shape.cell !== null) throw new Error("attach: a grid cell is bound by its grid")
      if (attached === node) return
      if (attached !== null) region.detach()
      attached = node
      attach(node)
    },
    detach() {
      if (attached === null) return
      leave(life)
      let node = attached
      attached = null
      detach(node)
    },
    onStateChange: undefined,
    onError: undefined,
    _lifecycle: life,
  }
  life.region = region
  life.notify = (r, s) => {
    region.onStateChange?.(r, s)
    ;(shape.grid as GridImpl<T> | null)?.notifyCell(r, s)
  }
  // The error goes to the region's listener, else its grid's, else the
  // console - resolved at the failure, so a listener set later counts.
  life.report = (r, e) => {
    let listener = region.onError ?? (shape.grid as GridImpl<T> | null)?.onError
    if (listener !== undefined) listener(r, e)
    else console.error("A region's load or build failed" + (r.label !== undefined ? ` (${r.label})` : "") + ":", e)
  }
  return life
}

/**
 * An explicit region handler: the lifecycle above for a region the gate
 * tests as one bounded core node (`attach` declares it with the node's
 * box, `detach` ends it). The 3d package's createRegion wraps this with
 * a group node; an app streaming non-scene content by distance uses it
 * directly over a bounded node of its own. Its data is kept between
 * visits under `retain`.
 */
export function createRegion<T = undefined>(opts: RegionOptions<T>): Region<T> {
  checkMargins(opts, "createRegion")
  let spec: RegionSpec = { enter: opts.enter, leave: opts.leave, weight: opts.weight, priority: opts.priority }
  let life: Lifecycle<T> = makeLifecycle<T>(
    opts,
    opts.hooks,
    new Retainer<T>(opts.retain === true ? 1 : 0),
    "region",
    { cell: null, grid: null, label: opts.label },
    (): spatial.RegionState | null => (life.region.node === null ? null : spatial.regionState(life.region.node)),
    (weight: number): void => {
      if (life.region.node !== null) spatial.setRegionWeight(life.region.node, weight)
    },
    node => {
      subscribe()
      spatial.setRegion(node, spec)
      byNode.set(node, life as Lifecycle<unknown>)
      // The declaration asks for the flush that first tests it.
      requestFrame()
    },
    node => {
      byNode.delete(node)
      try {
        spatial.setRegion(node, null)
      } catch {
        // The node went with its scene before the detach: the gate ended
        // the region with it.
      }
    },
  )
  return life.region
}

function checkMargins(opts: { enter: number; leave: number; weight?: number; priority?: number }, api: string): void {
  if (!(Number.isFinite(opts.enter) && opts.enter >= 0)) throw new Error(`${api}: enter must be a finite distance >= 0, got ${opts.enter}`)
  if (!(Number.isFinite(opts.leave) && opts.leave >= opts.enter)) throw new Error(`${api}: leave must be a finite distance >= enter, got ${opts.leave}`)
  if (opts.weight !== undefined && !(Number.isFinite(opts.weight) && opts.weight >= 0)) throw new Error(`${api}: weight must be finite and not negative, got ${opts.weight}`)
  if (opts.priority !== undefined && !Number.isInteger(opts.priority)) throw new Error(`${api}: priority must be an integer, got ${opts.priority}`)
}

class GridImpl<T> implements RegionGrid<T> {
  grid: GridId | null = null
  label: string | undefined
  onStateChange?: (region: Region<T>, state: RegionState) => void
  onError?: (region: Region<T>, error: unknown) => void
  private held = new Map<string, Lifecycle<T>>()
  private retainer: Retainer<T>
  private disposed = false
  constructor(private opts: RegionGridOptions<T>) {
    this.label = opts.label
    this.retainer = new Retainer<T>(opts.retain ?? 0)
  }

  start(): void {
    if (this.disposed) throw new Error("start: the grid was disposed")
    if (this.grid !== null) return
    subscribe()
    let o = this.opts
    this.grid = spatial.createGrid({
      origin: o.origin,
      cell: o.cell,
      enter: o.enter,
      leave: o.leave,
      bounds: o.bounds,
      wrap: o.wrap,
      weight: o.weight,
      limit: o.limit,
      priority: o.priority,
      label: o.label,
    })
    byGrid.set(this.grid, this as GridImpl<unknown>)
    requestFrame()
  }

  stop(): void {
    if (this.grid === null) return
    for (let life of [...this.held.values()]) this.leaveLife(life)
    byGrid.delete(this.grid)
    spatial.destroyGrid(this.grid)
    this.grid = null
  }

  cells(): Region<T>[] {
    return [...this.held.values()].map(l => l.region)
  }

  cellAt(i: number, j: number, k: number): Region<T> | null {
    return this.held.get(cellKey([i, j, k]))?.region ?? null
  }

  setLimit(limit: number | null): void {
    this.opts = { ...this.opts, limit: limit ?? undefined }
    if (this.grid !== null) spatial.setGridLimit(this.grid, limit)
  }

  setWeight(weight: number): void {
    this.opts = { ...this.opts, weight }
    if (this.grid !== null) spatial.setGridWeight(this.grid, weight)
  }

  forget(): void {
    this.retainer.clear()
  }

  dispose(): void {
    if (this.disposed) return
    this.stop()
    this.retainer.clear()
    this.disposed = true
  }

  notifyCell(region: Region<T>, state: RegionState): void {
    this.onStateChange?.(region, state)
  }

  enterCell(cell: RegionCell): void {
    let key = cellKey(cell)
    if (this.held.has(key)) return
    let grid = this.grid
    if (grid === null) return
    let life = makeLifecycle<T>(
      this.opts,
      this.opts.hooks,
      this.retainer,
      key,
      { cell, grid: this, label: this.opts.label },
      () => (this.grid === null ? null : spatial.cellState(this.grid, cell)),
      weight => {
        if (this.grid !== null) spatial.setCellWeight(this.grid, cell, weight)
      },
      () => {
        throw new Error("attach: a grid cell is bound by its grid")
      },
      () => {},
    )
    this.held.set(key, life)
    enter(life)
  }

  leaveCell(cell: RegionCell): void {
    let life = this.held.get(cellKey(cell))
    if (life !== undefined) this.leaveLife(life)
  }

  private leaveLife(life: Lifecycle<T>): void {
    leave(life)
    this.held.delete(life.key)
  }
}

function cellKey(cell: RegionCell): string {
  return cell[0] + "," + cell[1] + "," + cell[2]
}

/**
 * A grid of region handlers: the lifecycle above for every cell the gate
 * wants (flux:spatial createGrid), one Region handle per held cell from
 * its enter to its leave, `cell` carrying the index. `start` declares
 * the grid, `stop` ends it (every held cell leaves). The 3d package's
 * createRegionGrid wraps this with a group node per cell.
 */
export function createRegionGrid<T = undefined>(opts: RegionGridOptions<T>): RegionGrid<T> {
  checkMargins(opts, "createRegionGrid")
  if (opts.retain !== undefined && !(Number.isInteger(opts.retain) && opts.retain >= 0)) throw new Error(`createRegionGrid: retain must be a count >= 0, got ${opts.retain}`)
  return new GridImpl<T>(opts)
}
