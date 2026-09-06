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
//
// This file is the scene half: createScene and everything one scene
// instance owns (targets, views, sync, picking). The node layer it
// renders - the graph, transforms, meshes, lights - lives in node.ts,
// mesh.ts and light.ts, talking back through the SceneHooks seam
// (node.ts); the camera record is camera.ts, and the per-scene shadow
// and pointer subsystems are scene-shadows.ts / scene-pointer.ts,
// built here with the scene's seams as their deps.

import { addDraw, createCubeDrawTarget, createDrawTarget, depthTexture, destroyProgram, destroyRenderPipeline, destroyTexture, removeDraw, renderTarget, setDrawBuffers, setDrawOrder, setDrawParams, setDrawTextures, setTargetParams, setTargetRect, setTargetSize, setTargetTextures } from "@solidrt/core/gpu"
import * as spatial from "flux:spatial"
import type { NodeId } from "flux:spatial"
import type { DrawId, FilterMode, ProgramId, RenderPipelineId, ShaderParams, TextureId, WrapMode } from "@solidrt/core/gpu"
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
import { copy, mat4, transformPoint } from "./math.ts"
import { linearColor } from "./color.ts"
import type { Mat4, Vec3, Vec4 } from "./math.ts"
import { MAX_LIGHTS, MAX_SHADOW_MAPS } from "./glsl.ts"
import { cameraParams, cameraState, ensureCamera, makeCamera, updateCamera } from "./camera.ts"
import type { Camera, CameraState, CameraUpdate } from "./camera.ts"
import { makeShadowSystem } from "./scene-shadows.ts"
import { makePointerInput } from "./scene-pointer.ts"
import { layoutKey, validateGeometry } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import { acquireGeometryBuffers, releaseGeometryBuffers } from "./geometry-gpu.ts"
import { backgroundPipeline, missingAttributes, SKYBOX_FRAGMENT } from "./material.ts"
import { createEnvironmentPlaceholder, createPrefilter, probeFormat } from "./environment.ts"
import type { Prefilter } from "./environment.ts"
import type { Material } from "./material.ts"
import { orderEntries } from "./order.ts"
import { fillTransform, leaveScene, makeNode, worldInto } from "./node.ts"
import type { SceneHooks, SceneNode, ScenePointerEvent } from "./node.ts"
import { checkMask, instanceStride, localBounds } from "./mesh.ts"
import type { Mesh } from "./mesh.ts"
import type { CastingLight, Light } from "./light.ts"

const IDENTITY = mat4()
const RESOLVED = Promise.resolve()

// raycast()'s FFI carriers; values are copied at the boundary, so one of
// each serves every call.
let rayOriginScratch = new Float32Array(3)
let rayDirScratch = new Float32Array(3)

// pick()'s camera-ray scratch.
let pickDir: Vec3 = [0, 0, 0]
// worldInto's out for the light/center readbacks below; nothing here
// outlives a single call.
let worldScratch = mat4()

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

/** The output tone mapping (setToneMapping): "none" clamps, "aces" is the
 * filmic curve every engine ships (Three's ACESFilmic, Godot's ACES,
 * Unity's ACES). */
export type ToneMapping = "none" | "aces"

// The uToneMapping value per mode; the OUTPUT set branches on it.
const TONE_MAPPING_CODE: Record<ToneMapping, number> = { none: 0, aces: 1 }

export type SceneOptions = {
  /** The target's clear, written as given: encoded pixels, untouched by
   * exposure and tone mapping (a GLSL background or skybox goes through
   * both). */
  clearColor?: [number, number, number, number]
  /** Scene-wide fog; see setFog. */
  fog?: FogOptions
  /** Output tone mapping, default "none"; see setToneMapping. */
  toneMapping?: ToneMapping
  /** Output exposure, default 1; see setExposure. */
  exposure?: number
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
  /** Current camera state, exactly scene.camera. */
  camera(): CameraState
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
  /** Destroy the cube (idempotent; probes also die with the scene). */
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
// The output stage of a cube that holds light rather than display pixels
// (a probe's faces, a baked sky): no sRGB encode, no tone mapping, unit
// exposure - so the environment lookups read radiance.
const LINEAR_OUTPUT: ShaderParams = { uOutputEncode: 0, uToneMapping: TONE_MAPPING_CODE.none, uExposure: 1 }

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
  /** The camera as the next frame draws it: setCamera's own fields, a
   * fresh snapshot per call (arrays are copies - mutate freely, write
   * back through setCamera). Reflects a pending setCamera immediately. */
  camera(): CameraState
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
   * The output stage's tone mapping, applied by every library material,
   * the skybox and a GLSL background that ends with outputColor: "none"
   * (default) clamps the linear result, "aces" compresses highlights on
   * the filmic curve. One shared-params write (`uToneMapping`, the OUTPUT
   * set in `@solidrt/3d/glsl` declares), like Three's
   * renderer.toneMapping and Godot's Environment tonemap; a custom
   * fragment that writes fragColor directly is untouched. The clearColor
   * is not tone mapped: with a curve on, draw the backdrop as a
   * background.
   */
  setToneMapping(mode: ToneMapping): void
  /**
   * The output stage's exposure (default 1): the linear result is scaled
   * by it before tone mapping, so a scene lit in physical-ish units is
   * brought into range here rather than by dimming every light. One
   * shared-params write (`uExposure`), like Three's toneMappingExposure.
   */
  setExposure(exposure: number): void
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
   * view's own channel. A view's backdrop is its clearColor (the scene's
   * background draws on probes, not on views), and a view has no picking
   * or pointer events. Views die with the scene; `view.dispose()` drops
   * one early.
   */
  createView(opts: ViewOptions): View
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
   * exactly as the scene draws it, written LINEAR (uOutputEncode 0, no tone mapping, unit exposure,
   * so a sky ending in outputColor bakes its light; one writing fragColor
   * raw bakes those bytes as light) - then GGX-prefiltered into the chain
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
  // map turns a hit's core node back into the mesh.
  let byNode = new Map<NodeId, Mesh>()
  // Nodes whose transform changed since the last sync (deduped by the
  // _moved flag): what the light and transparent-order bookkeeping
  // reacts to, since which meshes moved is the core's knowledge now.
  let moved: SceneNode[] = []

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
  let orderDirty = false
  // The order last handed to the engine: a resort that lands on the same
  // permutation (the common case under a moving camera) issues nothing.
  let lastOrder: DrawId[] = []
  // `skybox`: the entry runs the library's cube-map fragment (its params
  // and textures are rewritable in place); false = app GLSL.
  // `sky` is the skybox source (its cube and knobs) for the bake, null
  // for a GLSL background.
  // `entries` is the background's draw entry per target that draws it:
  // the scene's own and every sky view's (a reflection probe), each first
  // in its list.
  let background: { pipeline: RenderPipelineId; program: ProgramId; sky: SkyboxOptions | null; entries: Map<TextureId, DrawId> } | null = null
  // Add the current background to `target` before entry `before` (its
  // first mesh entry; undefined appends, right for a target with none yet).
  let attachBackground = (target: TextureId, before: DrawId | undefined) => {
    if (background === null) return
    let sky = background.sky
    let params = sky === null ? null : skyboxParams(sky, "scene.setBackground")
    let textures = sky === null ? undefined : { uSky: sky.cube }
    background.entries.set(target, addDraw(target, background.pipeline, params, { vertexCount: 3, before, textures }))
  }
  // The environment cube bound as uEnv on every receiving target (the
  // light rewrite seeds it on new targets); null binds the placeholder,
  // the scene's own 1x1 black cube.
  let environment: TextureId | null = null
  let envPlaceholder = createEnvironmentPlaceholder((opts?.label ?? "scene") + "-env-none")
  let sortEntries = () => {
    orderDirty = false
    let order = orderEntries(meshes, camera.view, background?.entries.get(texture))
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
    if (v.shadowFilter !== null && !v.shadowFilter(mesh)) return
    if ((mesh.layers & v.mask) === 0) return
    // A shadow view lets a caster's material pick its own depth variant
    // (its cull side, cutout, skinning - or the class's shadowVertex,
    // the instanced placement); any other override view draws exactly
    // what it was given.
    let material = v.override !== null ? (v.shadowFilter !== null ? (mesh.material.shadow ?? v.override) : v.override) : mesh.material
    // An override pipeline cannot know an instanced mesh's record
    // layout, so the mesh is skipped - unless the variant chosen above
    // is instanced itself (a class with shadowVertex, whose attributes
    // are the class's own and must match the records like the main
    // material's did at add()).
    if (v.override !== null && inst !== null) {
      let attrs = material.instanceAttributes
      if (attrs === undefined) return
      if (instanceStride(attrs) !== inst.stride) {
        throw new Error(
          "Shadow material's instanceAttributes take " + instanceStride(attrs) + " floats but the mesh's records are " + inst.stride,
        )
      }
    }
    let bufs = mesh._buffers!
    let entry = addDraw(v.texture, material.pipeline(mesh.geometry.layout), entrySeed(material, v.override !== null ? null : mesh._params), {
      buffer: bufs.buffer,
      indexBuffer: bufs.index,
      indexFormat: bufs.indexFormat,
      // The mesh's own bindings ride with its own material and with any
      // skinned stand-in (a skinned shadow variant declares uBones and
      // needs the mesh's palette); other override programs may not
      // declare the names, and per-entry bindings validate strictly.
      textures: (material === mesh.material || material.skinned === true) && mesh._textures !== null ? { ...material.textures, ...mesh._textures } : material.textures,
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
    let order = orderEntries(meshes, v.camera.view, background?.entries.get(v.texture), m => v.entries.get(m as Mesh) ?? null)
    if (order.length === v.lastOrder.length && order.every((id, i) => id === v.lastOrder[i])) return
    v.lastOrder = order
    setDrawOrder(v.texture, order)
  }
  let disposeView = (v: ViewRecord) => {
    if (v.disposed) return
    v.disposed = true
    background?.entries.delete(v.texture)
    for (let mesh of v.entries.keys()) if (mesh._node !== null) spatial.unbindDraw(mesh._node, v.texture)
    v.entries.clear()
    for (let light of lights) if (light.type === "directional" && light._node !== null) spatial.unbindSlot(light._node, v.texture)
    // Drain the zeroed direction slots while the target still exists.
    spatial.flush()
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
    let v: ViewRecord = {
      texture:
        cube !== null
          ? createCubeDrawTarget(cube, null, {
              depth: true,
              // The prefilter reads the faces at the lod of each sample's
              // solid angle: a generated chain, refreshed per face render.
              mipmap,
              // Linear radiance, HDR where the device renders half float.
              format: probeFormat(),
              clearColor: vopts.clearColor,
              label: vopts.label ?? (opts?.label ?? "scene") + "-probe",
              autoFree: false,
            })
          : createDrawTarget(vopts.width, vopts.height, null, {
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
      cube,
      probeCube: null,
      sky,
      disposed: false,
    }
    views.push(v)
    // The background goes in first (no mesh entry exists yet), where every
    // later sort keeps it.
    if (sky) attachBackground(v.texture, undefined)
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
  // The shadow subsystem: the atlas, the per-caster shadow views (made
  // through makeView above) and their map cameras. The deps close over
  // this scene instance; writeLights reads the dealt slots back through
  // forEachShadowSlot/atlas and sync drives placeCameras/flushMatrices.
  let shadowSys = makeShadowSystem({
    lights,
    camera,
    targetSize: () => ({ width, height }),
    label: opts?.label ?? "scene",
    makeView,
    disposeView,
    markLightsDirty: () => {
      lightsDirty = true
    },
    schedule: () => hooks._schedule(),
  })

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
    shadowSys.placeCameras(cameraMoved)
    for (let v of views) {
      ensureCamera(v.camera, v.width, v.height)
      if (v.camera.pending) {
        v.camera.pending = false
        setTargetParams(v.texture, cameraParams(v.camera))
        if (transparentCount > 1) v.orderDirty = true
        if (v.shadowFilter !== null) shadowSys.markMatricesDirty()
      }
    }
    // Light bookkeeping first, so a fresh direction-slot bind is seeded
    // by the flush below in the same sync.
    if (lightsDirty) writeLights()
    // The matrices that render the maps are the ones receivers look up
    // with: one array to every receiving target per shadow-camera move.
    shadowSys.flushMatrices(params => receivingTargets(t => setTargetParams(t, params)))
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
      if (light.type !== "hemisphere" && lights.filter(l => l.type !== "hemisphere").length >= MAX_LIGHTS) {
        throw new Error("A scene takes at most " + MAX_LIGHTS + " lights (directional, spot and point together)")
      }
      lights.push(light)
      lightsDirty = true
      if (light.type !== "hemisphere" && light.castShadow) shadowSys.createShadow(light)
    },
    _detachLight(light) {
      let i = lights.indexOf(light)
      if (i >= 0) lights.splice(i, 1)
      lightsDirty = true
      if (light.type !== "hemisphere") shadowSys.destroyShadow(light)
      hooks._schedule()
    },
    _shadowChanged(light) {
      if (disposed) return
      shadowSys.shadowChanged(light)
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

  // Pointer dispatch (scene.handlers): capture/hover bookkeeping and the
  // bubble walk live in scene-pointer.ts; all it needs of the scene is
  // pick() and the target size.
  let pointer = makePointerInput({
    pick: (x, y) => scene.pick(x, y),
    targetSize: () => ({ width, height }),
  })

  let scene: Scene = {
    texture,
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
        for (let [target, entry] of background.entries) removeDraw(target, entry)
        destroyRenderPipeline(background.pipeline)
        destroyProgram(background.program)
        background = null
      }
      if (source === null) return
      let label = opts?.label ?? "scene"
      let built = sky === null ? backgroundPipeline(source as string, label + "-background") : backgroundPipeline(SKYBOX_FRAGMENT, label + "-skybox")
      background = { pipeline: built.pipeline, program: built.program, sky, entries: new Map() }
      // First in list order on every target that draws it: inserted before
      // the first mesh ENTRY (a layers-masked mesh has none), and every
      // later sort keeps it there.
      attachBackground(texture, meshes.find(m => m._entry !== null)?._entry ?? undefined)
      for (let v of views) if (v.sky && !v.disposed) attachBackground(v.texture, v.lastOrder[0] ?? v.entries.values().next().value)
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
      if (code === undefined) throw new Error('scene.setToneMapping: expected "none" or "aces", got ' + mode)
      scene.setParams({ uToneMapping: code })
    },
    setExposure(exposure) {
      if (!Number.isFinite(exposure) || exposure < 0) throw new Error("scene.setExposure: expected a finite number >= 0, got " + exposure)
      scene.setParams({ uExposure: exposure })
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
    handlers: pointer.handlers,
    handlersFor: pointer.handlersFor,
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
        camera: () => cameraState(v.camera),
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
      let probe = makeProbe({ position: [0, 0, 0], size, label: (opts?.label ?? "scene") + "-sky" }, 0)
      probe.update()
      return probe.finish()
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
      // The faces hold LINEAR radiance, untouched by the scene's output
      // stage: no sRGB encode, no tone mapping, unit exposure - the
      // probe's own names, so the scene's fan-out leaves them alone.
      for (let k of Object.keys(LINEAR_OUTPUT)) v.ownNames.add(k)
      setTargetParams(v.texture, LINEAR_OUTPUT)
      let chain = prefilter ? createPrefilter(size, v.texture, probeFormat(), (popts.label ?? (opts?.label ?? "scene") + "-probe") + "-chain") : null
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
          renderCubeFaces(v.texture, v.camera, size, position, chain !== null, face => {
            // Opaque order is a hint (front to back); only transparency
            // needs the per-face sort.
            if (face === 0 || transparentCount > 1) sortView(v)
          })
          chain?.run()
        },
        dispose() {
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
  // And the output stage at its defaults (exposure 1, no tone mapping),
  // which every library fragment declares.
  scene.setParams({ uExposure: 1, uToneMapping: TONE_MAPPING_CODE.none, uOutputEncode: 1 })
  if (opts?.fog !== undefined) scene.setFog(opts.fog)
  if (opts?.toneMapping !== undefined) scene.setToneMapping(opts.toneMapping)
  if (opts?.exposure !== undefined) scene.setExposure(opts.exposure)
  if (opts?.background !== undefined) scene.setBackground(opts.background)
  if (opts?.environment !== undefined) scene.setEnvironment(opts.environment)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => scene.dispose())
  return scene
}