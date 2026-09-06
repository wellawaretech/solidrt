// The retained node layer: the scene-graph objects (groups, and the base
// the meshes and lights extend), the add/remove tree walk binding them to
// the spatial core, and the transform write paths. Plain objects, no
// signals - scene.ts's header tells the full model. A node reaches its
// scene only through the SceneHooks seam, so this module has no runtime
// import of scene.ts (or of mesh.ts/light.ts - their imports here are
// types only).

import * as spatial from "flux:spatial"
import type { NodeId, NodeTransition } from "flux:spatial"
import { on } from "srt:events"
import type { ShaderParams, TextureId } from "@solidrt/core/gpu"
// The scene's lookAt() aims a node; math's builds a camera's view matrix -
// the same pairing (and the same name) as Three's Object3D/Matrix4.
import { compose, eulerFromQuat, identity, mat4, multiply, quat, quatFromFrame, transformPoint, updateRotation, updateScale } from "./math.ts"
import type { Mat4, Quat, TransformUpdate, Vec3, Vec4 } from "./math.ts"
import type { CastingLight, Light } from "./light.ts"
import type { Mesh } from "./mesh.ts"

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
let pointScratch: Vec4 = [0, 0, 0, 0]
let aimScratch: Vec3 = [0, 0, 0]
let upScratch: Vec3 = [0, 0, 0]
// setTransform's rotation compare happens AFTER conversion, so an euler and
// the quaternion it produces are the same write. Nothing outlives the call.
let rotScratch = quat()
let scaleScratch: Vec3 = [1, 1, 1]

// Settle routing: the core's "spatialTransitionEnd" event carries the node
// id, so nodes with a transition DECLARED (only those can settle) are
// indexed by their core id while in a scene, and one lazy subscription,
// started at the first declaration, routes to the node's onTransitionEnd.
// Target-only, like the element transitions.
let declared = new Map<NodeId, SceneNode>()
let subscribed = false

function declareTransition(id: NodeId, node: SceneNode): void {
  declared.set(id, node)
  if (subscribed) return
  subscribed = true
  on("spatialTransitionEnd", (event: { node: NodeId; component: TransitionEndEvent["component"] }) => {
    let node = declared.get(event.node)
    if (!node) return
    try {
      node.onTransitionEnd?.({ component: event.component })
    } catch (err) {
      console.error("Error in onTransitionEnd handler:", err)
    }
  })
}

// The scene half a node needs to reach: attach/detach entries and schedule
// a sync. Kept separate from the public Scene type so internals stay off
// the app-facing surface. The camera (uViewProj + uCamPos) is written
// through the shared channel only when it changes - attach never re-seeds
// it, because target state survives entry churn.
export type SceneHooks = {
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
  /** The mesh's castShadow flag changed: re-evaluate the filtered views. */
  _setCast(mesh: Mesh): void
  /** The mesh's layers bitmask changed: re-evaluate every target. */
  _setLayers(mesh: Mesh): void
  /** A light's castShadow/shadow options changed. */
  _shadowChanged(light: CastingLight): void
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
  /** A declared transition (setTransition) settled naturally on one
   * component; a cancel, snap or scene leave never fires. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
  /** The core node while in a scene (created at add, freed at remove). */
  _node: NodeId | null
  _moved: boolean
  _scene: SceneHooks | null
  /** The declared transition, re-applied on every scene enter. */
  _transition: NodeTransition | string | null
  /** Skin palette rows this node feeds (a model joint carries one per
   * skin): bound to the core at every scene enter, so the flush writes
   * `inverse(anchorWorld) * world * post` - the model-local bone matrix -
   * to the palette texture's row whenever the node moves. `anchor` is the
   * model root; null for non-joints (the common case pays one null check). */
  _palettes: { texture: TextureId; row: number; post: Float32Array; anchor: SceneNode }[] | null
  /** A culling-only local box (a model joint's influence region, in joint
   * space): bound at every scene enter so the joint's world box follows
   * the pose without joining the picking index. null for the common case. */
  _cullBounds: Float32Array | null
}

/** The settled component of a node transition. */
export type TransitionEndEvent = {
  component: "position" | "rotation" | "scale"
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

/** A bare node record (internal: the mesh and light constructors build on it). */
export function makeNode(kind: SceneNode["kind"]): SceneNode {
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
    _transition: null,
    _palettes: null,
    _cullBounds: null,
  }
}

export function createGroup(): SceneNode {
  return makeNode("group")
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
  if (node._transition !== null) {
    spatial.setTransition(node._node, node._transition)
    declareTransition(node._node, node)
  }
  // The parent is in the scene already (add() enters the child only then),
  // and the scene root is the one node without a parent.
  if (node.parent !== null && node.parent._node !== null) spatial.setParent(node._node, node.parent._node)
  if (node._palettes !== null) {
    // A joint's palette rows: the anchor (its model root) entered first -
    // enterScene recurses parents-first - so its core node is live. The
    // core drops the binding with the node at leaveScene.
    for (let p of node._palettes) {
      if (p.anchor._node !== null) spatial.bindTextureSlot(node._node, p.texture, p.row, p.post, p.anchor._node)
    }
  }
  if (node._cullBounds !== null) spatial.setCullBounds(node._node, node._cullBounds)
  if (node.kind === "mesh") scene._attach(node as Mesh)
  else if (node.kind === "light") scene._attachLight(node as Light)
  for (let c of node.children) enterScene(c, scene)
  scene._schedule()
}

export function leaveScene(node: SceneNode): void {
  let scene = node._scene
  if (scene && node.kind === "mesh") scene._detach(node as Mesh)
  else if (scene && node.kind === "light") scene._detachLight(node as Light)
  node._scene = null
  for (let c of node.children) leaveScene(c)
  if (node._node !== null) {
    declared.delete(node._node)
    spatial.destroyNode(node._node)
    node._node = null
  }
}

/** The node's local transform in the FFI carrier. */
export function fillTransform(node: SceneNode): Float32Array {
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
  spatial.writeTransform(node._node, fillTransform(node))
  node._scene._moved(node)
}

export type { TransformUpdate } from "./math.ts"

/**
 * Declare (or with null clear) how the node's transform writes animate:
 * once set, setTransform writes are TARGETS the core animates toward
 * (position/scale per lane, rotation along the quaternion geodesic - a
 * spring keeps its velocity through retargets, the pursuit-safe shape),
 * so JS writes once per target change instead of once per frame. A spec
 * per component (position, rotation, scale) plus `all`; each
 * `{ duration, bounce? }` (a spring, the default) / `{ duration, curve }`
 * (a tween) / a shorthand string like "300ms ease-out". The declaration
 * lives on the node and re-applies whenever it enters a scene; the pose
 * it enters with always snaps. Clearing cancels running tracks in place
 * (the node keeps its mid-flight transform) and later writes snap. Each
 * natural settle calls the node's `onTransitionEnd` with the component
 * (the raw "spatialTransitionEnd" engine event on srt:events stays for
 * flux:spatial consumers; it carries the core id, `_node`).
 */
export function setTransition(node: SceneNode, transition: NodeTransition | string | null): void {
  node._transition = transition
  if (node._node !== null) {
    spatial.setTransition(node._node, transition)
    if (transition === null) declared.delete(node._node)
    else declareTransition(node._node, node)
  }
}

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

// readTransform's FFI carrier (position 3, quaternion 4, scale 3).
let transformRead = new Float32Array(10)

/**
 * The node's CURRENT local transform as fresh arrays. In a scene this
 * reads the core - which is the truth for a node a clip player animates:
 * the players write core TRS directly, so the JS `position`/`quaternion`/
 * `scale` fields of animated joints go stale (they hold the last JS
 * write). Use this for pose reads on animated rigs (root-motion strips,
 * copying a skeleton); out of a scene it copies the JS fields.
 */
export function getTransform(node: SceneNode): { position: Vec3; quaternion: Quat; scale: Vec3 } {
  if (node._node !== null) {
    spatial.readTransform(node._node, transformRead)
    let t = transformRead
    return {
      position: [t[0]!, t[1]!, t[2]!],
      quaternion: [t[3]!, t[4]!, t[5]!, t[6]!],
      scale: [t[7]!, t[8]!, t[9]!],
    }
  }
  return {
    position: [node.position[0], node.position[1], node.position[2]],
    quaternion: [node.quaternion[0], node.quaternion[1], node.quaternion[2], node.quaternion[3]],
    scale: [node.scale[0], node.scale[1], node.scale[2]],
  }
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
export function worldInto(out: Mat4, node: SceneNode): Mat4 {
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