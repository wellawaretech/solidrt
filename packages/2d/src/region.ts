// Regions: the 2d form of core's region handlers (@solidrt/core
// createRegion/createRegionGrid over the spatial core's residency gate,
// the contract in okf/done/spatial-residency-gate.md; @solidrt/3d's
// region.ts is the same shape one dimension up). A region is a sprite
// GROUP whose CONTENT exists while a viewer wants it: an explicit region
// is a bounded group you place in the layer (a level section, a boss
// room; Unreal's level streaming volume), a region grid a root group
// whose every wanted cell becomes a child region at the cell's corner
// (Unreal's World Partition, Minecraft's chunk columns), so a cell's
// sprites are cell-local and float precision is per cell - a city's cars
// by block, a bullet-hell's enemies by zone, where the tile layer
// (tiles.ts) streams the ground. Every view of the layer is a viewer
// (views.ts pushes it), a streaming source is one more; the gate decides
// and paces; core's lifecycle loads, builds under the frame slice budget,
// unloads; this module adds what is 2d: the groups, the content hidden
// (and skipped by pick and the queries) until built, the proxy shown
// while it is not, the measured record bytes reported to the gate's
// limits, and the snapping teardown (a region leaves off screen: no exit
// plays, every slot recycles this flush).
//
// Ownership: a region owns what sits under its content and destroys it
// at leave. A sprite that must outlive its cell (an enemy chasing the
// player across the border, a projectile) is re-homed by the app -
// `setSpriteParent(sprite, elsewhere, { keepWorld: true })` as it
// crosses, or in `unload` at the latest - the way Minecraft moves an
// entity between chunks; a handle kept to a sprite the region destroyed
// is inert (`layer` null), never a dangling reference.
import { createRegion as createCoreRegion, createRegionGrid as createCoreGrid, createStreamingSource as createCoreStreamingSource } from "@solidrt/core"
import type { Region as CoreRegion, RegionGrid as CoreGrid, RegionHandler, RegionState } from "@solidrt/core"
import * as spatial from "flux:spatial"
import { addGroup, destroyGroup, destroySprite, POSE_FLOATS, setGroup, setSpriteParent, SPRITE_FLOATS } from "./layer.ts"
import type { GroupState, Sprite, SpriteGroup, SpriteLayer } from "./layer.ts"

export type { RegionState } from "@solidrt/core"

/** A grid cell's index, [i, j] (the core's third axis is the plane's
 * normal, always 0 here). */
export type RegionCell = [number, number]

/** The default residency band of a grid in cells of the largest finite
 * axis: wanted within half a cell of a view, kept until one and a half
 * cells away - the tile layer's defaults, one rule for the package. */
export const DEFAULT_ENTER_CELLS = 0.5
export const DEFAULT_LEAVE_CELLS = 1.5
/** Half depth of an explicit region's box along the plane's normal: the
 * viewers are orthographic views of the plane (a unit deep) and
 * streaming sources cubes about z = 0, so any slab across the plane is
 * met; a unit keeps a 3d camera looking at the plane from afar from
 * wanting it by its depth alone. */
const REGION_HALF_DEPTH = 1

/**
 * What a region's content is and how it comes and goes, the 2d form of
 * core's RegionHandler: `load` prepares the data off the thread (a file
 * read, a decode, a generator in a `"use isolate"` module; abortable
 * through the signal; absent, `build` gets `undefined`), `build` makes
 * the content under `region.content` - `addSprite(layer, { parent:
 * region.content, ... })` for each, positions cell-local; as a generator,
 * a `yield` after every batch of a few hundred spreads the creates over
 * frames under the slice budget (setSliceBudget) - `unload` runs on leave
 * BEFORE the content goes, with the data (an editable world reads its
 * cell back here; a sprite to keep is re-homed here at the latest), and
 * `proxy` makes the sprite or group drawn while the region is NOT shown
 * (a single far-ring stand-in, a silhouette; Unreal's HLOD), under
 * `region.group`: destroyed once the content shows, made again at leave.
 * Most layers pass none: a 2d far ring is the mip chain's job.
 */
export type RegionContent<T> = {
  load?: (region: Region<T>, signal: AbortSignal) => T | Promise<T>
  build: (region: Region<T>, data: T) => void | Iterator<void>
  unload?: (region: Region<T>, data: T) => void
  proxy?: (region: Region<T>) => Sprite | SpriteGroup
}

/**
 * A region handle. `group` is its node in the layer (at the cell's
 * corner for a grid cell; where you placed it for an explicit region),
 * `content` the child group what `build` made sits under, hidden until
 * the build completes (so pick, pickRect and the queries skip it), and
 * `proxy` the stand-in while the content is not shown. `state` is the
 * gate's "pending"/"stalled" until the enter, then "loading",
 * "building", "shown" or "failed" (RegionState); `data` the loaded data
 * while held; `cell` the index of a grid cell. Leaves snap: the content
 * is destroyed with no exit playing.
 */
export type Region<T = unknown> = {
  readonly layer: SpriteLayer
  /** The region's group: the cell's corner, or the explicit region's
   * placement (move it with setGroup; the box moves with it). */
  readonly group: SpriteGroup
  /** Where the content goes: `addSprite(layer, { parent: region.content
   * })`. Hidden until the build completes. */
  readonly content: SpriteGroup
  /** The proxy the handler made, while the region holds one. */
  readonly proxy: Sprite | SpriteGroup | null
  readonly state: RegionState
  readonly data: T | undefined
  readonly error: unknown
  readonly cell: RegionCell | null
  readonly label: string | undefined
  /** Measure the content's record bytes again (every sprite under the
   * content: its pose and sprite records and the material's style
   * records) and report them to the gate's limits - done once as the
   * build completes; call it after content added later, as the
   * component form does when its JSX mounts. Returns the bytes. */
  measure(): number
  /** Assignable: called after every state change. */
  onStateChange?: (region: Region<T>, state: RegionState) => void
  /** Assignable: called with what a load or build threw (the region is
   * then "failed"; reported to the console when no listener is set). */
  onError?: (region: Region<T>, error: unknown) => void
  /** @internal */
  _core: CoreRegion<T>
}

export type RegionOptions<T> = RegionContent<T> & {
  /** The group's position in its parent's frame (layer pixels at the
   * root): the box's top-left corner. */
  x: number
  y: number
  /** The box the gate tests, from the group's origin, in its frame. */
  width: number
  height: number
  /** Wanted within this distance of a viewer's view (layer pixels at
   * the root). */
  enter: number
  /** Kept until beyond this distance (at least `enter`). */
  leave: number
  /** The estimated record bytes the gate's limits admit the region by
   * until its content is measured (default 0: free under every limit). */
  weight?: number
  /** An integer: a higher priority enters first whatever the distance
   * and is evicted last (default 0). */
  priority?: number
  /** Keep the data of a left region so the next enter skips `load`
   * (default false). */
  retain?: boolean
  /** The group the region's group mounts under (null, the default: the
   * layer root). The gate tests the box through the world matrix, so a
   * moving parent carries the region. */
  parent?: SpriteGroup | null
  label?: string
}

export type RegionGridOptions<T> = RegionContent<T> & {
  /** The cell size per axis in layer pixels; 0 = unbounded along that
   * axis (`[512, 0]` streams a side-scroller by column). Not both 0. */
  cell: [number, number]
  /** The layer-pixel corner of cell [0, 0] (default the origin). */
  origin?: [number, number]
  /** The residency band in layer pixels: a cell enters while within
   * `enter` of a view and stays until beyond `leave` (`leave >=
   * enter`). Defaults: half a cell and one and a half cells (of the
   * largest finite axis), the tile layer's. Cover half the largest
   * sprite's extent with `enter`, or a sprite near a cell edge pops. */
  enter?: number
  leave?: number
  /** An inclusive cell-index box `[minI, minJ, maxI, maxJ]` outside which
   * no cell exists. */
  bounds?: [number, number, number, number]
  /** The estimated record bytes of one cell until its content is
   * measured (default 1). */
  weight?: number
  /** The grid's own cap on resident bytes (see setResidencyLimit for how
   * a cap evicts). */
  limit?: number
  /** An integer priority among grids and regions (default 0): a ground
   * grid before its decoration grid when the enter budget is short. */
  priority?: number
  /** How many left cells' data to keep (LRU) so a re-enter skips `load`
   * (default 0). */
  retain?: number
  label?: string
}

/**
 * A region grid: a root group of the layer (the gate's grid lives in
 * layer space, so the group sits at the root, at identity); every cell a
 * viewer wants is a child Region at the cell's corner for as long as it
 * is held. A sprite grid has no `wrap`: the layer draws no seam copies
 * of a cell (okf/done/2d-wrap-around.md has the recipe), so a wanted cell
 * could only ever be drawn at its canonical place.
 */
export type RegionGrid<T = unknown> = {
  readonly layer: SpriteLayer
  /** The grid's group, the cells' parent. */
  readonly group: SpriteGroup
  /** The held cells (from enter to leave), fresh array. */
  cells(): Region<T>[]
  cellAt(i: number, j: number): Region<T> | null
  /** The layer-pixel corner of a cell. */
  cellOrigin(cell: RegionCell): [number, number]
  /** Re-cap the grid's own byte limit (null = none). */
  setLimit(limit: number | null): void
  /** Re-estimate the bytes of an unmeasured cell. */
  setWeight(weight: number): void
  /** Drop the retained data. */
  forget(): void
  readonly label: string | undefined
  /** Assignable: called after every cell's state change. */
  onStateChange?: (region: Region<T>, state: RegionState) => void
  onError?: (region: Region<T>, error: unknown) => void
  /** End the grid (every held cell leaves) and destroy the group. */
  dispose(): void
  /** @internal */
  _core: CoreGrid<T>
}

// The 2d handle behind a core region: made at spawn (a grid cell) or at
// creation (an explicit region), looked up from the core handle in the
// handler's calls.
let handleOf = new WeakMap<CoreRegion<unknown>, Region<unknown>>()

// The handle's 2d half: the groups, the proxy, the proxy maker.
type HandleState<T> = {
  region: Region<T>
  layer: SpriteLayer
  group: SpriteGroup
  content: SpriteGroup
  proxy: Sprite | SpriteGroup | null
  makeProxy: RegionContent<T>["proxy"]
}

let stateOf = new WeakMap<Region<unknown>, HandleState<unknown>>()

function makeHandle<T>(layer: SpriteLayer, core: CoreRegion<T>, group: SpriteGroup, cell: RegionCell | null, label: string | undefined, makeProxy: RegionContent<T>["proxy"], grid: RegionGrid<T> | null): Region<T> {
  let content = addGroup(layer, { parent: group, visible: false })
  let state: HandleState<T> = { region: null!, layer, group, content, proxy: null, makeProxy }
  let region: Region<T> = {
    layer,
    group,
    content,
    get proxy() {
      return state.proxy
    },
    get state() {
      return core.state
    },
    get data() {
      return core.data
    },
    get error() {
      return core.error
    },
    cell,
    label,
    measure() {
      let bytes = contentBytes(state)
      core.measure(bytes)
      return bytes
    },
    onStateChange: undefined,
    onError: undefined,
    _core: core,
  }
  state.region = region
  core.onStateChange = (_, s) => region.onStateChange?.(region, s)
  // The error goes to the region's listener, else its grid's, else the
  // console (core would fall through the same way, but sees only this
  // forwarder); resolved at the failure, so a listener set later counts.
  core.onError = (_, e) => {
    let listener = region.onError ?? grid?.onError
    if (listener !== undefined) listener(region, e)
    else console.error("A region's load or build failed" + (label !== undefined ? ` (${label})` : "") + ":", e)
  }
  handleOf.set(core as CoreRegion<unknown>, region as Region<unknown>)
  stateOf.set(region as Region<unknown>, state as HandleState<unknown>)
  if (makeProxy !== undefined) showProxy(state)
  return region
}

function showProxy<T>(state: HandleState<T>): void {
  if (state.proxy !== null || state.makeProxy === undefined) return
  let proxy = state.makeProxy(state.region)
  if (proxy.layer !== state.layer) throw new Error("proxy: the stand-in belongs to another layer")
  // The maker may have mounted it anywhere: it sits under the region's
  // group, where it moves with the region and dies with it.
  if (proxy._parent !== state.group) {
    if ("_children" in proxy) setGroup(proxy, { parent: state.group })
    else setSpriteParent(proxy, state.group)
  }
  state.proxy = proxy
}

function hideProxy<T>(state: HandleState<T>): void {
  if (state.proxy === null) return
  let proxy = state.proxy
  state.proxy = null
  if ("_children" in proxy) destroyGroup(proxy, { snap: true })
  else destroySprite(proxy, { snap: true })
}

// The content shows: visible, measured, the proxy gone.
function shown<T>(core: CoreRegion<T>): void {
  let region = handleOf.get(core as CoreRegion<unknown>) as Region<T> | undefined
  if (region === undefined) return
  let state = stateOf.get(region as Region<unknown>) as HandleState<T>
  setGroup(state.content, { visible: true })
  core.measure(contentBytes(state))
  hideProxy(state)
}

// The content goes, snapping (a region leaves off screen: an exit would
// animate to nobody and hold the slots). An explicit region keeps its
// groups and gets its proxy back; a grid cell's group goes with it,
// proxy included.
function teardown<T>(core: CoreRegion<T>, keep: boolean): void {
  let region = handleOf.get(core as CoreRegion<unknown>) as Region<T> | undefined
  if (region === undefined) return
  let state = stateOf.get(region as Region<unknown>) as HandleState<T>
  if (keep) {
    for (let child of [...(state.content as GroupState)._children]) {
      if ("_children" in child) destroyGroup(child, { snap: true })
      else destroySprite(child, { snap: true })
    }
    setGroup(state.content, { visible: false })
    showProxy(state)
  } else {
    hideProxy(state)
    destroyGroup(state.group, { snap: true })
  }
}

/** The record bytes of one sprite on the layer: its pose and sprite
 * records plus the material's style records. */
function spriteBytes(layer: SpriteLayer): number {
  let bytes = (POSE_FLOATS + SPRITE_FLOATS) * Float32Array.BYTES_PER_ELEMENT
  for (let style of layer._styles) bytes += style.stream.stride
  return bytes
}

/** The content's record bytes: every sprite under it, groups walked. */
function contentBytes<T>(state: HandleState<T>): number {
  let per = spriteBytes(state.layer)
  let count = 0
  let walk = (group: GroupState) => {
    for (let child of group._children) {
      if ("_children" in child) walk(child)
      else count++
    }
  }
  walk(state.content as GroupState)
  return count * per
}

// The core handler over the handle: every call reaches the app with the
// 2d handle, not the core one.
function coreHandler<T>(content: RegionContent<T>): RegionHandler<T, CoreRegion<T>> {
  let handle = (core: CoreRegion<T>) => handleOf.get(core as CoreRegion<unknown>) as Region<T>
  return {
    load: content.load === undefined ? undefined : (core, signal) => content.load!(handle(core), signal),
    build: (core, data) => content.build(handle(core), data),
    unload: content.unload === undefined ? undefined : (core, data) => content.unload!(handle(core), data),
  }
}

function checkFinite(api: string, name: string, value: number): void {
  if (!Number.isFinite(value)) throw new Error(`${api}: ${name} must be finite, got ${value}`)
}

/**
 * An explicit region (Unreal's level streaming volume, a level section):
 * a group at (`x`, `y`) with a `width` x `height` box the gate tests
 * through its world matrix, placed in the layer like any group, its
 * content loaded and built while a viewer is within `enter` of the box
 * and kept until beyond `leave`. Declared to the gate from creation;
 * `dispose` ends it (its content leaves) and destroys the group.
 */
export function createRegion<T = undefined>(layer: SpriteLayer, opts: RegionOptions<T>): Region<T> & { dispose(): void } {
  checkFinite("createRegion", "x", opts.x)
  checkFinite("createRegion", "y", opts.y)
  if (!(Number.isFinite(opts.width) && opts.width >= 0 && Number.isFinite(opts.height) && opts.height >= 0)) {
    throw new Error(`createRegion: width and height must be finite and not negative, got ${opts.width} x ${opts.height}`)
  }
  if (opts.parent && opts.parent.layer !== layer) throw new Error("createRegion: parent group belongs to another layer")
  let core = createCoreRegion<T>({
    ...coreHandler(opts),
    enter: opts.enter,
    leave: opts.leave,
    weight: opts.weight,
    priority: opts.priority,
    retain: opts.retain,
    label: opts.label,
    hooks: { shown, teardown: core => teardown(core, true) },
  })
  let group = addGroup(layer, { x: opts.x, y: opts.y, parent: opts.parent ?? null })
  // A cull-only box: a group is never picked, and the gate reads the
  // cull box first.
  spatial.setCullBounds(group.node, new Float32Array([0, 0, -REGION_HALF_DEPTH, opts.width, opts.height, REGION_HALF_DEPTH]))
  let region = makeHandle(layer, core, group, null, opts.label, opts.proxy, null) as Region<T> & { dispose(): void }
  core.attach(group.node)
  let disposed = false
  region.dispose = () => {
    if (disposed) return
    disposed = true
    // The leave would stand the proxy up again; nothing is left to show.
    ;(stateOf.get(region as Region<unknown>) as HandleState<T>).makeProxy = undefined
    core.detach()
    destroyGroup(group, { snap: true })
  }
  return region
}

/**
 * A region grid (Unreal's World Partition grid, Minecraft's chunks, for
 * sprite populations): a root group of the layer; every cell a viewer
 * wants within `enter` of its view (kept until beyond `leave`) becomes
 * a child Region at the cell's corner, its content loaded and built
 * there, and leaves when no viewer wants it or a limit evicts it for a
 * nearer cell. Declared to the gate from creation; `dispose` ends it.
 */
export function createRegionGrid<T = undefined>(layer: SpriteLayer, opts: RegionGridOptions<T>): RegionGrid<T> {
  let size: [number, number] = [opts.cell[0], opts.cell[1]]
  if (size.some(v => !(Number.isFinite(v) && v >= 0))) throw new Error(`createRegionGrid: cell sizes must be finite and not negative (0 = unbounded), got ${size.join(" x ")}`)
  if (size[0] === 0 && size[1] === 0) throw new Error("createRegionGrid: a cell unbounded on both axes is one cell, not a grid")
  let origin: [number, number] = opts.origin === undefined ? [0, 0] : [opts.origin[0], opts.origin[1]]
  if (origin.some(v => !Number.isFinite(v))) throw new Error(`createRegionGrid: origin must be finite, got ${origin.join(", ")}`)
  if (opts.bounds !== undefined && (opts.bounds.length !== 4 || opts.bounds.some(v => !Number.isInteger(v)))) {
    throw new Error("createRegionGrid: bounds must be the integer cell-index box [minI, minJ, maxI, maxJ]")
  }
  let largest = Math.max(size[0], size[1])
  let enter = opts.enter ?? DEFAULT_ENTER_CELLS * largest
  let leave = opts.leave ?? Math.max(enter, DEFAULT_LEAVE_CELLS * largest)
  let group = addGroup(layer, {})
  let cellOrigin = (cell: RegionCell): [number, number] => [size[0] > 0 ? origin[0] + cell[0] * size[0] : origin[0], size[1] > 0 ? origin[1] + cell[1] * size[1] : origin[1]]
  let grid: RegionGrid<T> = null!
  let core = createCoreGrid<T>({
    ...coreHandler(opts),
    root: layer._root,
    origin: [origin[0], origin[1], 0],
    cell: [size[0], size[1], 0],
    enter,
    leave,
    bounds: opts.bounds === undefined ? undefined : [opts.bounds[0], opts.bounds[1], 0, opts.bounds[2], opts.bounds[3], 0],
    weight: opts.weight,
    limit: opts.limit,
    priority: opts.priority,
    retain: opts.retain,
    label: opts.label,
    hooks: {
      spawn(region) {
        let cell: RegionCell = [region.cell![0], region.cell![1]]
        let [x, y] = cellOrigin(cell)
        let child = addGroup(layer, { x, y, parent: group })
        makeHandle(layer, region, child, cell, opts.label, opts.proxy, grid)
      },
      shown,
      teardown: region => teardown(region, false),
    },
  })
  grid = {
    layer,
    group,
    cells: () => core.cells().map(r => handleOf.get(r as CoreRegion<unknown>) as Region<T>),
    cellAt(i, j) {
      let r = core.cellAt(i, j, 0)
      return r === null ? null : (handleOf.get(r as CoreRegion<unknown>) as Region<T>)
    },
    cellOrigin,
    setLimit: limit => core.setLimit(limit),
    setWeight: weight => core.setWeight(weight),
    forget: () => core.forget(),
    label: opts.label,
    onStateChange: undefined,
    onError: undefined,
    dispose() {
      core.dispose()
      destroyGroup(group, { snap: true })
    },
    _core: core,
  }
  core.onStateChange = (region, s) => {
    let child = handleOf.get(region as CoreRegion<unknown>) as Region<T> | undefined
    if (child !== undefined) grid.onStateChange?.(child, s)
  }
  core.start()
  return grid
}

/**
 * A streaming source in a layer's plane (core's createStreamingSource
 * with a 2d point, scoped to the layer's world): a point with a radius
 * that wants the layer's regions around it - a scrolling cut-scene's
 * look-ahead, a second player on a server, a teleport destination loaded
 * before the cut. A viewer to the gate and nothing to the views. Move it
 * with `setPosition`; `dispose` withdraws it.
 */
export type StreamingSource = {
  setPosition(x: number, y: number): void
  setRadius(radius: number): void
  readonly x: number
  readonly y: number
  readonly radius: number
  dispose(): void
}

export function createStreamingSource(layer: SpriteLayer, opts: { x: number; y: number; radius: number }): StreamingSource {
  let core = createCoreStreamingSource({ position: [opts.x, opts.y, 0], radius: opts.radius, root: layer._root })
  return {
    setPosition(x, y) {
      core.setPosition([x, y, 0])
    },
    setRadius(r) {
      core.setRadius(r)
    },
    get x() {
      return core.position[0]
    },
    get y() {
      return core.position[1]
    },
    get radius() {
      return core.radius
    },
    dispose() {
      core.dispose()
    },
  }
}
