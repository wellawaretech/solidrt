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
import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
// The scene's lookAt() aims a node; math's builds a camera's view matrix -
// the same pairing (and the same name) as Three's Object3D/Matrix4.
import { compose, eulerFromQuat, identity, mat4, multiply, quat, quatFromFrame, transformPoint, updateRotation, updateScale } from "./math.ts"
import type { Mat4, Quat, TransformUpdate, Vec3, Vec4 } from "./math.ts"
import type { CastingLight, Light } from "./light.ts"
import type { InstancedMesh, InstanceNode, Mesh } from "./mesh.ts"
import type { Scene, ViewHandle } from "./scene.ts"

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

// Free routing: destroy() leaves a node whose declaration carries an
// `exit` in the core as a LEAVING one (drawn, picked by nothing) until its
// exits settle, and the core's "spatialNodeFreed" event says when it is
// gone - the cue to detach its entries and recycle what was kept for it
// (an instance's record slot), then to run what was deferred to that
// moment (`then`: a disposer that reached the node mid-exit). Keyed by
// core id, with the scene the node was in (the handle's own `_scene` is
// already null: a destroyed handle routes no writes). One lazy
// subscription.
type Leaving = { node: SceneNode; scene: SceneHooks; then: (() => void)[] }
let leaving = new Map<NodeId, Leaving>()
let freeingSubscribed = false

function awaitFree(id: NodeId, node: SceneNode, scene: SceneHooks): void {
  leaving.set(id, { node, scene, then: [] })
  if (freeingSubscribed) return
  freeingSubscribed = true
  on("spatialNodeFreed", (event: { node: NodeId }) => {
    let entry = leaving.get(event.node)
    if (!entry) return
    leaving.delete(event.node)
    finishLeaving(entry)
  })
}

function finishLeaving(entry: Leaving): void {
  finishLeave(entry.node, entry.scene)
  for (let fn of entry.then) fn()
}

/**
 * Run `fn` once `node` has finished leaving, if it is on its way out
 * (destroyed, its exit still playing): returns true and holds `fn` for
 * the free; false when the node is not leaving, and the caller does its
 * work now. What a disposer uses when destroy may just have let the node
 * go: `disposeInstances` and `model.dispose` free buffers and textures
 * the corpse still draws with, so on a leaving node they wait for it
 * instead of cutting the exit short. A corpse freed early (`freeLeaving`,
 * a parent's `destroy`) runs the held work at that point.
 */
export function afterFree(node: SceneNode, fn: () => void): boolean {
  if (node._node === null) return false
  let entry = leaving.get(node._node)
  if (!entry) return false
  entry.then.push(fn)
  return true
}

/**
 * Free every leaving node `where` admits NOW (its exit cut short where it
 * stands) and finish its leave, held work included: the teardown paths
 * call this before they free what a corpse still draws with - a scene's
 * targets, a layer's buffers.
 */
export function freeLeaving(where: (node: SceneNode, scene: SceneHooks) => boolean): void {
  for (let [id, entry] of [...leaving]) {
    if (!where(entry.node, entry.scene)) continue
    leaving.delete(id)
    spatial.destroyNode(id)
    finishLeaving(entry)
  }
}

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
  /** An instance entered the scene (its mesh entered first): box, shape
   * and record binding. */
  _attachInstance(instance: InstanceNode): void
  _detachInstance(instance: InstanceNode): void
  /** A mesh is on its way out (destroy with an `exit`): drop it from pick
   * routing now - the core skips a leaving node anyway - while its entries
   * keep drawing until `_detach` at the free. */
  _exiting(mesh: Mesh): void
  _attachLight(light: Light): void
  _detachLight(light: Light): void
  _lightChanged(): void
  _setParams(mesh: Mesh, params: ShaderParams): void
  _setCount(mesh: Mesh): void
  /** Re-point the mesh's entries at its (replaced) instance buffers. */
  _setBuffer(mesh: Mesh): void
  /** The mesh's draw range changed (setDrawRange): apply it to its entries. */
  _setRange(mesh: Mesh): void
  /** One of the mesh's record streams has a dirty range: publish it at
   * the next sync. */
  _setRecords(mesh: Mesh): void
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
  kind: "group" | "mesh" | "light" | "instance"
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
   * sync their props here). Down/move/up/wheel/tap dispatch on the
   * nearest hit - the struck instance of an instanced mesh, else the mesh
   * - bubble through its ancestors and end at the scene's listeners
   * (stopPropagation stops the walk, and a stopped down claims the whole
   * press); enter/leave fire on the struck node alone. Events flow once
   * the element showing the scene carries `scene.handlers`. */
  onPointerDown?: (event: NodePointerEvent) => void
  onPointerMove?: (event: NodePointerEvent) => void
  onPointerUp?: (event: NodePointerEvent) => void
  onPointerEnter?: (event: NodePointerEvent) => void
  onPointerLeave?: (event: NodePointerEvent) => void
  /** The wheel over the node (NodeWheelEvent: `deltaX`/`deltaY`), bubbling
   * like down/move/up. */
  onWheel?: (event: NodeWheelEvent) => void
  /** A press released on this node within the slop, alone for its whole
   * press (NodeTapEvent: `tapCount`); bubbles like down/move/up. */
  onTap?: (event: NodeTapEvent) => void
  /** A declared transition (setTransition) settled naturally on one
   * component; a cancel, snap, scene leave or exit never fires. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
  /** The core node while in a scene (created at add, freed at remove or
   * at destroy's free - a destroyed node with an `exit` keeps it while
   * it animates out). */
  _node: NodeId | null
  /** Gone for good (destroy): every write is a no-op, add/remove throw or
   * skip, and the core node - if one is still animating out - is nobody's
   * to write. */
  _destroyed: boolean
  _moved: boolean
  /** The core writes this node's TRS itself (a clip player's target, a
   * root-motion anchor), so the JS mirror above is stale: setTransform
   * never short-circuits on it. Set once, never cleared. */
  _native: boolean
  _scene: SceneHooks | null
  /** The declared transition, re-applied on every scene enter. */
  _transition: NodeTransition | string | null
  /** Skin palette rows this node feeds (a model joint carries one per
   * skin): bound to the core at every scene enter, so the flush writes
   * `inverse(anchorWorld) * world * post` - the model-local bone matrix -
   * to the palette texture's row whenever the node moves. `anchor` is the
   * model root - the body's, for a piece bound onto its skeleton - and
   * null for non-joints (the common case pays one null check). */
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

/** What every scene pointer event carries, whichever handler sees it:
 * the element pointer vocabulary carried over, plus the 3D fields. */
export type SceneEventBase = {
  /** The instance struck when `mesh` is an instanced mesh (the walk
   * starts there), null otherwise - constant while the event bubbles. */
  instance: InstanceNode | null
  /** World-space hit point on `mesh`, or null when there is no mesh or
   * the ray misses it (a captured drag off the mesh, a leave). */
  point: Vec3 | null
  /** Camera-ray distance to `point` in world units, or null with it. */
  distance: number | null
  /** Pointer position in scene pixels - project()'s coordinate space
   * (a view's own pixels under a view leaf). */
  x: number
  y: number
  pointerId: number
  pointerType: string
  button?: number
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
  /**
   * The element event the scene's leaf received, in the leaf's own frame
   * (localX/localY, clientX/clientY, movementX/Y): what core's
   * recognizers consume, so a mesh drags itself through `createPan` and
   * a camera control's feed rides the same events the walk carries.
   */
  native: ElementPointerEvent
  /**
   * Stops the walk after the current handler: no ancestor and none of the
   * scene's listeners see the event. Stopping a DOWN claims the whole
   * press - that pointer's move, up and tap never reach the scene either,
   * so a mesh that drags itself stops its down once and an orbit control
   * fed at the root never turns under it.
   */
  stopPropagation(): void
}

/**
 * The event a mesh, instance or ancestor group handler receives: `mesh`
 * is the hit (or the captured mesh during a drag), constant while the
 * event bubbles; `currentTarget` the node whose handler is running.
 */
export type NodePointerEvent = SceneEventBase & {
  mesh: Mesh
  currentTarget: SceneNode
}

/**
 * The event as the SCENE's listeners see it (scene.listen, the `<Scene>`
 * pointer props; a view's under its own leaf), the last stop of the walk:
 * `mesh` is the hit it bubbled from, or null over empty space, where the
 * scene is the only target.
 */
export type ScenePointerEvent = SceneEventBase & {
  mesh: Mesh | null
  currentTarget: Scene | ViewHandle
}

type WheelFields = {
  /** The wheel delta as the element event reports it. */
  deltaX: number
  deltaY: number
  native: ElementWheelEvent
}
type TapFields = {
  /** 1 for a tap, 2 for the second of a double tap (the same target,
   * within the repeat interval and distance), and so on - DOM's `detail`,
   * Unity's `clickCount`. */
  tapCount: number
}

export type NodeWheelEvent = NodePointerEvent & WheelFields
export type SceneWheelEvent = ScenePointerEvent & WheelFields
/**
 * A press that released on the target it pressed without travelling past
 * the slop, the only pointer down for its whole press (a pinch never
 * taps). Dispatched after the up, bubbling the same way; `x`/`y` and
 * `point` are the release point.
 */
export type NodeTapEvent = NodePointerEvent & TapFields
export type SceneTapEvent = ScenePointerEvent & TapFields

/** A listener at the scene's (or a view's) root; see Scene.listen. */
export type ScenePointerListener = {
  onPointerDown?: (event: ScenePointerEvent) => void
  onPointerMove?: (event: ScenePointerEvent) => void
  onPointerUp?: (event: ScenePointerEvent) => void
  onWheel?: (event: SceneWheelEvent) => void
  onTap?: (event: SceneTapEvent) => void
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
    _destroyed: false,
    _native: false,
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

/** Attach `child` under `parent` (re-parenting detaches it first). An
 * instance is slot-bound to its mesh: addInstance places it. A destroyed
 * node, on either side, is gone for good and throws. */
export function add(parent: SceneNode, child: SceneNode): void {
  if (child.kind === "instance") throw new Error("add: an instance is slot-bound to its mesh - addInstance places it, destroy destroys it")
  if (child._destroyed) throw new Error("add: the child was destroyed")
  if (parent._destroyed) throw new Error("add: the parent was destroyed")
  if (child.parent !== null) remove(child)
  child.parent = parent
  parent.children.push(child)
  if (parent._scene) enterScene(child, parent._scene)
}

/**
 * Detach `child` from its parent (and its meshes from the scene) - Three's
 * `parent.remove(child)`, Godot's remove_child: the subtree stays intact
 * and re-adds cleanly, so this is the verb for a node that is coming back
 * (or about to be disposed). It SNAPS: a detached node plays no exit,
 * that is `destroy`'s. An instance is slot-bound to its mesh and cannot
 * be detached (destroy it); a destroyed node is already gone (no-op).
 */
export function remove(child: SceneNode): void {
  if (child.kind === "instance") throw new Error("remove: an instance is slot-bound to its mesh - destroy destroys it")
  if (child._destroyed) return
  if (child._scene) leaveScene(child)
  unlink(child)
}

/**
 * Destroy a node and everything under it - Unity's Destroy, Godot's
 * queue_free, @solidrt/2d's destroySprite/destroyGroup one dimension up:
 * the subtree is gone for good and every handle in it goes inert (writes
 * are no-ops, add throws, remove skips). This is the removal an `exit`
 * rides on: a node whose transition declares one animates each such
 * component to its exit value first and stays drawn meanwhile, a ghost
 * that no pick, raycast, overlap or sweep sees, then frees when the last
 * settles (no onTransitionEnd fires for an exit: the app already let go).
 * Children go first, each animating its own exit, and their parent - an
 * exit of its own or not - stays in the core as their frame until the
 * last of them has settled, so a dying character's parts leave in its
 * frame to the end; a mixer targeting a joint of the subtree keeps
 * driving it while it goes. An instance is destroyed like any node here
 * (its record slot recycles at the free). GPU resources stay on their own
 * disposers (disposeGeometry, disposeInstances, model.dispose), as a
 * sprite's atlas does; a disposer reaching a node still animating out
 * frees it on the spot. Outside a scene there is nothing to animate: the
 * subtree just goes inert.
 */
export function destroy(node: SceneNode): void {
  if (node._destroyed) return
  for (let c of node.children.slice()) destroy(c)
  letGo(node)
  unlink(node)
}

function unlink(node: SceneNode): void {
  let parent = node.parent
  if (parent !== null) {
    let i = parent.children.indexOf(node)
    if (i >= 0) parent.children.splice(i, 1)
    node.parent = null
  }
}

/**
 * The per-node half of destroy: mark the handle inert and let the core
 * node go through its declaration's exits (flux:spatial exitNode). A node
 * kept leaving finishes at the free (awaitFree); one freed on the spot,
 * or outside a scene, finishes now. The handle's scene reference goes
 * here so every scene-routed setter is a no-op from this point; the entry
 * on the leaving map carries the scene to the finish.
 */
function letGo(node: SceneNode): void {
  node._destroyed = true
  let scene = node._scene
  let id = node._node
  node._scene = null
  if (scene === null || id === null) {
    finishLeave(node, null)
    return
  }
  declared.delete(id)
  if (node.kind === "mesh") scene._exiting(node as Mesh)
  else if (node.kind === "instance") scene._detachInstance(node as InstanceNode)
  if (spatial.exitNode(id)) awaitFree(id, node, scene)
  else finishLeave(node, scene)
}

/**
 * The end of a destroyed node's leave, once its core node is gone (or
 * never was): its entries come off the scene, an instance's slot recycles
 * and the handle drops its core id. `scene` is null for a node that was
 * not in one.
 */
function finishLeave(node: SceneNode, scene: SceneHooks | null): void {
  node._node = null
  if (node.kind === "instance") {
    let instance = node as InstanceNode
    let mesh = instance.mesh
    if (mesh !== null) {
      let nodes = mesh._instances?.nodes
      if (nodes) {
        nodes.slots[instance._slot] = null
        nodes.free.push(instance._slot)
      }
      instance.mesh = null
    }
    return
  }
  if (scene === null) return
  if (node.kind === "mesh") scene._detach(node as Mesh)
  else if (node.kind === "light") scene._detachLight(node as Light)
}

export function enterScene(node: SceneNode, scene: SceneHooks): void {
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
  if (node.kind === "mesh") {
    spatial.setLayers(node._node, (node as Mesh).layers)
    scene._attach(node as Mesh)
  }
  else if (node.kind === "light") scene._attachLight(node as Light)
  else if (node.kind === "instance") scene._attachInstance(node as InstanceNode)
  for (let c of node.children) enterScene(c, scene)
  scene._schedule()
}

export function leaveScene(node: SceneNode): void {
  let scene = node._scene
  if (scene && node.kind === "mesh") scene._detach(node as Mesh)
  else if (scene && node.kind === "light") scene._detachLight(node as Light)
  else if (scene && node.kind === "instance") scene._detachInstance(node as InstanceNode)
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
 * it enters with snaps, unless a component's `from` (its lanes: `[x, y,
 * z]`, a quaternion for rotation) animates it in from there at every
 * scene enter; a component's `exit` is where it animates to when
 * `destroy` lets go of the node (see there). Either endpoint takes the
 * object form `{ value, duration?, curve?, bounce?, delay? }` to own its
 * direction's motion, and `delay` on an entry holds its writes; `stagger`
 * (ms) on a node spaces the enters and exits of its descendants that
 * begin in one frame (a Group's declaration, the element rule: the
 * orchestrator is always an ancestor). Clearing cancels running tracks in
 * place (the node keeps its mid-flight transform) and later writes snap. Each natural settle calls the node's
 * `onTransitionEnd` with the component (the raw "spatialTransitionEnd"
 * engine event on srt:events stays for flux:spatial consumers; it carries
 * the core id, `_node`).
 */
export function setTransition(node: SceneNode, transition: NodeTransition | string | null): void {
  if (node._destroyed) return
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
  if (node._destroyed) return
  // A no-op write costs nothing: driving every node from onFrame is the
  // intended shape, and most nodes did not move. Exact compares, like
  // setVisible - a value that survives a float round trip unchanged is the
  // same value, and an epsilon would need a scale-dependent one anyway.
  // Except on a node the core poses itself: its mirror is stale, so an
  // equal write may well be a real move (a character teleported back to
  // where it started) and always goes through.
  let changed = node._native
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
  if (node._destroyed) return
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
  if (node._destroyed || node.visible === visible) return
  node.visible = visible
  if (node._node !== null) {
    spatial.setVisible(node._node, visible)
    node._scene?._schedule()
  }
}