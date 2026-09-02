// The shadow half of a scene instance: the atlas every casting light's
// map is a tile of, the per-caster shadow views (made through the
// scene's own view machinery), and the map cameras (box, spot cone,
// cascades, point faces). One system per scene, built by makeShadowSystem with the
// scene's seams as `deps` - the deps type IS the coupling, documented.
// The receiving side (uShadowRect/uShadowFirst/uShadowMatrix writes onto
// targets) stays in scene.ts with the light fan-out; it reads the slots
// through forEachShadowSlot and the atlas through atlas().

import { createDrawTarget, depthTexture, destroyTexture, limits, setTargetRect, setTargetSize } from "@solidrt/core/gpu"
import type { ShaderParams, TextureId } from "@solidrt/core/gpu"
import { cascadeSplit, copy, frustumSliceSphere, lookAt as lookAtMatrix, mat4, snapToGrid, transformVector } from "./math.ts"
import type { Mat4, Vec3 } from "./math.ts"
import { MAX_SHADOW_MAPS } from "./glsl.ts"
import { shadowDepthMaterial } from "./material.ts"
import type { Material } from "./material.ts"
import { updateCamera } from "./camera.ts"
import type { Camera } from "./camera.ts"
import { worldInto } from "./node.ts"
import type { CastingLight, Light } from "./light.ts"
import type { Mesh } from "./mesh.ts"

// How a cascaded light slices the camera range: 0 uniform, 1
// logarithmic, halfway the "practical" split (near slices small, far
// ones not starved).
const CASCADE_SPLIT_LAMBDA = 0.5
// |y| of a light direction above this is straight up or down, where
// world up cannot serve as the shadow map's roll reference.
const VERTICAL_LIGHT = 0.99
// The spot shadow camera's far plane when the light has no `distance`
// cutoff - the directional box default.
const SPOT_SHADOW_FAR = 500

// Degrees added to a point-light face map's 90-degree fov: the guard
// band that keeps a face-seam fragment's occluder inside the map it
// samples (without it every seam shows a lit slit where each map's
// coverage ends at its edge). Costs a sliver of map resolution.
const POINT_SHADOW_FOV_GUARD = 4

// A point light's six face frusta in slot order (+X, -X, +Y, -Y, +Z,
// -Z - SHADOW_LOOKUP's dominant-axis select counts on it), each a
// square 90-degree perspective map from the light's world position.
// World-axis aligned: a point light has no direction and rotation does
// not matter. The up only orients the map inside its tile, which the
// lookup reads back through the same matrix, so any non-colinear
// choice works.
const POINT_FACES: { dir: Vec3; up: Vec3 }[] = [
  { dir: [1, 0, 0], up: [0, 1, 0] },
  { dir: [-1, 0, 0], up: [0, 1, 0] },
  { dir: [0, 1, 0], up: [0, 0, 1] },
  { dir: [0, -1, 0], up: [0, 0, 1] },
  { dir: [0, 0, 1], up: [0, 1, 0] },
  { dir: [0, 0, -1], up: [0, 1, 0] },
]

const IDENTITY = mat4()

// The uShadowAtlas binding while nothing casts: the depth texture of a
// one-texel draw target that renders nothing (its clear leaves depth 1,
// never shadowed), shared by every scene for the app. It must be a real
// depth texture - uShadowAtlas is a sampler2DShadow, and the engine
// refuses a color texture behind a comparison sampler.
let placeholder: TextureId | undefined

export function shadowPlaceholder(): TextureId {
  if (placeholder === undefined) {
    placeholder = createDrawTarget(1, 1, null, { depth: "texture", autoFree: false, label: "scene-shadow-none" })
  }
  return depthTexture(placeholder)
}

/** One tile's place in the atlas, in texels. */
export type ShadowRect = { x: number; y: number; width: number; height: number }

/** What the shadow system needs of a scene view: the tile target and the
 * camera it renders the map from. The scene's ViewRecord satisfies it;
 * the system never sees the rest. */
export type ShadowView = {
  texture: TextureId
  width: number
  height: number
  camera: Camera
}

/** The scene seams one shadow system runs on - the coupling, spelled
 * out. Arrays are the scene's own (read live, never copied); functions
 * close over the scene instance. */
export type ShadowSystemDeps<V extends ShadowView> = {
  /** The scene's light list, attach order = light index. */
  lights: Light[]
  /** The scene camera (cascades follow it). */
  camera: Camera
  /** The scene target's current size (the cascade fit's aspect). */
  targetSize(): { width: number; height: number }
  /** Base debug label ("scene" by default). */
  label: string
  /** Make one shadow view: a tile of `into` drawing `filter`'s meshes
   * with the depth override (the scene's makeView). */
  makeView(
    vopts: {
      width: number
      height: number
      into: TextureId
      x: number
      y: number
      overrideMaterial: Material
      clearColor: [number, number, number, number]
      label: string
    },
    filter: (mesh: Mesh) => boolean,
  ): V
  disposeView(view: V): void
  /** The light set changed shape: the scene owes a writeLights. */
  markLightsDirty(): void
  /** Schedule the scene's sync microtask. */
  schedule(): void
}

export type ShadowSystem = {
  /** The atlas while anything casts, else null. `texture` is the draw
   * target; its depth texture is the receivers' uShadowAtlas binding. */
  atlas(): { texture: TextureId; width: number; height: number } | null
  /** Enumerate the dealt map slots in light order, a light's cascades
   * consecutive and tightest first - the ONE enumeration the receiving
   * side is dealt by, so uShadowFirst/uShadowCount/uShadowRect and the
   * matrices always agree. `lightIndex` is the light's LIST index (its
   * uShadow*[i] slot): every non-hemisphere light counts, casting or
   * not, so it matches the lit loop's index. */
  forEachShadowSlot(fn: (slot: number, lightIndex: number, cascade: number, rect: ShadowRect) => void): void
  /** The light started casting (attach, or castShadow flipped on). */
  createShadow(light: CastingLight): void
  /** The light stopped casting (detach, or castShadow flipped off).
   * Safe when it never cast. */
  destroyShadow(light: CastingLight): void
  /** The light's castShadow/shadow options changed: rebuild, re-place or
   * re-fit whatever the change touches (SceneHooks._shadowChanged). */
  shadowChanged(light: CastingLight): void
  /** Re-place every caster's map cameras from its light's current world
   * matrix (each compares against the matrix it was last placed from, so
   * a scene animating elsewhere rewrites nothing). Run per sync;
   * `sceneCameraMoved` re-fits the cascaded lights too. */
  placeCameras(sceneCameraMoved: boolean): void
  /** The slot deal changed (writeLights) or a map camera wrote
   * (a shadow view's pending camera): the uShadowMatrix array is owed. */
  markMatricesDirty(): void
  /** Write the uShadowMatrix array through `write` if owed - the
   * matrices that render the maps are the ones receivers look up with. */
  flushMatrices(write: (params: ShaderParams) => void): void
  /** Drop the atlas and forget every caster. The shadow views die with
   * the scene's own view teardown, not here. */
  dispose(): void
}

export function makeShadowSystem<V extends ShadowView>(deps: ShadowSystemDeps<V>): ShadowSystem {
  // One per casting light: the views rendering its maps (one for a box
  // or spot light, `shadow.cascades` for a cascaded one, tightest
  // first) and `rects`, each tile's place in the atlas. `lastWorld` is
  // the light's world matrix the cameras were last placed from; `dirty`
  // forces a re-place (options changed).
  type Shadow = { light: CastingLight; views: V[]; lastWorld: Mat4; dirty: boolean; rects: ShadowRect[] }
  let shadows = new Map<CastingLight, Shadow>()
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
  // A caster's tile count: a directional light brings its cascades, a
  // point light its six faces, a spot exactly one map.
  let shadowTiles = (l: CastingLight): number =>
    l.type === "directional" ? l.shadow.cascades : l.type === "point" ? POINT_FACES.length : 1
  let eachSlot = (fn: (slot: number, lightIndex: number, shadow: Shadow, cascade: number) => void): void => {
    let slot = 0
    let i = 0
    for (let light of deps.lights) {
      if (light.type === "hemisphere") continue
      let shadow = shadows.get(light)
      if (shadow !== undefined) for (let c = 0; c < shadow.views.length; c++) fn(slot++, i, shadow, c)
      i++
    }
  }
  let forEachShadowSlot = (fn: (slot: number, lightIndex: number, cascade: number, rect: ShadowRect) => void): void => {
    eachSlot((slot, i, shadow, c) => fn(slot, i, c, shadow.rects[c]!))
  }
  // Place every shadow tile for the current caster set plus `adding` (not
  // yet in `shadows`; its rects are returned for the view creates), in
  // light order, a light's cascades consecutive. Sizes the atlas, moves
  // tiles whose place changed, and drops the atlas when nothing casts.
  // The rects reach receivers through the next light rewrite.
  let placeShadows = (adding: CastingLight | null): ShadowRect[] | null => {
    let casters: CastingLight[] = []
    for (let l of deps.lights) {
      if (l.type !== "hemisphere" && (shadows.has(l) || l === adding)) casters.push(l)
    }
    if (adding !== null && !casters.includes(adding)) casters.push(adding)
    deps.markLightsDirty()
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
      tiles += shadowTiles(l)
    }
    if (tiles > MAX_SHADOW_MAPS) {
      throw new Error(
        "The scene's shadow set is full: " + tiles + " maps over the " + MAX_SHADOW_MAPS +
          "-slot budget (a cascaded light claims shadow.cascades slots, a point light six)",
      )
    }
    let lay = shadowLayout(tiles, maxSize)
    if (shadowAtlas === null) {
      shadowAtlas = {
        texture: createDrawTarget(lay.width, lay.height, null, {
          depth: "texture",
          clearColor: [1, 1, 1, 1],
          label: deps.label + "-shadow-atlas",
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
      for (let c = 0; c < shadowTiles(l); c++, k++) {
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
  // A shadow's views: one square tile of the shadow atlas per map drawing
  // the casting meshes with the depth override from that map's frustum.
  // The light rewrite writes the rects in the light's slots on every
  // receiving target.
  let createShadow = (light: CastingLight) => {
    let rects = placeShadows(light)
    if (rects === null || shadowAtlas === null) return
    let atlas = shadowAtlas
    let views = rects.map(rect =>
      deps.makeView(
        {
          width: rect.width,
          height: rect.height,
          into: atlas.texture,
          x: rect.x,
          y: rect.y,
          overrideMaterial: shadowDepthMaterial(),
          clearColor: [1, 1, 1, 1],
          label: deps.label + "-shadow",
        },
        m => m.castShadow,
      ),
    )
    shadows.set(light, { light, views, lastWorld: mat4(), dirty: true, rects })
    deps.markLightsDirty()
  }
  let destroyShadow = (light: CastingLight) => {
    let shadow = shadows.get(light)
    if (shadow === undefined) return
    shadows.delete(light)
    for (let v of shadow.views) deps.disposeView(v)
    placeShadows(null)
    deps.markLightsDirty()
    deps.schedule()
  }
  // Place a shadow's cameras from its light's world matrix. A box light:
  // at its world position, looking along its world direction, the light
  // frustum as the orthographic extents. A spot light: the same pose
  // with a perspective camera, fov = its cone. A point light: the six
  // POINT_FACES frusta at its world position. Compared against the
  // matrix it was last placed from, so a scene animating elsewhere
  // rewrites nothing here. A cascaded light also follows the scene
  // camera (`cameraMoved`).
  let worldScratch = mat4()
  let shadowDir: Vec3 = [0, 0, 0]
  let cascadeScratch = mat4()
  let cascadeCenter: Vec3 = [0, 0, 0]
  let placeShadowCamera = (shadow: Shadow, cameraMoved: boolean) => {
    let light = shadow.light
    let m = worldInto(worldScratch, light)
    let cascaded = light.type === "directional" && shadow.views.length > 1
    if (!shadow.dirty && !(cascaded && cameraMoved) && m.every((x, i) => x === shadow.lastWorld[i])) return
    shadow.dirty = false
    copy(shadow.lastWorld, m)
    if (light.type === "point") {
      let far = light.distance > 0 ? light.distance : SPOT_SHADOW_FAR
      for (let f = 0; f < POINT_FACES.length; f++) {
        let face = POINT_FACES[f]!
        updateCamera(shadow.views[f]!.camera, {
          position: [m[12], m[13], m[14]],
          target: [m[12] + face.dir[0], m[13] + face.dir[1], m[14] + face.dir[2]],
          up: face.up,
          // A cube face spans exactly 90 degrees (square tile, aspect
          // 1); the guard band widens each map past its face so a
          // fragment at a face seam still finds its occluder inside the
          // map (the lookup's dominant-axis select is unchanged, so the
          // overlap is never sampled twice) - URP's fovBias.
          fov: 90 + POINT_SHADOW_FOV_GUARD,
          near: light.shadow.near,
          far,
        })
      }
      return
    }
    transformVector(shadowDir, m, light.direction)
    let len = Math.hypot(shadowDir[0], shadowDir[1], shadowDir[2]) || 1
    let d: Vec3 = [shadowDir[0] / len, shadowDir[1] / len, shadowDir[2] / len]
    // A sun straight down is the common case and the degenerate one for
    // world up: roll about z then (the map's orientation is invisible).
    let up: Vec3 = Math.abs(d[1]) > VERTICAL_LIGHT ? [0, 0, 1] : [0, 1, 0]
    if (light.type === "spot") {
      // The cone's circular footprint inscribes exactly in the square
      // map at fov = 2 * angle (aspect 1; both in degrees); everything
      // past the cone gets no light, so no shadow is lost to the
      // corners' margin.
      updateCamera(shadow.views[0]!.camera, {
        position: [m[12], m[13], m[14]],
        target: [m[12] + d[0], m[13] + d[1], m[14] + d[2]],
        up,
        fov: light.angle * 2,
        near: light.shadow.near,
        far: light.distance > 0 ? light.distance : SPOT_SHADOW_FAR,
      })
      return
    }
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
    let camera = deps.camera
    let n = shadow.views.length
    let near = camera.near
    let far = Math.min(camera.far, light.shadow.distance ?? Infinity)
    if (!(far > near)) far = near + 1
    // The light's rotation only: rows are its right, up and back axes.
    let basis = lookAtMatrix(cascadeScratch, [0, 0, 0], d, up)
    let size = deps.targetSize()
    let aspect = size.width / size.height
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
  // uShadowMatrix is one array param (the engine writes whole arrays), so
  // any shadow camera move rewrites all MAX_SHADOW_MAPS matrices, identity
  // in the slots that are not dealt.
  let shadowMatrices: number[] = new Array(MAX_SHADOW_MAPS * 16).fill(0)
  let shadowMatricesDirty = false
  return {
    atlas: () => shadowAtlas,
    forEachShadowSlot,
    createShadow,
    destroyShadow,
    shadowChanged(light) {
      let shadow = shadows.get(light)
      if (shadow !== undefined) {
        if (!light.castShadow) {
          destroyShadow(light)
          return
        }
        // A cascade count change is a different view set: rebuild it. A
        // mapSize change re-places every tile (the grid cell follows the
        // largest map).
        if (shadow.views.length !== shadowTiles(light)) {
          destroyShadow(light)
          createShadow(light)
          return
        }
        placeShadows(null)
        shadow.dirty = true
        deps.markLightsDirty()
        deps.schedule()
      } else if (light.castShadow) {
        createShadow(light)
      }
    },
    placeCameras(sceneCameraMoved) {
      for (let shadow of shadows.values()) placeShadowCamera(shadow, sceneCameraMoved)
    },
    markMatricesDirty() {
      shadowMatricesDirty = true
    },
    flushMatrices(write) {
      if (!shadowMatricesDirty) return
      shadowMatricesDirty = false
      let dealt = 0
      eachSlot((slot, _i, shadow, c) => {
        let m = shadow.views[c]!.camera.viewProj
        for (let k = 0; k < 16; k++) shadowMatrices[slot * 16 + k] = m[k]!
        dealt = slot + 1
      })
      for (let slot = dealt; slot < MAX_SHADOW_MAPS; slot++) for (let k = 0; k < 16; k++) shadowMatrices[slot * 16 + k] = IDENTITY[k]!
      write({ uShadowMatrix: shadowMatrices })
    },
    dispose() {
      shadows.clear()
      if (shadowAtlas !== null) {
        destroyTexture(shadowAtlas.texture)
        shadowAtlas = null
      }
    },
  }
}
