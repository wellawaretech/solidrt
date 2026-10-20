// Regions: the 3d form of core's region handlers (@solidrt/core
// createRegion/createRegionGrid over the spatial core's residency gate,
// the contract in okf/done/spatial-residency-gate.md). A region is a
// group node whose CONTENT exists while a viewer wants it: an explicit
// region is a bounded group you place in the scene (a level section, a
// building interior), a region grid a root group whose every wanted cell
// becomes a child region at the cell's world corner (Unreal's World
// Partition, Minecraft's chunk columns), so a cell's content is cell-local
// and float precision is per cell. The gate decides and paces (every
// camera is a viewer, a createStreamingSource is one too); core's
// lifecycle loads, builds under the frame slice budget, unloads; this
// module adds what is 3d: the nodes, the builders that yield between
// creates (buildModel over modelSteps, warm), the content hidden until
// built and the proxy shown while it is not (HLOD, Unreal's rule), the
// measured GPU bytes reported to the gate's limits, and the resource
// pool the freed buffers and textures wait in.
import { createRegion as createCoreRegion, createRegionGrid as createCoreGrid, createStreamingSource as createCoreStreamingSource } from "@solidrt/core"
import type { Region as CoreRegion, RegionCell, RegionGrid as CoreGrid, RegionHandler, RegionState, StreamingSource } from "@solidrt/core"
import type { Scene } from "./scene.ts"
import { ensureResourcePool } from "./resource-pool.ts"
import { acquireGeometryBuffers, releaseGeometryBuffers } from "./geometry-gpu.ts"
import type { GeometryBuffers } from "./geometry-gpu.ts"
import { geometryStreams } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import { modelSteps } from "./model.ts"
import type { Model, ModelOptions } from "./model.ts"
import type { ModelData } from "./gltf.ts"
import { add, createGroup, destroy, remove, setTransform, setVisible } from "./node.ts"
import type { SceneNode, SubtreeBinding } from "./node.ts"
import type { Mesh } from "./mesh.ts"
import type { Vec3 } from "./math.ts"

export type { RegionCell, RegionState, StreamingSource } from "@solidrt/core"

/** The bytes of one instance matrix record (a mat4 of floats). */
const INSTANCE_MATRIX_BYTES = 64
/** Floats per packed morph texel (rgba32f). */
const MORPH_TEXEL_BYTES = 16

/**
 * What a region's content is and how it comes and goes, the 3d form of
 * core's RegionHandler: `load` prepares the data off the thread (a file
 * read, `decodeModel`, a glTF parse in an isolate; abortable through the
 * signal; absent, `build` gets `undefined`), `build` makes the content
 * under `region.content` - as a generator, `yield* buildModel(region,
 * data)` and a `yield` after every create of your own, so the creates
 * spread over frames under the slice budget (setSliceBudget) - `unload`
 * runs on leave BEFORE the content goes, with the data (an editable
 * world reads its cell back here), and `proxy` makes the node drawn
 * while the region is NOT shown (a merged low-poly stand-in, a card;
 * Unreal's HLOD): hidden once the content shows, shown again at leave,
 * casting shadows meanwhile.
 */
export type RegionContent<T> = {
  load?: (region: RegionNode<T>, signal: AbortSignal) => T | Promise<T>
  build: (region: RegionNode<T>, data: T) => void | Iterator<void>
  unload?: (region: RegionNode<T>, data: T) => void
  proxy?: (region: RegionNode<T>) => SceneNode
}

/**
 * A region: a group node. Its `content` child holds what `build` made,
 * hidden until the build completes; its `proxy` child (when the handler
 * makes one) is drawn while the content is not shown. `state` is the
 * gate's "pending"/"stalled" until the enter, then "loading",
 * "building", "shown" or "failed" (RegionState); `data` the loaded data
 * while held; `cell` the index of a grid cell. Leaves snap: the content
 * is removed and disposed, never destroyed with an exit.
 */
export type RegionNode<T = unknown> = SceneNode & {
  kind: "group"
  /** Where the content goes: `add(region.content, mesh)`. Hidden until
   * the build completes. */
  readonly content: SceneNode
  /** The proxy node the handler made, while the region holds one. */
  readonly proxy: SceneNode | null
  readonly state: RegionState
  readonly data: T | undefined
  readonly error: unknown
  readonly cell: RegionCell | null
  readonly label: string | undefined
  /** Acquire a geometry's GPU buffers now (one create-sized step: call
   * it in `build` and `yield` after), held until the region shows or
   * leaves, so the `add` that follows finds them uploaded. */
  warm(geometry: Geometry): void
  /** Register something to dispose at leave after the content is
   * removed (a model, a mixer, record buffers). buildModel registers
   * its model. */
  own(resource: { dispose(): void }): void
  /** Measure the content's GPU bytes again and report them to the
   * gate's limits (done once as the build completes; call it after
   * content added later, as the component form does when its JSX
   * mounts). Returns the bytes. */
  measure(): number
  /** Assignable: called after every state change. */
  onStateChange?: (region: RegionNode<T>, state: RegionState) => void
  /** Assignable: called with what a load or build threw. */
  onError?: (region: RegionNode<T>, error: unknown) => void
  /** @internal */
  _core: CoreRegion<T>
}

export type RegionOptions<T> = RegionContent<T> & {
  /** The local box the gate tests, min xyz then max xyz. */
  bounds: ArrayLike<number>
  /** Wanted within this distance of a viewer's view volume (a load
   * distance, world units). */
  enter: number
  /** Kept until beyond this distance (at least `enter`). */
  leave: number
  /** The estimated GPU bytes the gate's limits admit the region by until
   * its content is measured (default 0: free under every limit). */
  weight?: number
  /** An integer: a higher priority enters first whatever the distance
   * and is evicted last (default 0). */
  priority?: number
  /** Keep the data of a left region so the next enter skips `load`
   * (default false). */
  retain?: boolean
  label?: string
}

export type RegionGridOptions<T> = RegionContent<T> & {
  /** The world corner of cell [0, 0, 0], in the grid node's space. */
  origin?: Vec3
  /** The cell size per axis; 0 = unbounded along that axis (`[32, 0,
   * 32]` is a column grid over XZ, `[32, 32, 32]` a voxel world). */
  cell: Vec3
  enter: number
  leave: number
  /** An inclusive cell-index box outside which no cell exists. */
  bounds?: [number, number, number, number, number, number]
  /** The estimated GPU bytes of one cell until its content is measured
   * (default 1). */
  weight?: number
  /** The grid's own cap on resident bytes. */
  limit?: number
  /** An integer priority among grids and regions (default 0): a collision
   * grid before decorative scatter. */
  priority?: number
  /** How many left cells' data to keep (LRU) so a re-enter skips `load`
   * (default 0). */
  retain?: number
  label?: string
}

/**
 * A region grid: a group node you add once; every cell the gate wants
 * becomes a child RegionNode at the cell's corner for as long as it is
 * held. The grid is declared while the node is in a scene. A 3d grid has
 * no `wrap`: a 3d scene has one world space, and a wrapped world is a
 * camera-teleport pattern in every 3d engine, not duplicated content.
 */
export type RegionGridNode<T = unknown> = SceneNode & {
  kind: "group"
  /** The held cells (from enter to leave), fresh array. */
  cells(): RegionNode<T>[]
  cellAt(i: number, j: number, k: number): RegionNode<T> | null
  /** The world corner of a cell, in the grid node's space. */
  cellOrigin(cell: RegionCell, out?: Vec3): Vec3
  /** Re-cap the grid's own byte limit (null = none). */
  setLimit(limit: number | null): void
  /** Re-estimate the bytes of an unmeasured cell. */
  setWeight(weight: number): void
  /** Drop the retained data. */
  forget(): void
  readonly label: string | undefined
  /** Assignable: called after every cell's state change. */
  onStateChange?: (region: RegionNode<T>, state: RegionState) => void
  onError?: (region: RegionNode<T>, error: unknown) => void
  /** End the grid (every held cell leaves) and destroy the node. */
  dispose(): void
  /** @internal */
  _core: CoreGrid<T>
}

// The 3d node behind a core region: made at spawn (a grid cell) or at
// creation (an explicit region), looked up from the core handle in the
// handler's calls.
let nodeOf = new WeakMap<CoreRegion<unknown>, RegionNode<unknown>>()

function checkBounds(bounds: ArrayLike<number>, api: string): Float32Array {
  if (bounds.length !== 6) throw new Error(api + ": bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
  let out = Float32Array.from(bounds)
  for (let k = 0; k < 3; k++) {
    if (!(Number.isFinite(out[k]!) && Number.isFinite(out[k + 3]!) && out[k]! <= out[k + 3]!)) throw new Error(api + ": bounds min must not exceed max on every axis")
  }
  return out
}

// The node's 3d half: the content and proxy children, the warm holds,
// the owned resources, the measured bytes.
type NodeState<T> = {
  node: RegionNode<T>
  content: SceneNode
  proxy: SceneNode | null
  holds: GeometryBuffers[]
  owned: { dispose(): void }[]
  makeProxy: ((region: RegionNode<T>) => SceneNode) | undefined
}

let stateOf = new WeakMap<RegionNode<unknown>, NodeState<unknown>>()

function makeRegionNode<T>(core: CoreRegion<T>, cell: RegionCell | null, label: string | undefined, makeProxy: RegionContent<T>["proxy"], grid: RegionGridNode<T> | null): RegionNode<T> {
  let node = createGroup() as RegionNode<T>
  let content = createGroup()
  setVisible(content, false)
  add(node, content)
  let state: NodeState<T> = { node, content, proxy: null, holds: [], owned: [], makeProxy }
  Object.defineProperties(node, {
    content: { value: content, enumerable: true },
    proxy: { get: () => state.proxy, enumerable: true },
    state: { get: () => core.state, enumerable: true },
    data: { get: () => core.data, enumerable: true },
    error: { get: () => core.error, enumerable: true },
    cell: { value: cell, enumerable: true },
    label: { value: label, enumerable: true },
    _core: { value: core },
  })
  node.warm = geometry => {
    state.holds.push(acquireGeometryBuffers(geometry))
  }
  node.own = resource => {
    state.owned.push(resource)
  }
  node.measure = () => {
    let bytes = contentBytes(state)
    core.measure(bytes)
    return bytes
  }
  core.onStateChange = (_, s) => node.onStateChange?.(node, s)
  // The error goes to the region's listener, else its grid's, else the
  // console (core would fall through the same way, but sees only this
  // forwarder); resolved at the failure, so a listener set later counts.
  core.onError = (_, e) => {
    let listener = node.onError ?? grid?.onError
    if (listener !== undefined) listener(node, e)
    else console.error("A region's load or build failed" + (label !== undefined ? ` (${label})` : "") + ":", e)
  }
  nodeOf.set(core as CoreRegion<unknown>, node as RegionNode<unknown>)
  stateOf.set(node as RegionNode<unknown>, state as NodeState<unknown>)
  if (makeProxy !== undefined) showProxy(state)
  return node
}

function showProxy<T>(state: NodeState<T>): void {
  if (state.proxy !== null || state.makeProxy === undefined) return
  let proxy = state.makeProxy(state.node)
  if (proxy.parent !== state.node) add(state.node, proxy)
  state.proxy = proxy
}

function hideProxy<T>(state: NodeState<T>): void {
  if (state.proxy === null) return
  destroy(state.proxy)
  state.proxy = null
}

function releaseHolds<T>(state: NodeState<T>): void {
  for (let hold of state.holds) releaseGeometryBuffers(hold)
  state.holds = []
}

// The content shows: measure it, let the holds go (the content's
// entries hold the buffers now), drop the proxy.
function shown<T>(core: CoreRegion<T>): void {
  let node = nodeOf.get(core as CoreRegion<unknown>) as RegionNode<T> | undefined
  if (node === undefined) return
  let state = stateOf.get(node as RegionNode<unknown>) as NodeState<T>
  setVisible(state.content, true)
  core.measure(contentBytes(state))
  releaseHolds(state)
  hideProxy(state)
}

// The content goes: removed and disposed (never destroyed with an exit),
// the holds released, the proxy back (an explicit region stays; a grid
// cell's node goes with it, proxy included).
function teardown<T>(core: CoreRegion<T>, keep: boolean): void {
  let node = nodeOf.get(core as CoreRegion<unknown>) as RegionNode<T> | undefined
  if (node === undefined) return
  let state = stateOf.get(node as RegionNode<unknown>) as NodeState<T>
  for (let child of state.content.children.slice()) {
    remove(child)
    destroy(child)
  }
  for (let resource of state.owned.slice().reverse()) resource.dispose()
  state.owned = []
  releaseHolds(state)
  setVisible(state.content, false)
  if (keep) showProxy(state)
  else {
    hideProxy(state)
    destroy(node)
  }
}

/** The GPU bytes of the content: every geometry under it once (vertices,
 * streams, indices, morph texels), every population's buffers, every
 * owned model's textures. */
function contentBytes<T>(state: NodeState<T>): number {
  let bytes = 0
  let seen = new Set<Geometry>()
  let walk = (n: SceneNode) => {
    if (n.kind === "mesh") {
      let mesh = n as Mesh
      if (!seen.has(mesh.geometry)) {
        seen.add(mesh.geometry)
        bytes += geometryBytes(mesh.geometry)
      }
      let inst = mesh._instances
      if (inst !== null) {
        if (inst.matrix !== null) bytes += inst.capacity * INSTANCE_MATRIX_BYTES
        for (let stream of inst.streams) bytes += inst.capacity * stream.stride
      }
    }
    for (let c of n.children) walk(c)
  }
  walk(state.content)
  for (let resource of state.owned) {
    let model = resource as Partial<Model>
    if (typeof model._textureBytes === "number") bytes += model._textureBytes
  }
  return bytes
}

/** A geometry's GPU bytes as uploaded. */
export function geometryBytes(geometry: Geometry): number {
  let bytes = geometry.indices.byteLength
  for (let stream of geometryStreams(geometry)) bytes += stream.vertices.byteLength
  if (geometry.morphs !== undefined) bytes += geometry.morphs.texels.length * (MORPH_TEXEL_BYTES / Float32Array.BYTES_PER_ELEMENT)
  return bytes
}

// The core handler over the node: every call reaches the app with the
// node, not the core handle.
function coreHandler<T>(content: RegionContent<T>): RegionHandler<T, CoreRegion<T>> {
  let node = (core: CoreRegion<T>) => nodeOf.get(core as CoreRegion<unknown>) as RegionNode<T>
  return {
    load: content.load === undefined ? undefined : (core, signal) => content.load!(node(core), signal),
    build: (core, data) => content.build(node(core), data),
    unload: content.unload === undefined ? undefined : (core, data) => content.unload!(node(core), data),
  }
}

/**
 * An explicit region (Unreal's level streaming volume, a level section):
 * a group node with `bounds` the gate tests through its world matrix,
 * placed in the scene like any node, its content loaded and built while
 * a viewer is within `enter` of the box and kept until beyond `leave`.
 * Declared to the gate while in a scene. `dispose` ends it and destroys
 * the node.
 */
export function createRegion<T = undefined>(opts: RegionOptions<T>): RegionNode<T> & { dispose(): void } {
  let bounds = checkBounds(opts.bounds, "createRegion")
  ensureResourcePool()
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
  let node = makeRegionNode(core, null, opts.label, opts.proxy, null) as RegionNode<T> & { dispose(): void }
  node._cullBounds = bounds
  let binding: SubtreeBinding = {
    enter() {
      if (node._node !== null) core.attach(node._node)
    },
    leave() {
      core.detach()
    },
  }
  node._bindings = [binding]
  node.dispose = () => {
    core.detach()
    destroy(node)
  }
  return node
}

/**
 * A region grid (Unreal's World Partition grid): a group node to add
 * once; while it is in a scene every cell a viewer wants within `enter`
 * of its view volume (kept until beyond `leave`, the far plane ignored:
 * load distances) becomes a child RegionNode at the cell's corner, its
 * content loaded and built there, and leaves when no viewer wants it or
 * a limit evicts it for a nearer cell.
 */
export function createRegionGrid<T = undefined>(opts: RegionGridOptions<T>): RegionGridNode<T> {
  let origin: Vec3 = opts.origin === undefined ? [0, 0, 0] : [opts.origin[0], opts.origin[1], opts.origin[2]]
  let size: Vec3 = [opts.cell[0], opts.cell[1], opts.cell[2]]
  if (size.some(v => !(Number.isFinite(v) && v >= 0))) throw new Error("createRegionGrid: cell sizes must be finite and not negative (0 = unbounded)")
  if (origin.some(v => !Number.isFinite(v))) throw new Error("createRegionGrid: origin must be finite")
  ensureResourcePool()
  let node = createGroup() as RegionGridNode<T>
  let cellOrigin = (cell: RegionCell, out: Vec3 = [0, 0, 0]): Vec3 => {
    for (let a = 0; a < 3; a++) out[a] = size[a]! > 0 ? origin[a]! + cell[a]! * size[a]! : origin[a]!
    return out
  }
  let core = createCoreGrid<T>({
    ...coreHandler(opts),
    origin: [origin[0], origin[1], origin[2]],
    cell: [size[0], size[1], size[2]],
    enter: opts.enter,
    leave: opts.leave,
    bounds: opts.bounds,
    weight: opts.weight,
    limit: opts.limit,
    priority: opts.priority,
    retain: opts.retain,
    label: opts.label,
    hooks: {
      spawn(region) {
        let child = makeRegionNode(region, region.cell, opts.label, opts.proxy, node)
        setTransform(child, { position: cellOrigin(region.cell!) })
        add(node, child)
      },
      shown,
      teardown: region => teardown(region, false),
    },
  })
  // Declared while in a scene: the gate's grid lives in world space, and
  // the cell nodes under this one carry the content. The node's own
  // transform is not folded into the gate's origin (a grid is placed at
  // identity, or `origin` says where it sits).
  // Declared under the scene's world (its root node, the top of the
  // grid's ancestry here), so only that scene's cameras and sources
  // want the cells.
  node._bindings = [
    {
      enter() {
        let top: SceneNode = node
        while (top.parent !== null) top = top.parent
        core.start(top._node!)
      },
      leave() {
        core.stop()
      },
    },
  ]
  core.onStateChange = (region, s) => {
    let child = nodeOf.get(region as CoreRegion<unknown>) as RegionNode<T> | undefined
    if (child !== undefined) node.onStateChange?.(child, s)
  }
  Object.defineProperties(node, {
    label: { value: opts.label, enumerable: true },
    _core: { value: core },
  })
  node.cells = () => core.cells().map(r => nodeOf.get(r as CoreRegion<unknown>) as RegionNode<T>)
  node.cellAt = (i, j, k) => {
    let r = core.cellAt(i, j, k)
    return r === null ? null : (nodeOf.get(r as CoreRegion<unknown>) as RegionNode<T>)
  }
  node.cellOrigin = cellOrigin
  node.setLimit = limit => core.setLimit(limit)
  node.setWeight = weight => core.setWeight(weight)
  node.forget = () => core.forget()
  node.dispose = () => {
    core.dispose()
    destroy(node)
  }
  return node
}

/**
 * Build a model inside a region's `build`: `let model = yield*
 * buildModel(region, data)` runs createModel's steps one per slice (an
 * image upload, the skin palettes, a part each), warms every part's
 * geometry buffers (one step each) so the add that follows creates
 * nothing, adds the model under the region's content and registers it
 * for disposal at leave. Returns the model, placed at identity: move it
 * with setTransform.
 */
export function* buildModel<T>(region: RegionNode<T>, data: ModelData, opts?: ModelOptions): Generator<void, Model> {
  let model = yield* modelSteps(data, { ...opts, label: opts?.label ?? region.label, autoFree: false })
  for (let part of model.parts) {
    region.warm(part.mesh.geometry)
    yield
  }
  add(region.content, model)
  region.own(model)
  return model
}

/**
 * A streaming source in a scene (core's createStreamingSource scoped to
 * the scene's world; Unreal's streaming source component): a point with
 * a radius that wants the scene's regions around it - the world ahead
 * of a fast vehicle, a teleport destination loaded before the cut, a
 * server-side player. A viewer to the gate and nothing to the renderer.
 * Move it with `setPosition`; `dispose` withdraws it.
 */
export function createStreamingSource(scene: Scene, opts: { position: Vec3; radius: number }): StreamingSource {
  return createCoreStreamingSource({ position: [opts.position[0], opts.position[1], opts.position[2]], radius: opts.radius, root: scene.root._node! })
}
