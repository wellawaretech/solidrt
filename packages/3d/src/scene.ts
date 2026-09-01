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

import { addDraw, createBuffer, createDrawTarget, createTexture, depthTexture, destroyBuffer, destroyProgram, destroyRenderPipeline, destroyTexture, limits, removeDraw, setDrawBuffers, setDrawOrder, setDrawParams, setTargetParams, setTargetRect, setTargetSize, setTargetTextures, writeBuffer } from "@solidrt/core/gpu"
import * as spatial from "flux:spatial"
import type { NodeId, NodeTransition } from "flux:spatial"
import { on } from "srt:events"
import type { BufferId, DrawId, FilterMode, ProgramId, RenderPipelineId, ShaderParams, TextureBindings, TextureId, VertexAttribute, WrapMode } from "@solidrt/core/gpu"
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
// The scene's lookAt() aims a node; math's builds a camera's view matrix -
// the same pairing (and the same name) as Three's Object3D/Matrix4.
import { cascadeSplit, compose, copy, eulerFromQuat, frustumSliceSphere, identity, lookAt as lookAtMatrix, mat4, multiply, orthographic, perspective, quat, quatFromFrame, snapToGrid, transformPoint, transformVector, updateRotation, updateScale } from "./math.ts"
import type { Mat4, Quat, TransformUpdate, Vec3, Vec4 } from "./math.ts"
import { MAX_CASCADES, MAX_LIGHTS, MAX_SHADOW_MAPS } from "./glsl.ts"
import { geometryBounds, layoutKey, plane, validateGeometry } from "./geometry.ts"
import { acquireGeometryBuffers, releaseGeometryBuffers } from "./geometry-gpu.ts"
import type { GeometryBuffers } from "./geometry-gpu.ts"
import type { Geometry } from "./geometry.ts"
import { backgroundPipeline, missingAttributes, shadowDepthMaterial } from "./material.ts"
import { orderEntries } from "./order.ts"
import type { Material } from "./material.ts"

const IDENTITY = mat4()
const RESOLVED = Promise.resolve()
// How a cascaded light slices the camera range: 0 uniform, 1
// logarithmic, halfway the "practical" split (near slices small, far
// ones not starved).
const CASCADE_SPLIT_LAMBDA = 0.5
// |y| of a light direction above this is straight up or down, where
// world up cannot serve as the shadow map's roll reference.
const VERTICAL_LIGHT = 0.99
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
  /** The mesh's castShadow flag changed: re-evaluate the filtered views. */
  _setCast(mesh: Mesh): void
  /** The mesh's layers bitmask changed: re-evaluate every target. */
  _setLayers(mesh: Mesh): void
  /** A light's castShadow/shadow options changed. */
  _shadowChanged(light: DirectionalLight): void
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
}

/** The orthographic frustum a casting light renders its shadow map from,
 * in the light's own space (x right, y up, looking along its direction),
 * Three's DirectionalLightShadow camera. Everything outside it is lit. */
export type ShadowCamera = { left: number; right: number; top: number; bottom: number; near: number; far: number }

export type ShadowOptions = {
  /** Shadow map resolution in texels, square (default 1024). */
  mapSize?: number
  /** Depth bias against acne, in the map's 0..1 depth (default 0). */
  bias?: number
  /** Offset a receiving point along its normal before the lookup, in
   * world units (default 0) - the acne fix that keeps contact shadows. */
  normalBias?: number
  /** The light frustum; absent keys keep the defaults +-5, 0.5..500.
   * Ignored by a cascaded light (its frustums follow the scene camera). */
  camera?: Partial<ShadowCamera>
  /** Split the shadow into this many cascades (1..MAX_CASCADES, default
   * 1 = the `camera` box). With more, the light renders one map per slice
   * of the SCENE camera's frustum (near .. far, tightest first), each
   * `mapSize` texels wide and fitted every time the camera or the light
   * moves, and a receiver samples the tightest one that has the point:
   * sharp contact shadows near the camera and coarser ones toward the
   * horizon, for a scene the box cannot cover at one map's resolution.
   * Views (`scene.createView`) sample the same maps, fitted to the scene
   * camera, not their own. Each cascade is a tile of the atlas. */
  cascades?: number
  /** How far from the scene camera a cascaded light shadows, in world
   * units (default null = the camera's far). The cascades span
   * near..distance and a point past it is lit, so pulling it in sharpens
   * every cascade; it also bounds the maps' depth range, which is what
   * `bias` is measured against. A box light ignores it. */
  distance?: number | null
}

/** A directional light node: parallel rays travelling along `direction`
 * in the node's LOCAL space, so a parent's rotation turns the light with
 * it (the default `[0, -1, 0]` is a sun straight overhead; the length is
 * ignored). Scale does not affect it, and neither does position UNLESS
 * it casts: a casting light's shadow camera sits at its WORLD position
 * looking along its world direction (Three's rule), so place a casting
 * sun above the scene. Write through setLight. */
export type DirectionalLight = SceneNode & {
  kind: "light"
  type: "directional"
  direction: Vec3
  /** Linear [r, g, b] 0..1. */
  color: Vec3
  intensity: number
  /** Render a shadow map from this light (any directional light may;
   * each map is a full extra pass over the casting meshes); meshes with
   * `castShadow` draw into it, `lit` materials read it (unless
   * `receiveShadow: false`). */
  castShadow: boolean
  /** The resolved shadow options (read; write through setLight). */
  shadow: { mapSize: number; bias: number; normalBias: number; camera: ShadowCamera; cascades: number; distance: number | null }
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
  /** Draw into the scene's shadow map (default false, Three's default).
   * Set with setCastShadow. A casting instanced mesh is skipped (the
   * depth pass cannot know its record layout). */
  castShadow: boolean
  /** Layer membership bitmask (default 1, Three's `object.layers`): a
   * target draws the mesh when its mask intersects this. Not inherited
   * from ancestor Groups (Three's and Godot's rule). Set with setLayers. */
  layers: number
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
  /** Per-mesh sampler bindings merged over the material's at attach (a
   * skin's uBones palette texture): create-time state, set before the
   * mesh joins a scene, applied only to entries drawn with the mesh's
   * OWN material - an override or shadow variant validates its bindings
   * against a program that may not declare these names. */
  _textures: TextureBindings | null
  /** Instance state when the mesh was made by createInstancedMesh; null on
   * an ordinary mesh. */
  _instances: MeshInstances | null
  /** True for a createSprite mesh: its quad faces the camera in the
   * vertex stage, so it picks by a unit box instead of its flat triangles. */
  _sprite: boolean
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
 * An instanced mesh is picked by its explicit population box and a sprite
 * by a unit box around its center, so those three are absent on their
 * hits. */
export type Hit = {
  mesh: Mesh
  distance: number
  point: Vec3
  normal?: Vec3
  face?: number
  uv?: [number, number]
}

/** A pixel's camera ray (screenRay): `direction` is NOT normalized - its
 * camera-forward component is 1, so `origin + w * direction` is the world
 * point at camera-forward distance `w` (unproject's mapping). */
export type ScreenRay = { origin: Vec3; direction: Vec3 }

/** Filters for one raycast query. */
export type RaycastOptions = {
  /**
   * Layer mask replacing the scene's for this query (Unity's layerMask,
   * Three's raycaster.layers): hits come from meshes whose `layers`
   * intersect it, wherever the scene mask would look. A mask the scene
   * excludes makes a ray-only mesh - a low-poly collision mesh living
   * undrawn in the same scene, the physics-collider pattern.
   */
  layers?: number
  /** Only these meshes report hits (Three's intersectObjects, an
   * include-list); composes with `layers`. */
  meshes?: Mesh[]
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

/** An orthographic projection's view-space extents, in world units (the
 * same box at every depth). */
export type OrthoExtent = { left: number; right: number; top: number; bottom: number }

export type CameraUpdate = {
  /** Vertical field of view in DEGREES (default 60). */
  fov?: number
  near?: number
  far?: number
  position?: Vec3
  target?: Vec3
  up?: Vec3
  /** An orthographic projection with these extents (`fov` is then
   * ignored); null returns to perspective. Three's OrthographicCamera as
   * a camera option: a top-down map, an isometric view, a shadow-map
   * light. */
  ortho?: OrthoExtent | null
}

/**
 * Scene fog, by RADIAL distance from the camera, in one of two forms:
 * linear (Three's `Fog`, Unity's linear mode) fades toward `color` from
 * `near` to `far` world units and is fully fogged past `far`; exp2
 * (Three's `FogExp2`, Unity's default) thickens as
 * `1 - exp(-(distance * density)^2)`, no start band, never quite opaque
 * - `density` 0.01 is about 63% fog at 100 units, 98% at 200. Either
 * form takes the height attenuation (Godot's fog height, Unreal's
 * height falloff): the fog is full at and below `height` (world y) and
 * thins above it by `exp(-(y - height) * heightFalloff)` - a valley
 * fills while the hilltops and the sky stay clear; `heightFalloff` 0.1
 * halves the fog every ~7 units of climb. Match
 * `color` to the clearColor or background - the background is not
 * fogged, so a mismatch shows as a band at the horizon - and, for the
 * linear form, set `far` at or inside the camera's far plane to hide
 * the clip.
 */
export type FogOptions = (
  | {
      /** Distance where the fade starts, world units; negative puts the
       * camera itself part-way into the fog (Three allows the same). */
      near: number
      /** Distance where the fade completes, world units; must exceed near. */
      far: number
    }
  | {
      /** Exp2 thickness per world unit, > 0 (0.005 haze .. 0.05 pea soup). */
      density: number
    }
) & {
  /** Straight [r, g, b], 0..1. */
  color: [number, number, number]
  /** World y at and below which the fog is full; default 0. Only acts
   * with a `heightFalloff`. */
  height?: number
  /** How fast the fog thins above `height`, per world unit (e-fold rate,
   * >= 0); default 0, no height attenuation. */
  heightFalloff?: number
}

// The shared-param writes a fog value compiles to (null = the three zeros
// that turn the factor off) - one source for scene.setFog and a view's own
// `fog`, so both validate and spell the uniforms identically.
function fogParams(fog: FogOptions | null): ShaderParams {
  if (fog === null) return { uFogInv: 0, uFogDensity: 0, uFogHeightFalloff: 0 }
  let color = [fog.color[0], fog.color[1], fog.color[2]]
  let height = fog.height ?? 0
  let falloff = fog.heightFalloff ?? 0
  if (!Number.isFinite(height) || !Number.isFinite(falloff) || falloff < 0) {
    throw new Error(`fog: height must be finite and heightFalloff finite and >= 0, got ${height} / ${falloff}`)
  }
  let params: ShaderParams = { uFogColor: color, uFogHeight: height, uFogHeightFalloff: falloff }
  if ("density" in fog) {
    let { density } = fog
    if (!Number.isFinite(density) || density <= 0) {
      throw new Error(`fog: density must be finite and > 0, got ${density}`)
    }
    return { ...params, uFogInv: 0, uFogDensity: density }
  }
  let { near, far } = fog
  if (!(Number.isFinite(near) && Number.isFinite(far)) || far <= near) {
    throw new Error(`fog: near/far must be finite with near < far, got ${near}..${far}`)
  }
  return { ...params, uFogNear: near, uFogInv: 1 / (far - near), uFogDensity: 0 }
}

// `params` minus `names` - the scene-params fan-out around a view's own
// names. Returns the input untouched (no copy) when nothing intersects.
function withoutNames(params: ShaderParams, names: Set<string>): ShaderParams {
  let hit = false
  for (let k of names) {
    if (k in params) {
      hit = true
      break
    }
  }
  if (!hit) return params
  let out: ShaderParams = {}
  for (let k of Object.keys(params)) if (!names.has(k)) out[k] = params[k]!
  return out
}

// Layer masks are 32-bit sets, Three's Object3D.layers width.
function checkMask(mask: number, site: string): number {
  if (!Number.isInteger(mask) || mask < 0 || mask > 0xffffffff) {
    throw new Error(site + ": layers must be an integer bitmask in 0..0xffffffff, got " + mask)
  }
  return mask
}

export type SceneOptions = {
  clearColor?: [number, number, number, number]
  /** Scene-wide fog; see setFog. */
  fog?: FogOptions
  /** The scene target's layer mask (bitmask, default 1): the scene draws
   * the meshes whose `layers` intersect it. Live via scene.setLayers. */
  layers?: number
  /** The scene target's depth storage: true (default) for a buffer,
   * "texture" for a sampleable one exposed as `scene.depthTexture` - the
   * input for a depth-reading post effect in `output` (depth fog, SSAO,
   * depth of field). Not with `samples` (the engine has no multisampled
   * sampleable depth): render larger and display smaller instead. */
  depth?: true | "texture"
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

export type ViewOptions = {
  width: number
  height: number
  /**
   * Every mesh draws with this material instead of its own (Three's
   * `scene.overrideMaterial`, scoped to the view): a depth pass, a normal
   * or id visualizer. The view then carries none of the meshes' own
   * bindings or params, and instanced meshes are skipped (the override's
   * vertex stage cannot know their record layout). An overridden view
   * draws in add order (no renderOrder or transparent sort).
   */
  overrideMaterial?: Material
  /** The view's layer mask (bitmask, default 1): the view draws the
   * meshes whose `layers` intersect it - a minimap admitting marker
   * meshes only. Live via view.setLayers. */
  layers?: number
  /**
   * The view's own fog: FogOptions overrides the scene's, null turns fog
   * off in this view (an unfogged minimap over a fogged scene). Absent
   * follows the scene. The fog names become view-owned params - the
   * scene's setParams/setFog fan-out skips them (see View.setParams).
   */
  fog?: FogOptions | null
  /** The view target's depth storage: true (default) for a buffer,
   * "texture" for a sampleable one exposed as `view.depthTexture`. Not
   * with `into` (the depth is the parent's). */
  depth?: true | "texture"
  clearColor?: [number, number, number, number]
  samples?: 1 | 2 | 4 | 8
  filter?: FilterMode
  wrap?: WrapMode
  label?: string
  /** Render into a rectangle of this draw target (an app-owned atlas)
   * instead of a target of the view's own: every view into one atlas
   * costs ONE pass. `x`/`y` (top-left origin, default 0) place the tile;
   * display it with `<d-texture src={atlas} srcX srcY srcW srcH>`. The
   * atlas carries depth and samples; `view.texture` is then the tile's id
   * (a draw target, not a texture) and `view.depthTexture` is null. */
  into?: TextureId
  x?: number
  y?: number
}

/** A second rendering of a scene from its own camera; see Scene.createView. */
export type View = {
  /** The view's output, an ordinary texture id. */
  texture: TextureId
  /** The view target's depth as a sampler-only texture id when created
   * with `depth: "texture"` (the shadow-map input), else null. */
  depthTexture: TextureId | null
  /** Partial camera update, exactly scene.setCamera. */
  setCamera(update: CameraUpdate): void
  setSize(width: number, height: number): void
  /** Move and resize a view created `into` an atlas (top-left origin);
   * throws on a view with a target of its own. */
  setRect(rect: { x: number; y: number; width: number; height: number }): void
  /** View-owned shared params on the view's target. Names written here
   * (and the `fog` option's) become the view's OWN: the scene's
   * setParams/setFog fan-out skips them from then on, so a view override
   * is never clobbered by the next scene-wide write. */
  setParams(params: ShaderParams): void
  /** Replace the view's layer mask (bitmask): entries for newly admitted
   * meshes attach, masked-out ones detach. */
  setLayers(mask: number): void
  /** Destroy the view's target (its entries die with it). Idempotent;
   * views also die with their scene. */
  dispose(): void
}

export type Scene = {
  /** The scene's output: an ordinary texture id (`<texture src>`). */
  texture: TextureId
  /** The scene target's depth as a sampler-only texture id when created
   * with `depth: "texture"`, else null. */
  depthTexture: TextureId | null
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
   * Set, replace, or remove (null) the scene's fog, linear
   * (`{ color, near, far }`) or exp2 (`{ color, density }`), either with
   * the optional `height`/`heightFalloff`. One shared-params write
   * (`uFogColor`, `uFogNear`, `uFogInv`, `uFogDensity`, `uFogHeight`,
   * `uFogHeightFalloff` - the set FOG in `@solidrt/3d/glsl` declares;
   * the form not in use is written 0), fanned out to every view like
   * setParams, so however many meshes fog it costs nothing per frame.
   * Every standard material (unlit, lit, sprite) composes it unless
   * created with `fog: false`; a shaderMaterial opts in by composing FOG.
   * The background is not fogged: match colors (see FogOptions).
   */
  setFog(fog: FogOptions | null): void
  /** Replace the scene target's layer mask (bitmask): entries for newly
   * admitted meshes attach, masked-out ones detach. Shadow views follow
   * this mask - what the scene cannot see must not darken it. */
  setLayers(mask: number): void
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
   * markers, labels). `w` is the point's camera-forward distance in world
   * units under either projection (useful for depth-ordering or
   * distance-scaling markers, and unproject's exact input). Returns null
   * for a point at or behind a PERSPECTIVE camera's plane - such a point
   * has no place on screen; a parallel projection places every point, so
   * an orthographic camera never returns null and `w` may be zero or
   * negative there (negative near is legal ortho). Reflects a pending
   * setCamera immediately.
   */
  project(point: Vec3): { x: number; y: number; w: number } | null
  /**
   * project()'s exact inverse (Unity's ScreenToWorldPoint, Godot's
   * project_position): the world point at scene pixel (`x`, `y`) and
   * camera-forward distance `w`, copied into `out` (or a fresh Vec3).
   * project()'s `w` round-trips under either projection - the
   * drag-at-depth recipe: project the grabbed point once, keep its `w`,
   * unproject each pointer move to slide the object at its original
   * depth. Reflects a pending setCamera immediately, like project().
   */
  unproject(x: number, y: number, w: number, out?: Vec3): Vec3
  /** The camera's view-projection matrix, copied into `out` (or a fresh
   * mat4). The batch escape hatch; for single points use project(). */
  viewProj(out?: Mat4): Mat4
  /**
   * Cast the camera ray through a scene pixel (top-left origin, y down -
   * project()'s space, the inverse direction) and return every visible
   * mesh it hits, nearest first. An ordinary mesh tests per triangle
   * against its geometry (hits carry `face`, `uv`, a world-space `normal`
   * facing the ray, and a ray through a knot's hole misses); an instanced
   * mesh or sprite is box-only. Broadphase runs over a BVH kept in step
   * by the sync walk, and a large geometry's triangles are BVH-indexed
   * too (built by the first ray that reaches it), so merged static
   * geometry stays cheap to query. Reflects pending setTransform/add
   * writes immediately (the sync is flushed).
   */
  pick(x: number, y: number): Hit[]
  /** pick()'s world-space half: the same query along an arbitrary ray.
   * `direction` need not be normalized; distances are world units. */
  raycast(origin: Vec3, direction: Vec3, opts?: RaycastOptions): Hit[]
  /**
   * pick()'s ray half (Unity's ScreenPointToRay, Godot's
   * project_ray_origin/normal): the camera ray through a scene pixel,
   * fresh arrays each call, for intersection work pick() cannot do - a
   * drag plane, a ground grid, a raycast with the hits filtered
   * yourself. `direction`'s camera-forward component is 1 (see
   * ScreenRay), and raycast() takes it as-is. Reflects a pending
   * setCamera immediately.
   */
  screenRay(x: number, y: number): ScreenRay
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
   * transforms or design-size fits (the hit test undoes them). A leaf laid out
   * at a different size needs handlersFor instead.
   */
  handlers: SceneHandlers
  /** handlers for a leaf whose LAYOUT size differs from the target size -
   * the supersampling pattern, where the target renders larger than the
   * box showing it. `layout` is read per event, so a resize-reactive
   * layout just works: `scene.handlersFor(() => ({ width: w(), height:
   * h() }))`. */
  handlersFor(layout: () => { width: number; height: number }): SceneHandlers
  /**
   * A second rendering of this scene: its own draw target and camera,
   * the same meshes and lights. Each mesh gets one entry in the view's
   * target, bound as one more draw sink of the mesh's core node, so a
   * move feeds every target from the one flush and the app writes
   * nothing per view. Views share the scene's geometry buffers and
   * (unless `overrideMaterial`) its materials; the light set and
   * scene.setParams names fan out to every view, view.setParams is the
   * view's own channel. The scene's background is not mirrored (a view's
   * backdrop is its clearColor), and a view has no picking or pointer
   * events. Views die with the scene; `view.dispose()` drops one early.
   */
  createView(opts: ViewOptions): View
  /** Destroy the target (entries die with it). Idempotent. Material
   * pipelines are shared and survive (app-lifetime, see material.ts);
   * geometry buffers are reference-counted and freed with their last
   * entry (see geometry-gpu.ts). */
  dispose(): void
}

// A camera: the scene's own and one per view, the same state and the same
// one-shared-write contract. `dirty` = the matrices need recomputing (a
// setCamera or a resize), `pending` = the GPU write is owed to the next
// sync. The recompute is split from the sync so project()/viewProj() see a
// fresh matrix right after setCamera, before the microtask runs.
type Camera = {
  fov: number
  near: number
  far: number
  eye: Vec3
  target: Vec3
  up: Vec3
  ortho: OrthoExtent | null
  dirty: boolean
  pending: boolean
  proj: Mat4
  view: Mat4
  viewProj: Mat4
}

function makeCamera(): Camera {
  return {
    fov: 60,
    near: 0.1,
    far: 100,
    eye: [0, 0, 3],
    target: [0, 0, 0],
    up: [0, 1, 0],
    ortho: null,
    dirty: true,
    pending: false,
    proj: mat4(),
    view: mat4(),
    viewProj: mat4(),
  }
}

function updateCamera(cam: Camera, update: CameraUpdate): void {
  if (update.fov !== undefined) cam.fov = update.fov
  if (update.near !== undefined) cam.near = update.near
  if (update.far !== undefined) cam.far = update.far
  if (update.position) cam.eye = [update.position[0], update.position[1], update.position[2]]
  if (update.target) cam.target = [update.target[0], update.target[1], update.target[2]]
  if (update.up) cam.up = [update.up[0], update.up[1], update.up[2]]
  if (update.ortho !== undefined) {
    let o = update.ortho
    cam.ortho = o === null ? null : { left: o.left, right: o.right, top: o.top, bottom: o.bottom }
  }
  cam.dirty = true
}

function ensureCamera(cam: Camera, width: number, height: number): void {
  if (!cam.dirty) return
  cam.dirty = false
  cam.pending = true
  let o = cam.ortho
  if (o === null) perspective(cam.proj, (cam.fov * Math.PI) / 180, width / height, cam.near, cam.far)
  else orthographic(cam.proj, o.left, o.right, o.top, o.bottom, cam.near, cam.far)
  lookAtMatrix(cam.view, cam.eye, cam.target, cam.up)
  multiply(cam.viewProj, cam.proj, cam.view)
}

// The camera is target state: one shared write, whatever the target holds.
// Entries are untouched - uModel is camera-independent, and uCamPos is
// stored even when no current material declares it. The camera basis rides
// along: the view matrix's first two rows are the camera's world-space
// right and up (no clip flip - that lives in the projection), so a
// billboard needs no reconstruction from uViewProj.
function cameraParams(cam: Camera): ShaderParams {
  let v = cam.view
  return { uViewProj: cam.viewProj, uCamPos: cam.eye, uCamRight: [v[0], v[4], v[8]], uCamUp: [v[1], v[5], v[9]] }
}

// A material reads attributes by name; the geometry's layout must carry
// every one it declares (the pipeline is built for that layout, so a
// missing channel would have no home) - an error, like the rest of the
// strict entry path. Extra channels are fine.
function checkLayout(material: Material, geometry: Geometry, what: string): void {
  let missing = missingAttributes(material, geometry.layout)
  if (missing.length > 0) {
    throw new Error(
      what + " reads attributes the geometry layout (" + layoutKey(geometry.layout) + ") lacks: " +
        missing.map(a => a.name + " " + a.format).join(", ") +
        " - add the channel with withAttribute()/withColors(), or use a material that does not read it",
    )
  }
}

// An entry's initial params. The uNormal seed keys off the material flag
// because entry params validate strictly - and a material declaring
// uNormal without using it therefore throws right here, at add().
function entrySeed(material: Material, params: ShaderParams | null): ShaderParams {
  return material.normalMatrix
    ? { uModel: IDENTITY, uNormal: IDENTITY, ...material.params, ...params }
    : { uModel: IDENTITY, ...material.params, ...params }
}

// The uShadowAtlas binding while nothing casts: one white
// texel (depth 1, never shadowed), shared by every scene for the app.
let placeholder: TextureId | undefined

function shadowPlaceholder(): TextureId {
  if (placeholder === undefined) {
    placeholder = createTexture(new Uint8Array([255, 255, 255, 255]), 1, 1, { autoFree: false, label: "scene-shadow-none" })
  }
  return placeholder
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
    _transition: null,
  }
}

export function createGroup(): SceneNode {
  return makeNode("group")
}

export type DirectionalLightOptions = {
  direction?: Vec3
  color?: Vec3
  intensity?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: ShadowOptions
}
export type HemisphereLightOptions = { sky?: Vec3; ground?: Vec3; intensity?: number }

/** Every directional light may cast: the cap is MAX_LIGHTS (every map
 * is a tile of the scene's one shadow atlas, so the pass count does not
 * follow it, the fill does). */
export const MAX_SHADOWS = MAX_LIGHTS
export { MAX_CASCADES }

function mergeShadow(into: DirectionalLight["shadow"], update: ShadowOptions): void {
  if (update.mapSize !== undefined) into.mapSize = update.mapSize
  if (update.bias !== undefined) into.bias = update.bias
  if (update.normalBias !== undefined) into.normalBias = update.normalBias
  if (update.camera !== undefined) Object.assign(into.camera, update.camera)
  if (update.cascades !== undefined) {
    let n = update.cascades
    if (!Number.isInteger(n) || n < 1 || n > MAX_CASCADES) throw new Error("shadow.cascades must be an integer from 1 to " + MAX_CASCADES)
    into.cascades = n
  }
  if (update.distance !== undefined) {
    let d = update.distance
    if (d !== null && !(d > 0)) throw new Error("shadow.distance must be a positive number or null")
    into.distance = d
  }
}

export function createDirectionalLight(opts: DirectionalLightOptions = {}): DirectionalLight {
  let light = makeNode("light") as DirectionalLight
  light.type = "directional"
  light.direction = [...(opts.direction ?? [0, -1, 0])] as Vec3
  light.color = [...(opts.color ?? [1, 1, 1])] as Vec3
  light.intensity = opts.intensity ?? 1
  light.castShadow = opts.castShadow === true
  light.shadow = { mapSize: 1024, bias: 0, normalBias: 0, camera: { left: -5, right: 5, top: 5, bottom: -5, near: 0.5, far: 500 }, cascades: 1, distance: null }
  if (opts.shadow !== undefined) mergeShadow(light.shadow, opts.shadow)
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
    let shadowChanged = false
    if (update.castShadow !== undefined && update.castShadow !== light.castShadow) {
      light.castShadow = update.castShadow
      shadowChanged = true
    }
    if (update.shadow !== undefined) {
      mergeShadow(light.shadow, update.shadow)
      shadowChanged = true
    }
    if (shadowChanged) light._scene?._shadowChanged(light)
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
  mesh.castShadow = false
  mesh.layers = 1
  mesh._entry = null
  mesh._buffers = null
  mesh._transparent = false
  mesh._center = [0, 0, 0]
  mesh._params = null
  mesh._textures = null
  mesh._instances = null
  mesh._sprite = false
  return mesh
}

// Every sprite draws the same unit quad, built once: geometry is data
// and its GPU buffers are acquired per mesh, so one shared value is the
// normal sharing story. The box is the quad's extent at any facing.
let spriteQuad: Geometry | undefined
const SPRITE_BOUNDS = new Float32Array([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5])

/**
 * A camera-facing quad, Three's `Sprite`: a unit plane drawn with a
 * `sprite()` material (any material works, but only a sprite material
 * turns the quad; there is no `geometry` argument). Size it with `scale` -
 * a scale of [2, 1, 1] is a 2 x 1 world-unit quad - and place it like any
 * mesh; its rotation is ignored, the camera decides the facing. Picking
 * is by a unit box around the center (the quad's reach at any facing, an
 * approximation), so hits carry no normal/face/uv.
 */
export function createSprite(material: Material): Mesh {
  if (spriteQuad === undefined) spriteQuad = plane({ label: "sprite" })
  let mesh = createMesh(spriteQuad, material)
  mesh._sprite = true
  return mesh
}

/** The local box picking and sorting work from: explicit instance bounds
 * when the mesh is instanced (null without them - no leaf, no hits), the
 * unit box for a sprite, the geometry's own bounds otherwise. */
function localBounds(mesh: Mesh): Float32Array | null {
  if (mesh._instances !== null) return mesh._instances.bounds
  return mesh._sprite ? SPRITE_BOUNDS : geometryBounds(mesh.geometry)
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
  if (node._transition !== null) {
    spatial.setTransition(node._node, node._transition)
    declareTransition(node._node, node)
  }
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
    declared.delete(node._node)
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

/** Draw the mesh into the scene's shadow map, or stop (see Mesh.castShadow). */
/**
 * Set a mesh's layer membership (bitmask, default 1). A target draws the
 * mesh when its mask intersects this: the scene's own mask (`layers` on
 * createScene, `scene.setLayers`), each view's (`layers` on createView,
 * `view.setLayers`); shadow views follow the scene's. A mesh masked out of
 * the scene is also skipped by pick()/raycast(), like an invisible one -
 * unless a raycast passes its own mask (RaycastOptions.layers), which is
 * how an undrawn collision-only mesh stays queryable. `layers: 0` draws
 * nowhere. Not inherited from ancestor Groups.
 */
export function setLayers(mesh: Mesh, layers: number): void {
  checkMask(layers, "setLayers")
  if (mesh.layers === layers) return
  mesh.layers = layers
  mesh._scene?._setLayers(mesh)
}

export function setCastShadow(mesh: Mesh, cast: boolean): void {
  if (mesh.castShadow === cast) return
  mesh.castShadow = cast
  mesh._scene?._setCast(mesh)
}

/** Swap a mesh's geometry: its draw entry is rebuilt (the scene re-sorts
 * the list, so the mesh keeps its place). */
export function setGeometry(mesh: Mesh, geometry: Geometry): void {
  if (mesh._sprite) throw new Error("setGeometry: a sprite draws the shared unit quad and takes no geometry")
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
  let depthMode = opts?.depth ?? true
  if (depthMode === "texture" && (opts?.samples ?? 1) > 1) {
    throw new Error('createScene: depth "texture" cannot combine with samples (no multisampled sampleable depth) - render larger and display smaller instead')
  }
  // The scene target's layer mask; shadow views follow it (a mesh the
  // scene cannot see must not darken it).
  let sceneMask = checkMask(opts?.layers ?? 1, "createScene")
  let texture = createDrawTarget(width, height, null, {
    depth: depthMode,
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
  // This rewrite runs on attach/detach/field changes (and a new view)
  // only and owns the rest: colors, count, hemisphere. The light set is
  // scene state, so it lands on the scene target and every view target.
  let vecScratch = new Float32Array(3)
  let writeLights = () => {
    lightsDirty = false
    let sky: Vec3 = [0, 0, 0]
    let ground: Vec3 = [0, 0, 0]
    let colors: number[] = []
    let bias: number[] = []
    let normalBias: number[] = []
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
      for (let v of views) spatial.bindDirectionSlot(light._node!, v.texture, "uLightDir", MAX_LIGHTS * 3, count, vecScratch)
      let c = light.color
      let k = light.intensity
      colors.push(c[0] * k, c[1] * k, c[2] * k)
      bias.push(light.shadow.bias)
      normalBias.push(light.shadow.normalBias)
      count++
    }
    for (let i = count; i < MAX_LIGHTS; i++) {
      colors.push(0, 0, 0)
      bias.push(0)
      normalBias.push(0)
    }
    // The shadow set rides with the lights. Per directional light i: its
    // map slots as uShadowFirst[i] + uShadowCount[i] (0 = a receiving
    // material draws that light plain) and its biases; per map slot j its
    // tile of the atlas as uShadowRect[j] in atlas UV (the whole map in
    // an unused slot - never read). The atlas depth binds once as
    // uShadowAtlas, the white placeholder when nothing casts, so every
    // receiving target always has the sampler bound.
    let first: number[] = new Array(MAX_LIGHTS).fill(0)
    let counts: number[] = new Array(MAX_LIGHTS).fill(0)
    let rects: number[] = []
    let atlas = shadowAtlas
    let maps: Record<string, TextureId> = { uShadowAtlas: atlas !== null ? depthTexture(atlas.texture) : shadowPlaceholder() }
    forEachShadowSlot((slot, i, shadow, c) => {
      if (c === 0) first[i] = slot
      counts[i] = counts[i]! + 1
      let r = shadow.rects[c]!
      let a = atlas!
      rects.push(r.x / a.width, r.y / a.height, r.width / a.width, r.height / a.height)
    })
    for (let slot = rects.length / 4; slot < MAX_SHADOW_MAPS; slot++) rects.push(0, 0, 1, 1)
    let params: ShaderParams = {
      uHemiSky: sky,
      uHemiGround: ground,
      uLightCount: count,
      uLightColor: colors,
      uShadowFirst: first,
      uShadowCount: counts,
      uShadowBias: bias,
      uShadowNormalBias: normalBias,
      uShadowRect: rects,
    }
    receivingTargets(t => {
      setTargetParams(t, params)
      setTargetTextures(t, maps)
    })
    // A slot change (a light attached, detached or reordered) moves every
    // matrix too: rewrite the whole array once.
    shadowMatricesDirty = true
  }
  let orderDirty = false
  // The order last handed to the engine: a resort that lands on the same
  // permutation (the common case under a moving camera) issues nothing.
  let lastOrder: DrawId[] = []
  let background: { entry: DrawId; pipeline: RenderPipelineId; program: ProgramId } | null = null
  let sortEntries = () => {
    orderDirty = false
    let order = orderEntries(meshes, camera.view, background?.entry)
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
  let camera = makeCamera()
  let clip: Vec4 = [0, 0, 0, 0]
  let pickOrigin: Vec3 = [0, 0, 0]

  // The camera ray through a scene pixel, into the pickOrigin/pickDir
  // scratches. The direction keeps a camera-forward component of 1
  // (perspective: (cx, cy, -1) in the camera frame; ortho: unit forward),
  // so origin + w * direction is the world point at camera-forward
  // distance w - what unproject() banks on. pick()/screenRay()/
  // unproject() all cast exactly this ray.
  let pixelRay = (x: number, y: number): void => {
    ensureCamera(camera, width, height)
    let v = camera.view
    let o = camera.ortho
    if (o === null) {
      // The camera-frame ray through the pixel, inverting project()'s
      // mapping: the baked y-down clip flip is why pixel y converts with
      // no negation there and one here.
      let f = 1 / Math.tan(((camera.fov * Math.PI) / 180) / 2)
      let cx = (((x / width) * 2 - 1) * (width / height)) / f
      let cy = -((y / height) * 2 - 1) / f
      // The view's upper 3x3 rows are the camera axes, so its transpose
      // carries the camera-space direction (cx, cy, -1) to world.
      pickDir[0] = cx * v[0] + cy * v[1] - v[2]
      pickDir[1] = cx * v[4] + cy * v[5] - v[6]
      pickDir[2] = cx * v[8] + cy * v[9] - v[10]
      pickOrigin[0] = camera.eye[0]
      pickOrigin[1] = camera.eye[1]
      pickOrigin[2] = camera.eye[2]
      return
    }
    // Orthographic: every ray runs along the camera's forward axis; the
    // pixel picks where on the camera plane it starts (top row = top).
    let cx = o.left + (x / width) * (o.right - o.left)
    let cy = o.top + (y / height) * (o.bottom - o.top)
    pickOrigin[0] = camera.eye[0] + cx * v[0] + cy * v[1]
    pickOrigin[1] = camera.eye[1] + cx * v[4] + cy * v[5]
    pickOrigin[2] = camera.eye[2] + cx * v[8] + cy * v[9]
    pickDir[0] = -v[2]
    pickDir[1] = -v[6]
    pickDir[2] = -v[10]
  }

  // Views (scene.createView): more targets drawing the same meshes from
  // their own cameras. A view holds one entry per mesh in its target,
  // bound as one more draw sink of the mesh's core node, so the flush
  // that writes the scene's entry writes the view's too. Sorted like the
  // scene (view-space keys from the view's own camera); an overridden
  // view is not sorted at all.
  type ViewRecord = {
    texture: TextureId
    width: number
    height: number
    override: Material | null
    /** Non-null marks a SHADOW view and names its caster set (m =>
     * m.castShadow). Re-evaluated per mesh by _setCast; also picks the
     * caster's own shadow material variant over the depth override. */
    shadowFilter: ((mesh: Mesh) => boolean) | null
    /** Layer mask: the view draws the meshes whose `layers` intersect
     * it. A shadow view's follows the scene's. */
    mask: number
    /** Names the view set itself (view.setParams, the fog option): the
     * scene's fan-out skips them so a view override survives scene-wide
     * writes. */
    ownNames: Set<string>
    camera: Camera
    entries: Map<Mesh, DrawId>
    orderDirty: boolean
    lastOrder: DrawId[]
    disposed: boolean
  }
  let views: ViewRecord[] = []
  // Every name scene.setParams has merged so far, replayed on a new view.
  let sceneParams: ShaderParams = {}
  // The shadows, one per casting directional light: the internal views
  // rendering its maps (tiles of the shadow atlas, depth override, casting
  // meshes only; one for a box light, `shadow.cascades` for a cascaded
  // one, tightest first) and `rects`, each tile's place in the atlas in
  // texels. `lastWorld` is the light's world matrix the shadow cameras
  // were last placed from; `dirty` forces a re-place (options changed).
  type ShadowRect = { x: number; y: number; width: number; height: number }
  type Shadow = { light: DirectionalLight; views: ViewRecord[]; lastWorld: Mat4; dirty: boolean; rects: ShadowRect[] }
  let shadows = new Map<DirectionalLight, Shadow>()
  // The map slots: every casting light's maps in light order, a light's
  // cascades consecutive and tightest first. The ONE enumeration the
  // receiving side is dealt by - rects and first/count in writeLights,
  // matrices in sync - so uShadowFirst/uShadowCount agree with both.
  // `i` is the light's directional index (its uShadow*[i] slot).
  let forEachShadowSlot = (fn: (slot: number, i: number, shadow: Shadow, cascade: number) => void): void => {
    let slot = 0
    let i = 0
    for (let light of lights) {
      if (light.type !== "directional") continue
      let shadow = shadows.get(light)
      if (shadow !== undefined) for (let c = 0; c < shadow.views.length; c++) fn(slot++, i, shadow, c)
      i++
    }
  }
  // The shadow atlas: ONE depth-texture target every casting light's map
  // is a tile of, so N maps render as one pass and receivers sample one
  // sampler through per-map rects (uShadowRect). Created with the first
  // caster, destroyed with the last. Laid out as a grid of cells the
  // largest mapSize wide, scaled down uniformly when that would exceed
  // the device's texture size: tile size follows the budget.
  let shadowAtlas: { texture: TextureId; width: number; height: number } | null = null
  let shadowLayout = (count: number, maxSize: number) => {
    let cols = Math.ceil(Math.sqrt(count))
    let rows = Math.ceil(count / cols)
    let scale = Math.min(1, limits.maxTextureSize / (cols * maxSize), limits.maxTextureSize / (rows * maxSize))
    let cell = Math.max(1, Math.floor(maxSize * scale))
    return { cols, cell, scale, width: cols * cell, height: rows * cell }
  }
  // Place every shadow tile for the current caster set plus `adding` (not
  // yet in `shadows`; its rects are returned for the view creates), in
  // light order, a light's cascades consecutive. Sizes the atlas, moves
  // tiles whose place changed, and drops the atlas when nothing casts.
  // The rects reach receivers through the next light rewrite.
  let placeShadows = (adding: DirectionalLight | null): ShadowRect[] | null => {
    let casters: DirectionalLight[] = []
    for (let l of lights) if (l.type === "directional" && (shadows.has(l) || l === adding)) casters.push(l)
    if (adding !== null && !casters.includes(adding)) casters.push(adding)
    lightsDirty = true
    if (casters.length === 0) {
      if (shadowAtlas !== null) {
        destroyTexture(shadowAtlas.texture)
        shadowAtlas = null
      }
      return null
    }
    let maxSize = 1
    let tiles = 0
    for (let l of casters) {
      maxSize = Math.max(maxSize, l.shadow.mapSize)
      tiles += l.shadow.cascades
    }
    let lay = shadowLayout(tiles, maxSize)
    if (shadowAtlas === null) {
      shadowAtlas = {
        texture: createDrawTarget(lay.width, lay.height, null, {
          depth: "texture",
          clearColor: [1, 1, 1, 1],
          label: (opts?.label ?? "scene") + "-shadow-atlas",
          autoFree: false,
        }),
        width: lay.width,
        height: lay.height,
      }
    } else if (shadowAtlas.width !== lay.width || shadowAtlas.height !== lay.height) {
      setTargetSize(shadowAtlas.texture, lay.width, lay.height)
      shadowAtlas.width = lay.width
      shadowAtlas.height = lay.height
    }
    let placed: ShadowRect[] | null = null
    let k = 0
    for (let l of casters) {
      let size = Math.max(1, Math.floor(l.shadow.mapSize * lay.scale))
      let shadow = shadows.get(l)
      for (let c = 0; c < l.shadow.cascades; c++, k++) {
        let rect: ShadowRect = { x: (k % lay.cols) * lay.cell, y: Math.floor(k / lay.cols) * lay.cell, width: size, height: size }
        if (shadow === undefined) {
          if (placed === null) placed = []
          placed.push(rect)
          continue
        }
        let r = shadow.rects[c]!
        if (r.x === rect.x && r.y === rect.y && r.width === rect.width && r.height === rect.height) continue
        shadow.rects[c] = rect
        let view = shadow.views[c]!
        setTargetRect(view.texture, rect)
        view.width = rect.width
        view.height = rect.height
        // A tile's texel size moved: the cascade fit snaps to it.
        shadow.dirty = true
      }
    }
    return placed
  }
  let shadowDir: Vec3 = [0, 0, 0]
  // uShadowMatrix is one array param (the engine writes whole arrays), so
  // any shadow camera move rewrites all MAX_SHADOW_MAPS matrices, identity
  // in the slots that are not dealt.
  let shadowMatrices: number[] = new Array(MAX_SHADOW_MAPS * 16).fill(0)
  let shadowMatricesDirty = false
  // Every target a receiving material can draw into: the scene's and each
  // view's but the shadow views (binding a target's own depth into it
  // would be same-pass feedback).
  let receivingTargets = (fn: (target: TextureId) => void) => {
    fn(texture)
    for (let v of views) if (v.shadowFilter === null) fn(v.texture)
  }
  // The mesh's entry in the scene's OWN target - what mesh._entry is.
  // Created when the scene mask admits the mesh, dropped when it stops:
  // the same lifecycle a view entry has, so `_buffers` (not `_entry`) is
  // the attached-to-the-scene sentinel.
  let attachScene = (mesh: Mesh) => {
    if (mesh._entry !== null) return
    if ((mesh.layers & sceneMask) === 0) return
    let inst = mesh._instances
    let bufs = mesh._buffers!
    // The entry starts switched off: it has no world matrix yet - the walk
    // in sync() computes one - and _schedule() defers that to a microtask,
    // so added live it would draw at the seeded identity until then. The
    // mismatch branch in sync() turns it on in the same pass that writes
    // uModel.
    mesh._entry = addDraw(texture, mesh.material.pipeline(mesh.geometry.layout), entrySeed(mesh.material, mesh._params), {
      buffer: bufs.buffer,
      indexBuffer: bufs.index,
      indexFormat: bufs.indexFormat,
      textures: mesh._textures !== null ? { ...mesh.material.textures, ...mesh._textures } : mesh.material.textures,
      instanceBuffer: inst !== null ? inst.buffer : undefined,
      instanceCount: 0,
    })
    // The core turns the entry on (with the world matrix) at the next
    // flush, and off again whenever the node or an ancestor hides.
    spatial.bindDraw(mesh._node!, texture, mesh._entry, mesh.material.normalMatrix === true, inst !== null ? inst.count : 1)
    orderDirty = true
  }
  let detachScene = (mesh: Mesh) => {
    if (mesh._entry === null) return
    if (mesh._node !== null) spatial.unbindDraw(mesh._node, texture)
    if (!disposed) removeDraw(texture, mesh._entry)
    mesh._entry = null
    orderDirty = true
  }
  let attachView = (v: ViewRecord, mesh: Mesh) => {
    if (v.entries.has(mesh)) return
    let inst = mesh._instances
    if (v.override !== null && inst !== null) return
    if (v.shadowFilter !== null && !v.shadowFilter(mesh)) return
    if ((mesh.layers & v.mask) === 0) return
    // A shadow view lets a caster's material pick its own depth variant
    // (its cull side); any other override view draws exactly what it was
    // given.
    let material = v.override !== null ? (v.shadowFilter !== null ? (mesh.material.shadow ?? v.override) : v.override) : mesh.material
    let bufs = mesh._buffers!
    let entry = addDraw(v.texture, material.pipeline(mesh.geometry.layout), entrySeed(material, v.override !== null ? null : mesh._params), {
      buffer: bufs.buffer,
      indexBuffer: bufs.index,
      indexFormat: bufs.indexFormat,
      // The mesh's own bindings ride only with its own material: an
      // override (or shadow variant) program may not declare their names,
      // and per-entry bindings validate strictly.
      textures: material === mesh.material && mesh._textures !== null ? { ...material.textures, ...mesh._textures } : material.textures,
      instanceBuffer: inst !== null ? inst.buffer : undefined,
      instanceCount: 0,
    })
    spatial.bindDraw(mesh._node!, v.texture, entry, material.normalMatrix === true, inst !== null ? inst.count : 1)
    v.entries.set(mesh, entry)
    v.orderDirty = true
  }
  let detachView = (v: ViewRecord, mesh: Mesh) => {
    let entry = v.entries.get(mesh)
    if (entry === undefined) return
    v.entries.delete(mesh)
    if (mesh._node !== null) spatial.unbindDraw(mesh._node, v.texture)
    if (!v.disposed) removeDraw(v.texture, entry)
    v.orderDirty = true
  }
  let sortView = (v: ViewRecord) => {
    v.orderDirty = false
    if (v.override !== null) return
    let order = orderEntries(meshes, v.camera.view, undefined, m => v.entries.get(m as Mesh) ?? null)
    if (order.length === v.lastOrder.length && order.every((id, i) => id === v.lastOrder[i])) return
    v.lastOrder = order
    setDrawOrder(v.texture, order)
  }
  let disposeView = (v: ViewRecord) => {
    if (v.disposed) return
    v.disposed = true
    for (let mesh of v.entries.keys()) if (mesh._node !== null) spatial.unbindDraw(mesh._node, v.texture)
    v.entries.clear()
    for (let light of lights) if (light.type === "directional" && light._node !== null) spatial.unbindSlot(light._node, v.texture)
    // Drain the zeroed direction slots while the target still exists.
    spatial.flush()
    destroyTexture(v.texture)
    let i = views.indexOf(v)
    if (i >= 0) views.splice(i, 1)
  }
  // A view record: the target, seeded with everything the scene target
  // already holds (the light set - rewritten for every target, the simple
  // write - the merged scene params, the shadow map binding), then one
  // entry per mesh the filter admits.
  let makeView = (vopts: ViewOptions, shadowFilter: ((mesh: Mesh) => boolean) | null): ViewRecord => {
    let override = vopts.overrideMaterial ?? null
    if (override !== null) {
      for (let mesh of meshes) if (mesh._instances === null) checkLayout(override, mesh.geometry, "View override material")
    }
    let tiled = vopts.into !== undefined
    let v: ViewRecord = {
      texture: createDrawTarget(vopts.width, vopts.height, null, {
        depth: tiled ? undefined : (vopts.depth ?? true),
        clearColor: vopts.clearColor,
        filter: vopts.filter,
        wrap: vopts.wrap,
        samples: vopts.samples,
        label: vopts.label ?? (opts?.label ?? "scene") + "-view",
        autoFree: false,
        into: vopts.into,
        x: vopts.x,
        y: vopts.y,
      }),
      width: vopts.width,
      height: vopts.height,
      override,
      shadowFilter,
      mask: shadowFilter !== null ? sceneMask : checkMask(vopts.layers ?? 1, "createView"),
      ownNames: new Set(),
      camera: makeCamera(),
      entries: new Map(),
      orderDirty: true,
      lastOrder: [],
      disposed: false,
    }
    views.push(v)
    // The light rewrite seeds the shadow set (maps, casts, biases,
    // matrices) on the new target too.
    lightsDirty = true
    // The view's own fog claims its names before the scene-params seed,
    // so the seed (and every later fan-out) leaves them to the view.
    let ownFog = vopts.fog !== undefined ? fogParams(vopts.fog) : null
    if (ownFog !== null) for (let k of Object.keys(ownFog)) v.ownNames.add(k)
    setTargetParams(v.texture, ownFog === null ? sceneParams : withoutNames(sceneParams, v.ownNames))
    if (ownFog !== null) setTargetParams(v.texture, ownFog)
    for (let mesh of meshes) attachView(v, mesh)
    hooks._schedule()
    return v
  }
  // A shadow's views: one square tile of the shadow atlas per map drawing
  // the casting meshes with the depth override from that map's frustum.
  // The light rewrite writes the rects in the light's slots on every
  // receiving target.
  let createShadow = (light: DirectionalLight) => {
    let rects = placeShadows(light)
    if (rects === null || shadowAtlas === null) return
    let atlas = shadowAtlas
    let views = rects.map(rect =>
      makeView(
        {
          width: rect.width,
          height: rect.height,
          into: atlas.texture,
          x: rect.x,
          y: rect.y,
          overrideMaterial: shadowDepthMaterial(),
          clearColor: [1, 1, 1, 1],
          label: (opts?.label ?? "scene") + "-shadow",
        },
        m => m.castShadow,
      ),
    )
    shadows.set(light, { light, views, lastWorld: mat4(), dirty: true, rects })
    lightsDirty = true
  }
  let destroyShadow = (light: DirectionalLight) => {
    let shadow = shadows.get(light)
    if (shadow === undefined) return
    shadows.delete(light)
    for (let v of shadow.views) disposeView(v)
    placeShadows(null)
    lightsDirty = true
    hooks._schedule()
  }
  // Place a shadow's cameras from its light's world matrix. A box light:
  // at its world position, looking along its world direction, the light
  // frustum as the orthographic extents. Compared against the matrix it
  // was last placed from, so a scene animating elsewhere rewrites nothing
  // here. A cascaded light also follows the scene camera (`cameraMoved`).
  let cascadeScratch = mat4()
  let cascadeCenter: Vec3 = [0, 0, 0]
  let placeShadowCamera = (shadow: Shadow, cameraMoved: boolean) => {
    let light = shadow.light
    let m = worldInto(worldScratch, light)
    let cascaded = shadow.views.length > 1
    if (!shadow.dirty && !(cascaded && cameraMoved) && m.every((x, i) => x === shadow.lastWorld[i])) return
    shadow.dirty = false
    copy(shadow.lastWorld, m)
    transformVector(shadowDir, m, light.direction)
    let len = Math.hypot(shadowDir[0], shadowDir[1], shadowDir[2]) || 1
    let d: Vec3 = [shadowDir[0] / len, shadowDir[1] / len, shadowDir[2] / len]
    // A sun straight down is the common case and the degenerate one for
    // world up: roll about z then (the map's orientation is invisible).
    let up: Vec3 = Math.abs(d[1]) > VERTICAL_LIGHT ? [0, 0, 1] : [0, 1, 0]
    if (!cascaded) {
      let c = light.shadow.camera
      updateCamera(shadow.views[0]!.camera, {
        position: [m[12], m[13], m[14]],
        target: [m[12] + d[0], m[13] + d[1], m[14] + d[2]],
        up,
        ortho: { left: c.left, right: c.right, top: c.top, bottom: c.bottom },
        near: c.near,
        far: c.far,
      })
      return
    }
    // Cascades: the scene camera's range near..far (far capped by
    // shadow.distance) sliced by cascadeSplit, each slice's bounding
    // sphere (frustumSliceSphere) as an orthographic box looking along
    // the light, its centre snapped to the map's texel grid in light
    // space (snapToGrid) so the shadow edges do not swim as the camera
    // moves. The box reaches back toward the light by the whole range,
    // so a caster outside the slice still casts into it.
    let n = shadow.views.length
    let near = camera.near
    let far = Math.min(camera.far, light.shadow.distance ?? Infinity)
    if (!(far > near)) far = near + 1
    // The light's rotation only: rows are its right, up and back axes.
    let basis = lookAtMatrix(cascadeScratch, [0, 0, 0], d, up)
    let aspect = width / height
    let zn = near
    for (let c = 0; c < n; c++) {
      let zf = cascadeSplit(near, far, c, n, CASCADE_SPLIT_LAMBDA)
      let radius = frustumSliceSphere(cascadeCenter, camera, aspect, zn, zf)
      zn = zf
      let view = shadow.views[c]!
      // A texel is 2r / mapSize world units.
      snapToGrid(cascadeCenter, cascadeCenter, basis, (2 * radius) / view.width)
      let back = radius + far
      updateCamera(view.camera, {
        position: [cascadeCenter[0] - d[0] * back, cascadeCenter[1] - d[1] * back, cascadeCenter[2] - d[2] * back],
        target: cascadeCenter,
        up,
        ortho: { left: -radius, right: radius, top: radius, bottom: -radius },
        near: 0,
        far: back + radius,
      })
    }
  }

  let sync = () => {
    scheduled = false
    if (disposed) return
    ensureCamera(camera, width, height)
    let cameraMoved = camera.pending
    if (camera.pending) {
      camera.pending = false
      setTargetParams(texture, cameraParams(camera))
      if (transparentCount > 1) orderDirty = true
    }
    for (let shadow of shadows.values()) placeShadowCamera(shadow, cameraMoved)
    for (let v of views) {
      ensureCamera(v.camera, v.width, v.height)
      if (v.camera.pending) {
        v.camera.pending = false
        setTargetParams(v.texture, cameraParams(v.camera))
        if (transparentCount > 1) v.orderDirty = true
        if (v.shadowFilter !== null) shadowMatricesDirty = true
      }
    }
    // Light bookkeeping first, so a fresh direction-slot bind is seeded
    // by the flush below in the same sync.
    if (lightsDirty) writeLights()
    // The matrices that render the maps are the ones receivers look up
    // with: one array to every receiving target per shadow-camera move.
    if (shadowMatricesDirty) {
      shadowMatricesDirty = false
      let dealt = 0
      forEachShadowSlot((slot, _i, shadow, c) => {
        let m = shadow.views[c]!.camera.viewProj
        for (let k = 0; k < 16; k++) shadowMatrices[slot * 16 + k] = m[k]!
        dealt = slot + 1
      })
      for (let slot = dealt; slot < MAX_SHADOW_MAPS; slot++) for (let k = 0; k < 16; k++) shadowMatrices[slot * 16 + k] = IDENTITY[k]!
      let params: ShaderParams = { uShadowMatrix: shadowMatrices }
      receivingTargets(t => setTargetParams(t, params))
    }
    // The core recomputes the moved subtrees and writes every entry's
    // uModel/uNormal, visibility switch and direction slots.
    spatial.flush()
    if (moved.length > 0) {
      // Which meshes moved is the core's knowledge now, so any move with
      // two or more transparent meshes re-sorts (sortEntries issues nothing
      // when the permutation is unchanged).
      if (transparentCount > 1) {
        orderDirty = true
        for (let v of views) v.orderDirty = true
      }
      for (let n of moved) n._moved = false
      moved.length = 0
    }
    // The sort keys are world-space and camera-independent: refreshed once
    // for every sort this sync.
    if (orderDirty || views.some(v => v.orderDirty)) refreshCenters()
    if (orderDirty) sortEntries()
    for (let v of views) if (v.orderDirty) sortView(v)
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
      if (light.type === "directional" && light.castShadow) createShadow(light)
    },
    _detachLight(light) {
      let i = lights.indexOf(light)
      if (i >= 0) lights.splice(i, 1)
      lightsDirty = true
      if (light.type === "directional") destroyShadow(light)
      hooks._schedule()
    },
    _shadowChanged(light) {
      if (disposed) return
      let shadow = shadows.get(light)
      if (shadow !== undefined) {
        if (!light.castShadow) {
          destroyShadow(light)
          return
        }
        // A cascade count change is a different view set: rebuild it. A
        // mapSize change re-places every tile (the grid cell follows the
        // largest map).
        if (shadow.views.length !== light.shadow.cascades) {
          destroyShadow(light)
          createShadow(light)
          return
        }
        placeShadows(null)
        shadow.dirty = true
        lightsDirty = true
        hooks._schedule()
      } else if (light.castShadow) {
        createShadow(light)
      }
    },
    _setCast(mesh) {
      if (mesh._buffers === null || disposed) return
      for (let v of views) {
        if (v.shadowFilter === null) continue
        if (v.shadowFilter(mesh)) attachView(v, mesh)
        else detachView(v, mesh)
      }
      hooks._schedule()
    },
    _setLayers(mesh) {
      if (mesh._buffers === null || disposed) return
      if ((mesh.layers & sceneMask) !== 0) attachScene(mesh)
      else detachScene(mesh)
      for (let v of views) {
        if ((mesh.layers & v.mask) !== 0) attachView(v, mesh)
        else detachView(v, mesh)
      }
      hooks._schedule()
    },
    _lightChanged() {
      lightsDirty = true
      hooks._schedule()
    },
    _attach(mesh) {
      if (disposed) return
      validateGeometry(mesh.geometry)
      checkLayout(mesh.material, mesh.geometry, "Mesh material")
      // Every check before any mutation, so a rejected mesh is attached
      // nowhere - the views' override materials included.
      for (let v of views) {
        if (v.override !== null && mesh._instances === null) checkLayout(v.override, mesh.geometry, "View override material")
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
      attachScene(mesh)
      for (let v of views) attachView(v, mesh)
      // Picking: the local box puts the node in the core index; an
      // ordinary mesh also gets its geometry's triangle shape, an
      // instanced one is box-only (records are opaque, and without
      // explicit bounds it is not picked at all), as is a sprite (its
      // triangles lie wherever the camera is, not where the geometry says).
      spatial.setBounds(mesh._node!, localBounds(mesh))
      spatial.setShape(mesh._node!, inst === null && !mesh._sprite ? bufs.shape : null)
      byNode.set(mesh._node!, mesh)
      meshes.push(mesh)
      mesh._transparent = mesh.material.transparent === true
      if (mesh._transparent) transparentCount++
      orderDirty = true
      this._schedule()
    },
    _detach(mesh) {
      if (mesh._buffers !== null) {
        for (let v of views) detachView(v, mesh)
        detachScene(mesh)
        if (mesh._node !== null) {
          spatial.setShape(mesh._node, null)
          spatial.setBounds(mesh._node, null)
          byNode.delete(mesh._node)
        }
        releaseGeometryBuffers(mesh._buffers)
        mesh._buffers = null
        let i = meshes.indexOf(mesh)
        if (i >= 0) meshes.splice(i, 1)
        if (mesh._transparent) transparentCount--
        orderDirty = true
      }
      mesh._entry = null
    },
    _setParams(mesh, params) {
      if (mesh._buffers === null || disposed) return
      if (mesh._entry !== null) setDrawParams(texture, mesh._entry, params)
      // A view drawing the mesh's own material carries its params too; an
      // overridden view has none of them.
      for (let v of views) {
        let entry = v.entries.get(mesh)
        if (entry !== undefined && v.override === null) setDrawParams(v.texture, entry, params)
      }
    },
    _setCount(mesh) {
      // The core composes the count with the visibility switch: a hidden
      // entry stays at 0 and the unhide restores the new count.
      if (mesh._buffers !== null && mesh._node !== null && !disposed && mesh._instances !== null) {
        spatial.setDrawCount(mesh._node, mesh._instances.count)
      }
    },
    _setBuffer(mesh) {
      // The entry keeps its range (at most the old capacity, so the larger
      // buffer always passes the swap's bounds check); the caller destroys
      // the old buffer after this, which the entry held alive until now.
      if (mesh._buffers !== null && !disposed && mesh._instances !== null) {
        if (mesh._entry !== null) setDrawBuffers(texture, mesh._entry, { instanceBuffer: mesh._instances.buffer })
        for (let v of views) {
          let entry = v.entries.get(mesh)
          if (entry !== undefined) setDrawBuffers(v.texture, entry, { instanceBuffer: mesh._instances.buffer })
        }
      }
    },
    _reorder() {
      orderDirty = true
      for (let v of views) v.orderDirty = true
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
  // The first light rewrite seeds the (empty) light set and the shadow
  // slots - placeholders, no casts - so receivers draw plain from the
  // first frame.
  lightsDirty = true
  hooks._schedule()

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
  // every transform above it, design-size fits included), so a leaf laid out at
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
    depthTexture: depthMode === "texture" ? depthTexture(texture) : null,
    root,
    setCamera(update) {
      updateCamera(camera, update)
      hooks._schedule()
    },
    setSize(w, h) {
      if (disposed || (w === width && h === height)) return
      width = w
      height = h
      setTargetSize(texture, w, h)
      camera.dirty = true
      hooks._schedule()
    },
    setParams(params) {
      if (disposed) return
      Object.assign(sceneParams, params)
      setTargetParams(texture, params)
      for (let v of views) {
        // A view's own names (view.setParams, its fog) win over the
        // scene-wide fan-out.
        let fanned = v.ownNames.size === 0 ? params : withoutNames(params, v.ownNames)
        if (fanned !== params && Object.keys(fanned).length === 0) continue
        setTargetParams(v.texture, fanned)
      }
    },
    setFog(fog) {
      scene.setParams(fogParams(fog))
    },
    setLayers(mask) {
      checkMask(mask, "scene.setLayers")
      if (disposed || sceneMask === mask) return
      sceneMask = mask
      for (let mesh of meshes) {
        if ((mesh.layers & mask) !== 0) attachScene(mesh)
        else detachScene(mesh)
      }
      // Shadow views follow the scene's mask: what the scene cannot see
      // must not darken it.
      for (let v of views) {
        if (v.shadowFilter === null) continue
        v.mask = mask
        for (let mesh of meshes) {
          if ((mesh.layers & mask) !== 0) attachView(v, mesh)
          else detachView(v, mesh)
        }
      }
      hooks._schedule()
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
      // Before the first mesh ENTRY - a layers-masked mesh has none.
      let entry = addDraw(texture, built.pipeline, null, { vertexCount: 3, before: meshes.find(m => m._entry !== null)?._entry ?? undefined })
      background = { entry, pipeline: built.pipeline, program: built.program }
    },
    project(point) {
      ensureCamera(camera, width, height)
      transformPoint(clip, camera.viewProj, point)
      let w = clip[3]
      if (w < 1e-6) return null
      // perspective() bakes the y-down clip flip, so NDC maps straight to
      // top-left-origin pixels with no negation here.
      let x = ((clip[0] / w) * 0.5 + 0.5) * width
      let y = ((clip[1] / w) * 0.5 + 0.5) * height
      if (camera.ortho !== null) {
        // An orthographic clip w is 1 everywhere (every point projects, the
        // divides above are no-ops) and carries no depth, so `w` reports
        // the camera-forward distance off the view row instead - the same
        // meaning as the perspective clip w, and unproject's exact input.
        let v = camera.view
        w = -(v[2] * point[0] + v[6] * point[1] + v[10] * point[2] + v[14])
      }
      return { x, y, w }
    },
    viewProj(out) {
      ensureCamera(camera, width, height)
      return copy(out ?? mat4(), camera.viewProj)
    },
    pick(x, y) {
      pixelRay(x, y)
      return scene.raycast(pickOrigin, pickDir)
    },
    unproject(x, y, w, out = [0, 0, 0]) {
      pixelRay(x, y)
      out[0] = pickOrigin[0] + w * pickDir[0]
      out[1] = pickOrigin[1] + w * pickDir[1]
      out[2] = pickOrigin[2] + w * pickDir[2]
      return out
    },
    screenRay(x, y) {
      pixelRay(x, y)
      return {
        origin: [pickOrigin[0], pickOrigin[1], pickOrigin[2]],
        direction: [pickDir[0], pickDir[1], pickDir[2]],
      }
    },
    raycast(origin, direction, rayOpts) {
      // Flush pending writes: picking sees the tree as the app just wrote
      // it, the same immediacy contract as lookAt()/project(). (The queued
      // microtask still runs and finds nothing dirty - harmless.)
      if (scheduled) sync()
      if (disposed) return []
      let mask = rayOpts?.layers !== undefined ? checkMask(rayOpts.layers, "raycast") : sceneMask
      let include = rayOpts?.meshes !== undefined ? new Set(rayOpts.meshes) : null
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
        // A mesh the query mask excludes is skipped like an invisible one;
        // the mask defaults to the scene's, so an undrawn layer needs an
        // explicit opts.layers to report.
        if ((mesh.layers & mask) === 0) continue
        if (include !== null && !include.has(mesh)) continue
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
    createView(vopts) {
      if (disposed) throw new Error("createView: the scene is disposed")
      let v = makeView(vopts, null)
      return {
        texture: v.texture,
        depthTexture: vopts.depth === "texture" && vopts.into === undefined ? depthTexture(v.texture) : null,
        setCamera(update) {
          updateCamera(v.camera, update)
          hooks._schedule()
        },
        setSize(w, h) {
          if (v.disposed || (w === v.width && h === v.height)) return
          v.width = w
          v.height = h
          setTargetSize(v.texture, w, h)
          v.camera.dirty = true
          hooks._schedule()
        },
        setRect(rect) {
          if (v.disposed) return
          setTargetRect(v.texture, rect)
          if (rect.width === v.width && rect.height === v.height) return
          v.width = rect.width
          v.height = rect.height
          v.camera.dirty = true
          hooks._schedule()
        },
        setParams(params) {
          if (v.disposed) return
          for (let k of Object.keys(params)) v.ownNames.add(k)
          setTargetParams(v.texture, params)
        },
        setLayers(mask) {
          checkMask(mask, "view.setLayers")
          if (v.disposed || v.mask === mask) return
          v.mask = mask
          for (let mesh of meshes) {
            if ((mesh.layers & mask) !== 0) attachView(v, mesh)
            else detachView(v, mesh)
          }
          hooks._schedule()
        },
        dispose() {
          disposeView(v)
        },
      }
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
      // targets still exist; afterwards their groups are gone.
      spatial.flush()
      destroyTexture(texture)
      shadows.clear()
      for (let v of views.slice()) disposeView(v)
      if (shadowAtlas !== null) {
        destroyTexture(shadowAtlas.texture)
        shadowAtlas = null
      }
      if (background !== null) {
        // The entry died with the target; the pipeline and program are the
        // scene's own (unlike shared material pipelines), so they go too.
        destroyRenderPipeline(background.pipeline)
        destroyProgram(background.program)
        background = null
      }
    },
  }
  // The fog set starts at "none" (uFogInv and uFogDensity 0 are factor 0,
  // uFogHeightFalloff 0 is no attenuation) so every material that declares
  // it has coverage from the first frame; a target tolerates the names
  // when nothing declares them.
  scene.setParams({ uFogColor: [0, 0, 0], uFogNear: 0, uFogInv: 0, uFogDensity: 0, uFogHeight: 0, uFogHeightFalloff: 0 })
  if (opts?.fog !== undefined) scene.setFog(opts.fog)
  if (opts?.background !== undefined) scene.setBackground(opts.background)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => scene.dispose())
  return scene
}
