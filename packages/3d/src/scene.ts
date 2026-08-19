// The retained scene: plain objects and dirty flags, no signals - the hot
// path (a moved node) is flat imperative code, and reactivity stays at the
// component boundary (components.tsx). A scene compiles to one draw
// target: every mesh is one draw entry whose uModel (and, for materials
// declaring it, uNormal) this module keeps in step with the tree, and the
// camera is the target's SHARED uViewProj + uCamPos + uCamRight/uCamUp -
// one setTargetParams per camera move, not one write per mesh. The
// non-matrix names ride unconditionally: shared params tolerate zero
// coverage (stored and skipped until a declaring material arrives), so no
// bookkeeping tracks who reads them. scene.setParams merges app-owned
// names into the same set.
// Mutations batch to a microtask, so a burst of writes (a whole subtree
// moved, many effects in one flush) syncs once.
//
// Rendering itself belongs to the runtime: the target is an ordinary
// `render: "auto"` draw target that re-renders when its entries change, so
// a static scene costs zero passes and this module registers no frame
// loop. Continuous animation is the app's onFrame writing transforms -
// each write lands here, the microtask syncs the affected uModels, and the
// flush renders once that frame.

import { addDraw, createDrawTarget, destroyProgram, destroyRenderPipeline, destroyTexture, removeDraw, setDrawOrder, setDrawParams, setDrawRange, setTargetParams, setTargetSize } from "@solidrt/core/gpu"
import type { DrawId, FilterMode, ProgramId, RenderPipelineId, ShaderParams, TextureId, WrapMode } from "@solidrt/core/gpu"
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
// The scene's lookAt() aims a node; math's builds a camera's view matrix -
// the same pairing (and the same name) as Three's Object3D/Matrix4.
import { compose, copy, eulerFromQuat, identity, invertAffine, lookAt as lookAtMatrix, mat4, multiply, normalMatrix, perspective, quat, quatFromFrame, transformPoint, transformVector, updateRotation, updateScale } from "./math.ts"
import type { Mat4, Quat, TransformUpdate, Vec3, Vec4 } from "./math.ts"
import { geometryBounds } from "./geometry.ts"
import { geometryBuffers } from "./geometry-gpu.ts"
import type { Geometry } from "./geometry.ts"
import { backgroundPipeline } from "./material.ts"
import { orderEntries } from "./order.ts"
import type { Material } from "./material.ts"
import { createBvh, rayBoxDistance } from "./bvh.ts"

const IDENTITY = mat4()
const RESOLVED = Promise.resolve()
// lookAt()'s default roll reference. Read-only: quatFromFrame never
// writes its inputs, so one shared vector is safe.
const WORLD_UP: Vec3 = [0, 1, 0]
// Param values are snapshotted at the FFI boundary (addDraw shares
// IDENTITY the same way), so one scratch serves every uNormal write.
let normalScratch = mat4()
// lookAt()/worldPosition() scratch: the ancestor walk recomputes worlds
// without touching node state, so nothing here outlives a single call.
let worldScratch = mat4()
let localScratch = mat4()
let pointScratch: Vec4 = [0, 0, 0, 0]
let aimScratch: Vec3 = [0, 0, 0]
let upScratch: Vec3 = [0, 0, 0]
// Picking narrowphase scratch: one candidate is tested at a time, so one
// set serves every raycast.
let pickInv = mat4()
let pickOrigin: Vec4 = [0, 0, 0, 0]
let pickDir: Vec3 = [0, 0, 0]
// setTransform's rotation compare happens AFTER conversion, so an euler and
// the quaternion it produces are the same write. Nothing outlives the call.
let rotScratch = quat()
let scaleScratch: Vec3 = [1, 1, 1]

// The scene half a node needs to reach: attach/detach entries and schedule
// a sync. Kept separate from the public Scene type so internals stay off
// the app-facing surface. The camera (uViewProj + uCamPos) is written
// through the shared channel only when it changes - attach never re-seeds
// it, because target state survives entry churn.
type SceneHooks = {
  _schedule(): void
  _attach(mesh: Mesh): void
  _detach(mesh: Mesh): void
  _setParams(mesh: Mesh, params: ShaderParams): void
  _reorder(): void
}

export type SceneNode = {
  kind: "group" | "mesh"
  parent: SceneNode | null
  children: SceneNode[]
  /** Read freely; write through setTransform/setVisible so changes sync. */
  position: Vec3
  /** The stored rotation, always a UNIT quaternion. Euler triples convert
   * on the way in (setTransform's `rotation`) and out (getRotation) - there
   * is no second rotation field to fall out of step with this one. */
  quaternion: Quat
  scale: Vec3
  visible: boolean
  /** Pointer event handlers - plain fields, assign freely (they touch no
   * GPU state, so they need no setTransform-style write path; components
   * sync their props here). Down/move/up dispatch on the hit mesh and
   * bubble through its ancestors (stopPropagation stops the walk);
   * enter/leave fire on the mesh alone. Events flow once the element
   * showing the scene carries `scene.handlers`. */
  onPointerDown?: (event: ScenePointerEvent) => void
  onPointerMove?: (event: ScenePointerEvent) => void
  onPointerUp?: (event: ScenePointerEvent) => void
  onPointerEnter?: (event: ScenePointerEvent) => void
  onPointerLeave?: (event: ScenePointerEvent) => void
  _localDirty: boolean
  _local: Mat4
  _world: Mat4
  _scene: SceneHooks | null
}

export type Mesh = SceneNode & {
  kind: "mesh"
  geometry: Geometry
  material: Material
  /** Explicit draw-order key (default 0), Three's name: lower draws first.
   * Sorts within the opaque group and within the transparent group; the
   * transparent group always follows the opaque one. Set with setRenderOrder. */
  renderOrder: number
  _entry: DrawId | null
  /** material.transparent as of the last attach - the entry's actual
   * pipeline state, and what _detach counts against (setMaterial swaps
   * mesh.material before the rebuild). */
  _transparent: boolean
  /** World-space center of the geometry bounds, kept by the sync walk
   * beside the picking leaf: the transparent sort key. */
  _center: Vec3
  _hidden: boolean
  _fresh: boolean
  _params: ShaderParams | null
  _pickLeaf: number | null
}

/** One picking intersection: the mesh, the camera-ray distance in world
 * units, and the world-space point - Three's intersect result minus the
 * triangle fields (`face`, `uv`), which cannot exist at the volume tier. */
export type Hit = {
  mesh: Mesh
  distance: number
  point: Vec3
}

/**
 * The event a mesh (or ancestor group) handler receives: the element
 * pointer vocabulary carried over, plus the 3D fields. `point`/`distance`
 * are null exactly when the ray misses the dispatch mesh - which happens
 * only during a captured drag or on a leave.
 */
export type ScenePointerEvent = {
  /** The mesh the event is about (the hit, or the captured mesh during a
   * drag) - constant while the event bubbles. */
  mesh: Mesh
  /** Node whose handler is running; changes as the event bubbles. */
  currentTarget: SceneNode
  /** World-space hit point on `mesh`, or null when the ray misses it. */
  point: Vec3 | null
  /** Camera-ray distance to `point` in world units, or null with it. */
  distance: number | null
  /** Pointer position in scene pixels - project()'s coordinate space. */
  x: number
  y: number
  pointerId: number
  pointerType: string
  button?: number
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
  /** Stops the bubble walk after the current handler. */
  stopPropagation(): void
}

/** Element handlers wiring a scene's pointer events: spread onto whatever
 * element shows `scene.texture` (the built-in `<Scene>` leaf wires them
 * automatically). `scene.handlers` expects the leaf laid out at the target
 * size; a split-resolution leaf (supersampling) uses scene.handlersFor. */
export type SceneHandlers = {
  onPointerDown(event: ElementPointerEvent): void
  onPointerMove(event: ElementPointerEvent): void
  onPointerUp(event: ElementPointerEvent): void
  onPointerLeave(event: ElementPointerEvent): void
}

export type CameraUpdate = {
  /** Vertical field of view in DEGREES (default 60). */
  fov?: number
  near?: number
  far?: number
  position?: Vec3
  target?: Vec3
  up?: Vec3
}

export type SceneOptions = {
  clearColor?: [number, number, number, number]
  /** Fragment GLSL drawn behind the meshes, inside the scene's own pass -
   * see setBackground. */
  background?: string
  label?: string
  /** `autoFree: false` opts out of owner-scoped auto-dispose (then call dispose yourself). */
  autoFree?: boolean
  filter?: FilterMode
  wrap?: WrapMode
}

export type Scene = {
  /** The scene's output: an ordinary texture id (`<texture src>`). */
  texture: TextureId
  /** The tree root; add(scene.root, node) attaches top-level nodes. */
  root: SceneNode
  /** Partial camera update; absent keys keep their current value. */
  setCamera(update: CameraUpdate): void
  setSize(width: number, height: number): void
  /**
   * Scene-wide uniforms: merge app-owned names into the target's SHARED
   * params, beside the standard uViewProj/uCamPos/uCamRight/uCamUp the
   * camera writes. One write per frame however many meshes read the name
   * (a clock, a sun direction, fog) - the per-mesh channel is
   * setMeshParams. Merge semantics, no unset; a material that does not
   * declare a name simply skips it. Frame-rate-safe like setTransform.
   */
  setParams(params: ShaderParams): void
  /**
   * Set, replace, or remove (null) the scene's background: fragment GLSL
   * drawn as the FIRST entry of the scene's own pass - one target, no
   * second texture layer, no separate resize plumbing. The fragment gets
   * the shader-target contract exactly (vUV 0..1 top-left origin,
   * iResolution, fragColor; no `#version` line means the standard
   * preamble), so a source written for createShaderTexture ports verbatim.
   * It draws with depth off before every mesh and covers the whole target,
   * so the clearColor stops being visible. Three's `scene.background =
   * color` is `clearColor` here; the texture form can arrive later as a
   * non-breaking widening. No app-driven uniforms in v1 - a background is
   * static art (anything animated is a mesh's own shaderMaterial).
   */
  setBackground(source: string | null): void
  /**
   * Project a world point to scene pixels: origin top-left, y down - the
   * output texture's own coordinate space, ready for overlay layout (HUD
   * markers, labels). `w` is the clip-space w, the point's camera-forward
   * distance (useful for depth-ordering or distance-scaling markers).
   * Returns null for a point at or behind the camera plane - such a point
   * has no place on screen. Reflects a pending setCamera immediately.
   */
  project(point: Vec3): { x: number; y: number; w: number } | null
  /** The camera's view-projection matrix, copied into `out` (or a fresh
   * mat4). The batch escape hatch; for single points use project(). */
  viewProj(out?: Mat4): Mat4
  /**
   * Cast the camera ray through a scene pixel (top-left origin, y down -
   * project()'s space, the inverse direction) and return every visible
   * mesh it hits, nearest first. The volume tier: hits test the mesh's
   * bounding box, transformed exactly (any node transform, including
   * non-uniform scale), so a hit through a concave gap - a knot's hole -
   * still reports. Broadphase runs over a BVH kept in step by the sync
   * walk: a query costs O(log meshes), not O(meshes). Reflects pending
   * setTransform/add writes immediately (the sync is flushed).
   */
  pick(x: number, y: number): Hit[]
  /** pick()'s world-space half: the same query along an arbitrary ray.
   * `direction` need not be normalized; distances are world units. */
  raycast(origin: Vec3, direction: Vec3): Hit[]
  /**
   * Element pointer handlers driving the mesh event fields
   * (onPointerDown/Move/Up/Enter/Leave on nodes): spread onto the element
   * that shows `scene.texture`. The `<Scene>` component's built-in leaf
   * carries them automatically; with `output` (or imperative use), spread
   * them yourself: `<texture src={scene.texture} {...scene.handlers} />`.
   * Semantics mirror element pointer events: nearest hit wins, down/move/
   * up bubble mesh -> ancestors, pointer-down captures the mesh until up
   * (moves keep flowing to it off-mesh, the platform's captured-drag
   * rule), enter/leave pair on hover changes. Hover reacts to pointer
   * MOTION - a mesh animating under a still pointer fires nothing until
   * the pointer moves (the element hit-test has the same limit).
   *
   * Coordinates assume the leaf is LAID OUT at the target size - true for
   * the built-in leaf and a d-texture at natural size, under any ancestor
   * transforms or viewBox fits (the hit test undoes them). A leaf laid out
   * at a different size needs handlersFor instead.
   */
  handlers: SceneHandlers
  /** handlers for a leaf whose LAYOUT size differs from the target size -
   * the supersampling pattern, where the target renders larger than the
   * box showing it. `layout` is read per event, so a resize-reactive
   * layout just works: `scene.handlersFor(() => ({ width: w(), height:
   * h() }))`. */
  handlersFor(layout: () => { width: number; height: number }): SceneHandlers
  /** Destroy the target (entries die with it). Idempotent. Geometry
   * buffers and material pipelines are shared and survive - they are
   * app-lifetime (see geometry.ts / material.ts). */
  dispose(): void
}

function makeNode(kind: "group" | "mesh"): SceneNode {
  return {
    kind,
    parent: null,
    children: [],
    position: [0, 0, 0],
    quaternion: [0, 0, 0, 1],
    scale: [1, 1, 1],
    visible: true,
    _localDirty: true,
    _local: mat4(),
    _world: mat4(),
    _scene: null,
  }
}

export function createGroup(): SceneNode {
  return makeNode("group")
}

export function createMesh(geometry: Geometry, material: Material): Mesh {
  let mesh = makeNode("mesh") as Mesh
  mesh.geometry = geometry
  mesh.material = material
  mesh.renderOrder = 0
  mesh._entry = null
  mesh._transparent = false
  mesh._center = [0, 0, 0]
  mesh._hidden = false
  mesh._fresh = false
  mesh._params = null
  mesh._pickLeaf = null
  return mesh
}

/** Attach `child` under `parent` (re-parenting detaches it first). */
export function add(parent: SceneNode, child: SceneNode): void {
  if (child.parent !== null) remove(child)
  child.parent = parent
  parent.children.push(child)
  child._localDirty = true
  if (parent._scene) enterScene(child, parent._scene)
}

/** Detach `child` from its parent (and its meshes from the scene). */
export function remove(child: SceneNode): void {
  if (child._scene) leaveScene(child)
  let parent = child.parent
  if (parent !== null) {
    let i = parent.children.indexOf(child)
    if (i >= 0) parent.children.splice(i, 1)
    child.parent = null
  }
}

function enterScene(node: SceneNode, scene: SceneHooks): void {
  node._scene = scene
  node._localDirty = true
  if (node.kind === "mesh") scene._attach(node as Mesh)
  for (let c of node.children) enterScene(c, scene)
  scene._schedule()
}

function leaveScene(node: SceneNode): void {
  let scene = node._scene
  if (scene && node.kind === "mesh") scene._detach(node as Mesh)
  node._scene = null
  for (let c of node.children) leaveScene(c)
}

export type { TransformUpdate } from "./math.ts"

/**
 * The one write path for node transforms (so the scene knows to sync).
 * Values are copied in; absent keys keep their current value. This is also
 * the frame-rate escape hatch: call it from onFrame on a node grabbed via
 * `ref`, bypassing signals entirely.
 *
 * A write that changes nothing schedules nothing, so driving every node
 * unconditionally from onFrame costs only the compare for the nodes that
 * did not move. Rotation is compared after conversion, so passing an euler
 * equal to the node's current quaternion is also a no-op.
 */
export function setTransform(node: SceneNode, update: TransformUpdate): void {
  // A no-op write costs nothing: driving every node from onFrame is the
  // intended shape, and most nodes did not move. Exact compares, like
  // setVisible - a value that survives a float round trip unchanged is the
  // same value, and an epsilon would need a scale-dependent one anyway.
  let changed = false
  let p = update.position
  if (p && (p[0] !== node.position[0] || p[1] !== node.position[1] || p[2] !== node.position[2])) {
    node.position[0] = p[0]
    node.position[1] = p[1]
    node.position[2] = p[2]
    changed = true
  }
  if (updateRotation(rotScratch, update, "setTransform")) {
    let n = node.quaternion
    if (rotScratch[0] !== n[0] || rotScratch[1] !== n[1] || rotScratch[2] !== n[2] || rotScratch[3] !== n[3]) {
      n[0] = rotScratch[0]
      n[1] = rotScratch[1]
      n[2] = rotScratch[2]
      n[3] = rotScratch[3]
      changed = true
    }
  }
  if (update.scale !== undefined) {
    updateScale(scaleScratch, update.scale)
    if (scaleScratch[0] !== node.scale[0] || scaleScratch[1] !== node.scale[1] || scaleScratch[2] !== node.scale[2]) {
      node.scale[0] = scaleScratch[0]
      node.scale[1] = scaleScratch[1]
      node.scale[2] = scaleScratch[2]
      changed = true
    }
  }
  if (!changed) return
  node._localDirty = true
  node._scene?._schedule()
}

/**
 * Aim a node at a WORLD-space point, Three's `Object3D.lookAt`: the node's
 * local +z ends up pointing at `target`, with `up` (world space, default
 * +y) choosing the roll about that axis. Ancestor transforms are undone,
 * so the aim holds under a rotated group - the ancestor chain is brought
 * up to date on the spot rather than waiting for the pending sync.
 *
 * +z because that is the library's own sweep axis (`extrude`, `sweep`,
 * `tube` run along z), so aiming their output needs no correction. For a
 * y-axis solid (`cylinder`, `cone`) reach for `quatFromTo` instead, which
 * takes the axis to aim as an argument.
 *
 * Writes `node.quaternion` - an ordinary rotation afterwards, readable and
 * overwritable by setTransform. To aim along a DIRECTION rather than at a
 * point, add it to the node's world position (`worldPosition`), the same
 * conversion Three asks for.
 *
 * Exact for rotation and uniform scale in the ancestor chain; a
 * non-uniformly scaled ancestor shears the frame and the aim is
 * approximate, exactly as in Three (both read the parent's upper 3x3 as
 * if it were a rotation).
 */
export function lookAt(node: SceneNode, target: Vec3, up: Vec3 = WORLD_UP): void {
  let parent = node.parent
  if (parent === null) {
    // No ancestors: parent space IS world space, aim straight from the
    // node's own position.
    aimScratch[0] = target[0] - node.position[0]
    aimScratch[1] = target[1] - node.position[1]
    aimScratch[2] = target[2] - node.position[2]
    quatFromFrame(node.quaternion, aimScratch, up)
  } else {
    let world = worldInto(worldScratch, parent)
    transformPoint(pointScratch, world, node.position)
    aimScratch[0] = target[0] - pointScratch[0]
    aimScratch[1] = target[1] - pointScratch[1]
    aimScratch[2] = target[2] - pointScratch[2]
    // World -> parent space for both vectors: rotating forward and up
    // rotates the frame they build, so converting the inputs is the same
    // as converting the resulting rotation, and needs no matrix inverse.
    unrotate(aimScratch, world, aimScratch)
    unrotate(upScratch, world, up)
    quatFromFrame(node.quaternion, aimScratch, upScratch)
  }
  node._localDirty = true
  node._scene?._schedule()
}

/**
 * A node's rotation as Euler radians in XYZ order, copied into `out` (or a
 * fresh Vec3). A convenience for reading and debugging, NOT a peer of
 * `node.quaternion`: the conversion is lossy in the sense that it cannot
 * recover the triple that was written (see eulerFromQuat), only a triple
 * that means the same rotation. Anything composing or interpolating
 * rotations should work with the quaternion.
 */
export function getRotation(node: SceneNode, out: Vec3 = [0, 0, 0]): Vec3 {
  return eulerFromQuat(out, node.quaternion)
}

/**
 * A node's position in world space, copied into `out` (or a fresh Vec3) -
 * Three's `getWorldPosition`. Brings the ancestor chain up to date first,
 * so it is exact before the pending sync has run.
 */
export function worldPosition(node: SceneNode, out: Vec3 = [0, 0, 0]): Vec3 {
  let world = worldInto(worldScratch, node)
  out[0] = world[12]
  out[1] = world[13]
  out[2] = world[14]
  return out
}

/**
 * `out` = node's world matrix, composing any dirty locals up the chain
 * WITHOUT clearing their flags: the pending sync still has to see them to
 * write uModel. One shared local scratch serves any depth - each frame
 * uses it only after its recursive call has returned.
 */
function worldInto(out: Mat4, node: SceneNode): Mat4 {
  if (node.parent === null) identity(out)
  else worldInto(out, node.parent)
  let local = node._localDirty
    ? compose(localScratch, node.position, node.quaternion, node.scale)
    : node._local
  return multiply(out, out, local)
}

/**
 * `out` = v with m's rotation undone: the transpose of m's upper 3x3 with
 * its columns normalized, so uniform scale divides out. out may alias v.
 */
function unrotate(out: Vec3, m: Mat4, v: Vec3): Vec3 {
  let x = v[0], y = v[1], z = v[2]
  let l0 = Math.hypot(m[0], m[1], m[2]) || 1
  let l1 = Math.hypot(m[4], m[5], m[6]) || 1
  let l2 = Math.hypot(m[8], m[9], m[10]) || 1
  out[0] = (m[0] * x + m[1] * y + m[2] * z) / l0
  out[1] = (m[4] * x + m[5] * y + m[6] * z) / l1
  out[2] = (m[8] * x + m[9] * y + m[10] * z) / l2
  return out
}

/** Show or hide a node and its whole subtree (a hidden mesh costs one
 * `instanceCount: 0` draw range - the entry stays, drawing nothing). */
export function setVisible(node: SceneNode, visible: boolean): void {
  if (node.visible === visible) return
  node.visible = visible
  node._scene?._schedule()
}

/** Set a mesh's explicit draw-order key (see Mesh.renderOrder). */
export function setRenderOrder(mesh: Mesh, order: number): void {
  if (mesh.renderOrder === order) return
  mesh.renderOrder = order
  mesh._scene?._reorder()
}

/** Swap a mesh's geometry: its draw entry is rebuilt (the scene re-sorts
 * the list, so the mesh keeps its place). */
export function setGeometry(mesh: Mesh, geometry: Geometry): void {
  if (mesh.geometry === geometry) return
  mesh.geometry = geometry
  rebuildEntry(mesh)
}

/** Swap a mesh's material: its draw entry is rebuilt. */
export function setMaterial(mesh: Mesh, material: Material): void {
  if (mesh.material === material) return
  mesh.material = material
  rebuildEntry(mesh)
}

function rebuildEntry(mesh: Mesh): void {
  let scene = mesh._scene
  if (scene) {
    scene._detach(mesh)
    scene._attach(mesh)
  }
}

/**
 * Write per-mesh uniforms - the channel for a custom material's app-driven
 * values (a time, a per-object tint). Names must be
 * declared and used by the mesh's material shaders (unknown names throw at
 * the call site, the engine's validation contract). Values persist on the
 * mesh: they survive geometry/material entry rebuilds and re-apply then.
 * Also the frame-rate path - like setTransform, call it from onFrame
 * freely.
 */
export function setMeshParams(mesh: Mesh, params: ShaderParams): void {
  if (mesh._params === null) mesh._params = {}
  Object.assign(mesh._params, params)
  mesh._scene?._setParams(mesh, params)
}

/**
 * Create a scene rendering into a depth-buffered draw target of the given
 * size. Returns the scene handle; `scene.texture` is the output. Inside a
 * reactive scope the scene disposes with the owner (opt out with
 * `autoFree: false`); outside one, call `dispose()` yourself.
 */
export function createScene(width: number, height: number, opts?: SceneOptions): Scene {
  let texture = createDrawTarget(width, height, null, {
    depth: true,
    clearColor: opts?.clearColor,
    filter: opts?.filter,
    wrap: opts?.wrap,
    label: opts?.label ?? "scene",
    autoFree: false,
  })
  let disposed = false
  let scheduled = false

  // Picking state: the broadphase tree over world boxes, kept current by
  // the sync walk (the meshes it touches are exactly the leaves to move),
  // and the pointer bookkeeping behind scene.handlers.
  let bvh = createBvh<Mesh>()
  let capture = new Map<number, Mesh>()
  let hover = new Map<number, Mesh>()

  // Live meshes (those holding a draw entry) in add order; the background
  // entry never joins this list. Draw order is derived from it by
  // orderEntries (order.ts) whenever orderDirty. Camera moves and
  // transparent-mesh moves only dirty the order when two or more transparent
  // meshes exist - fewer cannot change relative order.
  let meshes: Mesh[] = []
  let transparentCount = 0
  let orderDirty = false
  // The order last handed to the engine: a resort that lands on the same
  // permutation (the common case under a moving camera) issues nothing.
  let lastOrder: DrawId[] = []
  let background: { entry: DrawId; pipeline: RenderPipelineId; program: ProgramId } | null = null
  let sortEntries = () => {
    orderDirty = false
    let order = orderEntries(meshes, view, background?.entry)
    if (order.length === lastOrder.length && order.every((id, i) => id === lastOrder[i])) return
    lastOrder = order
    setDrawOrder(texture, order)
  }

  // Reinsert or refit a mesh's broadphase leaf from its fresh world matrix:
  // the local box's center/extents carried through the absolute matrix (the
  // standard tight-AABB-of-a-transformed-AABB construction).
  let updateLeaf = (mesh: Mesh): void => {
    let b = geometryBounds(mesh.geometry)
    let m = mesh._world
    let cx = (b[0]! + b[3]!) / 2
    let cy = (b[1]! + b[4]!) / 2
    let cz = (b[2]! + b[5]!) / 2
    let ex = (b[3]! - b[0]!) / 2
    let ey = (b[4]! - b[1]!) / 2
    let ez = (b[5]! - b[2]!) / 2
    let wx = m[0] * cx + m[4] * cy + m[8] * cz + m[12]
    let wy = m[1] * cx + m[5] * cy + m[9] * cz + m[13]
    let wz = m[2] * cx + m[6] * cy + m[10] * cz + m[14]
    mesh._center[0] = wx
    mesh._center[1] = wy
    mesh._center[2] = wz
    let rx = Math.abs(m[0]) * ex + Math.abs(m[4]) * ey + Math.abs(m[8]) * ez
    let ry = Math.abs(m[1]) * ex + Math.abs(m[5]) * ey + Math.abs(m[9]) * ez
    let rz = Math.abs(m[2]) * ex + Math.abs(m[6]) * ey + Math.abs(m[10]) * ez
    if (mesh._pickLeaf === null) {
      mesh._pickLeaf = bvh.insert(mesh, wx - rx, wy - ry, wz - rz, wx + rx, wy + ry, wz + rz)
    } else {
      bvh.update(mesh._pickLeaf, wx - rx, wy - ry, wz - rz, wx + rx, wy + ry, wz + rz)
    }
  }

  let fov = 60
  let near = 0.1
  let far = 100
  let eye: Vec3 = [0, 0, 3]
  let target: Vec3 = [0, 0, 0]
  let up: Vec3 = [0, 1, 0]
  let cameraDirty = true
  let cameraPending = false
  let proj = mat4()
  let view = mat4()
  let viewProj = mat4()
  let clip: Vec4 = [0, 0, 0, 0]

  // Matrix recompute, split from sync so project()/viewProj() see a fresh
  // matrix right after setCamera, before the microtask runs. cameraPending
  // keeps the GPU write owed to the next sync.
  let ensureCamera = () => {
    if (!cameraDirty) return
    cameraDirty = false
    cameraPending = true
    perspective(proj, (fov * Math.PI) / 180, width / height, near, far)
    lookAtMatrix(view, eye, target, up)
    multiply(viewProj, proj, view)
  }

  let sync = () => {
    scheduled = false
    if (disposed) return
    ensureCamera()
    if (cameraPending) {
      // The camera is target state: one shared write, whatever the scene
      // holds. Entries are untouched - uModel is camera-independent, and
      // uCamPos is stored even when no current material declares it.
      cameraPending = false
      // The camera basis rides along: the view matrix's first two rows are
      // the camera's world-space right and up (no clip flip - that lives in
      // the projection), so a billboard needs no reconstruction from uViewProj.
      setTargetParams(texture, {
        uViewProj: viewProj,
        uCamPos: eye,
        uCamRight: [view[0], view[4], view[8]],
        uCamUp: [view[1], view[5], view[9]],
      })
      if (transparentCount > 1) orderDirty = true
    }
    let walk = (node: SceneNode, parentChanged: boolean, parentVisible: boolean) => {
      let changed = parentChanged
      if (node._localDirty) {
        compose(node._local, node.position, node.quaternion, node.scale)
        node._localDirty = false
        changed = true
      }
      if (changed) {
        multiply(node._world, node.parent ? node.parent._world : IDENTITY, node._local)
      }
      let shown = parentVisible && node.visible
      if (node.kind === "mesh") {
        let mesh = node as Mesh
        if (mesh._entry !== null) {
          if (mesh._hidden === shown) {
            // Mismatch: flip the entry's cheap off switch.
            setDrawRange(texture, mesh._entry, { instanceCount: shown ? 1 : 0 })
            mesh._hidden = !shown
            if (shown) mesh._fresh = true
          }
          if (changed && mesh._transparent && transparentCount > 1) orderDirty = true
          if (!mesh._hidden && (changed || mesh._fresh)) {
            if (mesh.material.normalMatrix) {
              setDrawParams(texture, mesh._entry, {
                uModel: mesh._world,
                uNormal: normalMatrix(normalScratch, mesh._world),
              })
            } else {
              setDrawParams(texture, mesh._entry, { uModel: mesh._world })
            }
            mesh._fresh = false
          } else if (changed) {
            // Moved while hidden: write the fresh matrix on unhide.
            mesh._fresh = true
          }
          // The broadphase leaf follows the world matrix - hidden meshes
          // included (they stay in the tree and are skipped at query time,
          // so unhiding never picks against a stale box).
          if (changed || mesh._pickLeaf === null) updateLeaf(mesh)
        }
      }
      for (let c of node.children) walk(c, changed, shown)
    }
    walk(root, false, true)
    if (orderDirty) sortEntries()
  }

  let hooks: SceneHooks = {
    _schedule() {
      if (scheduled || disposed) return
      scheduled = true
      RESOLVED.then(sync)
    },
    _attach(mesh) {
      if (disposed) return
      // Layout is stride: a mismatched pair would not miss a channel, it
      // would read garbage - so it is an error here, like the rest of the
      // strict entry path.
      let geoLayout = mesh.geometry.layout ?? "standard"
      let matLayout = mesh.material.layout ?? "standard"
      if (geoLayout !== matLayout) {
        throw new Error(
          "Mesh geometry layout '" + geoLayout + "' does not match its material's '" + matLayout +
            "' - a material reading aColor needs withColors() geometry, and colored geometry needs such a material",
        )
      }
      let bufs = geometryBuffers(mesh.geometry)
      // The uNormal seed keys off the material flag because entry params
      // validate strictly - and a material declaring uNormal without using
      // it therefore throws right here, at add().
      let seed: ShaderParams = mesh.material.normalMatrix
        ? { uModel: IDENTITY, uNormal: IDENTITY, ...mesh.material.params, ...mesh._params }
        : { uModel: IDENTITY, ...mesh.material.params, ...mesh._params }
      // The entry starts switched off: it has no world matrix yet - the walk
      // in sync() computes one - and _schedule() defers that to a microtask,
      // so added live it would draw at the seeded identity until then. The
      // mismatch branch in sync() turns it on in the same pass that writes
      // uModel.
      mesh._entry = addDraw(texture, mesh.material.pipeline(), seed, {
        buffer: bufs.buffer,
        indexBuffer: bufs.index,
        indexFormat: bufs.indexFormat,
        textures: mesh.material.textures,
        instanceCount: 0,
      })
      meshes.push(mesh)
      mesh._transparent = mesh.material.transparent === true
      if (mesh._transparent) transparentCount++
      orderDirty = true
      mesh._hidden = true
      mesh._fresh = true
      this._schedule()
    },
    _detach(mesh) {
      if (mesh._entry !== null) {
        if (!disposed) removeDraw(texture, mesh._entry)
        let i = meshes.indexOf(mesh)
        if (i >= 0) meshes.splice(i, 1)
        if (mesh._transparent) transparentCount--
        orderDirty = true
      }
      mesh._entry = null
      // The leaf goes with the entry: a geometry swap rebuilds the entry,
      // and re-inserting is what picks up the new local bounds.
      if (mesh._pickLeaf !== null) {
        bvh.remove(mesh._pickLeaf)
        mesh._pickLeaf = null
      }
    },
    _setParams(mesh, params) {
      if (mesh._entry !== null && !disposed) setDrawParams(texture, mesh._entry, params)
    },
    _reorder() {
      orderDirty = true
      this._schedule()
    },
  }

  let root = makeNode("group")
  root._scene = hooks

  // --- Pointer event dispatch (behind scene.handlers) ---

  type BubbleName = "onPointerDown" | "onPointerMove" | "onPointerUp"
  type InternalEvent = ScenePointerEvent & { _stopped: boolean }

  let makeEvent = (e: ElementPointerEvent, mesh: Mesh, x: number, y: number, point: Vec3 | null, distance: number | null): InternalEvent => {
    let event: InternalEvent = {
      mesh,
      currentTarget: mesh,
      point,
      distance,
      x,
      y,
      pointerId: e.pointerId,
      pointerType: e.pointerType,
      button: e.button,
      shiftKey: e.shiftKey,
      ctrlKey: e.ctrlKey,
      altKey: e.altKey,
      metaKey: e.metaKey,
      _stopped: false,
      stopPropagation() {
        event._stopped = true
      },
    }
    return event
  }

  let bubble = (name: BubbleName, event: InternalEvent): void => {
    for (let n: SceneNode | null = event.mesh; n !== null && !event._stopped; n = n.parent) {
      let handler = n[name]
      if (handler) {
        event.currentTarget = n
        handler(event)
      }
    }
  }

  // The captured mesh's own hit, if the ray still strikes it.
  let hitOn = (mesh: Mesh, x: number, y: number): Hit | null => {
    for (let h of scene.pick(x, y)) if (h.mesh === mesh) return h
    return null
  }

  // localX/localY arrive in the leaf's LAYOUT frame (the hit test undoes
  // every transform above it, viewBox fits included), so a leaf laid out at
  // the target size - the built-in <Scene> leaf, a d-texture at natural
  // size - is already in scene pixels. Only a leaf deliberately laid out at
  // a DIFFERENT size (the supersampling pattern) needs the ratio, and only
  // the app knows that layout: handlersFor takes it.
  let makeHandlers = (layout: (() => { width: number; height: number }) | null): SceneHandlers => {
    let eventX = 0
    let eventY = 0
    let toScene = (e: ElementPointerEvent): void => {
      if (layout === null) {
        eventX = e.localX
        eventY = e.localY
        return
      }
      let l = layout()
      eventX = e.localX * (l.width > 0 ? width / l.width : 1)
      eventY = e.localY * (l.height > 0 ? height / l.height : 1)
    }
    return {
      onPointerDown(e) {
        toScene(e)
        let hit = scene.pick(eventX, eventY)[0]
        if (hit === undefined) return
        capture.set(e.pointerId, hit.mesh)
        bubble("onPointerDown", makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
      },
      onPointerMove(e) {
        toScene(e)
        let captured = capture.get(e.pointerId)
        if (captured !== undefined) {
          let hit = hitOn(captured, eventX, eventY)
          bubble("onPointerMove", makeEvent(e, captured, eventX, eventY, hit ? hit.point : null, hit ? hit.distance : null))
          return
        }
        let hit = scene.pick(eventX, eventY)[0]
        let prev = hover.get(e.pointerId)
        if (prev !== hit?.mesh) {
          if (prev !== undefined) {
            hover.delete(e.pointerId)
            prev.onPointerLeave?.(makeEvent(e, prev, eventX, eventY, null, null))
          }
          if (hit !== undefined) {
            hover.set(e.pointerId, hit.mesh)
            hit.mesh.onPointerEnter?.(makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
          }
        }
        if (hit !== undefined) {
          bubble("onPointerMove", makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
        }
      },
      onPointerUp(e) {
        toScene(e)
        let captured = capture.get(e.pointerId)
        if (captured !== undefined) {
          capture.delete(e.pointerId)
          let hit = hitOn(captured, eventX, eventY)
          bubble("onPointerUp", makeEvent(e, captured, eventX, eventY, hit ? hit.point : null, hit ? hit.distance : null))
          return
        }
        let hit = scene.pick(eventX, eventY)[0]
        if (hit !== undefined) {
          bubble("onPointerUp", makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
        }
      },
      onPointerLeave(e) {
        let prev = hover.get(e.pointerId)
        if (prev !== undefined) {
          hover.delete(e.pointerId)
          toScene(e)
          prev.onPointerLeave?.(makeEvent(e, prev, eventX, eventY, null, null))
        }
      },
    }
  }
  let handlers = makeHandlers(null)

  let scene: Scene = {
    texture,
    root,
    setCamera(update) {
      if (update.fov !== undefined) fov = update.fov
      if (update.near !== undefined) near = update.near
      if (update.far !== undefined) far = update.far
      if (update.position) eye = [update.position[0], update.position[1], update.position[2]]
      if (update.target) target = [update.target[0], update.target[1], update.target[2]]
      if (update.up) up = [update.up[0], update.up[1], update.up[2]]
      cameraDirty = true
      hooks._schedule()
    },
    setSize(w, h) {
      if (disposed || (w === width && h === height)) return
      width = w
      height = h
      setTargetSize(texture, w, h)
      cameraDirty = true
      hooks._schedule()
    },
    setParams(params) {
      if (!disposed) setTargetParams(texture, params)
    },
    setBackground(source) {
      if (disposed) return
      if (background !== null) {
        removeDraw(texture, background.entry)
        destroyRenderPipeline(background.pipeline)
        destroyProgram(background.program)
        background = null
      }
      if (source === null) return
      let built = backgroundPipeline(source, (opts?.label ?? "scene") + "-background")
      // First in list order: inserted before the first mesh entry, and every
      // later sort keeps it there.
      let entry = addDraw(texture, built.pipeline, null, { vertexCount: 3, before: meshes[0]?._entry ?? undefined })
      background = { entry, pipeline: built.pipeline, program: built.program }
    },
    project(point) {
      ensureCamera()
      transformPoint(clip, viewProj, point)
      let w = clip[3]
      if (w < 1e-6) return null
      // perspective() bakes the y-down clip flip, so NDC maps straight to
      // top-left-origin pixels with no negation here.
      return { x: ((clip[0] / w) * 0.5 + 0.5) * width, y: ((clip[1] / w) * 0.5 + 0.5) * height, w }
    },
    viewProj(out) {
      ensureCamera()
      return copy(out ?? mat4(), viewProj)
    },
    pick(x, y) {
      ensureCamera()
      // The camera-frame ray through the pixel, inverting project()'s
      // mapping: the baked y-down clip flip is why pixel y converts with
      // no negation there and one here.
      let f = 1 / Math.tan(((fov * Math.PI) / 180) / 2)
      let cx = (((x / width) * 2 - 1) * (width / height)) / f
      let cy = -((y / height) * 2 - 1) / f
      // The view's upper 3x3 rows are the camera axes, so its transpose
      // carries the camera-space direction (cx, cy, -1) to world.
      pickDir[0] = cx * view[0] + cy * view[1] - view[2]
      pickDir[1] = cx * view[4] + cy * view[5] - view[6]
      pickDir[2] = cx * view[8] + cy * view[9] - view[10]
      return scene.raycast(eye, pickDir)
    },
    raycast(origin, direction) {
      // Flush pending writes: picking sees the tree as the app just wrote
      // it, the same immediacy contract as lookAt()/project(). (The queued
      // microtask still runs and finds nothing dirty - harmless.)
      if (scheduled) sync()
      let dx = direction[0]
      let dy = direction[1]
      let dz = direction[2]
      let len = Math.hypot(dx, dy, dz)
      if (len === 0 || disposed) return []
      dx /= len
      dy /= len
      dz /= len
      let ox = origin[0]
      let oy = origin[1]
      let oz = origin[2]
      let hits: Hit[] = []
      bvh.raycast(ox, oy, oz, dx, dy, dz, mesh => {
        if (mesh._hidden || mesh._entry === null) return
        // Narrowphase: the ray in the mesh's local frame against its tight
        // local box - exact under any affine world transform. The local
        // direction stays unnormalized on purpose: an affine map preserves
        // the ray parameter, so t is world units as-is.
        invertAffine(pickInv, mesh._world)
        transformPoint(pickOrigin, pickInv, origin)
        pickDir[0] = dx
        pickDir[1] = dy
        pickDir[2] = dz
        transformVector(pickDir, pickInv, pickDir)
        let b = geometryBounds(mesh.geometry)
        let t = rayBoxDistance(
          pickOrigin[0], pickOrigin[1], pickOrigin[2],
          pickDir[0], pickDir[1], pickDir[2],
          b[0]!, b[1]!, b[2]!, b[3]!, b[4]!, b[5]!,
        )
        if (t >= 0) hits.push({ mesh, distance: t, point: [ox + dx * t, oy + dy * t, oz + dz * t] })
      })
      hits.sort((a, b) => a.distance - b.distance)
      return hits
    },
    handlers,
    handlersFor(layout) {
      return makeHandlers(layout)
    },
    dispose() {
      if (disposed) return
      disposed = true
      destroyTexture(texture)
      if (background !== null) {
        // The entry died with the target; the pipeline and program are the
        // scene's own (unlike shared material pipelines), so they go too.
        destroyRenderPipeline(background.pipeline)
        destroyProgram(background.program)
        background = null
      }
    },
  }
  if (opts?.background !== undefined) scene.setBackground(opts.background)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => scene.dispose())
  return scene
}
