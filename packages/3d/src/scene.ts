// The retained scene: plain objects, no signals - the hot path (a moved
// node) is flat imperative code, and reactivity stays at the component
// boundary (components/). The transform hierarchy itself lives in the
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
//
// This file is the scene half: createScene and everything one scene
// instance owns (targets, views, sync, picking). The node layer it
// renders - the graph, transforms, meshes, lights - lives in node.ts,
// mesh.ts and light.ts, talking back through the SceneHooks seam
// (node.ts); the camera record is camera.ts, and the per-scene shadow
// and pointer subsystems are scene-shadows.ts / scene-pointer.ts,
// built here with the scene's seams as their deps.

import { addDraw, createCubeDrawTarget, createDrawTarget, depthTexture, destroyProgram, destroyRenderPipeline, destroyTexture, removeDraw, renderTarget, setDrawBuffers, setDrawParams, setDrawRange, setDrawTextures, setTargetParams, setTargetRect, setTargetSize, setTargetTextures } from "@solidrt/core/gpu"
import * as spatial from "flux:spatial"
import type { BindDrawOptions, Impact as CoreImpact, NodeId, QueryFilter } from "flux:spatial"
import type { BufferId, DrawId, FilterMode, ProgramId, RenderPipelineId, ShaderParams, TextureBindings, TextureId, WrapMode } from "@solidrt/core/gpu"
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
import { copy, mat4, transformPoint } from "./math.ts"
import { linearColor, premultipliedColor } from "./color.ts"
import type { Mat4, Quat, Vec3, Vec4 } from "./math.ts"
import { MAX_LIGHTS, MAX_SHADOW_MAPS } from "./glsl.ts"
import { cameraParams, cameraState, ensureCamera, makeCamera, updateCamera } from "./camera.ts"
import type { Camera, CameraState, CameraUpdate } from "./camera.ts"
import { makeShadowSystem } from "./scene-shadows.ts"
import { makePointerInput } from "./scene-pointer.ts"
import { geometryKey, geometryTopology, validateGeometry } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import { acquireGeometryBuffers, releaseGeometryBuffers } from "./geometry-gpu.ts"
import { backgroundPipeline, missingAttributes, SKYBOX_FRAGMENT } from "./material.ts"
import { disposeResolve, makeResolve, replaceResolve, resizeResolve, setResolveBloom } from "./resolve.ts"
import type { BloomOptions, ResolveInput, ResolveOptions, ResolveRecord } from "./resolve.ts"
import { createEnvironmentPlaceholder, createPrefilter, bufferFormat } from "./environment.ts"
import type { Prefilter } from "./environment.ts"
import type { Material } from "./material.ts"
import { activateMorph, fillTransform, freeLeaving, leaveScene, makeNode, setTransition } from "./node.ts"
import type { SceneHooks, SceneNode, ScenePointerListener } from "./node.ts"
import { checkInstancePairing, checkMask, instanceBinding, localBounds, publishRecords } from "./mesh.ts"
import type { InstancedMesh, InstanceNode, Mesh } from "./mesh.ts"
import type { CastingLight, Light } from "./light.ts"

const IDENTITY = mat4()
const RESOLVED = Promise.resolve()

// raycast()'s FFI carriers; values are copied at the boundary, so one of
// each serves every call.
let rayOriginScratch = new Float32Array(3)
let rayDirScratch = new Float32Array(3)
// overlap()/sweep()'s carriers: the volume in flux:spatial's packed
// layout, one array per kind (capsule: a, b, radius; box: center, half
// extents, rotation), and the sweep's motion.
let capsuleScratch = new Float32Array(7)
let boxScratch = new Float32Array(10)
let motionScratch = new Float32Array(3)
const IDENTITY_QUAT: Quat = [0, 0, 0, 1]

/** Pack a volume into its kind's scratch array and name the kind; a
 * sphere is a capsule whose segment is one point. */
function packVolume(volume: Volume, site: string): "capsule" | "box" {
  if ("halfExtents" in volume) {
    let s = boxScratch
    let h = volume.halfExtents
    if (!(h[0] >= 0 && h[1] >= 0 && h[2] >= 0)) throw new Error(site + ": halfExtents must be >= 0, got " + h)
    let q = volume.rotation ?? IDENTITY_QUAT
    s[0] = volume.center[0]
    s[1] = volume.center[1]
    s[2] = volume.center[2]
    s[3] = h[0]
    s[4] = h[1]
    s[5] = h[2]
    s[6] = q[0]
    s[7] = q[1]
    s[8] = q[2]
    s[9] = q[3]
    return "box"
  }
  if (!(volume.radius >= 0)) throw new Error(site + ": radius must be >= 0, got " + volume.radius)
  let s = capsuleScratch
  let a = "center" in volume ? volume.center : volume.a
  let b = "center" in volume ? volume.center : volume.b
  s[0] = a[0]
  s[1] = a[1]
  s[2] = a[2]
  s[3] = b[0]
  s[4] = b[1]
  s[5] = b[2]
  s[6] = volume.radius
  return "capsule"
}

// pick()'s camera-ray scratch.
let pickDir: Vec3 = [0, 0, 0]

/** One picking intersection, Three's intersect result: the mesh, the
 * camera-ray distance in world units, the world-space point, and for a
 * triangle hit (every ordinary mesh - the test is per triangle, so a ray
 * through a knot's hole misses) the world-space geometric `normal` facing
 * the ray, the triangle index `face` and the interpolated texture `uv`.
 * An instanced mesh is picked per instance, by the geometry's triangles
 * placed as that instance (`instance` names it, Three's instanceId). A
 * record mesh is picked by the faces of its explicit population box and
 * a sprite by those of a unit box around its center, so their hits carry
 * the struck face's `normal` and no `face`/`uv`. */
export type Hit = {
  mesh: Mesh
  /** The instance struck, for an instanced mesh. */
  instance?: InstanceNode
  distance: number
  point: Vec3
  normal: Vec3
  face?: number
  uv?: [number, number]
}

/** A pixel's camera ray (screenRay): `direction` is NOT normalized - its
 * camera-forward component is 1, so `origin + w * direction` is the world
 * point at camera-forward distance `w` (unproject's mapping). */
export type ScreenRay = { origin: Vec3; direction: Vec3 }

/** Filters for one raycast/overlap/sweep query. */
export type QueryOptions = {
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

/** A query volume for overlap()/sweep(): a sphere, a capsule (the radius
 * swept along the segment a-b) or an oriented box (half extents along
 * the axes of `rotation`, identity when absent) - Unity's Overlap/Cast
 * trio, Godot's Sphere/Capsule/BoxShape3D. */
export type Sphere = { center: Vec3; radius: number }
export type Capsule = { a: Vec3; b: Vec3; radius: number }
export type OrientedBox = { center: Vec3; halfExtents: Vec3; rotation?: Quat }
export type Volume = Sphere | Capsule | OrientedBox

/** One mesh an overlap() volume touches: its deepest contact - the point
 * on the mesh, the unit direction out of it, and the depth along that
 * direction that clears the contact (Unity's ComputePenetration, Godot's
 * get_rest_info, per mesh). `instance` names the instance touched on an
 * instanced mesh, which reports once per instance. */
export type Overlap = { mesh: Mesh; instance?: InstanceNode; point: Vec3; normal: Vec3; depth: number }

/** One mesh a sweep() volume touches on its way: `time` is the fraction
 * of the motion at first touch (0 for a volume already in contact and
 * moving in), `point` the touch point on the mesh, `normal` the unit
 * normal there facing the volume (Unity's RaycastHit from a cast,
 * Godot's KinematicCollision3D). `instance` as on Overlap. */
export type Impact = { mesh: Mesh; instance?: InstanceNode; time: number; point: Vec3; normal: Vec3 }

export type MoveOptions = QueryOptions & {
  /** The direction floors face (default [0, 1, 0]). */
  up?: Vec3
  /** Largest angle (radians) between a contact normal and `up` that still
   * counts as floor (default 45 degrees). Steeper contacts are walls, and
   * the body slides down them. */
  floorMaxAngle?: number
  /** Most contacts one call slides along (default 6). */
  maxSlides?: number
  /** Gap kept from every surface, world units (default 0.01). */
  skin?: number
  /** How far below the body a floor is pulled to when the motion does not
   * rise (default 0.1; 0 disables): what keeps a walker on a ramp going
   * down, and what decides `floor` at the end of the move - a body that
   * ends higher than this above its floor is airborne, whatever it
   * touched on the way. A rising motion (a jump) never snaps. */
  floorSnap?: number
}

export type MoveResult = {
  /** The displacement the body gets: add it to the body's position. */
  motion: Vec3
  /** The unit normal of the floor the body ends the move on - within
   * `floorSnap` below it, snapped onto - else null (airborne, or on a
   * slope too steep to stand on). With `floorSnap: 0` it is a floor met
   * during the move instead. */
  floor: Vec3 | null
  /** Whether a wall (a contact steeper than a floor and flatter than a
   * ceiling) or a ceiling was met. */
  wall: boolean
  ceiling: boolean
  /** Every contact met, in order, the floor snap's last. */
  hits: Impact[]
}

/** Element handlers wiring a scene's (or a view's) pointer events: spread
 * onto whatever element shows its texture (the built-in `<Scene>` and
 * `<View3d>` leaves wire them automatically). `scene.handlers` expects
 * the leaf laid out at the target size; a split-resolution leaf
 * (supersampling) uses scene.handlersFor. */
export type SceneHandlers = {
  onPointerDown(event: ElementPointerEvent): void
  onPointerMove(event: ElementPointerEvent): void
  onPointerUp(event: ElementPointerEvent): void
  onPointerLeave(event: ElementPointerEvent): void
  onWheel(event: ElementWheelEvent): void
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
  /** Straight [r, g, b], 0..1, sRGB like every color option (decoded to
   * linear light for the mix). */
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
  let color = linearColor(fog.color)
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

/**
 * A skybox: the scene's background sampled from a cube map along each
 * pixel's view ray (setBackground's object form). Three's
 * `scene.background = cubeTexture` with `backgroundIntensity` and
 * `backgroundRotation`; Unity's Skybox/Cubemap `_Exposure` and
 * `_Rotation`; Godot's `background_energy_multiplier` and `sky_rotation`.
 * Under an orthographic camera every pixel looks the same way, so the
 * skybox is one flat color there (a sky needs perspective in every
 * engine).
 */
export type SkyboxOptions = {
  /** A cube map (createCubeTexture, faces in +X, -X, +Y, -Y, +Z, -Z
   * order), sampled as GL defines it: each face is what a lookup in that
   * direction returns. No lookup flip (Godot's and Unity's convention):
   * a Three-style face set (each face as seen from inside) is mirrored
   * per image at load, not in the shader. A 2D texture id throws. */
  cube: TextureId
  /** Multiplier on the sampled color, >= 0; default 1. */
  intensity?: number
  /** Turn about world y in RADIANS (default 0): the sky turns as a node
   * with rotation [0, r, 0] would. Radians like node rotation and Three;
   * Unity's `_Rotation` is degrees. */
  rotation?: number
}

/**
 * The scene's environment: a cube map every `lit` material with
 * `reflectivity` mirrors (scene.setEnvironment). The same three fields
 * as a skybox, and typically the same cube - Three's `scene.environment`
 * with `environmentIntensity` and `environmentRotation`; Unity's
 * environment reflections source, Godot's reflected light from the sky.
 */
export type EnvironmentOptions = {
  /** A cube map (loadEnvironment's baked chain; equirectToCube for an LDR
   * panorama; createCubeTexture with `mipmap: true` for a hand-baked
   * sky), faces in +X, -X, +Y, -Y, +Z, -Z order, looked up like the
   * skybox (no flip). A 2D texture id throws. */
  cube: TextureId
  /** Multiplier on the reflected color, >= 0; default 1. */
  intensity?: number
  /** Turn about world y in RADIANS (default 0), the skybox's convention:
   * the environment turns as a node with rotation [0, r, 0] would. */
  rotation?: number
}

// The uniform turn a rotated cube map is looked up through: the INVERSE
// of the sky's turn, because a lookup along view direction v must find
// the texel that sat at R(-r) v before the sky turned by +r.
function cubeTurn(rotation: number): Mat4 {
  let c = Math.cos(rotation)
  let n = Math.sin(rotation)
  // prettier-ignore
  return [
    c, 0, n, 0,
    0, 1, 0, 0,
    -n, 0, c, 0,
    0, 0, 0, 1,
  ]
}

function checkCubeKnobs(o: { intensity?: number; rotation?: number }, site: string): { intensity: number; rotation: number } {
  let intensity = o.intensity ?? 1
  let rotation = o.rotation ?? 0
  if (!Number.isFinite(intensity) || intensity < 0) throw new Error(site + ": intensity must be a finite number >= 0, got " + intensity)
  if (!Number.isFinite(rotation)) throw new Error(site + ": rotation must be a finite angle in radians, got " + rotation)
  return { intensity, rotation }
}

// The entry params a skybox compiles to.
function skyboxParams(sky: SkyboxOptions, site: string): ShaderParams {
  let k = checkCubeKnobs(sky, site)
  return { uSkyIntensity: k.intensity, uSkyRotation: cubeTurn(k.rotation) }
}

// The shared params an environment compiles to (null = off: uEnvOn 0
// makes every reflective material's term vanish; the set ENVIRONMENT in
// `@solidrt/3d/glsl` declares).
function environmentParams(env: EnvironmentOptions | null): ShaderParams {
  if (env === null) return { uEnvIntensity: 0, uEnvRotation: cubeTurn(0), uEnvOn: 0 }
  let k = checkCubeKnobs(env, "scene.setEnvironment")
  return { uEnvIntensity: k.intensity, uEnvRotation: cubeTurn(k.rotation), uEnvOn: 1 }
}

export type { BloomOptions, ResolveInput, ResolveOptions } from "./resolve.ts"

/** The resolve's tone mapping (setToneMapping): "none" clamps, "aces" is
 * the filmic curve every engine ships (Three's ACESFilmic, Godot's ACES,
 * Unity's ACES), "agx" Blender's AgX (Three, Godot 4.3+) and "neutral"
 * the Khronos PBR Neutral curve (Three, Unity) that keeps product colors
 * where a filmic curve shifts them. */
export type ToneMapping = "none" | "aces" | "agx" | "neutral"

// The uToneMapping value per mode; the RESOLVE set branches on it.
const TONE_MAPPING_CODE: Record<ToneMapping, number> = { none: 0, aces: 1, agx: 2, neutral: 3 }
// The core's draw queues (flux:spatial bindDraw), drawn in this order.
const QUEUE_BACKGROUND = 0
const QUEUE_OPAQUE = 1
const QUEUE_CUTOUT = 2
const QUEUE_TRANSPARENT = 3
let drawQueue = (m: Material): number =>
  m.transparent === true ? QUEUE_TRANSPARENT : m.cutout === true ? QUEUE_CUTOUT : QUEUE_OPAQUE
// A mesh entry's draw sink: what the core writes into it and where it
// sorts, from the material the entry draws with.
let drawBinding = (material: Material, mesh: Mesh, inst: { count: number } | null): BindDrawOptions => ({
  normal: material.normalMatrix === true,
  count: inst !== null ? inst.count : 1,
  fade: material.lodFade === true,
  queue: drawQueue(material),
  renderOrder: mesh.renderOrder,
})

/** The RESOLVE set's params for a resolve pass of your own (an app-owned
 * atlas tiled views render into, resolved through `resolveFragment` from
 * `/glsl`): `uToneMapping` and `uExposure` as the scene's setToneMapping
 * and setExposure would write them - a shader target validates its
 * params strictly, so both are always written. */
export function resolveParams(opts?: { toneMapping?: ToneMapping; exposure?: number }): ShaderParams {
  let mode = opts?.toneMapping ?? "none"
  let code = TONE_MAPPING_CODE[mode]
  if (code === undefined) throw new Error('resolveParams: expected "none", "aces", "agx" or "neutral", got ' + mode)
  let exposure = opts?.exposure ?? 1
  if (!Number.isFinite(exposure) || exposure < 0) throw new Error("resolveParams: expected a finite exposure >= 0, got " + exposure)
  return { uToneMapping: code, uExposure: exposure }
}

export type SceneOptions = {
  /** The buffer's clear: an sRGB color like every color option (decoded
   * to linear light on write), so the backdrop takes the scene's exposure
   * and tone mapping in the resolve exactly as a background does. */
  clearColor?: [number, number, number, number]
  /** The resolve fragment (see setResolve), or a function of the buffer
   * id returning it - the chain over `hdrTexture` is built right there;
   * absent is DEFAULT_RESOLVE. */
  resolve?: ResolveInput
  /** The stock bloom on the resolve; see setBloom. Views follow it
   * unless they carry a `bloom` of their own. */
  bloom?: BloomOptions
  /** Scene-wide fog; see setFog. */
  fog?: FogOptions
  /** Output tone mapping, default "none"; see setToneMapping. */
  toneMapping?: ToneMapping
  /** Output exposure, default 1; see setExposure. */
  exposure?: number
  /** LOD quality knob, default 1: every measured projected size is
   * multiplied by it before the level select, so below 1 the groups hand
   * over to their far levels sooner (Unity's lodBias). Live via
   * scene.setLodBias; views and shadow tiles follow it. */
  lodBias?: number
  /** The scene target's layer mask (bitmask, default 1): the scene draws
   * the meshes whose `layers` intersect it. Live via scene.setLayers. */
  layers?: number
  /** The scene target's depth storage: true (default) for a buffer,
   * "texture" for a sampleable one exposed as `scene.depthTexture` - the
   * input for a depth-reading post effect in `output` (depth fog, SSAO,
   * depth of field). Not with `samples` (the engine has no multisampled
   * sampleable depth): render larger and display smaller instead. */
  depth?: true | "texture"
  /** The background drawn behind the meshes, inside the scene's own pass:
   * fragment GLSL or a skybox - see setBackground. */
  background?: string | SkyboxOptions
  /** The cube map reflective materials mirror; see setEnvironment. */
  environment?: EnvironmentOptions
  label?: string
  /** `stagger` (ms) on the scene's root node - `setTransition(scene.root,
   * { stagger })` at creation: every node added straight under the root
   * that enters or leaves in one frame is spaced by `index * stagger`,
   * the whole-scene form of a Group's stagger (nested groups declaring
   * their own win for what is under them). */
  stagger?: number
  /** `autoFree: false` opts out of owner-scoped auto-dispose (then call dispose yourself). */
  autoFree?: boolean
  filter?: FilterMode
  wrap?: WrapMode
  /** Multisample count of the target (1, 2, 4 or 8; default 1). Storage-only
   * anti-aliasing of mesh edges; see createDrawTarget. */
  samples?: 1 | 2 | 4 | 8
  /** Where a per-scene budget error goes: the light cap and the
   * shadow-slot budget are checked over the settled set at the sync and
   * reported once per change of the set, the scene rendering on with
   * what fits. Without a handler the sync throws it, uncaught (a
   * microtask): the `<Scene>` component hands one in and rethrows inside
   * the tree, so the app's error boundary shows it. */
  onError?: (error: Error) => void
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
  /** The view buffer's clear, an sRGB color like the scene's. */
  clearColor?: [number, number, number, number]
  samples?: 1 | 2 | 4 | 8
  filter?: FilterMode
  wrap?: WrapMode
  label?: string
  /** The view's resolve fragment (see Scene.setResolve), or a function
   * of the view's buffer id; not with `into` (a tile renders linear into
   * the atlas, which the app resolves once). */
  resolve?: ResolveInput
  /** The view's own bloom: BloomOptions overrides the scene's, null turns
   * it off in this view (a clean minimap). Absent follows the scene's
   * setBloom, like `fog`. Not with `into`. */
  bloom?: BloomOptions | null
  /** Render into a rectangle of this draw target (an app-owned atlas)
   * instead of a target of the view's own: every view into one atlas
   * costs ONE pass. `x`/`y` (top-left origin, default 0) place the tile;
   * display it with `<d-texture src={atlas} srcX srcY srcW srcH>`. The
   * atlas carries depth, samples and the format; `view.texture` is then
   * the tile's id (a draw target, not a texture), `view.depthTexture` and
   * `view.hdrTexture` are null, and the tile holds LINEAR light like every
   * buffer - resolve the atlas once with a DEFAULT_RESOLVE pass over it
   * (`uScene` bound to the atlas) and display that. */
  into?: TextureId
  x?: number
  y?: number
}

/** A second rendering of a scene from its own camera; see Scene.createView. */
export type ViewHandle = {
  /** The view's output, an ordinary texture id: its resolve (or, tiled,
   * the tile's id). */
  texture: TextureId
  /** The view's buffer, the linear light its resolve reads (sampler-only;
   * null for a tiled view). */
  hdrTexture: TextureId | null
  /** The view buffer's depth as a sampler-only texture id when created
   * with `depth: "texture"` (the shadow-map input), else null. */
  depthTexture: TextureId | null
  /** Replace the view's resolve fragment; exactly Scene.setResolve. Throws
   * on a tiled view. */
  setResolve(resolve: ResolveInput): void
  /** The view's own bloom from now on (null = off): the scene's setBloom
   * skips this view afterwards. Throws on a tiled view. */
  setBloom(bloom: BloomOptions | null): void
  /** Partial camera update, exactly scene.setCamera. */
  setCamera(update: CameraUpdate): void
  /** Current camera state, exactly scene.camera. */
  camera(): CameraState
  setSize(width: number, height: number): void
  /** The view's target size in pixels, as setSize/setRect last left it. */
  size(): { width: number; height: number }
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
  /** The level a LOD group draws in this view (scene.lodLevel through
   * the view's camera). */
  lodLevel(group: SceneNode): number | null
  /** The camera ray through one of the VIEW's pixels, hits nearest first:
   * scene.pick through this view's camera and size, so a mesh under a
   * minimap is picked where the minimap shows it (the scene's query
   * filters apply: visibility, the view's own layer mask is not one). */
  pick(x: number, y: number): Hit[]
  /** Element pointer handlers for the element showing `texture`: the
   * scene's dispatch with THIS VIEW as the root of the walk - nodes get
   * their ordinary handlers, picked through the view's camera, the
   * view's `listen` is the last stop, `x`/`y` are view pixels. The
   * `<View3d>` leaf carries them; an `output` leaf spreads them itself. */
  handlers: SceneHandlers
  /** handlers for a leaf laid out at another size; exactly
   * scene.handlersFor. */
  handlersFor(layout: () => { width: number; height: number }): SceneHandlers
  /** A listener at this view's root; exactly scene.listen. */
  listen(listener: ScenePointerListener): () => void
  /** Destroy the view's target (its entries die with it). Idempotent;
   * views also die with their scene. */
  dispose(): void
}

/** A dynamic environment rendered from a point in the scene; see
 * Scene.createReflectionProbe. */
export type ReflectionProbeOptions = {
  /** Where the probe looks out from, world space (live via setPosition). */
  position: Vec3
  /** Face edge in texels; default 128 (Unity's probe default). */
  size?: number
  /** The face cameras' near and far planes (defaults 0.1 and 100). */
  near?: number
  far?: number
  /** Layer mask (bitmask, default 1): which meshes the probe sees. */
  layers?: number
  clearColor?: [number, number, number, number]
  /** Convolve the faces into the roughness chain after each update
   * (default true: Unity's and Godot's probes; `standard` blurs it by
   * roughness like a baked environment). `false` keeps the sharp cube
   * alone - Three's CubeCamera - at a sixth of the passes. */
  prefilter?: boolean
  label?: string
}

export type ReflectionProbe = {
  /** The cube map: what `environment={{ cube }}` and `background={{
   * cube }}` take. The prefiltered chain (roughness 0 sharp at level 0,
   * blurred below, the same rule as a baked .srte), or with `prefilter:
   * false` the sharp faces alone - then `standard` reflects it at its
   * roughness 0 look whatever its roughness. */
  cube: TextureId
  setPosition(position: Vec3): void
  /** Render the six faces now - six passes over the scene's draw list,
   * seeing the meshes where the last frame's flush placed them - then
   * (unless `prefilter: false`) the chain: one small pass per face per
   * level. Call it when the surroundings changed (every frame for a
   * moving scene, once for a still one). The probe's own cube is never
   * sampled by the faces it renders (a black environment stands in): one
   * bounce. */
  update(): void
  /** Destroy the cube (idempotent; probes also die with the scene). A
   * scene whose `environment` or `background` names this cube drops it
   * first (setEnvironment(null), setBackground(null)), so nothing samples
   * a destroyed texture: set another when the probe's owner leaves. */
  dispose(): void
}

// The face cameras of a reflection probe, GL order (+X, -X, +Y, -Y, +Z,
// -Z): the direction each looks along and its up vector, chosen so that
// through the x-mirrored projection (Camera.mirror) each face lands in
// GL's own cube layout - plain world-up cameras, unlike Three's rolled
// CubeCamera set, because target images here are stored top-down.
const PROBE_FACE_DIRECTION: Vec3[] = [
  [1, 0, 0],
  [-1, 0, 0],
  [0, 1, 0],
  [0, -1, 0],
  [0, 0, 1],
  [0, 0, -1],
]
const PROBE_FACE_UP: Vec3[] = [
  [0, 1, 0],
  [0, 1, 0],
  [0, 0, -1],
  [0, 0, 1],
  [0, 1, 0],
  [0, 1, 0],
]
// Default face edge of a reflection probe (Unity's default resolution),
// and of a baked background (the bake tool's default).
const PROBE_SIZE = 128
// A cube face spans a quarter turn.
const PROBE_FOV = 90
export type Scene = {
  /** The scene's output: an ordinary texture id (`<texture src>`) - the
   * resolve of the buffer, encoded display pixels. */
  texture: TextureId
  /** The scene's BUFFER: the target the meshes draw into, premultiplied
   * linear light (half float where the device renders it, see
   * bufferFormat), sampler-only - the input of a radiance-reading pass (a
   * bloom chain) whose result a custom resolve adds back in. */
  hdrTexture: TextureId
  /** The buffer's depth as a sampler-only texture id when created with
   * `depth: "texture"`, else null. */
  depthTexture: TextureId | null
  /** The tree root; add(scene.root, node) attaches top-level nodes. */
  root: SceneNode
  /** Partial camera update; absent keys keep their current value. */
  setCamera(update: CameraUpdate): void
  /** The camera as the next frame draws it: setCamera's own fields, a
   * fresh snapshot per call (arrays are copies - mutate freely, write
   * back through setCamera). Reflects a pending setCamera immediately. */
  camera(): CameraState
  setSize(width: number, height: number): void
  /** The target size in pixels, as setSize last left it (a fill-mode
   * `<Scene>` sets it from its layout box, so before the first layout
   * this is the creation size). The aspect a camera control's fit()
   * frames against. */
  size(): { width: number; height: number }
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
   * Set, replace, or remove (null) the scene's background, drawn as the
   * FIRST entry of the scene's own pass - one target, no second texture
   * layer, no separate resize plumbing. It draws with depth off before
   * every mesh and covers the whole target, so the clearColor stops being
   * visible. Two forms:
   *
   * Fragment GLSL. The fragment gets the shader-target contract (vUV 0..1
   * top-left origin, iResolution, fragColor; no `#version` line means the
   * standard preamble), so a source written for createShaderTexture ports
   * verbatim, PLUS `vRay`: the world-space view ray through the pixel
   * (unnormalized; normalize it), which makes a directional sky - a
   * horizon gradient, a sun disc, stars - a few lines of fragment code.
   * The background is an ordinary scene entry, so it may also declare
   * `uniform vec3 uCamPos` (the ray's origin) and any name written through
   * scene.setParams (an app clock for an animated sky).
   *
   * A skybox (SkyboxOptions): `{ cube, intensity?, rotation? }` samples a
   * cube map along the same ray. Replacing one skybox with another
   * rewrites the entry's params and textures without recompiling, so
   * `rotation` can animate from the reactive prop.
   *
   * Three's `scene.background = color` is `clearColor` here; a 2D image
   * form can arrive later as a non-breaking widening.
   */
  setBackground(source: string | SkyboxOptions | null): void
  /**
   * Set, replace, or remove (null) the scene's environment: the cube map
   * every `lit` material created with `reflectivity` mirrors, blurred by
   * its `shininess`. Scene-level like Three's `scene.environment`,
   * Unity's environment reflections and Godot's sky-lit reflections: one
   * cube bound on every target the scene draws into, one shared-params
   * write for intensity and rotation, however many meshes reflect (a
   * custom material composes ENVIRONMENT from `@solidrt/3d/glsl`). No
   * per-material map. Typically the skybox's own cube, turned with it.
   * Not an ambient light source yet: the hemisphere light stays the
   * ambient term.
   */
  setEnvironment(env: EnvironmentOptions | null): void
  /**
   * The resolve's tone mapping, applied once per pixel to the buffer -
   * every material, the background, the clearColor alike: "none"
   * (default) clamps the linear result, "aces" compresses highlights on
   * the filmic curve. One shared-params write (`uToneMapping`, the
   * RESOLVE set in `@solidrt/3d/glsl` declares), like Three's
   * renderer.toneMapping and Godot's Environment tonemap.
   */
  setToneMapping(mode: ToneMapping): void
  /**
   * The resolve's exposure (default 1): the buffer is scaled by it before
   * tone mapping, so a scene lit in physical-ish units is brought into
   * range here rather than by dimming every light. One shared-params
   * write (`uExposure`), like Three's toneMappingExposure.
   */
  setExposure(exposure: number): void
  /**
   * Replace the scene's RESOLVE: the one full-screen pass that turns the
   * buffer (`hdrTexture`, premultiplied linear light) into `texture`
   * (display pixels) - the place a post effect that needs radiance
   * lives (Godot's glow sits in its tonemap pass; Three's bloom reads the
   * HDR render before OutputPass). Fragment GLSL with the shader-target
   * contract (vUV 0..1 top-left origin, iResolution, fragColor; no
   * `#version` line means the standard preamble), `uniform sampler2D
   * uScene` (the buffer) and the RESOLVE set declared: end with
   * `fragColor = resolveColor(rgb, alpha)` for the scene's exposure, tone
   * mapping and encode. The object form binds extra textures the source
   * declares (a bloom chain's result). Any name written through
   * scene.setParams is readable. The default is DEFAULT_RESOLVE (one
   * sample through resolveColor). A textures-only change rebinds in
   * place; a source change recompiles. A function form receives the
   * buffer id, like the option.
   */
  setResolve(resolve: ResolveInput): void
  /**
   * The stock bloom, or null for none (default): radiance above
   * `threshold` (default 1) blurred over `radius` rounds (default 2) at a
   * quarter of the target's size and added back at `intensity` (default
   * 0.5) - Godot's glow, Unity's Bloom, Three's UnrealBloomPass. A chain
   * of small passes per resolving target, re-rendered when the buffer
   * is; the default resolve composes it, and a custom one does by
   * declaring `uBloom` / `uBloomIntensity` (start from BLOOM_RESOLVE in
   * `@solidrt/3d/glsl`). Fans out to every view that has not set a
   * bloom of its own, like setFog.
   */
  setBloom(bloom: BloomOptions | null): void
  /** The LOD quality knob (SceneOptions.lodBias): a multiplier on every
   * measured projected size, for the scene, its views and its shadow
   * tiles. Below 1 switches to far levels sooner. */
  setLodBias(bias: number): void
  /** The level a LOD group draws in the scene's own render (the thing to
   * read while tuning thresholds): the index into its levels, nearest
   * first, `levels.length` when culled past the last one, null before
   * the scene measured it. Views read their own through view.lodLevel;
   * an InstancedLod picks per instance, so this is its group's own
   * pick, not any instance's. Throws for a node with no levels. */
  lodLevel(group: SceneNode): number | null
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
   * mesh or sprite is tested by its box's faces (a `normal`, no
   * `face`/`uv`). Broadphase runs over a BVH kept in step
   * by the sync walk, and a large geometry's triangles are BVH-indexed
   * too (built by the first ray that reaches it), so merged static
   * geometry stays cheap to query. Reflects pending setTransform/add
   * writes immediately (the sync is flushed).
   */
  pick(x: number, y: number): Hit[]
  /** pick()'s world-space half: the same query along an arbitrary ray.
   * `direction` need not be normalized; distances are world units. */
  raycast(origin: Vec3, direction: Vec3, opts?: QueryOptions): Hit[]
  /**
   * Every mesh the volume touches (Unity's OverlapSphere/Box/Capsule,
   * Godot's intersect_shape), each with its deepest contact. Tested per
   * triangle in world space against the same shapes raycast() uses, so
   * an undrawn collider layer answers through `opts.layers`, and any
   * transform holds; an instanced mesh or sprite counts by its box.
   * Surfaces, not solids: a volume wholly inside a closed mesh with no
   * triangle in reach touches nothing. Unordered; reflects pending writes
   * like raycast().
   */
  overlap(volume: Volume, opts?: QueryOptions): Overlap[]
  /**
   * The volume moved by `motion`: every mesh it touches on the way, at
   * its first touch, earliest first (Unity's SphereCast/CapsuleCast/
   * BoxCast, Godot's cast_motion). A volume already in contact reports
   * time 0 while the motion closes in, and nothing while it leaves or
   * slides along the contact - what lets a move-and-slide proceed along
   * a wall. A zero motion touches nothing. Same testing as overlap().
   */
  sweep(volume: Volume, motion: Vec3, opts?: QueryOptions): Impact[]
  /**
   * Move a body `motion` through the scene's colliders, sliding along
   * what it hits (Godot's move_and_slide, Unity's CharacterController.Move):
   * a capsule for a character, a sphere for a ball, a box for a crate.
   * The body first pushes out of anything it starts inside, then sweeps
   * and slides up to `maxSlides` times, then, unless the motion rises,
   * snaps down onto a floor within `floorSnap` - the floor it reports is
   * the one it ends on. One core call per body per frame; the loop's
   * sweeps and overlaps never leave the core. Colliders are whatever
   * `opts` selects (`layers`/`meshes`, as for sweep). Gravity is the
   * caller's: fold the fall into `motion` each frame, zero it while
   * `floor` is set (the snap keeps reporting the floor while the body
   * stands still); a walkable floor absorbs the vertical part.
   */
  moveAndSlide(volume: Volume, motion: Vec3, opts?: MoveOptions): MoveResult
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
   * Element pointer handlers driving the node event fields
   * (onPointerDown/Move/Up/Enter/Leave/Wheel/Tap on nodes) and the
   * scene's listeners: spread onto the element that shows
   * `scene.texture`. The `<Scene>` component's built-in leaf carries them
   * automatically; with `output` (or imperative use), spread them
   * yourself: `<texture src={scene.texture} {...scene.handlers} />`.
   * The element event model one tree deeper, with the scene as the root
   * of the walk: the nearest hit is the target (the struck instance of an
   * instanced mesh, else the mesh), down/move/up/wheel bubble through its
   * ancestors and end at the scene's listeners (`listen`); over empty
   * space the walk is the scene alone. stopPropagation stops the walk,
   * and a stopped down claims the whole press. Capture is per pointer to
   * the press target, the scene included: a drag keeps delivering to what
   * it pressed, with `point`/`distance` null while the ray misses it.
   * Enter/leave pair on the struck node alone, on hover changes. Taps are
   * synthesized (a same-target release within the slop, alone for the
   * press, `tapCount` for repeats). Hover reacts to pointer MOTION - a
   * mesh animating under a still pointer fires nothing until the pointer
   * moves (the element hit-test has the same limit).
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
   * Add a listener at the root of the pointer walk (the `<Scene>`
   * component's onPointerDown/Move/Up, onWheel and onTap props do this):
   * it sees every event the node chain lets through, hit or miss -
   * `event.mesh` is the hit it bubbled from, null over empty space - with
   * `x`/`y` in scene pixels. Listeners all run, in registration order
   * (the root is the last stop, nothing is left to claim); a stopped DOWN
   * claims the whole press for the chain, so that pointer's move, up and
   * tap never arrive. Returns the remover. A pointer feed listens here
   * through feedPointer, so a camera control bound to the feed respects a
   * mesh's claim.
   */
  listen(listener: ScenePointerListener): () => void
  /**
   * A second rendering of this scene: its own draw target and camera,
   * the same meshes and lights. Each mesh gets one entry in the view's
   * target, bound as one more draw sink of the mesh's core node, so a
   * move feeds every target from the one flush and the app writes
   * nothing per view. Views share the scene's geometry buffers and
   * (unless `overrideMaterial`) its materials; the light set and
   * scene.setParams names fan out to every view, view.setParams is the
   * view's own channel. A view's backdrop is its clearColor (the scene's
   * background draws on probes, not on views), and a view has no picking
   * or pointer events. Views die with the scene; `view.dispose()` drops
   * one early.
   */
  createView(opts: ViewOptions): ViewHandle
  /**
   * A reflection probe: the scene rendered into a cube map from a point,
   * six faces at 90 degrees - Three's CubeCamera, Unity's and Godot's
   * realtime ReflectionProbe - for the environment a moving object
   * mirrors its surroundings through. A view under the hood (one entry
   * list, the light set and scene params fanned out) that also draws the
   * scene's background behind the meshes, as a probe does in every
   * engine, rendered only by
   * `probe.update()`, which the app calls when the surroundings moved:
   * six scene passes each time, then the GPU prefilter into the roughness
   * chain (unless `prefilter: false`), so `standard` blurs it by
   * roughness exactly as it does a baked environment. Probes die with
   * the scene.
   */
  createReflectionProbe(opts: ReflectionProbeOptions): ReflectionProbe
  /**
   * Bake the background into an environment: a reflection probe at the
   * origin that sees no mesh (layer mask 0), so its six `size` faces
   * (default 128) hold the background alone - the GLSL sky or the skybox,
   * exactly as the scene draws it, LINEAR like every buffer (a sky
   * fragment writes light, never display pixels) - then GGX-prefiltered
   * into the chain
   * `standard` samples by roughness: Godot's sky-to-radiance bake, a
   * procedural sky lighting the scene, or a hi-res LDR skybox reduced to
   * an environment. A snapshot: bake again when the sky changed. Returns
   * the cube for `environment={{ cube }}` - a mipmapped cube draw target,
   * 8-bit linear like a probe, NOT auto-freed (an environment normally
   * lives as long as the app; destroyTexture it otherwise). Throws
   * without a background.
   */
  bakeBackground(size?: number): TextureId
  /** Destroy the target (entries die with it). Idempotent. Material
   * pipelines are shared and survive (app-lifetime, see material.ts);
   * geometry buffers are reference-counted and freed with their last
   * entry (see geometry-gpu.ts). */
  dispose(): void
}

// A material reads attributes by name; the geometry's layout must carry
// every one it declares (the pipeline is built for that layout, so a
// missing channel would have no home) - an error, like the rest of the
// strict entry path. Extra channels are fine.
function checkLayout(material: Material, geometry: Geometry, what: string): void {
  let missing = missingAttributes(material, geometry)
  if (missing.length > 0) {
    throw new Error(
      what + " reads attributes the geometry layout (" + geometryKey(geometry) + ") lacks: " +
        missing.map(a => a.name + " " + a.format).join(", ") +
        " - add the channel with withAttribute()/withColors(), or use a material that does not read it",
    )
  }
}

// The debug name a mesh's draw entries report in the resource inventory
// (`/gpu`): its geometry's label (a model part's name, a generator's) -
// the same name whether the part is a plain mesh or a population - else a
// population's own label, else none.
function drawLabel(mesh: Mesh): string | undefined {
  return mesh.geometry.label ?? mesh._instances?.label
}

// The index range a mesh's entries draw: its own (setDrawRange) or the
// whole index list.
function meshRange(mesh: Mesh): { firstIndex: number; indexCount: number } {
  return mesh._range === null ? { firstIndex: 0, indexCount: mesh.geometry.indices.length } : { firstIndex: mesh._range.first, indexCount: mesh._range.count }
}

// An entry's initial params. The uNormal seed keys off the material flag
// because entry params validate strictly - and a material declaring
// uNormal without using it therefore throws right here, at add().
// The solid uLodFade: a fresh entry draws whole until the core writes a
// band position (a GL uniform reads zero until written, which would
// discard everything).
const LOD_SOLID = [1, 1]

function entrySeed(material: Material, params: ShaderParams | null): ShaderParams {
  let seed: ShaderParams = { uModel: IDENTITY }
  if (material.normalMatrix) seed.uNormal = IDENTITY
  if (material.lodFade) seed.uLodFade = LOD_SOLID
  return { ...seed, ...material.params, ...params }
}

// A morphing entry's bindings: the geometry's packed targets (uMorphs and
// its width) and the weights texture (uMorphWeights): the mesh's owner's
// one row, or the population's row-per-slot texture on an instanced mesh
// (the shader reads row uMorphRow + gl_InstanceID). Only for a material
// whose program declares them (Material.morphed - the mesh's own, a
// morphed shadow stand-in); any other validates its bindings strictly.
// A plain owner's register is bound here if it was not yet (a mesh owner
// nothing wrote before its first morphing entry).
type MorphEntry = { textures: TextureBindings; params: ShaderParams }
function morphEntry(material: Material, mesh: Mesh): MorphEntry | null {
  if (material.morphed !== true) return null
  let packed = mesh._buffers?.morphs
  if (packed === null || packed === undefined) return null
  let weights = morphWeightsTexture(mesh)
  if (weights === null) return null
  return {
    textures: { uMorphs: packed.texture, uMorphWeights: weights },
    params: { uMorphWidth: packed.width, uMorphRow: 0 },
  }
}

// The weights texture a mesh's morphing entries bind, activating a plain
// owner's register on the way; null when nothing owns weights.
function morphWeightsTexture(mesh: Mesh): TextureId | null {
  let inst = mesh._instances
  if (inst !== null) return inst.morph?.texture ?? null
  return mesh._morphOwner === null ? null : activateMorph(mesh._morphOwner).texture
}

// An entry's sampler bindings: the material's, the mesh's own (a skin's
// uBones) when the material is the mesh's or a skinned stand-in (a
// skinned shadow variant declares uBones and needs the mesh's palette;
// other override programs may not declare the names, and per-entry
// bindings validate strictly), and the morph bindings when it morphs.
// A morphing material over geometry without targets has nothing to walk
// (and its uMorphs binding nothing to bind): rejected at add(), like a
// layout mismatch. A record mesh has no instance nodes to own weights,
// so its records cannot morph; an instanced mesh morphs per instance.
function checkMorph(material: Material, mesh: Mesh, site: string): void {
  if (material.morphed !== true) return
  if (mesh.geometry.morphs === undefined) throw new Error(site + " morphs (morph: true) but the geometry carries no morph targets")
  let inst = mesh._instances
  if (inst !== null && inst.nodes === null) throw new Error(site + " morphs, but a record mesh has no instance nodes to own weights (createInstancedMesh morphs per instance)")
  if (inst === null && mesh._morphOwner === null) throw new Error(site + " morphs but the mesh has no morph owner")
}

function entryTextures(material: Material, mesh: Mesh, morph: MorphEntry | null): TextureBindings | undefined {
  let textures = material.textures
  if ((material === mesh.material || material.skinned === true) && mesh._textures !== null) textures = { ...textures, ...mesh._textures }
  if (morph !== null) textures = { ...textures, ...morph.textures }
  return textures
}

// A buffer's clear color: the sRGB option decoded to premultiplied linear
// light, what the buffer holds and the resolve tone maps.
function bufferClear(color: [number, number, number, number] | undefined): [number, number, number, number] | undefined {
  if (color === undefined) return undefined
  let c = premultipliedColor(color)
  return [c[0]!, c[1]!, c[2]!, c[3]!]
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
  let label = opts?.label ?? "scene"
  // `texture` is the scene's BUFFER throughout this file: the draw target
  // every entry, sink, sort and param write names. The handle's `texture`
  // is its resolve (below), the displayable id.
  let texture = createDrawTarget(width, height, null, {
    depth: depthMode,
    format: bufferFormat(),
    clearColor: bufferClear(opts?.clearColor),
    samples: opts?.samples,
    label,
    autoFree: false,
  })
  spatial.setDrawSort(texture, true)
  let resolve = makeResolve(texture, width, height, opts?.resolve, { filter: opts?.filter, wrap: opts?.wrap }, label)
  let disposed = false
  let scheduled = false

  // Picking: the index and the narrowphase live in the spatial core; this
  // map turns a hit's core node back into the mesh, or the instance of an
  // instanced mesh (each instance node is its own leaf).
  let byNode = new Map<NodeId, Mesh | InstanceNode>()
  // The filter raycast/overlap/sweep hand the core: the scene root (the
  // arena is shared with other scenes and 2d layers), the query mask (the
  // scene's by default, so an undrawn layer needs an explicit opts.layers
  // to report) and, for an include-list, the meshes' nodes - an instanced
  // mesh's instances each being a leaf of their own.
  // One filter object per scene, refilled per query (the bindings read it
  // synchronously), so a query allocates nothing but its hits.
  let filter: QueryFilter = {}
  // `target` names the draw target whose LOD levels the query sees: the
  // scene's for its own queries, a view's for view.pick.
  let queryFilter = (qopts: QueryOptions | undefined, site: string, target: TextureId = texture): QueryFilter => {
    filter.root = root._node!
    filter.target = target
    filter.layers = qopts?.layers !== undefined ? checkMask(qopts.layers, site) : sceneMask
    if (qopts?.meshes === undefined) {
      filter.nodes = undefined
    } else {
      let nodes: NodeId[] = []
      for (let mesh of qopts.meshes) {
        if (mesh._node !== null) nodes.push(mesh._node)
        if (mesh._instances !== null) nodes.push(...instanceGroup(mesh as InstancedMesh))
      }
      filter.nodes = nodes
    }
    return filter
  }
  // A hit's core node back to its mesh (and instance).
  let leafOf = (node: NodeId): { mesh: Mesh; instance: InstanceNode | null } | null => {
    let leaf = byNode.get(node)
    if (leaf === undefined) return null
    let instance = leaf.kind === "instance" ? leaf : null
    let mesh = instance !== null ? instance.mesh : (leaf as Mesh)
    if (mesh === null) return null
    return { mesh, instance }
  }
  let impactOf = (h: CoreImpact): Impact | null => {
    let leaf = leafOf(h.node)
    if (leaf === null) return null
    let impact: Impact = { mesh: leaf.mesh, time: h.time, point: h.point, normal: h.normal }
    if (leaf.instance !== null) impact.instance = leaf.instance
    return impact
  }
  // Nodes whose transform changed since the last sync (deduped by the
  // _moved flag): what the light and transparent-order bookkeeping
  // reacts to, since which meshes moved is the core's knowledge now.
  let moved: SceneNode[] = []
  // Populated meshes with record writes to publish at the next sync.
  let recordsDirty = new Set<Mesh>()
  // Instanced meshes whose cull group - their live instance nodes - changed
  // since the last sync: one setCullGroup per mesh however many entered
  // or left.
  let groupDirty = new Set<InstancedMesh>()
  // bindMatrixRecord's buffer list, refilled per instance.
  let instanceBuffers: BufferId[] = []
  let instanceGroup = (mesh: InstancedMesh): NodeId[] => {
    let members: NodeId[] = []
    for (let n of mesh._instances.nodes.slots) if (n !== null && n._node !== null) members.push(n._node)
    return members
  }

  // Live meshes (those holding a draw entry) in add order; the background
  // entry never joins this list. Draw order is the core's (setDrawSort on
  // every sorted target): each mesh's bind carries its sort key (setDrawKey
  // re-keys on a renderOrder change), and a flush that moved a node,
  // changed a binding or moved a target's view re-keys that target's
  // entries.
  let meshes: Mesh[] = []
  let keyDraw = (mesh: Mesh) => spatial.setDrawKey(mesh._node!, drawQueue(mesh.material), mesh.renderOrder)
  // Attached lights in attach order (= light index); any change to the
  // set, a light's fields, or a light's world matrix rewrites the shared
  // light params at the end of the sync - one write, however many meshes.
  let lights: Light[] = []
  let lightsDirty = false
  // The list changed shape (attach, detach) since sync last checked the
  // light cap.
  let lightSetDirty = false
  // uLightDir and uLightPos are CORE-DRIVEN: each light's slots are
  // shared-slot sinks following the node's world transform - the
  // direction slot (bindDirectionSlot) with -direction as the local
  // vector (the shader wants the vector TOWARD the light; a spot's axis
  // the same way), the position slot (bindPositionSlot) for the
  // positional types - so a light that merely moves costs no JS.
  // This rewrite runs on attach/detach/field changes (and a new view)
  // only and owns the rest: types, colors, cone/falloff params, count,
  // hemisphere. The light set is scene state, so it lands on the scene
  // target and every view target.
  let vecScratch = new Float32Array(3)
  // SPOT_PENUMBRA_MIN floors the cone's inner-outer cosine window so a
  // hard rim (penumbra 0) is still a defined smoothstep edge pair.
  const SPOT_PENUMBRA_MIN = 1e-3
  let writeLights = () => {
    lightsDirty = false
    let sky: Vec3 = [0, 0, 0]
    let ground: Vec3 = [0, 0, 0]
    let types: number[] = []
    let colors: number[] = []
    let coneFalloff: number[] = []
    let bias: number[] = []
    let normalBias: number[] = []
    let radius: number[] = []
    let count = 0
    for (let light of lights) {
      if (light.type === "hemisphere") {
        let k = light.intensity
        let s = linearColor(light.sky)
        let g = linearColor(light.ground)
        sky = [s[0]! * k, s[1]! * k, s[2]! * k]
        ground = [g[0]! * k, g[1]! * k, g[2]! * k]
        continue
      }
      // A light past the cap is not lit (sync reports the set once): its
      // slots would overrun the MAX_LIGHTS arrays.
      if (count >= MAX_LIGHTS) continue
      if (light.type !== "point") {
        vecScratch[0] = -light.direction[0]
        vecScratch[1] = -light.direction[1]
        vecScratch[2] = -light.direction[2]
        spatial.bindDirectionSlot(light._node!, texture, "uLightDir", MAX_LIGHTS * 3, count, vecScratch)
        for (let v of views) spatial.bindDirectionSlot(light._node!, v.texture, "uLightDir", MAX_LIGHTS * 3, count, vecScratch)
      }
      if (light.type !== "directional") {
        spatial.bindPositionSlot(light._node!, texture, "uLightPos", MAX_LIGHTS * 3, count)
        for (let v of views) spatial.bindPositionSlot(light._node!, v.texture, "uLightPos", MAX_LIGHTS * 3, count)
      }
      types.push(light.type === "directional" ? 0 : light.type === "spot" ? 1 : 2)
      let c = linearColor(light.color)
      let k = light.intensity
      colors.push(c[0]! * k, c[1]! * k, c[2]! * k)
      if (light.type === "spot") {
        let pen = Math.max(light.penumbra, SPOT_PENUMBRA_MIN)
        let rad = (light.angle * Math.PI) / 180
        coneFalloff.push(Math.cos(rad * (1 - pen)), Math.cos(rad), light.distance, light.decay)
      } else if (light.type === "point") {
        coneFalloff.push(0, 0, light.distance, light.decay)
      } else {
        coneFalloff.push(0, 0, 0, 0)
      }
      bias.push(light.shadow.bias)
      normalBias.push(light.shadow.normalBias)
      radius.push(light.shadow.radius)
      count++
    }
    for (let i = count; i < MAX_LIGHTS; i++) {
      types.push(0)
      colors.push(0, 0, 0)
      coneFalloff.push(0, 0, 0, 0)
      bias.push(0)
      normalBias.push(0)
      radius.push(1)
    }
    // The shadow set rides with the lights. Per casting light i: its
    // map slots as uShadowFirst[i] + uShadowCount[i] (0 = a receiving
    // material draws that light plain) and its biases; per map slot j its
    // tile of the atlas as uShadowRect[j] in atlas UV (the whole map in
    // an unused slot - never read). The atlas depth binds once as
    // uShadowAtlas, the white placeholder when nothing casts, so every
    // receiving target always has the sampler bound.
    let first: number[] = new Array(MAX_LIGHTS).fill(0)
    let counts: number[] = new Array(MAX_LIGHTS).fill(0)
    let rects: number[] = []
    let atlas = shadowSys.atlas()
    let maps: Record<string, TextureId> = {
      uShadowAtlas: atlas !== null ? depthTexture(atlas.texture) : shadowSys.placeholder(),
      uEnv: environment ?? envPlaceholder,
    }
    shadowSys.forEachShadowSlot((slot, i, c, r) => {
      if (c === 0) first[i] = slot
      counts[i] = counts[i]! + 1
      let a = atlas!
      rects.push(r.x / a.width, r.y / a.height, r.width / a.width, r.height / a.height)
    })
    for (let slot = rects.length / 4; slot < MAX_SHADOW_MAPS; slot++) rects.push(0, 0, 1, 1)
    let params: ShaderParams = {
      uHemiSky: sky,
      uHemiGround: ground,
      uLightCount: count,
      uLightType: types,
      uLightColor: colors,
      uLightParams: coneFalloff,
      uShadowFirst: first,
      uShadowCount: counts,
      uShadowBias: bias,
      uShadowNormalBias: normalBias,
      uShadowRadius: radius,
      uShadowRect: rects,
    }
    receivingTargets(t => {
      setTargetParams(t, params)
      // A probe never samples its own cube while rendering it (a same-pass
      // feedback the engine rejects): its faces see a black environment.
      setTargetTextures(t, ownsEnvironment(t) ? { ...maps, uEnv: envPlaceholder } : maps)
    })
    // A slot change (a light attached, detached or reordered) moves every
    // matrix too: rewrite the whole array once.
    shadowSys.markMatricesDirty()
  }
  // `skybox`: the entry runs the library's cube-map fragment (its params
  // and textures are rewritable in place); false = app GLSL.
  // `sky` is the skybox source (its cube and knobs) for the bake, null
  // for a GLSL background.
  // `entries` is the background's draw entry per target that draws it:
  // the scene's own and every sky view's (a reflection probe). Each is
  // bound to the scene root in the background queue, so the core's sort
  // keeps it first and its visibility follows the root's; the root's
  // matrix is not written (the program has no uModel).
  let background: { pipeline: RenderPipelineId; program: ProgramId; sky: SkyboxOptions | null; entries: Map<TextureId, DrawId> } | null = null
  let attachBackground = (target: TextureId) => {
    if (background === null) return
    let sky = background.sky
    let params = sky === null ? null : skyboxParams(sky, "scene.setBackground")
    let textures = sky === null ? undefined : { uSky: sky.cube }
    let entry = addDraw(target, background.pipeline, params, { vertexCount: 3, textures, instanceCount: 0 })
    background.entries.set(target, entry)
    spatial.bindDraw(root._node!, target, entry, { params: false, queue: QUEUE_BACKGROUND })
  }
  let detachBackground = (target: TextureId, entry: DrawId) => {
    if (root._node !== null) spatial.unbindDraw(root._node, target)
    removeDraw(target, entry)
  }
  // The environment cube bound as uEnv on every receiving target (the
  // light rewrite seeds it on new targets); null binds the placeholder,
  // the scene's own 1x1 black cube.
  let environment: TextureId | null = null
  let envPlaceholder = createEnvironmentPlaceholder(label + "-env-none")
  let camera = makeCamera()
  let clip: Vec4 = [0, 0, 0, 0]
  let pickOrigin: Vec3 = [0, 0, 0]

  // The camera ray through a pixel of `cam` over a tw x th target, into the
  // pickOrigin/pickDir
  // scratches. The direction keeps a camera-forward component of 1
  // (perspective: (cx, cy, -1) in the camera frame; ortho: unit forward),
  // so origin + w * direction is the world point at camera-forward
  // distance w - what unproject() banks on. pick()/screenRay()/
  // unproject() all cast exactly this ray.
  let pixelRayOf = (cam: Camera, tw: number, th: number, x: number, y: number): void => {
    ensureCamera(cam, tw, th)
    let v = cam.view
    let o = cam.ortho
    if (o === null) {
      // The camera-frame ray through the pixel, inverting project()'s
      // mapping: the baked y-down clip flip is why pixel y converts with
      // no negation there and one here.
      let f = 1 / Math.tan(((cam.fov * Math.PI) / 180) / 2)
      let cx = (((x / tw) * 2 - 1) * (tw / th)) / f
      let cy = -((y / th) * 2 - 1) / f
      // The view's upper 3x3 rows are the camera axes, so its transpose
      // carries the camera-space direction (cx, cy, -1) to world.
      pickDir[0] = cx * v[0] + cy * v[1] - v[2]
      pickDir[1] = cx * v[4] + cy * v[5] - v[6]
      pickDir[2] = cx * v[8] + cy * v[9] - v[10]
      pickOrigin[0] = cam.eye[0]
      pickOrigin[1] = cam.eye[1]
      pickOrigin[2] = cam.eye[2]
      return
    }
    // Orthographic: every ray runs along the camera's forward axis; the
    // pixel picks where on the camera plane it starts (top row = top).
    let cx = o.left + (x / tw) * (o.right - o.left)
    let cy = o.top + (y / th) * (o.bottom - o.top)
    pickOrigin[0] = cam.eye[0] + cx * v[0] + cy * v[1]
    pickOrigin[1] = cam.eye[1] + cx * v[4] + cy * v[5]
    pickOrigin[2] = cam.eye[2] + cx * v[8] + cy * v[9]
    pickDir[0] = -v[2]
    pickDir[1] = -v[6]
    pickDir[2] = -v[10]
  }
  let pixelRay = (x: number, y: number): void => pixelRayOf(camera, width, height, x, y)

  // Views (scene.createView): more targets drawing the same meshes from
  // their own cameras. A view holds one entry per mesh in its target,
  // bound as one more draw sink of the mesh's core node, so the flush
  // that writes the scene's entry writes the view's too. Sorted like the
  // scene (view-space keys from the view's own camera); an overridden
  // view is not sorted at all.
  type ViewRecord = {
    /** The view's BUFFER (a 2D draw target, a tile, or a cube). */
    texture: TextureId
    /** The view's resolve, the displayable output; null for a shadow
     * view, a probe and a tile (the atlas is the app's to resolve). */
    resolve: ResolveRecord | null
    /** Whether the view set its own bloom (the `bloom` option or
     * view.setBloom): the scene's setBloom then skips it. */
    ownBloom: boolean
    width: number
    height: number
    /** Debug label: the option, else the scene's plus -view/-probe. */
    label: string
    override: Material | null
    /** The instanced meshes an override view leaves out: the override
     * declares no instanceBuffers, so it cannot place their records
     * (ViewOptions.overrideMaterial). Kept so sync() reports them once
     * per view, over the settled set. */
    skipped: Set<Mesh>
    skippedReported: boolean
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
    /** A reflection probe's target is a cube draw target of this face
     * edge, rendered face by face through probe.update(); null for a
     * 2D view. */
    cube: number | null
    /** A probe's public cube: the chain prefiltered from its target, or
     * the target itself when sharp (null for a 2D view). When it is the
     * scene's environment, the probe's own faces bind the placeholder. */
    probeCube: TextureId | null
    /** Whether the scene's background draws on this target, first in its
     * list (a probe: the sky behind the meshes); a plain view's backdrop
     * is its clearColor. */
    sky: boolean
    disposed: boolean
  }
  let views: ViewRecord[] = []
  // Every name scene.setParams has merged so far, replayed on a new view.
  let sceneParams: ShaderParams = {}
  // The scene's bloom (setBloom), applied to every resolving view that
  // has not set its own.
  let sceneBloom: BloomOptions | null = null
  // Every target a receiving material can draw into: the scene's and each
  // view's but the shadow views (binding a target's own depth into it
  // would be same-pass feedback).
  let receivingTargets = (fn: (target: TextureId) => void) => {
    fn(texture)
    for (let v of views) if (v.shadowFilter === null) fn(v.texture)
  }
  // Whether receiving target `t` is the probe whose cube is the scene's
  // environment (the faces, or the chain prefiltered from them): it
  // samples the placeholder instead of itself.
  let ownsEnvironment = (t: TextureId) => environment !== null && views.some(v => v.texture === t && v.probeCube === environment)
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
    let morph = morphEntry(mesh.material, mesh)
    mesh._entry = addDraw(texture, mesh.material.pipeline(mesh.geometry, geometryTopology(mesh.geometry)), entrySeed(mesh.material, morph === null ? mesh._params : { ...morph.params, ...mesh._params }), {
      buffers: [...bufs.buffers, ...(inst !== null ? instanceBinding(mesh.material, inst) : [])],
      indexBuffer: bufs.index,
      indexFormat: bufs.indexFormat,
      ...meshRange(mesh),
      textures: entryTextures(mesh.material, mesh, morph),
      instanceCount: 0,
      label: drawLabel(mesh),
    })
    // The core turns the entry on (with the world matrix) at the next
    // flush, and off again whenever the node or an ancestor hides.
    spatial.bindDraw(mesh._node!, texture, mesh._entry, drawBinding(mesh.material, mesh, inst))
  }
  let detachScene = (mesh: Mesh) => {
    if (mesh._entry === null) return
    if (mesh._node !== null) spatial.unbindDraw(mesh._node, texture)
    if (!disposed) removeDraw(texture, mesh._entry)
    mesh._entry = null
  }
  // The material a view draws a mesh with: a shadow view lets a caster's
  // material pick its own depth variant (its cull side, cutout, skinning
  // - or the class's shadowVertex, the instanced placement); any other
  // override view draws exactly what it was given.
  let viewMaterial = (v: ViewRecord, mesh: Mesh): Material =>
    v.override !== null ? (v.shadowFilter !== null ? (mesh.material.shadow ?? v.override) : v.override) : mesh.material
  let attachView = (v: ViewRecord, mesh: Mesh) => {
    if (v.entries.has(mesh)) return
    let inst = mesh._instances
    if (v.shadowFilter !== null && !v.shadowFilter(mesh)) return
    // Lines and points cast nothing: a one-pixel wireframe in a shadow
    // map is speckle on the floor, never a shadow anyone wanted.
    if (v.shadowFilter !== null && geometryTopology(mesh.geometry) !== "triangles") return
    if ((mesh.layers & v.mask) === 0) return
    let material = viewMaterial(v, mesh)
    // An override pipeline cannot know a populated mesh's record layout,
    // so the mesh is skipped - unless the variant chosen above is
    // instanced itself (a class with shadowVertex, a stock material's
    // instanced depth pass), whose attributes must fit the records like
    // the main material's did at add().
    if (v.override !== null && inst !== null) {
      if (material.instanceBuffers === undefined) {
        // A caster without an instanced shadow variant casts none by
        // that rule; an app's override view dropping a mesh is invisible
        // in its output, so it is recorded for sync() to report.
        if (v.shadowFilter === null) v.skipped.add(mesh)
        return
      }
      checkInstancePairing(material, inst, "Shadow material")
    }
    let bufs = mesh._buffers!
    let morph = morphEntry(material, mesh)
    let params = v.override !== null ? (morph === null ? null : morph.params) : morph === null ? mesh._params : { ...morph.params, ...mesh._params }
    let entry = addDraw(v.texture, material.pipeline(mesh.geometry, geometryTopology(mesh.geometry)), entrySeed(material, params), {
      buffers: [...bufs.buffers, ...(inst !== null ? instanceBinding(material, inst) : [])],
      indexBuffer: bufs.index,
      indexFormat: bufs.indexFormat,
      ...meshRange(mesh),
      textures: entryTextures(material, mesh, morph),
      instanceCount: 0,
      label: drawLabel(mesh),
    })
    spatial.bindDraw(mesh._node!, v.texture, entry, drawBinding(material, mesh, inst))
    v.entries.set(mesh, entry)
  }
  let detachView = (v: ViewRecord, mesh: Mesh) => {
    v.skipped.delete(mesh)
    let entry = v.entries.get(mesh)
    if (entry === undefined) return
    v.entries.delete(mesh)
    if (mesh._node !== null) spatial.unbindDraw(mesh._node, v.texture)
    if (!v.disposed) removeDraw(v.texture, entry)
  }
  let disposeView = (v: ViewRecord) => {
    if (v.disposed) return
    v.disposed = true
    if (background?.entries.delete(v.texture) && root._node !== null) spatial.unbindDraw(root._node, v.texture)
    for (let mesh of v.entries.keys()) if (mesh._node !== null) spatial.unbindDraw(mesh._node, v.texture)
    v.entries.clear()
    for (let light of lights) if (light.type === "directional" && light._node !== null) spatial.unbindSlot(light._node, v.texture)
    spatial.setView(v.texture, null)
    // Drain the zeroed direction slots while the target still exists.
    spatial.flush()
    if (v.resolve !== null) disposeResolve(v.resolve)
    destroyTexture(v.texture)
    let i = views.indexOf(v)
    if (i >= 0) views.splice(i, 1)
  }
  // The six face renders of cube target `target` from `position` through
  // `cam`, a mirrored 90-degree camera (PROBE_FACE_DIRECTION and
  // PROBE_FACE_UP): each face's camera lands in the target's params,
  // `perFace` runs (a probe's sort), then the face renders. With `chain`
  // (a `mipmap: true` target, the prefilter's source) the first five faces
  // render into level 0 explicitly - an explicit level regenerates nothing
  // - and only the last face's plain render rebuilds the cube's chain: one
  // generation per six faces instead of six (a whole-cube generateMipmap
  // is the expensive step on some drivers).
  let renderCubeFaces = (target: TextureId, cam: Camera, size: number, position: Vec3, chain: boolean, perFace?: (face: number) => void) => {
    let last = PROBE_FACE_DIRECTION.length - 1
    for (let face = 0; face <= last; face++) {
      let d = PROBE_FACE_DIRECTION[face]!
      updateCamera(cam, { position, target: [position[0] + d[0], position[1] + d[1], position[2] + d[2]], up: PROBE_FACE_UP[face] })
      ensureCamera(cam, size, size)
      cam.pending = false
      setTargetParams(target, cameraParams(cam))
      perFace?.(face)
      if (chain && face < last) renderTarget(target, face, 0)
      else renderTarget(target, face)
    }
  }
  // A view record: the target, seeded with everything the scene target
  // already holds (the light set - rewritten for every target, the simple
  // write - the merged scene params, the shadow map binding), then one
  // entry per mesh the filter admits.
  let makeView = (
    vopts: ViewOptions,
    shadowFilter: ((mesh: Mesh) => boolean) | null,
    cube: number | null = null,
    mipmap = false,
    sky = false,
  ): ViewRecord => {
    let override = vopts.overrideMaterial ?? null
    if (override !== null) {
      for (let mesh of meshes) if (mesh._instances === null) checkLayout(override, mesh.geometry, "View override material")
    }
    let tiled = vopts.into !== undefined
    if (tiled && vopts.resolve !== undefined) throw new Error("createView: a tiled view (into) has no resolve of its own - resolve the atlas")
    if (tiled && vopts.bloom !== undefined) throw new Error("createView: a tiled view (into) has no resolve to bloom on - resolve the atlas")
    let viewLabel = vopts.label ?? label + (cube !== null ? "-probe" : "-view")
    let buffer =
      cube !== null
        ? createCubeDrawTarget(cube, null, {
            depth: true,
            // The prefilter reads the faces at the lod of each sample's
            // solid angle: a generated chain, refreshed per face render.
            mipmap,
            format: bufferFormat(),
            clearColor: bufferClear(vopts.clearColor),
            label: viewLabel,
            autoFree: false,
          })
        : createDrawTarget(vopts.width, vopts.height, null, {
            depth: tiled ? undefined : (vopts.depth ?? true),
            format: tiled ? undefined : bufferFormat(),
            clearColor: bufferClear(vopts.clearColor),
            samples: vopts.samples,
            label: viewLabel,
            autoFree: false,
            into: vopts.into,
            x: vopts.x,
            y: vopts.y,
          })
    // An override view (a shadow tile, a depth or id pass) draws in add
    // order; every other view sorts like the scene.
    if (override === null) spatial.setDrawSort(buffer, true)
    // A shadow tile measures LOD by the scene camera; every other view
    // by its own, under the scene's bias.
    if (shadowFilter !== null) spatial.setLodReference(buffer, texture)
    else spatial.setLodBias(buffer, lodBias)
    let v: ViewRecord = {
      texture: buffer,
      // A shadow view is depth only, a probe's faces are read as a cube,
      // a tile is the atlas's: none of them displays, so none resolves.
      resolve:
        cube === null && shadowFilter === null && !tiled
          ? makeResolve(buffer, vopts.width, vopts.height, vopts.resolve, { filter: vopts.filter, wrap: vopts.wrap }, viewLabel)
          : null,
      ownBloom: vopts.bloom !== undefined,
      width: vopts.width,
      height: vopts.height,
      label: viewLabel,
      override,
      skipped: new Set(),
      skippedReported: false,
      shadowFilter,
      mask: shadowFilter !== null ? sceneMask : checkMask(vopts.layers ?? 1, "createView"),
      ownNames: new Set(),
      camera: makeCamera(),
      entries: new Map(),
      cube,
      probeCube: null,
      sky,
      disposed: false,
    }
    views.push(v)
    if (sky) attachBackground(v.texture)
    // The light rewrite seeds the shadow set (maps, casts, biases,
    // matrices) on the new target too.
    lightsDirty = true
    // The view's own fog claims its names before the scene-params seed,
    // so the seed (and every later fan-out) leaves them to the view.
    let ownFog = vopts.fog !== undefined ? fogParams(vopts.fog) : null
    if (ownFog !== null) for (let k of Object.keys(ownFog)) v.ownNames.add(k)
    setTargetParams(v.texture, ownFog === null ? sceneParams : withoutNames(sceneParams, v.ownNames))
    if (ownFog !== null) setTargetParams(v.texture, ownFog)
    if (v.resolve !== null) {
      setTargetParams(v.resolve.target, sceneParams)
      let bloom = vopts.bloom !== undefined ? vopts.bloom : sceneBloom
      if (bloom !== null) setResolveBloom(v.resolve, v.texture, vopts.width, vopts.height, bloom)
    }
    for (let mesh of meshes) attachView(v, mesh)
    hooks._schedule()
    return v
  }
  // The shadow subsystem: the atlas, the per-caster shadow views (made
  // through makeView above) and their map cameras. The deps close over
  // this scene instance; writeLights reads the dealt slots back through
  // forEachShadowSlot/atlas and sync drives placeCameras/flushMatrices.
  let shadowSys = makeShadowSystem({
    lights,
    camera,
    targetSize: () => ({ width, height }),
    label,
    makeView,
    disposeView,
    markLightsDirty: () => {
      lightsDirty = true
    },
    schedule: () => hooks._schedule(),
  })

  // A camera write is also the target's view for the core: what it culls
  // by, measures projected size with (eye, forward, focal, ortho) and
  // sorts depth against - set before the flush below, which switches the
  // entries outside it off. The scene and each view measure by their own
  // camera; a shadow tile by the SCENE camera, through the LOD reference
  // set at its creation, so a caster draws the level the camera sees and
  // its shadow matches. Cube face renders (probes) set it per face: six
  // cameras share one target, so each face sets its view and flushes
  // before it renders.
  let viewScratch = new Float32Array(16)
  let projScratch = new Float32Array(16)
  let setView = (target: TextureId, cam: Camera) => {
    viewScratch.set(cam.view)
    projScratch.set(cam.proj)
    spatial.setView(target, viewScratch, projScratch)
  }
  let lodBias = opts?.lodBias ?? 1
  if (!(lodBias > 0 && Number.isFinite(lodBias))) throw new Error("createScene: lodBias must be a positive number, got " + lodBias)
  spatial.setLodBias(texture, lodBias)
  let sync = () => {
    scheduled = false
    if (disposed) return
    // An override view's skipped instanced meshes (attachView), reported
    // once per view over the settled set, as the budgets below are: the
    // meshes of one flush are all in by this microtask, so the count is
    // the whole set, not the first arrival.
    for (let v of views) {
      if (v.skippedReported || v.skipped.size === 0) continue
      v.skippedReported = true
      let n = v.skipped.size
      console.warn(
        `View '${v.label}' override material skips ${n} instanced ${n === 1 ? "mesh" : "meshes"}: it declares no instanceBuffers, so it cannot place their records, and they draw in no pass of this view; give it an instanced variant to include them`,
      )
    }
    // A per-scene budget - the light cap, the shadow slots - is tested
    // here over the settled set, never at attach: a declarative swap
    // (<Show>, <Switch>) attaches the incoming branch before the outgoing
    // one detaches, inside one Solid flush, and both are gone by this
    // microtask. A set past a budget is reported once, at the end, after
    // every write below, so the scene is never left half-written.
    let budgetError: Error | null = null
    if (lightSetDirty) {
      lightSetDirty = false
      let n = lights.filter(l => l.type !== "hemisphere").length
      if (n > MAX_LIGHTS) {
        budgetError = new Error(
          "A scene takes at most " + MAX_LIGHTS + " lights (directional, spot and point together): " + n + " attached, the lights past the cap are not lit",
        )
      }
    }
    let shadowError = shadowSys.settle()
    if (budgetError === null) budgetError = shadowError
    ensureCamera(camera, width, height)
    let cameraMoved = camera.pending
    if (camera.pending) {
      camera.pending = false
      setTargetParams(texture, cameraParams(camera))
      setView(texture, camera)
    }
    shadowSys.placeCameras(cameraMoved)
    for (let v of views) {
      ensureCamera(v.camera, v.width, v.height)
      if (v.camera.pending) {
        v.camera.pending = false
        setTargetParams(v.texture, cameraParams(v.camera))
        setView(v.texture, v.camera)
        if (v.shadowFilter !== null) shadowSys.markMatricesDirty()
      }
    }
    // Light bookkeeping first, so a fresh direction-slot bind is seeded
    // by the flush below in the same sync.
    if (lightsDirty) writeLights()
    // The populated meshes' record writes since the last sync: one
    // coalesced buffer write per dirty stream.
    if (recordsDirty.size > 0) {
      for (let m of recordsDirty) publishRecords(m)
      recordsDirty.clear()
    }
    // An instanced mesh without explicit bounds is culled by the union of
    // its instances' boxes: the core's cull group (a skinned part's joint
    // group, generalized) over the live instance nodes.
    if (groupDirty.size > 0) {
      for (let m of groupDirty) if (m._node !== null && m._buffers !== null && m._instances !== null) spatial.setCullGroup(m._node, instanceGroup(m))
      groupDirty.clear()
    }
    // The matrices that render the maps are the ones receivers look up
    // with: one array to every receiving target per shadow-camera move.
    shadowSys.flushMatrices(params => receivingTargets(t => setTargetParams(t, params)))
    // The core recomputes the moved subtrees and writes every entry's
    // uModel/uNormal, visibility switch and direction slots.
    spatial.flush()
    if (moved.length > 0) {
      for (let n of moved) n._moved = false
      moved.length = 0
    }
    if (budgetError !== null) {
      if (opts?.onError !== undefined) opts.onError(budgetError)
      else throw budgetError
    }
  }

  let hooks: SceneHooks = {
    _schedule() {
      if (scheduled || disposed) return
      scheduled = true
      RESOLVED.then(sync)
    },
    _attachLight(light) {
      if (disposed) return
      lights.push(light)
      lightsDirty = true
      if (light.type === "hemisphere") return
      lightSetDirty = true
      if (light.castShadow) shadowSys.invalidate(light)
    },
    _detachLight(light) {
      let i = lights.indexOf(light)
      if (i >= 0) lights.splice(i, 1)
      lightsDirty = true
      if (light.type !== "hemisphere") {
        lightSetDirty = true
        shadowSys.invalidate(light)
      }
      hooks._schedule()
    },
    _shadowChanged(light) {
      if (disposed) return
      shadowSys.invalidate(light)
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
      if (mesh._node !== null) spatial.setLayers(mesh._node, mesh.layers)
      if (mesh._instances !== null) {
        for (let n of instanceGroup(mesh as InstancedMesh)) spatial.setLayers(n, mesh.layers)
      }
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
      checkMorph(mesh.material, mesh, "Mesh material")
      // Every check before any mutation, so a rejected mesh is attached
      // nowhere - the views' override materials included.
      for (let v of views) {
        if (v.override !== null && mesh._instances === null) {
          checkLayout(v.override, mesh.geometry, "View override material")
          checkMorph(v.override, mesh, "View override material")
        }
      }
      // Instancing pairs the same way layout does: the pipeline's instance
      // attributes describe the mesh's record buffers, so one without the
      // other (or a record stride from a different attribute list) would
      // bind garbage - errors here, at add().
      let inst = mesh._instances
      if (mesh.material.instanceBuffers !== undefined && inst === null) {
        throw new Error(
          "Material declares instanceBuffers - create its meshes with createInstancedMesh or createRecordMesh, not createMesh",
        )
      }
      if (inst !== null) checkInstancePairing(mesh.material, inst, "Mesh material")
      // An anchored population's records are relative to the anchor, so
      // it must be an ancestor (the core's contract: an anchor move then
      // restages every record); anything else binds records that never
      // follow it - errors here, at add().
      if (inst !== null && inst.anchor !== null) {
        let p: SceneNode | null = mesh.parent
        while (p !== null && p !== inst.anchor) p = p.parent
        if (p === null) throw new Error("An instanced mesh's anchor must be one of its ancestors when it is added to a scene")
      }
      let bufs = acquireGeometryBuffers(mesh.geometry)
      mesh._buffers = bufs
      attachScene(mesh)
      for (let v of views) attachView(v, mesh)
      // Record writes made while the mesh was out of a scene kept their
      // dirty ranges: publish them now.
      if (inst !== null && inst.streams.some(s => s.dirty !== null)) recordsDirty.add(mesh)
      // Picking: an ordinary mesh's node carries its geometry's shape,
      // which is its box in the core index (following every
      // updateVertices) and its triangle narrowphase. An instanced mesh's
      // instances are the leaves that pick, each with the shape, so its
      // own node stays OUT of the index: explicit population bounds are
      // its cull box only (else the cull group of its instances). A
      // record mesh's records are opaque, so its explicit bounds are its
      // box in the index (without them it is not picked at all), as is a
      // sprite's unit box (its triangles lie wherever the camera is, not
      // where the geometry says).
      if (inst === null && !mesh._sprite) spatial.setShape(mesh._node!, bufs.shape)
      else if (inst !== null && inst.nodes !== null) spatial.setCullBounds(mesh._node!, inst.bounds)
      else spatial.setBounds(mesh._node!, localBounds(mesh))
      // A rebuilt entry (setGeometry) re-shapes the live instances to the
      // new geometry; their record bindings are untouched. On a first
      // entry none has a core node yet - they enter after the mesh -
      // except under an anchor, where an instance in an earlier branch
      // of the anchor's subtree is already in and waiting to be bound.
      if (inst !== null && inst.nodes !== null) {
        for (let n of inst.nodes.slots) {
          if (n === null || n._node === null) continue
          if (inst.anchor !== null && !byNode.has(n._node)) this._attachInstance(n)
          else spatial.setShape(n._node, bufs.shape)
        }
      }
      // Culling: the gate and margin only when off the defaults, and a
      // skinned part's joint group (joints entered before it: a model
      // adds its node tree before its parts).
      if (!mesh.frustumCulled || mesh.cullMargin !== 0) spatial.setCull(mesh._node!, mesh.frustumCulled, mesh.cullMargin)
      if (mesh._cullJoints !== null) {
        let joints: NodeId[] = []
        for (let j of mesh._cullJoints) if (j._node !== null) joints.push(j._node)
        spatial.setCullGroup(mesh._node!, joints)
      }
      // An instanced mesh without explicit bounds is culled by its
      // instances' union, the same group over the instance nodes - set
      // at the sync, once they have entered (they enter after the mesh).
      if (inst !== null && inst.nodes !== null && inst.bounds === null) groupDirty.add(mesh as InstancedMesh)
      byNode.set(mesh._node!, mesh)
      meshes.push(mesh)
      this._schedule()
    },
    _detach(mesh) {
      if (mesh._buffers !== null) {
        recordsDirty.delete(mesh)
        groupDirty.delete(mesh as InstancedMesh)
        for (let v of views) detachView(v, mesh)
        detachScene(mesh)
        if (mesh._node !== null) {
          spatial.setShape(mesh._node, null)
          if (mesh._instances !== null && mesh._instances.nodes !== null) spatial.setCullBounds(mesh._node, null)
          else spatial.setBounds(mesh._node, null)
          byNode.delete(mesh._node)
        }
        releaseGeometryBuffers(mesh._buffers)
        mesh._buffers = null
        let i = meshes.indexOf(mesh)
        if (i >= 0) meshes.splice(i, 1)
      }
      mesh._entry = null
    },
    _attachInstance(instance) {
      if (disposed) return
      let mesh = instance.mesh
      // The mesh entered first (parents enter before children), so its
      // geometry buffers and core node are live - a mesh masked out of
      // the scene included, whose instances still pick.
      if (mesh === null || mesh._buffers === null || mesh._node === null || instance._node === null) return
      // A population with an anchor may enter AFTER an instance living
      // elsewhere in the anchor's subtree (a later sibling branch); the
      // mesh's own attach then binds the instances it finds waiting.
      let anchor = (mesh._instances.anchor ?? mesh)._node
      if (anchor === null || byNode.has(instance._node)) return
      spatial.setShape(instance._node, mesh._buffers.shape)
      spatial.setLayers(instance._node, mesh.layers)
      // The record is the instance's placement relative to the anchor -
      // the mesh node itself, or the ancestor the mesh sits under at
      // identity - staged by the core at the next flush. One buffer per
      // LOD level of an instanced LOD (the population is the group and
      // the anchor, its levels its children at identity).
      let levels = mesh._instances.levels
      instanceBuffers.length = 0
      if (levels === null) instanceBuffers.push(mesh._instances.matrix)
      else for (let l of levels) instanceBuffers.push(l._instances.matrix)
      spatial.bindMatrixRecord(instance._node, instanceBuffers, instance._slot, anchor)
      // A morphing population: the instance's register takes its slot's
      // row now (a recycled slot's row is overwritten, not inherited).
      if (mesh._instances.morph !== null) activateMorph(instance)
      byNode.set(instance._node, instance)
      if (mesh._instances.bounds === null) groupDirty.add(mesh)
      this._schedule()
    },
    _exiting(mesh) {
      if (mesh._node !== null) byNode.delete(mesh._node)
    },
    _detachInstance(instance) {
      // The core node is destroyed right after (or animates out, picked
      // by nothing): its box, shape and record binding go with it, and
      // the slot hides at the next flush. The mesh's group loses the
      // member at the sync (a mesh leaving with its instances is skipped
      // there: its node is gone by then).
      if (instance._node !== null) byNode.delete(instance._node)
      let mesh = instance.mesh
      if (mesh !== null && mesh._instances !== null && mesh._instances.bounds === null) groupDirty.add(mesh)
      this._schedule()
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
      // The entries keep their range (at most the old capacity, so the
      // larger buffers always pass the swap's bounds check); the caller
      // destroys the old buffers after this, which the entries held alive
      // until now. Each entry swaps the buffers its own material binds
      // (the full list: the geometry's buffer stays, the instance
      // buffers follow the population).
      let inst = mesh._instances
      let bufs = mesh._buffers
      if (bufs !== null && !disposed && inst !== null) {
        if (mesh._entry !== null) setDrawBuffers(texture, mesh._entry, { buffers: [...bufs.buffers, ...instanceBinding(mesh.material, inst)] })
        for (let v of views) {
          let entry = v.entries.get(mesh)
          if (entry !== undefined) setDrawBuffers(v.texture, entry, { buffers: [...bufs.buffers, ...instanceBinding(viewMaterial(v, mesh), inst)] })
        }
      }
    },
    _setMorphTexture(mesh) {
      // The replaced population texture goes to every morphing entry of
      // the mesh (its own, a morphed shadow stand-in's); the caller
      // destroys the old texture after this.
      let weights = mesh._instances?.morph?.texture
      if (weights === undefined || mesh._buffers === null || disposed) return
      if (mesh._entry !== null && mesh.material.morphed === true) setDrawTextures(texture, mesh._entry, { uMorphWeights: weights })
      for (let v of views) {
        let entry = v.entries.get(mesh)
        if (entry !== undefined && viewMaterial(v, mesh).morphed === true) setDrawTextures(v.texture, entry, { uMorphWeights: weights })
      }
    },
    _setRange(mesh) {
      // The index range travels to every entry of the mesh; the instance
      // count (the core's visibility switch) is not in the update, so it
      // keeps whatever the core last wrote.
      if (mesh._buffers === null || disposed) return
      let range = meshRange(mesh)
      if (mesh._entry !== null) setDrawRange(texture, mesh._entry, range)
      for (let v of views) {
        let entry = v.entries.get(mesh)
        if (entry !== undefined) setDrawRange(v.texture, entry, range)
      }
    },
    _setRecords(mesh) {
      if (disposed || mesh._buffers === null) return
      recordsDirty.add(mesh)
      this._schedule()
    },
    _reorder(mesh) {
      if (disposed || mesh._buffers === null || mesh._node === null) return
      keyDraw(mesh)
      this._schedule()
    },
    _moved(node) {
      if (!node._moved) {
        node._moved = true
        moved.push(node)
      }
      this._schedule()
    },
    _bindLod(node) {
      if (disposed || node._node === null) return
      let config = node._lod
      let levels: { node?: NodeId; size: number }[] = []
      if (config !== null) {
        for (let l of config.levels) {
          if (l.node === null) levels.push({ size: l.size })
          else if (l.node._node !== null) levels.push({ node: l.node._node, size: l.size })
        }
      }
      spatial.setLod(node._node, levels, config?.fade ?? 0, texture)
      this._schedule()
    },
  }

  let root = makeNode("group")
  root._scene = hooks
  root._node = spatial.createNode(fillTransform(root), true)
  if (opts?.stagger !== undefined) setTransition(root, { stagger: opts.stagger })
  // The first light rewrite seeds the (empty) light set and the shadow
  // slots - placeholders, no casts - so receivers draw plain from the
  // first frame.
  lightsDirty = true
  hooks._schedule()

  // The scene's root listeners (scene.listen): the last stop of the
  // pointer walk scene-pointer.ts runs behind scene.handlers.
  let listeners = new Set<ScenePointerListener>()

  // The raycast every pick shares: `target` names the draw target whose
  // LOD levels the query sees (the scene's own, or a view's).
  let raycastAs = (target: TextureId, origin: Vec3, direction: Vec3, rayOpts: QueryOptions | undefined): Hit[] => {
    // Flush pending writes: picking sees the tree as the app just wrote
    // it, the same immediacy contract as lookAt()/project(). (The queued
    // microtask still runs and finds nothing dirty - harmless.)
    if (scheduled) sync()
    if (disposed) return []
    let f = queryFilter(rayOpts, "raycast", target)
    let hits: Hit[] = []
    rayOriginScratch[0] = origin[0]
    rayOriginScratch[1] = origin[1]
    rayOriginScratch[2] = origin[2]
    rayDirScratch[0] = direction[0]
    rayDirScratch[1] = direction[1]
    rayDirScratch[2] = direction[2]
    for (let h of spatial.raycast(rayOriginScratch, rayDirScratch, f)) {
      let leaf = leafOf(h.node)
      if (leaf === null) continue
      let hit: Hit = { mesh: leaf.mesh, distance: h.distance, point: h.point, normal: h.normal }
      if (leaf.instance !== null) hit.instance = leaf.instance
      if (h.face !== undefined) hit.face = h.face
      if (h.uv !== undefined) hit.uv = h.uv
      hits.push(hit)
    }
    return hits
  }

  let scene: Scene = {
    texture: resolve.target,
    hdrTexture: texture,
    depthTexture: depthMode === "texture" ? depthTexture(texture) : null,
    root,
    setCamera(update) {
      updateCamera(camera, update)
      hooks._schedule()
    },
    camera: () => cameraState(camera),
    setSize(w, h) {
      if (disposed || (w === width && h === height)) return
      width = w
      height = h
      setTargetSize(texture, w, h)
      resizeResolve(resolve, w, h)
      camera.dirty = true
      hooks._schedule()
    },
    size: () => ({ width, height }),
    setParams(params) {
      if (disposed) return
      Object.assign(sceneParams, params)
      setTargetParams(texture, params)
      setTargetParams(resolve.target, params)
      for (let v of views) {
        // A view's own names (view.setParams, its fog) win over the
        // scene-wide fan-out.
        let fanned = v.ownNames.size === 0 ? params : withoutNames(params, v.ownNames)
        if (fanned !== params && Object.keys(fanned).length === 0) continue
        setTargetParams(v.texture, fanned)
        if (v.resolve !== null) setTargetParams(v.resolve.target, fanned)
      }
    },
    setFog(fog) {
      scene.setParams(fogParams(fog))
    },
    setLodBias(bias) {
      if (!(bias > 0 && Number.isFinite(bias))) throw new Error("scene.setLodBias: bias must be a positive number, got " + bias)
      if (disposed || bias === lodBias) return
      lodBias = bias
      spatial.setLodBias(texture, bias)
      // Shadow tiles take the scene's through their LOD reference.
      for (let v of views) if (v.shadowFilter === null && !v.disposed) spatial.setLodBias(v.texture, bias)
      hooks._schedule()
    },
    lodLevel(group) {
      if (disposed || group._node === null) return null
      return spatial.lodLevel(group._node, texture)
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
      // A skybox replacing a skybox keeps its entries: same program, new
      // params and cube on every target.
      if (source !== null && typeof source !== "string" && background !== null && background.sky !== null) {
        let params = skyboxParams(source, "scene.setBackground")
        for (let [target, entry] of background.entries) {
          setDrawParams(target, entry, params)
          setDrawTextures(target, entry, { uSky: source.cube })
        }
        background.sky = source
        return
      }
      let sky = source === null || typeof source === "string" ? null : source
      if (sky !== null) skyboxParams(sky, "scene.setBackground")
      if (background !== null) {
        for (let [target, entry] of background.entries) detachBackground(target, entry)
        destroyRenderPipeline(background.pipeline)
        destroyProgram(background.program)
        background = null
      }
      if (source === null) return
      let built = sky === null ? backgroundPipeline(source as string, label + "-background") : backgroundPipeline(SKYBOX_FRAGMENT, label + "-skybox")
      background = { pipeline: built.pipeline, program: built.program, sky, entries: new Map() }
      attachBackground(texture)
      for (let v of views) if (v.sky && !v.disposed) attachBackground(v.texture)
    },
    setEnvironment(env) {
      if (disposed) return
      let params = environmentParams(env)
      environment = env === null ? null : env.cube
      let cube = environment ?? envPlaceholder
      // A probe's own faces never sample the probe (see writeLights).
      receivingTargets(t => setTargetTextures(t, { uEnv: ownsEnvironment(t) ? envPlaceholder : cube }))
      scene.setParams(params)
    },
    setToneMapping(mode) {
      let code = TONE_MAPPING_CODE[mode]
      if (code === undefined) throw new Error('scene.setToneMapping: expected "none", "aces", "agx" or "neutral", got ' + mode)
      scene.setParams({ uToneMapping: code })
    },
    setExposure(exposure) {
      if (!Number.isFinite(exposure) || exposure < 0) throw new Error("scene.setExposure: expected a finite number >= 0, got " + exposure)
      scene.setParams({ uExposure: exposure })
    },
    setResolve(r) {
      if (disposed) return
      replaceResolve(resolve, texture, r)
      hooks._schedule()
    },
    setBloom(bloom) {
      if (disposed) return
      sceneBloom = bloom
      setResolveBloom(resolve, texture, width, height, bloom)
      for (let v of views) if (v.resolve !== null && !v.ownBloom) setResolveBloom(v.resolve, v.texture, v.width, v.height, bloom)
      hooks._schedule()
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
      return raycastAs(texture, pickOrigin, pickDir, undefined)
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
      return raycastAs(texture, origin, direction, rayOpts)
    },
    overlap(volume, qopts) {
      if (scheduled) sync()
      if (disposed) return []
      let f = queryFilter(qopts, "overlap")
      let kind = packVolume(volume, "overlap")
      let out: Overlap[] = []
      for (let h of spatial.overlap(kind, kind === "box" ? boxScratch : capsuleScratch, f)) {
        let leaf = leafOf(h.node)
        if (leaf === null) continue
        let contact: Overlap = { mesh: leaf.mesh, point: h.point, normal: h.normal, depth: h.depth }
        if (leaf.instance !== null) contact.instance = leaf.instance
        out.push(contact)
      }
      return out
    },
    sweep(volume, motion, qopts) {
      if (scheduled) sync()
      if (disposed) return []
      let f = queryFilter(qopts, "sweep")
      let kind = packVolume(volume, "sweep")
      motionScratch[0] = motion[0]
      motionScratch[1] = motion[1]
      motionScratch[2] = motion[2]
      let out: Impact[] = []
      for (let h of spatial.sweep(kind, kind === "box" ? boxScratch : capsuleScratch, motionScratch, f)) {
        let impact = impactOf(h)
        if (impact !== null) out.push(impact)
      }
      return out
    },
    moveAndSlide(volume, motion, mopts) {
      if (scheduled) sync()
      let kind = packVolume(volume, "moveAndSlide")
      motionScratch[0] = motion[0]
      motionScratch[1] = motion[1]
      motionScratch[2] = motion[2]
      if (disposed) return { motion: [motion[0], motion[1], motion[2]], floor: null, wall: false, ceiling: false, hits: [] }
      let f = queryFilter(mopts, "moveAndSlide")
      let r = spatial.moveAndSlide(kind, kind === "box" ? boxScratch : capsuleScratch, motionScratch, mopts, f)
      let hits: Impact[] = []
      for (let h of r.hits) {
        let impact = impactOf(h)
        if (impact !== null) hits.push(impact)
      }
      return { motion: r.motion, floor: r.floor, wall: r.wall, ceiling: r.ceiling, hits }
    },
    get handlers() {
      return pointer.handlers
    },
    handlersFor(layout) {
      return pointer.handlersFor(layout)
    },
    listen(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    createView(vopts) {
      if (disposed) throw new Error("createView: the scene is disposed")
      let v = makeView(vopts, null)
      // The view's own walk: its listeners, its camera's pick, and a
      // dispatch with its own capture and hover bookkeeping.
      let viewListeners = new Set<ScenePointerListener>()
      let handle: ViewHandle = {
        texture: v.resolve !== null ? v.resolve.target : v.texture,
        hdrTexture: v.resolve !== null ? v.texture : null,
        depthTexture: vopts.depth === "texture" && vopts.into === undefined ? depthTexture(v.texture) : null,
        setCamera(update) {
          updateCamera(v.camera, update)
          hooks._schedule()
        },
        camera: () => cameraState(v.camera),
        setSize(w, h) {
          if (v.disposed || (w === v.width && h === v.height)) return
          v.width = w
          v.height = h
          setTargetSize(v.texture, w, h)
          if (v.resolve !== null) resizeResolve(v.resolve, w, h)
          v.camera.dirty = true
          hooks._schedule()
        },
        size: () => ({ width: v.width, height: v.height }),
        setResolve(r) {
          if (v.resolve === null) throw new Error("view.setResolve: a tiled view has no resolve of its own - resolve the atlas")
          if (v.disposed) return
          replaceResolve(v.resolve, v.texture, r)
          hooks._schedule()
        },
        setBloom(bloom) {
          if (v.resolve === null) throw new Error("view.setBloom: a tiled view has no resolve to bloom on - resolve the atlas")
          if (v.disposed) return
          v.ownBloom = true
          setResolveBloom(v.resolve, v.texture, v.width, v.height, bloom)
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
          if (v.resolve !== null) setTargetParams(v.resolve.target, params)
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
        lodLevel(group) {
          if (v.disposed || group._node === null) return null
          return spatial.lodLevel(group._node, v.texture)
        },
        pick(x, y) {
          if (v.disposed) return []
          pixelRayOf(v.camera, v.width, v.height, x, y)
          return raycastAs(v.texture, pickOrigin, pickDir, undefined)
        },
        get handlers() {
          return viewPointer.handlers
        },
        handlersFor(layout) {
          return viewPointer.handlersFor(layout)
        },
        listen(listener) {
          viewListeners.add(listener)
          return () => {
            viewListeners.delete(listener)
          }
        },
        dispose() {
          viewListeners.clear()
          disposeView(v)
        },
      }
      let viewPointer = makePointerInput({
        pick: (x, y) => handle.pick(x, y),
        targetSize: () => ({ width: v.width, height: v.height }),
        root: handle,
        listeners: viewListeners,
      })
      return handle
    },
    createReflectionProbe(popts) {
      if (disposed) throw new Error("createReflectionProbe: the scene is disposed")
      let { cube, setPosition, update, dispose } = makeProbe(popts, checkMask(popts.layers ?? 1, "createReflectionProbe"))
      return { cube, setPosition, update, dispose }
    },
    bakeBackground(size = PROBE_SIZE) {
      if (disposed) throw new Error("bakeBackground: the scene is disposed")
      if (background === null) throw new Error("scene.bakeBackground: the scene has no background to bake")
      if (!Number.isInteger(size) || size < 1) throw new Error("scene.bakeBackground: size must be a positive integer, got " + size)
      // A probe that sees no mesh (mask 0) draws the background alone: the
      // same face cameras, linear output and prefilter as any probe.
      let probe = makeProbe({ position: [0, 0, 0], size, label: label + "-sky" }, 0)
      probe.update()
      return probe.finish()
    },
    dispose() {
      if (disposed) return
      disposed = true
      listeners.clear()
      // Full tree-side teardown, not just the target: every node leaves
      // the scene (entries' geometry-buffer references and pick leaves
      // dropped, core nodes freed), so a disposed scene leaves no
      // bookkeeping behind and the JS tree survives as plain data.
      // Destroyed nodes still animating out are no longer in the tree:
      // they free on the spot first.
      freeLeaving((_, s) => s === hooks)
      for (let c of root.children.slice()) leaveScene(c)
      root._scene = null
      if (root._node !== null) {
        spatial.destroyNode(root._node)
        root._node = null
      }
      spatial.setView(texture, null)
      // Drain the zeroed direction slots the teardown queued while the
      // targets still exist; afterwards their groups are gone.
      spatial.flush()
      disposeResolve(resolve)
      destroyTexture(texture)
      for (let v of views.slice()) disposeView(v)
      shadowSys.dispose()
      destroyTexture(envPlaceholder)
      if (background !== null) {
        // The entries died with the targets; the pipeline and program are
        // the scene's own (unlike shared material pipelines), so they go too.
        destroyRenderPipeline(background.pipeline)
        destroyProgram(background.program)
        background = null
      }
    },
  }
  // Pointer dispatch (scene.handlers): capture/hover bookkeeping, the
  // walk and the tap tracker live in scene-pointer.ts; all it needs of
  // the scene is pick(), the target size, and the root it ends at.
  let pointer = makePointerInput({
    pick: (x, y) => scene.pick(x, y),
    targetSize: () => ({ width, height }),
    root: scene,
    listeners,
  })
  // A reflection probe over layer mask `mask` (see createReflectionProbe):
  // the public object plus `finish`, which keeps the prefiltered chain and
  // drops everything else - the bake's one-shot use.
  let makeProbe = (popts: ReflectionProbeOptions, mask: number): ReflectionProbe & { finish(): TextureId } => {
      let size = popts.size ?? PROBE_SIZE
      if (!Number.isInteger(size) || size < 1) throw new Error("createReflectionProbe: size must be a positive integer, got " + size)
      let prefilter = popts.prefilter !== false
      let v = makeView(
        { width: size, height: size, layers: mask, clearColor: popts.clearColor, label: popts.label },
        null,
        size,
        prefilter,
        true,
      )
      v.camera.mirror = true
      updateCamera(v.camera, { fov: PROBE_FOV, near: popts.near, far: popts.far })
      let chain = prefilter ? createPrefilter(size, v.texture, bufferFormat(), (popts.label ?? label + "-probe") + "-chain") : null
      v.probeCube = chain?.cube ?? v.texture
      let position: Vec3 = [popts.position[0], popts.position[1], popts.position[2]]
      return {
        cube: v.probeCube,
        setPosition(p) {
          position = [p[0], p[1], p[2]]
        },
        update() {
          if (v.disposed) return
          // The scene's pending state (lights, params, the fan-out to this
          // target) lands before the faces read it.
          sync()
          renderCubeFaces(v.texture, v.camera, size, position, chain !== null, () => {
            // Each face looks a different way: its own view, flushed so
            // the core culls, sorts and measures the face before it
            // renders.
            setView(v.texture, v.camera)
            spatial.flush()
          })
          chain?.run()
        },
        dispose() {
          if (v.disposed) return
          // The scene may sample this very cube - `environment={{ cube }}`,
          // a skybox `background` - so those drop first: a subtree that
          // owned the environment leaves the scene unlit by it, never
          // sampling a destroyed texture until something re-bakes.
          if (environment === v.probeCube) scene.setEnvironment(null)
          if (background !== null && background.sky !== null && background.sky.cube === v.probeCube) scene.setBackground(null)
          chain?.dispose()
          disposeView(v)
        },
        finish() {
          if (chain === null) throw new Error("finish: a sharp probe has no chain to keep")
          let cube = chain.finish()
          disposeView(v)
          return cube
        },
      }
  }
  // The fog set starts at "none" (uFogInv and uFogDensity 0 are factor 0,
  // uFogHeightFalloff 0 is no attenuation) so every material that declares
  // it has coverage from the first frame; a target tolerates the names
  // when nothing declares them.
  scene.setParams({ uFogColor: [0, 0, 0], uFogNear: 0, uFogInv: 0, uFogDensity: 0, uFogHeight: 0, uFogHeightFalloff: 0 })
  // Likewise the environment set starts at "none" (uEnvOn 0), so a
  // reflective material has coverage before setEnvironment.
  scene.setParams(environmentParams(null))
  // And the resolve at its defaults (exposure 1, no tone mapping).
  scene.setParams({ uExposure: 1, uToneMapping: TONE_MAPPING_CODE.none })
  if (opts?.fog !== undefined) scene.setFog(opts.fog)
  if (opts?.toneMapping !== undefined) scene.setToneMapping(opts.toneMapping)
  if (opts?.exposure !== undefined) scene.setExposure(opts.exposure)
  if (opts?.bloom !== undefined) scene.setBloom(opts.bloom)
  if (opts?.background !== undefined) scene.setBackground(opts.background)
  if (opts?.environment !== undefined) scene.setEnvironment(opts.environment)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => scene.dispose())
  return scene
}