// The retained scene: plain objects, no signals - the hot path (a moved
// node) is flat imperative code, and reactivity stays at the component
// boundary (components.tsx). The transform hierarchy itself lives in the
// spatial core (flux:spatial): every node in a scene has a core node, JS
// keeps the LOCAL transform as the readable source of truth and forwards
// each write, and the core's flush recomputes only the moved subtrees and
// writes each mesh entry's uModel (and, for materials declaring it,
// uNormal) - so a move costs its subtree, never the scene. A scene
// compiles to one draw target: every mesh is one draw entry, and the
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
// each write lands in the core, the microtask flushes it, and the frame
// renders once.
//
// Still in JS this stage (see okf/backlog/spatial-core.md): the picking
// broadphase and its leaves, the transparent sort's centers and the light
// params. They read world matrices back from the core, and only for the
// subtrees that moved since they last looked.

import { addDraw, createBuffer, createDrawTarget, destroyBuffer, destroyProgram, destroyRenderPipeline, destroyTexture, removeDraw, setDrawBuffers, setDrawOrder, setDrawParams, setTargetParams, setTargetSize, writeBuffer } from "@solidrt/core/gpu"
import * as spatial from "flux:spatial"
import type { NodeId } from "flux:spatial"
import type { BufferId, DrawId, FilterMode, ProgramId, RenderPipelineId, ShaderParams, TextureId, VertexAttribute, WrapMode } from "@solidrt/core/gpu"
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
// The scene's lookAt() aims a node; math's builds a camera's view matrix -
// the same pairing (and the same name) as Three's Object3D/Matrix4.
import { compose, copy, eulerFromQuat, identity, lookAt as lookAtMatrix, mat4, multiply, perspective, quat, quatFromFrame, transformPoint, updateRotation, updateScale } from "./math.ts"
import type { Mat4, Quat, TransformUpdate, Vec3, Vec4 } from "./math.ts"
import { MAX_LIGHTS } from "./glsl.ts"
import { geometryBounds, layoutKey, validateGeometry } from "./geometry.ts"
import { acquireGeometryBuffers, releaseGeometryBuffers } from "./geometry-gpu.ts"
import type { GeometryBuffers } from "./geometry-gpu.ts"
import type { Geometry } from "./geometry.ts"
import { backgroundPipeline, missingAttributes } from "./material.ts"
import { orderEntries } from "./order.ts"
import type { Material } from "./material.ts"

const IDENTITY = mat4()
const RESOLVED = Promise.resolve()
// lookAt()'s default roll reference. Read-only: quatFromFrame never
// writes its inputs, so one shared vector is safe.
const WORLD_UP: Vec3 = [0, 1, 0]
// The FFI carriers: one transform write (position, quaternion, scale) and
// one world-matrix read. Values are copied at the boundary, so one of each
// serves every call.
let transformScratch = new Float32Array(10)
let worldRead = new Float32Array(16)
// lookAt()/worldPosition() scratch: nothing here outlives a single call.
let worldScratch = mat4()
let localScratch = mat4()
let rayOriginScratch = new Float32Array(3)
let rayDirScratch = new Float32Array(3)
let pointScratch: Vec4 = [0, 0, 0, 0]
let aimScratch: Vec3 = [0, 0, 0]
let upScratch: Vec3 = [0, 0, 0]
// pick()'s camera-ray scratch.
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
  _attachLight(light: Light): void
  _detachLight(light: Light): void
  _lightChanged(): void
  _setParams(mesh: Mesh, params: ShaderParams): void
  _setCount(mesh: Mesh): void
  /** Re-point the mesh's entry at its (replaced) instance buffer. */
  _setBuffer(mesh: Mesh): void
  _reorder(): void
  /** The node's transform changed (for the sort and light bookkeeping). */
  _moved(node: SceneNode): void
}

export type SceneNode = {
  kind: "group" | "mesh" | "light"
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
  /** The core node while in a scene (created at add, freed at remove). */
  _node: NodeId | null
  _moved: boolean
  _scene: SceneHooks | null
}

/** A directional light node: parallel rays travelling along `direction`
 * in the node's LOCAL space, so a parent's rotation turns the light with
 * it (the default `[0, -1, 0]` is a sun straight overhead; the length is
 * ignored). Position and scale do not affect it. Write through setLight. */
export type DirectionalLight = SceneNode & {
  kind: "light"
  type: "directional"
  direction: Vec3
  /** Linear [r, g, b] 0..1. */
  color: Vec3
  intensity: number
}

/** The ambient term: a sky/ground gradient by the WORLD normal's
 * vertical tilt (fixed to world up, not the node's). One per scene - the
 * last attached wins. Write through setLight. */
export type HemisphereLight = SceneNode & {
  kind: "light"
  type: "hemisphere"
  sky: Vec3
  ground: Vec3
  intensity: number
}

export type Light = DirectionalLight | HemisphereLight

export type Mesh = SceneNode & {
  kind: "mesh"
  geometry: Geometry
  material: Material
  /** Explicit draw-order key (default 0), Three's name: lower draws first.
   * Sorts within the opaque group and within the transparent group; the
   * transparent group always follows the opaque one. Set with setRenderOrder. */
  renderOrder: number
  _entry: DrawId | null
  /** The geometry-buffer reference the entry was built from, acquired at
   * attach and what _detach releases - like _transparent, a snapshot,
   * because setGeometry swaps mesh.geometry before the rebuild. */
  _buffers: GeometryBuffers | null
  /** material.transparent as of the last attach - the entry's actual
   * pipeline state, and what _detach counts against (setMaterial swaps
   * mesh.material before the rebuild). */
  _transparent: boolean
  /** World-space center of the local bounds, refreshed at sort time: the
   * transparent sort key. */
  _center: Vec3
  _params: ShaderParams | null
  /** Instance state when the mesh was made by createInstancedMesh; null on
   * an ordinary mesh. */
  _instances: MeshInstances | null
}

/** The per-mesh half of instancing: the record buffer and its bookkeeping.
 * Read the public fields freely; write through setInstances /
 * setInstanceCount so the draw range follows. */
export type MeshInstances = {
  /** The GPU record buffer, owned by the mesh (disposeInstances frees it). */
  buffer: BufferId
  /** Floats per record - the material's instanceAttributes summed. */
  stride: number
  /** Records the buffer has room for; doubles when setInstances writes
   * more (a replacement buffer, never a resize). */
  capacity: number
  /** The buffer label, carried to replacement buffers on growth. */
  label: string | undefined
  /** Records currently drawn (the entry's instanceCount while visible). */
  count: number
  /** Explicit LOCAL bounds covering the whole population ([minX, minY,
   * minZ, maxX, maxY, maxZ]), or null: the mesh then has no picking leaf -
   * records are opaque data, so the library cannot derive where the
   * instances are. */
  bounds: Float32Array | null
}

/** A mesh from createInstancedMesh: an ordinary Mesh whose entry draws
 * `instances.count` copies of the geometry, one record each. */
export type InstancedMesh = Mesh & { _instances: MeshInstances }

/** One picking intersection, Three's intersect result: the mesh, the
 * camera-ray distance in world units, the world-space point, and for a
 * triangle hit (every ordinary mesh - the test is per triangle, so a ray
 * through a knot's hole misses) the world-space geometric `normal` facing
 * the ray, the triangle index `face` and the interpolated texture `uv`.
 * An instanced mesh is picked by its explicit population box, so those
 * three are absent on its hits. */
export type Hit = {
  mesh: Mesh
  distance: number
  point: Vec3
  normal?: Vec3
  face?: number
  uv?: [number, number]
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
  /** Multisample count of the target (1, 2, 4 or 8; default 1). Storage-only
   * anti-aliasing of mesh edges; see createDrawTarget. */
  samples?: 1 | 2 | 4 | 8
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
  /** Destroy the target (entries die with it). Idempotent. Material
   * pipelines are shared and survive (app-lifetime, see material.ts);
   * geometry buffers are reference-counted and freed with their last
   * entry (see geometry-gpu.ts). */
  dispose(): void
}

function makeNode(kind: SceneNode["kind"]): SceneNode {
  return {
    kind,
    parent: null,
    children: [],
    position: [0, 0, 0],
    quaternion: [0, 0, 0, 1],
    scale: [1, 1, 1],
    visible: true,
    _node: null,
    _moved: false,
    _scene: null,
  }
}

export function createGroup(): SceneNode {
  return makeNode("group")
}

export type DirectionalLightOptions = { direction?: Vec3; color?: Vec3; intensity?: number }
export type HemisphereLightOptions = { sky?: Vec3; ground?: Vec3; intensity?: number }

export function createDirectionalLight(opts: DirectionalLightOptions = {}): DirectionalLight {
  let light = makeNode("light") as DirectionalLight
  light.type = "directional"
  light.direction = [...(opts.direction ?? [0, -1, 0])] as Vec3
  light.color = [...(opts.color ?? [1, 1, 1])] as Vec3
  light.intensity = opts.intensity ?? 1
  return light
}

export function createHemisphereLight(opts: HemisphereLightOptions = {}): HemisphereLight {
  let light = makeNode("light") as HemisphereLight
  light.type = "hemisphere"
  light.sky = [...(opts.sky ?? [1, 1, 1])] as Vec3
  light.ground = [...(opts.ground ?? [0.2, 0.2, 0.2])] as Vec3
  light.intensity = opts.intensity ?? 1
  return light
}

/** The write path for a light's own fields (color, intensity, direction
 * or sky/ground); absent keys keep their value. Its placement goes
 * through setTransform like any node. Frame-rate-safe. */
export function setLight(light: DirectionalLight, update: DirectionalLightOptions): void
export function setLight(light: HemisphereLight, update: HemisphereLightOptions): void
export function setLight(light: Light, update: DirectionalLightOptions & HemisphereLightOptions): void {
  if (update.intensity !== undefined) light.intensity = update.intensity
  if (light.type === "directional") {
    if (update.direction !== undefined) light.direction = [...update.direction] as Vec3
    if (update.color !== undefined) light.color = [...update.color] as Vec3
  } else {
    if (update.sky !== undefined) light.sky = [...update.sky] as Vec3
    if (update.ground !== undefined) light.ground = [...update.ground] as Vec3
  }
  light._scene?._lightChanged()
}

export function createMesh(geometry: Geometry, material: Material): Mesh {
  let mesh = makeNode("mesh") as Mesh
  mesh.geometry = geometry
  mesh.material = material
  mesh.renderOrder = 0
  mesh._entry = null
  mesh._buffers = null
  mesh._transparent = false
  mesh._center = [0, 0, 0]
  mesh._params = null
  mesh._instances = null
  return mesh
}

/** The local box picking and sorting work from: explicit instance bounds
 * when the mesh is instanced (null without them - no leaf, no hits), the
 * geometry's own bounds otherwise. */
function localBounds(mesh: Mesh): Float32Array | null {
  return mesh._instances !== null ? mesh._instances.bounds : geometryBounds(mesh.geometry)
}

const ATTRIBUTE_FLOATS: Record<VertexAttribute["format"], number> = { f32: 1, vec2: 2, vec3: 3, vec4: 4 }

function instanceStride(attributes: VertexAttribute[]): number {
  let stride = 0
  for (let a of attributes) stride += ATTRIBUTE_FLOATS[a.format]
  return stride
}

export type InstancedMeshOptions = {
  /** LOCAL bounds covering every instance the records place ([minX, minY,
   * minZ, maxX, maxY, maxZ] - geometryBounds' shape), copied in. Records
   * are opaque data, so only the app knows where its instances are: with
   * bounds the mesh picks and transparent-sorts like any other
   * (conservatively - one box around the whole population); without, it
   * has no picking leaf and pointer events never target it. */
  bounds?: ArrayLike<number>
  /** Debug label for the record buffer. */
  label?: string
}

/**
 * A mesh drawing `geometry` once per record of `records`: one draw entry,
 * one uModel write, N instances - the shape for forests, particles, and
 * every fleet whose per-copy data is a few floats rather than a merged
 * vertex buffer. The material must declare `instanceAttributes`
 * (shaderMaterialClass); its vertex stage reads each record through those
 * `in` variables. `records` is the interleaved attribute data (stride =
 * the attributes' floats summed) and is uploaded here; its length is the
 * buffer's initial capacity, which setInstances grows past on demand.
 * `count` limits how many records draw (default all), up to capacity.
 *
 * The result is an ordinary Mesh: add/remove, setTransform (uModel places
 * the whole population), setVisible (hiding zeroes the drawn count,
 * unhiding restores it), setMeshParams and renderOrder all apply. Update
 * records with setInstances, the drawn count with setInstanceCount, and
 * free the record buffer with disposeInstances when done for good.
 */
export function createInstancedMesh(
  geometry: Geometry,
  material: Material,
  records: Float32Array,
  count?: number,
  opts?: InstancedMeshOptions,
): InstancedMesh {
  let attributes = material.instanceAttributes
  if (attributes === undefined) {
    throw new Error(
      "createInstancedMesh: the material declares no instanceAttributes - build it with shaderMaterialClass({ instanceAttributes: [...] })",
    )
  }
  let stride = instanceStride(attributes)
  if (records.length % stride !== 0) {
    throw new Error(
      "createInstancedMesh: " + records.length + " floats is not a whole number of " + stride + "-float records",
    )
  }
  let bounds: Float32Array | null = null
  if (opts?.bounds !== undefined) {
    if (opts.bounds.length !== 6) {
      throw new Error("createInstancedMesh: bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
    }
    bounds = new Float32Array(6)
    for (let i = 0; i < 6; i++) bounds[i] = opts.bounds[i]!
  }
  let capacity = records.length / stride
  let mesh = createMesh(geometry, material) as InstancedMesh
  mesh._instances = {
    buffer: createBuffer(records, { autoFree: false, label: opts?.label }),
    stride,
    capacity,
    label: opts?.label,
    count: Math.max(0, Math.min(Math.floor(count ?? capacity), capacity)),
    bounds,
  }
  return mesh
}

/**
 * Overwrite an instanced mesh's records from the start of its buffer and
 * (by default) draw exactly the records written - pass `count` to draw
 * fewer, or to keep more previously written ones alive past a partial
 * rewrite. More records than the buffer holds grow it: capacity doubles
 * (or jumps to the records written when that is more), a new buffer is
 * created and written, the mesh's entry is re-pointed at it and the old
 * buffer freed - so a population grows without a new mesh, with the
 * copies amortized like any dynamic array (size the initial records to
 * skip them). Frame-rate-safe like setMeshParams when no growth happens.
 */
export function setInstances(mesh: InstancedMesh, records: Float32Array, count?: number): void {
  let inst = mesh._instances
  if (records.length % inst.stride !== 0) {
    throw new Error("setInstances: " + records.length + " floats is not a whole number of " + inst.stride + "-float records")
  }
  let written = records.length / inst.stride
  if (written > inst.capacity) {
    let previous = inst.buffer
    inst.capacity = Math.max(written, inst.capacity * 2)
    inst.buffer = createBuffer(inst.capacity * inst.stride * 4, { autoFree: false, label: inst.label })
    mesh._scene?._setBuffer(mesh)
    destroyBuffer(previous)
  }
  writeBuffer(inst.buffer, records)
  setInstanceCount(mesh, count ?? written)
}

/** Set how many records draw (clamped to [0, capacity]). The visibility
 * switch composes: a hidden mesh stores the count and draws it on unhide. */
export function setInstanceCount(mesh: InstancedMesh, count: number): void {
  let inst = mesh._instances
  let n = Math.max(0, Math.min(Math.floor(count), inst.capacity))
  if (n === inst.count) return
  inst.count = n
  mesh._scene?._setCount(mesh)
}

/**
 * Detach the mesh (if attached) and free its record buffer. The buffer is
 * mesh-owned with no reference count (unlike geometry buffers it is never
 * shared), so this is the one explicit free; the mesh cannot be re-added
 * afterwards.
 */
export function disposeInstances(mesh: InstancedMesh): void {
  let inst: MeshInstances | null = mesh._instances
  if (inst === null) return
  if (mesh._scene) remove(mesh)
  destroyBuffer(inst.buffer)
  ;(mesh as Mesh)._instances = null
}

/** Attach `child` under `parent` (re-parenting detaches it first). */
export function add(parent: SceneNode, child: SceneNode): void {
  if (child.parent !== null) remove(child)
  child.parent = parent
  parent.children.push(child)
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
  node._node = spatial.createNode(fillTransform(node), node.visible)
  // The parent is in the scene already (add() enters the child only then),
  // and the scene root is the one node without a parent.
  if (node.parent !== null && node.parent._node !== null) spatial.setParent(node._node, node.parent._node)
  if (node.kind === "mesh") scene._attach(node as Mesh)
  else if (node.kind === "light") scene._attachLight(node as Light)
  for (let c of node.children) enterScene(c, scene)
  scene._schedule()
}

function leaveScene(node: SceneNode): void {
  let scene = node._scene
  if (scene && node.kind === "mesh") scene._detach(node as Mesh)
  else if (scene && node.kind === "light") scene._detachLight(node as Light)
  node._scene = null
  for (let c of node.children) leaveScene(c)
  if (node._node !== null) {
    spatial.destroyNode(node._node)
    node._node = null
  }
}

/** The node's local transform in the FFI carrier. */
function fillTransform(node: SceneNode): Float32Array {
  let t = transformScratch
  t[0] = node.position[0]; t[1] = node.position[1]; t[2] = node.position[2]
  t[3] = node.quaternion[0]; t[4] = node.quaternion[1]; t[5] = node.quaternion[2]; t[6] = node.quaternion[3]
  t[7] = node.scale[0]; t[8] = node.scale[1]; t[9] = node.scale[2]
  return t
}

/** Forward a changed local transform to the core (no-op outside a scene:
 * entering pushes the whole transform). */
function pushTransform(node: SceneNode): void {
  if (node._node === null || node._scene === null) return
  spatial.setTransform(node._node, fillTransform(node))
  node._scene._moved(node)
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
  pushTransform(node)
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
  pushTransform(node)
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
 * `out` = node's world matrix as the tree stands now. In a scene that is
 * one core read (pending writes included, nothing cleared); outside one
 * the chain is composed here. Scene membership is subtree-closed, so the
 * recursion meets a core node at the first in-scene ancestor at the
 * latest. One shared local scratch serves any depth - each frame uses it
 * only after its recursive call has returned.
 */
function worldInto(out: Mat4, node: SceneNode): Mat4 {
  if (node._node !== null) {
    spatial.worldMatrix(node._node, worldRead)
    for (let i = 0; i < 16; i++) out[i] = worldRead[i]!
    return out
  }
  if (node.parent === null) identity(out)
  else worldInto(out, node.parent)
  return multiply(out, out, compose(localScratch, node.position, node.quaternion, node.scale))
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
  if (node._node !== null) {
    spatial.setVisible(node._node, visible)
    node._scene?._schedule()
  }
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
    samples: opts?.samples,
    label: opts?.label ?? "scene",
    autoFree: false,
  })
  let disposed = false
  let scheduled = false

  // Picking: the index and the narrowphase live in the spatial core; this
  // map turns a hit's core node back into the mesh. The pointer
  // bookkeeping behind scene.handlers follows.
  let byNode = new Map<NodeId, Mesh>()
  // Nodes whose transform changed since the last sync (deduped by the
  // _moved flag): what the light and transparent-order bookkeeping
  // reacts to, since which meshes moved is the core's knowledge now.
  let moved: SceneNode[] = []
  let capture = new Map<number, Mesh>()
  let hover = new Map<number, Mesh>()

  // Live meshes (those holding a draw entry) in add order; the background
  // entry never joins this list. Draw order is derived from it by
  // orderEntries (order.ts) whenever orderDirty. Camera moves and
  // transparent-mesh moves only dirty the order when two or more transparent
  // meshes exist - fewer cannot change relative order.
  let meshes: Mesh[] = []
  let transparentCount = 0
  // Attached lights in attach order (= light index); any change to the
  // set, a light's fields, or a light's world matrix rewrites the shared
  // light params at the end of the sync - one write, however many meshes.
  let lights: Light[] = []
  let lightsDirty = false
  // uLightDir is CORE-DRIVEN: each directional light's slot is a
  // shared-slot sink (bindDirectionSlot) following the node's world
  // rotation, with -direction as the local vector (the shader wants the
  // vector TOWARD the light) - so a light that merely moves costs no JS.
  // This rewrite runs on attach/detach/field changes only and owns the
  // rest: colors, count, hemisphere.
  let vecScratch = new Float32Array(3)
  let writeLights = () => {
    lightsDirty = false
    let sky: Vec3 = [0, 0, 0]
    let ground: Vec3 = [0, 0, 0]
    let colors: number[] = []
    let count = 0
    for (let light of lights) {
      if (light.type === "hemisphere") {
        let k = light.intensity
        sky = [light.sky[0] * k, light.sky[1] * k, light.sky[2] * k]
        ground = [light.ground[0] * k, light.ground[1] * k, light.ground[2] * k]
        continue
      }
      vecScratch[0] = -light.direction[0]
      vecScratch[1] = -light.direction[1]
      vecScratch[2] = -light.direction[2]
      spatial.bindDirectionSlot(light._node!, texture, "uLightDir", MAX_LIGHTS * 3, count, vecScratch)
      let c = light.color
      let k = light.intensity
      colors.push(c[0] * k, c[1] * k, c[2] * k)
      count++
    }
    for (let i = count; i < MAX_LIGHTS; i++) colors.push(0, 0, 0)
    setTargetParams(texture, { uHemiSky: sky, uHemiGround: ground, uLightCount: count, uLightColor: colors })
  }
  let orderDirty = false
  // The order last handed to the engine: a resort that lands on the same
  // permutation (the common case under a moving camera) issues nothing.
  let lastOrder: DrawId[] = []
  let background: { entry: DrawId; pipeline: RenderPipelineId; program: ProgramId } | null = null
  let sortEntries = () => {
    orderDirty = false
    refreshCenters()
    let order = orderEntries(meshes, view, background?.entry)
    if (order.length === lastOrder.length && order.every((id, i) => id === lastOrder[i])) return
    lastOrder = order
    setDrawOrder(texture, order)
  }

  // The transparent sort keys: each transparent mesh's local-bounds
  // center carried through its world matrix (read from the core), at
  // sort time only - opaque meshes never need one.
  let refreshCenters = () => {
    if (transparentCount < 2) return
    for (let mesh of meshes) {
      if (!mesh._transparent || mesh._node === null) continue
      let b = localBounds(mesh)
      let m = worldInto(worldScratch, mesh)
      let cx = 0, cy = 0, cz = 0
      if (b !== null) {
        cx = (b[0]! + b[3]!) / 2
        cy = (b[1]! + b[4]!) / 2
        cz = (b[2]! + b[5]!) / 2
      }
      mesh._center[0] = m[0] * cx + m[4] * cy + m[8] * cz + m[12]
      mesh._center[1] = m[1] * cx + m[5] * cy + m[9] * cz + m[13]
      mesh._center[2] = m[2] * cx + m[6] * cy + m[10] * cz + m[14]
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
    // Light bookkeeping first, so a fresh direction-slot bind is seeded
    // by the flush below in the same sync.
    if (lightsDirty) writeLights()
    // The core recomputes the moved subtrees and writes every entry's
    // uModel/uNormal, visibility switch and direction slots.
    spatial.flush()
    if (moved.length > 0) {
      // Which meshes moved is the core's knowledge now, so any move with
      // two or more transparent meshes re-sorts (sortEntries issues nothing
      // when the permutation is unchanged).
      if (transparentCount > 1) orderDirty = true
      for (let n of moved) n._moved = false
      moved.length = 0
    }
    if (orderDirty) sortEntries()
  }

  let hooks: SceneHooks = {
    _schedule() {
      if (scheduled || disposed) return
      scheduled = true
      RESOLVED.then(sync)
    },
    _attachLight(light) {
      if (disposed) return
      if (light.type === "directional" && lights.filter(l => l.type === "directional").length >= MAX_LIGHTS) {
        throw new Error("A scene takes at most " + MAX_LIGHTS + " directional lights")
      }
      lights.push(light)
      lightsDirty = true
    },
    _detachLight(light) {
      let i = lights.indexOf(light)
      if (i >= 0) lights.splice(i, 1)
      lightsDirty = true
      hooks._schedule()
    },
    _lightChanged() {
      lightsDirty = true
      hooks._schedule()
    },
    _attach(mesh) {
      if (disposed) return
      // A material reads attributes by name; the geometry's layout must
      // carry every one it declares (the pipeline is built for that layout,
      // so a missing channel would have no home) - an error here, like the
      // rest of the strict entry path. Extra channels are fine.
      validateGeometry(mesh.geometry)
      let missing = missingAttributes(mesh.material, mesh.geometry.layout)
      if (missing.length > 0) {
        throw new Error(
          "Mesh geometry layout (" + layoutKey(mesh.geometry.layout) + ") lacks attributes its material reads: " +
            missing.map(a => a.name + " " + a.format).join(", ") +
            " - add the channel with withAttribute()/withColors(), or use a material that does not read it",
        )
      }
      // Instancing pairs the same way layout does: the pipeline's instance
      // attributes describe the mesh's record buffer, so one without the
      // other (or a record stride from a different attribute list) would
      // bind garbage - errors here, at add().
      let inst = mesh._instances
      let instAttrs = mesh.material.instanceAttributes
      if (instAttrs !== undefined && inst === null) {
        throw new Error(
          "Material declares instanceAttributes - create its meshes with createInstancedMesh (records included), not createMesh",
        )
      }
      if (inst !== null) {
        if (instAttrs === undefined) {
          throw new Error("Instanced mesh with a non-instanced material - the material must declare instanceAttributes")
        }
        let stride = instanceStride(instAttrs)
        if (stride !== inst.stride) {
          throw new Error(
            "Instanced mesh records are " + inst.stride + " floats but the material's instanceAttributes take " + stride,
          )
        }
      }
      let bufs = acquireGeometryBuffers(mesh.geometry)
      mesh._buffers = bufs
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
      mesh._entry = addDraw(texture, mesh.material.pipeline(mesh.geometry.layout), seed, {
        buffer: bufs.buffer,
        indexBuffer: bufs.index,
        indexFormat: bufs.indexFormat,
        textures: mesh.material.textures,
        instanceBuffer: inst !== null ? inst.buffer : undefined,
        instanceCount: 0,
      })
      // The core turns the entry on (with the world matrix) at the next
      // flush, and off again whenever the node or an ancestor hides.
      spatial.bindDraw(mesh._node!, texture, mesh._entry, mesh.material.normalMatrix === true, inst !== null ? inst.count : 1)
      // Picking: the local box puts the node in the core index; an
      // ordinary mesh also gets its geometry's triangle shape, an
      // instanced one is box-only (records are opaque, and without
      // explicit bounds it is not picked at all).
      spatial.setBounds(mesh._node!, localBounds(mesh))
      spatial.setShape(mesh._node!, inst === null ? bufs.shape : null)
      byNode.set(mesh._node!, mesh)
      meshes.push(mesh)
      mesh._transparent = mesh.material.transparent === true
      if (mesh._transparent) transparentCount++
      orderDirty = true
      this._schedule()
    },
    _detach(mesh) {
      if (mesh._entry !== null) {
        if (mesh._node !== null) {
          spatial.setShape(mesh._node, null)
          spatial.setBounds(mesh._node, null)
          spatial.unbindDraw(mesh._node)
          byNode.delete(mesh._node)
        }
        if (!disposed) removeDraw(texture, mesh._entry)
        if (mesh._buffers !== null) releaseGeometryBuffers(mesh._buffers)
        mesh._buffers = null
        let i = meshes.indexOf(mesh)
        if (i >= 0) meshes.splice(i, 1)
        if (mesh._transparent) transparentCount--
        orderDirty = true
      }
      mesh._entry = null
    },
    _setParams(mesh, params) {
      if (mesh._entry !== null && !disposed) setDrawParams(texture, mesh._entry, params)
    },
    _setCount(mesh) {
      // The core composes the count with the visibility switch: a hidden
      // entry stays at 0 and the unhide restores the new count.
      if (mesh._entry !== null && mesh._node !== null && !disposed && mesh._instances !== null) {
        spatial.setDrawCount(mesh._node, mesh._instances.count)
      }
    },
    _setBuffer(mesh) {
      // The entry keeps its range (at most the old capacity, so the larger
      // buffer always passes the swap's bounds check); the caller destroys
      // the old buffer after this, which the entry held alive until now.
      if (mesh._entry !== null && !disposed && mesh._instances !== null) {
        setDrawBuffers(texture, mesh._entry, { instanceBuffer: mesh._instances.buffer })
      }
    },
    _reorder() {
      orderDirty = true
      this._schedule()
    },
    _moved(node) {
      if (!node._moved) {
        node._moved = true
        moved.push(node)
      }
      this._schedule()
    },
  }

  let root = makeNode("group")
  root._scene = hooks
  root._node = spatial.createNode(fillTransform(root), true)

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
      if (disposed) return []
      let hits: Hit[] = []
      rayOriginScratch[0] = origin[0]
      rayOriginScratch[1] = origin[1]
      rayOriginScratch[2] = origin[2]
      rayDirScratch[0] = direction[0]
      rayDirScratch[1] = direction[1]
      rayDirScratch[2] = direction[2]
      for (let h of spatial.raycast(rayOriginScratch, rayDirScratch)) {
        let mesh = byNode.get(h.node)
        if (mesh === undefined) continue
        let hit: Hit = { mesh, distance: h.distance, point: h.point }
        if (h.normal !== undefined) hit.normal = h.normal
        if (h.face !== undefined) hit.face = h.face
        if (h.uv !== undefined) hit.uv = h.uv
        hits.push(hit)
      }
      return hits
    },
    handlers,
    handlersFor(layout) {
      return makeHandlers(layout)
    },
    dispose() {
      if (disposed) return
      disposed = true
      // Full tree-side teardown, not just the target: every node leaves
      // the scene (entries' geometry-buffer references and pick leaves
      // dropped, core nodes freed), so a disposed scene leaves no
      // bookkeeping behind and the JS tree survives as plain data.
      for (let c of root.children.slice()) leaveScene(c)
      root._scene = null
      if (root._node !== null) {
        spatial.destroyNode(root._node)
        root._node = null
      }
      // Drain the zeroed direction slots the teardown queued while the
      // target still exists; afterwards their groups are gone.
      spatial.flush()
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
