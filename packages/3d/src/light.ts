// Lights: the directional light (with its shadow options) and the
// hemisphere ambient, their constructors and the setLight write path.
// Scene reactions go through the node's SceneHooks (node.ts).

import { MAX_CASCADES } from "./glsl.ts"
import type { Vec3 } from "./math.ts"
import { checkMask } from "./mesh.ts"
import { makeNode } from "./node.ts"
import type { SceneNode } from "./node.ts"

// The default light layers mask: every bit set, so a light lights every
// mesh unless told otherwise (the Unity and Godot default).
const ALL_LAYERS = 0xffffffff

/** The orthographic frustum a casting light renders its shadow map from,
 * in the light's own space (x right, y up, looking along its direction),
 * Three's DirectionalLightShadow camera. Everything outside it is lit. */
export type ShadowCamera = { left: number; right: number; top: number; bottom: number; near: number; far: number }

/** The box a directional light without a `camera` option renders its one
 * map from: +-5 world units, 0.5..500 deep. */
export const DEFAULT_SHADOW_CAMERA: ShadowCamera = { left: -5, right: 5, top: 5, bottom: -5, near: 0.5, far: 500 }

export type ShadowOptions = {
  /** Shadow map resolution in texels, square; null (the default) is the
   * scene's quality tier's (QualityRow.mapSize: 512, 1024 or 2048). */
  mapSize?: number | null
  /** Depth bias against acne, in the map's 0..1 depth (default 0). */
  bias?: number
  /** Offset a receiving point along its normal before the lookup, in
   * world units (default 0) - the acne fix that keeps contact shadows. */
  normalBias?: number
  /** Filter radius in map texels, or null (the default) for the scene's
   * quality tier's width (1.5, 2.5 or 3.5: Unity's soft shadows, whose
   * tier sets the tent). 1 is the single hardware compare tap (a 2x2
   * bilinear PCF); above 1 a disc of the tier's taps spread over
   * `radius` texels, Three's `shadow.radius`, Godot's shadow_blur - a
   * uniform softening for the texel stairs a close receiver shows. Under
   * a light with a size (`angularSize`, `sourceRadius`) it is the FLOOR
   * the penumbra never narrows below (HDRP's min filter size). */
  radius?: number | null
  /** The light frustum of a one-map (box) light; absent keys keep the
   * current values, which start at DEFAULT_SHADOW_CAMERA (+-5, 0.5..500).
   * Null returns to the default box. Setting one makes the light a box
   * light unless `cascades` says otherwise (below); ignored by a cascaded
   * light (its frustums follow the scene camera). */
  camera?: Partial<ShadowCamera> | null
  /** Split the shadow into this many cascades (1..MAX_CASCADES; 1 = the
   * `camera` box). With more, the light renders one map per slice of the
   * SCENE camera's frustum (near .. far, tightest first), each `mapSize`
   * texels wide and fitted every time the camera or the light moves, and
   * a receiver samples the tightest one that has the point: sharp contact
   * shadows near the camera and coarser ones toward the horizon, for a
   * scene the box cannot cover at one map's resolution. Views
   * (`scene.createView`) sample the same maps, fitted to the scene
   * camera, not their own. Each cascade is a tile of the atlas. Null (the
   * default) is the scene's quality tier's count (QualityRow.cascades)
   * for a light that authored no `camera` box, and 1 for one that did:
   * an authored box is the light's frustum, and the tier never silently
   * replaces it. `splits` needs an explicit count. */
  cascades?: number | null
  /** How far from the scene camera a cascaded light shadows, in world
   * units (default null = the camera's far). The cascades span
   * near..distance and a point past it is lit, so pulling it in sharpens
   * every cascade; it also bounds the maps' depth range, which is what
   * `bias` is measured against. A box light ignores it. */
  distance?: number | null
  /** Where the cascades split: `cascades - 1` fractions of the
   * near..distance range, strictly ascending in 0..1, each the far bound
   * of one slice (Godot's shadow_split_1..3, Unity's cascade splits).
   * Absent or null (the default) the practical split places them,
   * halfway between uniform and logarithmic; give them to push
   * resolution outward (a high viewpoint over distant detail) or pull it
   * in (close quarters). A box light ignores it. */
  splits?: number[] | null
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
  /** The light's angular size in DEGREES, its disc's diameter as seen
   * from the scene (the sun's is about 0.53); default 0, a point source.
   * Above 0 a casting light's shadows harden where a caster meets its
   * receiver and widen with the distance between them (contact-hardening
   * soft shadows): the penumbra grows by the distance times the size.
   * Godot's light_angular_distance, Unreal's source angle, HDRP's
   * angular diameter. Write through setLight. */
  angularSize: number
  /** Layer filter bitmask (default all bits set): the light lights and
   * shadows only the meshes whose `layers` intersect it - Unity's
   * cullingMask, Godot's light_cull_mask. Write through setLight. */
  layers: number
  /** Render a shadow map from this light (any directional light may;
   * each map is a full extra pass over the casting meshes); meshes with
   * `castShadow` draw into it, `lit` materials read it (unless
   * `receiveShadow: false`). */
  castShadow: boolean
  /** The resolved shadow options (read; write through setLight). */
  shadow: {
    mapSize: number | null
    bias: number
    normalBias: number
    radius: number | null
    /** The authored box, or null for a light that set none (the default
     * box when it renders one map; see ShadowOptions.camera). */
    camera: ShadowCamera | null
    cascades: number | null
    distance: number | null
    splits: number[] | null
  }
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
  /** Layer filter bitmask (default all bits set; DirectionalLight.layers
   * is the contract). */
  layers: number
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
  /** The light source's radius in world units; default 0, a point
   * source. Above 0 a casting light's shadows harden at a contact and
   * widen with the caster-to-receiver distance over the light-to-caster
   * distance (contact-hardening soft shadows). Godot's light_size,
   * Unreal's source radius, HDRP's shape radius. Write through setLight. */
  sourceRadius: number
  /** Render a shadow map from this light: the directional machinery
   * with a perspective camera at the light's world position along its
   * world direction, fov = the cone (2 * angle), far = `distance` (or
   * the default when 0). One map, one shadow slot. */
  castShadow: boolean
  /** The resolved shadow options (read; write through setLight). */
  shadow: { mapSize: number | null; bias: number; normalBias: number; radius: number | null; near: number }
}

/** The perspective-map shadow options a spot or point light takes. */
export type SpotShadowOptions = {
  /** Shadow map resolution in texels, square; null (the default) is the
   * scene's quality tier's (QualityRow.mapSize). A point light renders
   * SIX maps at this size, one per face. */
  mapSize?: number | null
  /** Depth bias against acne, in the map's 0..1 depth (default 0). Note
   * a perspective map's depth is nonlinear: `normalBias` is the knob to
   * reach for first. */
  bias?: number
  /** Offset a receiving point along its normal before the lookup, in
   * world units (default 0). */
  normalBias?: number
  /** Filter radius in map texels, or null (the default) for the scene's
   * quality tier's width; 1 is the single hardware compare tap, above 1 a
   * disc of the tier's taps over `radius` texels (ShadowOptions.radius is
   * the contract); the floor under a light with a `sourceRadius`. */
  radius?: number | null
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
  /** Layer filter bitmask (default all bits set; DirectionalLight.layers
   * is the contract). */
  layers: number
  /** Falloff cutoff in world units (0 = no cutoff). */
  distance: number
  /** Falloff exponent (default 2). */
  decay: number
  /** The light source's radius in world units; default 0, a point
   * source (SpotLight.sourceRadius is the contract). */
  sourceRadius: number
  /** Render shadow maps from this light: six 90-degree face maps
   * (perspective, world-axis aligned) as tiles of the scene's shadow
   * atlas, claiming six shadow slots; a receiver picks the face by the
   * light-to-point direction. Far = `distance` (or the directional
   * default when 0), so give a casting bulb a distance. */
  castShadow: boolean
  /** The resolved shadow options (read; write through setLight). */
  shadow: { mapSize: number | null; bias: number; normalBias: number; radius: number | null; near: number }
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
  /** Layer filter bitmask (default all bits set; DirectionalLight.layers
   * is the contract): a mesh outside it gets no hemisphere term (its own
   * `Surface.ambient` still applies). */
  layers: number
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
  /** Angular size in degrees, default 0 (DirectionalLight.angularSize). */
  angularSize?: number
  /** Layer filter bitmask, default all bits set (the light lights and
   * shadows only meshes whose `layers` intersect it). */
  layers?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: ShadowOptions
}
export type SpotLightOptions = {
  direction?: Vec3
  color?: Vec3
  intensity?: number
  /** Layer filter bitmask, default all bits set. */
  layers?: number
  distance?: number
  angle?: number
  penumbra?: number
  decay?: number
  /** Source radius in world units, default 0 (SpotLight.sourceRadius). */
  sourceRadius?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: SpotShadowOptions
}
export type PointLightOptions = {
  color?: Vec3
  intensity?: number
  /** Layer filter bitmask, default all bits set. */
  layers?: number
  distance?: number
  decay?: number
  /** Source radius in world units, default 0 (SpotLight.sourceRadius). */
  sourceRadius?: number
  castShadow?: boolean
  /** Shadow-map options, merged key by key (setLight keeps unmentioned ones). */
  shadow?: SpotShadowOptions
}
export type HemisphereLightOptions = {
  sky?: Vec3
  ground?: Vec3
  intensity?: number
  /** Layer filter bitmask, default all bits set. */
  layers?: number
}

// Every light may cast; the bound is the shadow-slot budget
// (MAX_SHADOW_MAPS in glsl.ts, exported from the root with MAX_LIGHTS and
// MAX_CASCADES: a directional light claims `shadow.cascades` slots, a
// point light six, a spot one). Every map is a tile of the scene's one
// shadow atlas, so the pass count never follows the caster count, the
// fill does.

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

function checkRadius(radius: number | null): number | null {
  if (radius === null) return null
  if (!(radius >= 1)) throw new Error("shadow.radius must be >= 1 (1 = the single hardware tap), or null for the scene tier's width")
  return radius
}

// A directional light's disc spans less than the whole sky.
const ANGULAR_SIZE_MAX = 180

function checkAngularSize(size: number): number {
  if (!(size >= 0 && size < ANGULAR_SIZE_MAX)) throw new Error("Light angularSize must be in [0, 180) degrees (0 = a point source)")
  return size
}

function checkSourceRadius(radius: number): number {
  if (!(radius >= 0)) throw new Error("Light sourceRadius must be >= 0 (0 = a point source)")
  return radius
}

function checkMapSize(mapSize: number | null): number | null {
  if (mapSize === null) return null
  if (!(Number.isInteger(mapSize) && mapSize >= 1)) throw new Error("shadow.mapSize must be a positive integer, or null for the scene tier's")
  return mapSize
}

function mergeShadow(into: DirectionalLight["shadow"], update: ShadowOptions): void {
  if (update.mapSize !== undefined) into.mapSize = checkMapSize(update.mapSize)
  if (update.bias !== undefined) into.bias = update.bias
  if (update.normalBias !== undefined) into.normalBias = update.normalBias
  if (update.radius !== undefined) into.radius = checkRadius(update.radius)
  if (update.camera === null) {
    into.camera = null
  } else if (update.camera !== undefined) {
    // Only the keys with a value: an undefined one is absent and keeps
    // the extent, which the light projection reads as is. The first
    // write authors the box, starting from the default one.
    let camera = into.camera ?? { ...DEFAULT_SHADOW_CAMERA }
    for (let key of Object.keys(update.camera) as (keyof ShadowCamera)[]) {
      let value = update.camera[key]
      if (value !== undefined) camera[key] = value
    }
    into.camera = camera
  }
  if (update.cascades !== undefined) {
    let n = update.cascades
    if (n !== null && (!Number.isInteger(n) || n < 1 || n > MAX_CASCADES)) {
      throw new Error("shadow.cascades must be an integer from 1 to " + MAX_CASCADES + ", or null for the scene tier's")
    }
    into.cascades = n
  }
  if (update.distance !== undefined) {
    let d = update.distance
    if (d !== null && !(d > 0)) throw new Error("shadow.distance must be a positive number or null")
    into.distance = d
  }
  if (update.splits !== undefined) {
    let s = update.splits
    if (s !== null) {
      if (!Array.isArray(s)) throw new Error("shadow.splits must be an array of fractions or null")
      s.forEach((t, i) => {
        if (!(t > 0 && t < 1)) throw new Error("shadow.splits[" + i + "] must be a fraction strictly between 0 and 1")
        if (i > 0 && !(t > s[i - 1]!)) throw new Error("shadow.splits must be strictly ascending")
      })
    }
    into.splits = s === null ? null : [...s]
  }
  // Checked after both merge: cascades and splits may arrive in one update.
  if (into.splits !== null) {
    if (into.cascades === null) throw new Error("shadow.splits places explicit cascades - give cascades with it")
    if (into.splits.length !== into.cascades - 1) {
      throw new Error("shadow.splits must hold cascades - 1 fractions (" + (into.cascades - 1) + " for " + into.cascades + " cascades), got " + into.splits.length)
    }
  }
}

export function createDirectionalLight(opts: DirectionalLightOptions = {}): DirectionalLight {
  let light = makeNode("light") as DirectionalLight
  light.type = "directional"
  light.direction = [...(opts.direction ?? [0, -1, 0])] as Vec3
  light.color = [...(opts.color ?? [1, 1, 1])] as Vec3
  light.intensity = opts.intensity ?? 1
  light.angularSize = opts.angularSize !== undefined ? checkAngularSize(opts.angularSize) : 0
  light.layers = opts.layers !== undefined ? checkMask(opts.layers, "createDirectionalLight") : ALL_LAYERS
  light.castShadow = opts.castShadow === true
  light.shadow = { mapSize: null, bias: 0, normalBias: 0, radius: null, camera: null, cascades: null, distance: null, splits: null }
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
  light.layers = opts.layers !== undefined ? checkMask(opts.layers, "createSpotLight") : ALL_LAYERS
  light.distance = opts.distance ?? 0
  light.angle = opts.angle ?? 60
  light.penumbra = opts.penumbra ?? 0
  light.decay = opts.decay ?? 2
  light.sourceRadius = opts.sourceRadius !== undefined ? checkSourceRadius(opts.sourceRadius) : 0
  light.castShadow = opts.castShadow === true
  light.shadow = { mapSize: null, bias: 0, normalBias: 0, radius: null, near: 0.5 }
  if (opts.shadow !== undefined) mergeSpotShadow(light.shadow, opts.shadow)
  return light
}

function mergeSpotShadow(into: SpotLight["shadow"], update: SpotShadowOptions): void {
  if (update.mapSize !== undefined) into.mapSize = checkMapSize(update.mapSize)
  if (update.bias !== undefined) into.bias = update.bias
  if (update.normalBias !== undefined) into.normalBias = update.normalBias
  if (update.radius !== undefined) into.radius = checkRadius(update.radius)
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
  light.layers = opts.layers !== undefined ? checkMask(opts.layers, "createPointLight") : ALL_LAYERS
  light.distance = opts.distance ?? 0
  light.decay = opts.decay ?? 2
  light.sourceRadius = opts.sourceRadius !== undefined ? checkSourceRadius(opts.sourceRadius) : 0
  light.castShadow = opts.castShadow === true
  light.shadow = { mapSize: null, bias: 0, normalBias: 0, radius: null, near: 0.5 }
  if (opts.shadow !== undefined) mergeSpotShadow(light.shadow, opts.shadow)
  return light
}

export function createHemisphereLight(opts: HemisphereLightOptions = {}): HemisphereLight {
  let light = makeNode("light") as HemisphereLight
  light.type = "hemisphere"
  light.sky = [...(opts.sky ?? [1, 1, 1])] as Vec3
  light.ground = [...(opts.ground ?? [0.2, 0.2, 0.2])] as Vec3
  light.intensity = opts.intensity ?? 1
  light.layers = opts.layers !== undefined ? checkMask(opts.layers, "createHemisphereLight") : ALL_LAYERS
  return light
}

/** The write path for a light's own fields (color, intensity, direction,
 * layers, the cone and falloff, or sky/ground); absent keys keep their
 * value. Its placement goes through setTransform like any node.
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
  if (update.layers !== undefined) {
    let mask = checkMask(update.layers, "setLight")
    if (mask !== light.layers) {
      light.layers = mask
      // A casting light's shadow views re-filter their casters by the new
      // mask (a re-attach, never a tile rebuild - the maps stay).
      light._scene?._lightLayersChanged(light)
    }
  }
  if (light.type === "directional") {
    if (update.direction !== undefined) light.direction = [...update.direction] as Vec3
    if (update.color !== undefined) light.color = [...update.color] as Vec3
    if (update.angularSize !== undefined) light.angularSize = checkAngularSize(update.angularSize)
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
    if (update.sourceRadius !== undefined) light.sourceRadius = checkSourceRadius(update.sourceRadius)
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
    if (update.sourceRadius !== undefined) light.sourceRadius = checkSourceRadius(update.sourceRadius)
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