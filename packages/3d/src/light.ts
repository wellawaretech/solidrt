// Lights: the directional light (with its shadow options) and the
// hemisphere ambient, their constructors and the setLight write path.
// Scene reactions go through the node's SceneHooks (node.ts).

import { MAX_CASCADES, MAX_LIGHTS } from "./glsl.ts"
import type { Vec3 } from "./math.ts"
import { makeNode } from "./node.ts"
import type { SceneNode } from "./node.ts"

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
  /** Filter radius in map texels (default 1): 1 is the single hardware
   * compare tap (a 2x2 bilinear PCF); above 1 a 3x3 grid of those taps
   * spread `radius` texels apart, Three's `shadow.radius` - softer edges
   * for the texel stairs a close receiver shows, at nine taps. */
  radius?: number
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
  shadow: { mapSize: number; bias: number; normalBias: number; radius: number; camera: ShadowCamera; cascades: number; distance: number | null }
}

/** A spot light node: a cone of light from the node's WORLD position
 * along `direction` in its LOCAL space (default [0, -1, 0], a lamp
 * aimed straight down), so a parent's rotation turns it and setTransform
 * places it - deliberately a direction, not Three's target object, the
 * DirectionalLight rule. The cone fades across `penumbra` of `angle`
 * and the strength falls off as `1 / d^decay`, windowed to zero at
 * `distance` (Three's punctual-light falloff). Write through setLight. */
export type SpotLight = SceneNode & {
  kind: "light"
  type: "spot"
  direction: Vec3
  /** Linear [r, g, b] 0..1. */
  color: Vec3
  intensity: number
  /** Falloff cutoff in world units; the light is zero past it (0 = no
   * cutoff, Three's rule). */
  distance: number
  /** The cone's half-angle in DEGREES from `direction` (default 60, at
   * most 90). Degrees like camera fov and like Unity/Godot; Three's
   * radians `SpotLight.angle` converts as `angle * 180 / PI`. */
  angle: number
  /** 0..1: the outer fraction of the cone that fades to the rim
   * (default 0, a hard edge). */
  penumbra: number
  /** Falloff exponent (default 2, physical inverse-square; 0 = no
   * distance falloff). */
  decay: number
  /** Render a shadow map from this light: the directional machinery
   * with a perspective camera at the light's world position along its
   * world direction, fov = the cone (2 * angle), far = `distance` (or
   * the default when 0). One map, one shadow slot. */
  castShadow: boolean
  /** The resolved shadow options (read; write through setLight). */
  shadow: { mapSize: number; bias: number; normalBias: number; radius: number; near: number }
}

/** The perspective-map shadow options a spot or point light takes. */
export type SpotShadowOptions = {
  /** Shadow map resolution in texels, square (default 1024). A point
   * light renders SIX maps at this size, one per face. */
  mapSize?: number
  /** Depth bias against acne, in the map's 0..1 depth (default 0). Note
   * a perspective map's depth is nonlinear: `normalBias` is the knob to
   * reach for first. */
  bias?: number
  /** Offset a receiving point along its normal before the lookup, in
   * world units (default 0). */
  normalBias?: number
  /** Filter radius in map texels (default 1): 1 is the single hardware
   * compare tap (a 2x2 bilinear PCF); above 1 a 3x3 grid of those taps
   * spread `radius` texels apart, Three's `shadow.radius` - softer edges
   * for the texel stairs a close receiver shows, at nine taps. */
  radius?: number
  /** The shadow camera's near plane in world units (default 0.5); its
   * far is the light's `distance` (or the directional default, 500,
   * when 0). */
  near?: number
}

/** A point light node: light in every direction from the node's WORLD
 * position (setTransform places it; rotation and scale do not matter),
 * with the same `distance`/`decay` falloff as a spot. Write through
 * setLight. */
export type PointLight = SceneNode & {
  kind: "light"
  type: "point"
  /** Linear [r, g, b] 0..1. */
  color: Vec3
  intensity: number
  /** Falloff cutoff in world units (0 = no cutoff). */
  distance: number
  /** Falloff exponent (default 2). */
  decay: number
  /** Render shadow maps from this light: six 90-degree face maps
   * (perspective, world-axis aligned) as tiles of the scene's shadow
   * atlas, claiming six shadow slots; a receiver picks the face by the
   * light-to-point direction. Far = `distance` (or the directional
   * default when 0), so give a casting bulb a distance. */
  castShadow: boolean
  /** The resolved shadow options (read; write through setLight). */
  shadow: { mapSize: number; bias: number; normalBias: number; radius: number; near: number }
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

export type Light = DirectionalLight | SpotLight | PointLight | HemisphereLight

/** The light types that can render a shadow map - all of them: a
 * directional light's box or cascades, a spot's one cone map, a point
 * light's six face maps. */
export type CastingLight = DirectionalLight | SpotLight | PointLight

// Every light `color` (and the hemisphere's sky/ground) is sRGB 0..1
// like a material color, decoded when the scene writes the light list;
// `intensity` scales it in linear light.
export type DirectionalLightOptions = {
  direction?: Vec3
  color?: Vec3
  intensity?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: ShadowOptions
}
export type SpotLightOptions = {
  direction?: Vec3
  color?: Vec3
  intensity?: number
  distance?: number
  angle?: number
  penumbra?: number
  decay?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: SpotShadowOptions
}
export type PointLightOptions = {
  color?: Vec3
  intensity?: number
  distance?: number
  decay?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: SpotShadowOptions
}
export type HemisphereLightOptions = { sky?: Vec3; ground?: Vec3; intensity?: number }

/** Every light may cast; the real bound is the shadow-slot budget
 * (MAX_SHADOW_MAPS: a directional light claims `shadow.cascades` slots,
 * a point light six, a spot one, and a caster past the budget throws at
 * attach). Every map is a tile of the scene's one shadow atlas, so the
 * pass count never follows the caster count, the fill does. */
export const MAX_SHADOWS = MAX_LIGHTS
export { MAX_CASCADES }

// The spot cone bounds: a half-angle in (0, 90] degrees, penumbra a
// 0..1 fraction of it.
const SPOT_ANGLE_MAX = 90

function checkSpot(update: SpotLightOptions): void {
  if (update.angle !== undefined && !(update.angle > 0 && update.angle <= SPOT_ANGLE_MAX)) {
    throw new Error("Spot angle must be in (0, 90] degrees")
  }
  if (update.penumbra !== undefined && !(update.penumbra >= 0 && update.penumbra <= 1)) {
    throw new Error("Spot penumbra must be in 0..1")
  }
  checkFalloff(update)
}

function checkFalloff(update: { distance?: number; decay?: number }): void {
  if (update.distance !== undefined && !(update.distance >= 0)) throw new Error("Light distance must be >= 0 (0 = no cutoff)")
  if (update.decay !== undefined && !(update.decay >= 0)) throw new Error("Light decay must be >= 0")
}

function checkRadius(radius: number): number {
  if (!(radius >= 1)) throw new Error("shadow.radius must be >= 1 (1 = the single hardware tap)")
  return radius
}

function mergeShadow(into: DirectionalLight["shadow"], update: ShadowOptions): void {
  if (update.mapSize !== undefined) into.mapSize = update.mapSize
  if (update.bias !== undefined) into.bias = update.bias
  if (update.normalBias !== undefined) into.normalBias = update.normalBias
  if (update.radius !== undefined) into.radius = checkRadius(update.radius)
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
  light.shadow = { mapSize: 1024, bias: 0, normalBias: 0, radius: 1, camera: { left: -5, right: 5, top: 5, bottom: -5, near: 0.5, far: 500 }, cascades: 1, distance: null }
  if (opts.shadow !== undefined) mergeShadow(light.shadow, opts.shadow)
  return light
}

export function createSpotLight(opts: SpotLightOptions = {}): SpotLight {
  checkSpot(opts)
  let light = makeNode("light") as SpotLight
  light.type = "spot"
  light.direction = [...(opts.direction ?? [0, -1, 0])] as Vec3
  light.color = [...(opts.color ?? [1, 1, 1])] as Vec3
  light.intensity = opts.intensity ?? 1
  light.distance = opts.distance ?? 0
  light.angle = opts.angle ?? 60
  light.penumbra = opts.penumbra ?? 0
  light.decay = opts.decay ?? 2
  light.castShadow = opts.castShadow === true
  light.shadow = { mapSize: 1024, bias: 0, normalBias: 0, radius: 1, near: 0.5 }
  if (opts.shadow !== undefined) mergeSpotShadow(light.shadow, opts.shadow)
  return light
}

function mergeSpotShadow(into: SpotLight["shadow"], update: SpotShadowOptions): void {
  if (update.mapSize !== undefined) into.mapSize = update.mapSize
  if (update.bias !== undefined) into.bias = update.bias
  if (update.normalBias !== undefined) into.normalBias = update.normalBias
  if (update.near !== undefined) {
    if (!(update.near > 0)) throw new Error("shadow.near must be a positive number")
    into.near = update.near
  }
}

export function createPointLight(opts: PointLightOptions = {}): PointLight {
  checkFalloff(opts)
  let light = makeNode("light") as PointLight
  light.type = "point"
  light.color = [...(opts.color ?? [1, 1, 1])] as Vec3
  light.intensity = opts.intensity ?? 1
  light.distance = opts.distance ?? 0
  light.decay = opts.decay ?? 2
  light.castShadow = opts.castShadow === true
  light.shadow = { mapSize: 1024, bias: 0, normalBias: 0, radius: 1, near: 0.5 }
  if (opts.shadow !== undefined) mergeSpotShadow(light.shadow, opts.shadow)
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

/** The write path for a light's own fields (color, intensity, direction,
 * the cone and falloff, or sky/ground); absent keys keep their value.
 * Its placement goes through setTransform like any node.
 * Frame-rate-safe. */
export function setLight(light: DirectionalLight, update: DirectionalLightOptions): void
export function setLight(light: SpotLight, update: SpotLightOptions): void
export function setLight(light: PointLight, update: PointLightOptions): void
export function setLight(light: HemisphereLight, update: HemisphereLightOptions): void
export function setLight(
  light: Light,
  update: DirectionalLightOptions & SpotLightOptions & PointLightOptions & HemisphereLightOptions,
): void {
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
  } else if (light.type === "spot") {
    checkSpot(update)
    if (update.direction !== undefined) light.direction = [...update.direction] as Vec3
    if (update.color !== undefined) light.color = [...update.color] as Vec3
    if (update.distance !== undefined) light.distance = update.distance
    if (update.angle !== undefined) light.angle = update.angle
    if (update.penumbra !== undefined) light.penumbra = update.penumbra
    if (update.decay !== undefined) light.decay = update.decay
    let shadowChanged = false
    if (update.castShadow !== undefined && update.castShadow !== light.castShadow) {
      light.castShadow = update.castShadow
      shadowChanged = true
    }
    if (update.shadow !== undefined) {
      mergeSpotShadow(light.shadow, update.shadow)
      shadowChanged = true
    }
    if (shadowChanged) light._scene?._shadowChanged(light)
  } else if (light.type === "point") {
    checkFalloff(update)
    if (update.color !== undefined) light.color = [...update.color] as Vec3
    if (update.distance !== undefined) light.distance = update.distance
    if (update.decay !== undefined) light.decay = update.decay
    let shadowChanged = false
    if (update.castShadow !== undefined && update.castShadow !== light.castShadow) {
      light.castShadow = update.castShadow
      shadowChanged = true
    }
    if (update.shadow !== undefined) {
      mergeSpotShadow(light.shadow, update.shadow)
      shadowChanged = true
    }
    if (shadowChanged) light._scene?._shadowChanged(light)
  } else {
    if (update.sky !== undefined) light.sky = [...update.sky] as Vec3
    if (update.ground !== undefined) light.ground = [...update.ground] as Vec3
  }
  light._scene?._lightChanged()
}