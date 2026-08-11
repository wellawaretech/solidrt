// The retained scene: plain objects and dirty flags, no signals - the hot
// path (a moved node) is flat imperative code, and reactivity stays at the
// component boundary (components.tsx). A scene compiles to one draw
// target: every mesh is one draw entry whose uModel (and, for materials
// declaring it, uNormal) this module keeps in step with the tree, and the
// camera is the target's SHARED uViewProj + uCamPos - one setTargetParams
// per camera move, not one write per mesh. uCamPos rides unconditionally:
// shared params tolerate zero coverage (stored and skipped until a
// declaring material arrives), so no bookkeeping tracks who reads it.
// Mutations batch to a microtask, so a burst of writes (a whole subtree
// moved, many effects in one flush) syncs once.
//
// Rendering itself belongs to the runtime: the target is an ordinary
// `render: "auto"` draw target that re-renders when its entries change, so
// a static scene costs zero passes and this module registers no frame
// loop. Continuous animation is the app's onFrame writing transforms -
// each write lands here, the microtask syncs the affected uModels, and the
// flush renders once that frame.

import { addDraw, createDrawTarget, destroyTexture, removeDraw, setDrawParams, setDrawRange, setTargetParams, setTargetSize } from "@solidrt/core/gpu"
import type { DrawId, FilterMode, ShaderParams, TextureId, WrapMode } from "@solidrt/core/gpu"
import { getOwner, onCleanup } from "@solidrt/core"
// The scene's lookAt() aims a node; math's builds a camera's view matrix -
// the same pairing (and the same name) as Three's Object3D/Matrix4.
import { compose, copy, eulerFromQuat, identity, lookAt as lookAtMatrix, mat4, multiply, normalMatrix, perspective, quatFromEuler, quatFromFrame, quatNormalize, transformPoint } from "./math.ts"
import type { Mat4, Quat, Vec3, Vec4 } from "./math.ts"
import { geometryBuffers } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { Material } from "./material.ts"

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
  _localDirty: boolean
  _local: Mat4
  _world: Mat4
  _scene: SceneHooks | null
}

export type Mesh = SceneNode & {
  kind: "mesh"
  geometry: Geometry
  material: Material
  _entry: DrawId | null
  _hidden: boolean
  _fresh: boolean
  _params: ShaderParams | null
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
  mesh._entry = null
  mesh._hidden = false
  mesh._fresh = false
  mesh._params = null
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

export type TransformUpdate = {
  position?: Vec3
  /** Euler radians in XYZ order (x first), Three's `Euler` default -
   * converted to the node's quaternion on write. */
  rotation?: Vec3
  /** The rotation itself. Normalized on write, so a hand-built or
   * drifted quaternion cannot silently scale the geometry. Passing this
   * together with `rotation` is an error, not a precedence question. */
  quaternion?: Quat
  /** A number is uniform scale. */
  scale?: Vec3 | number
}

/**
 * The one write path for node transforms (so the scene knows to sync).
 * Values are copied in; absent keys keep their current value. This is also
 * the frame-rate escape hatch: call it from onFrame on a node grabbed via
 * `ref`, bypassing signals entirely.
 */
export function setTransform(node: SceneNode, update: TransformUpdate): void {
  let p = update.position
  if (p) {
    node.position[0] = p[0]
    node.position[1] = p[1]
    node.position[2] = p[2]
  }
  let r = update.rotation
  let q = update.quaternion
  if (r !== undefined && q !== undefined) {
    throw new Error("Pass rotation or quaternion to setTransform, not both")
  }
  if (r !== undefined) quatFromEuler(node.quaternion, r)
  else if (q !== undefined) quatNormalize(node.quaternion, q)
  let s = update.scale
  if (s !== undefined) {
    if (typeof s === "number") {
      node.scale[0] = s
      node.scale[1] = s
      node.scale[2] = s
    } else {
      node.scale[0] = s[0]
      node.scale[1] = s[1]
      node.scale[2] = s[2]
    }
  }
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

/** Swap a mesh's geometry: its draw entry is rebuilt (appended last -
 * order is irrelevant while every entry is opaque and depth-tested). */
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
      setTargetParams(texture, { uViewProj: viewProj, uCamPos: eye })
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
        }
      }
      for (let c of node.children) walk(c, changed, shown)
    }
    walk(root, false, true)
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
      mesh._entry = addDraw(texture, mesh.material.pipeline(), seed, {
        buffer: bufs.buffer,
        indexBuffer: bufs.index,
        indexFormat: bufs.indexFormat,
        textures: mesh.material.textures,
      })
      mesh._hidden = false
      mesh._fresh = true
      this._schedule()
    },
    _detach(mesh) {
      if (mesh._entry !== null && !disposed) removeDraw(texture, mesh._entry)
      mesh._entry = null
    },
    _setParams(mesh, params) {
      if (mesh._entry !== null && !disposed) setDrawParams(texture, mesh._entry, params)
    },
  }

  let root = makeNode("group")
  root._scene = hooks

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
    dispose() {
      if (disposed) return
      disposed = true
      destroyTexture(texture)
    },
  }
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => scene.dispose())
  return scene
}
