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