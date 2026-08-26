// The Solid face: PascalCase components over context, syncing the retained
// scene (scene.ts) - no new intrinsic elements, no renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained nodes and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// node with `ref` and call setTransform from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the scene.

import { createContext, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, TextureId, VoidComponent } from "@solidrt/core"
import {
  add,
  createDirectionalLight,
  createGroup,
  createHemisphereLight,
  createInstancedMesh,
  createMesh,
  createScene,
  createSprite,
  disposeInstances,
  remove,
  setGeometry,
  setInstanceCount,
  setInstances,
  setLight,
  setCastShadow,
  setMaterial,
  setMeshParams,
  setRenderOrder,
  setTransform,
  setVisible,
} from "./scene.ts"
import type { ShaderParams } from "@solidrt/core/gpu"
import type { DirectionalLight as DirectionalLightNode, HemisphereLight as HemisphereLightNode, InstancedMesh as InstancedMeshNode, Mesh as MeshNode, Scene as SceneHandle, SceneNode, ScenePointerEvent, ShadowOptions } from "./scene.ts"
import type { Geometry } from "./geometry.ts"
import type { Material } from "./material.ts"
import type { Quat, Vec3 } from "./math.ts"

type SceneCtx = { scene: SceneHandle; parent: SceneNode }
let SceneContext = createContext<SceneCtx>()

/**
 * The enclosing scene and parent node - the imperative escape hatch inside
 * a component subtree (throws outside a `<Scene>`).
 */
export function useScene(): SceneCtx {
  return useContext(SceneContext)
}

export type TransformProps = {
  position?: Vec3
  /** Euler radians in XYZ order (x first), Three's `Euler` default. */
  rotation?: Vec3
  /** The rotation as a quaternion - what the node stores. Pass this or
   * `rotation`, not both. */
  quaternion?: Quat
  scale?: Vec3 | number
  visible?: boolean
}

/**
 * Mesh pointer events, the element vocabulary one tree deeper: the nearest
 * hit mesh receives the event, down/move/up bubble to ancestor Groups
 * (stopPropagation stops the walk), enter/leave pair on the mesh alone.
 * Events flow while the element showing the scene carries scene.handlers -
 * the built-in <Scene> leaf does (opt out with events={false}); an `output`
 * leaf spreads them itself.
 */
export type PointerEventProps = {
  onPointerDown?: (event: ScenePointerEvent) => void
  onPointerMove?: (event: ScenePointerEvent) => void
  onPointerUp?: (event: ScenePointerEvent) => void
  /** Meshes only: a Group never receives enter/leave. */
  onPointerEnter?: (event: ScenePointerEvent) => void
  onPointerLeave?: (event: ScenePointerEvent) => void
}

function syncNode(node: SceneNode, props: TransformProps & PointerEventProps): void {
  createEffect(
    () => [props.position, props.rotation, props.quaternion, props.scale, props.visible] as const,
    ([position, rotation, quaternion, scale, visible]) => {
      setTransform(node, { position, rotation, quaternion, scale })
      setVisible(node, visible !== false)
    },
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerEnter, props.onPointerLeave] as const,
    ([down, move, up, enter, leave]) => {
      node.onPointerDown = down
      node.onPointerMove = move
      node.onPointerUp = up
      node.onPointerEnter = enter
      node.onPointerLeave = leave
    },
  )
}

export type SceneProps = {
  /** Target pixels. With `output`, the leaf's own width/height are layout,
   * so render size and display size separate (supersampling). */
  width: number
  height: number
  clearColor?: [number, number, number, number]
  /** Fragment GLSL drawn behind the meshes, inside the scene's own pass
   * (scene.setBackground): vUV/iResolution/fragColor contract, so a
   * createShaderTexture backdrop ports verbatim. Reactive - swapping the
   * source replaces the background; undefined removes it. Three's
   * `scene.background = color` is `clearColor` here. */
  background?: string
  label?: string
  /** Multisample count (1, 2, 4 or 8; default 1): anti-aliased mesh edges.
   * Fixed at creation. */
  samples?: 1 | 2 | 4 | 8
  ref?: (scene: SceneHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the scene's
   * texture id, and its return renders in place of the built-in `<texture>`
   * leaf - a `<d-texture>`, a leaf carrying paint/pointer/layout props, or
   * a post-effect chain (a shader target sampling the id; created in the
   * callback it disposes with the Scene). Return null to render no leaf.
   * Mesh pointer events then need the scene's handlers on your leaf:
   * `<texture src={texture} {...useScene().scene.handlers} />`.
   */
  output?: (texture: TextureId) => Element
  /**
   * Mesh pointer events (default on): the built-in leaf carries
   * scene.handlers, so Mesh/Group onPointer* props receive events. `false`
   * detaches them - the leaf then costs no pointer routing at all.
   */
  events?: boolean
}

/**
 * Owns a draw target and composites it as an ordinary `<texture>` leaf, so
 * the output takes layout, transforms, blendMode, and pointer events like
 * any element - or hand `output` the texture id and compose it yourself.
 * Children (Mesh/Group/PerspectiveCamera) render nothing themselves - they
 * populate the retained scene through context.
 */
export let Scene: ParentComponent<SceneProps> = props => {
  let scene = untrack(() =>
    createScene(props.width, props.height, { clearColor: props.clearColor, label: props.label, samples: props.samples }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => scene.setSize(w, h),
  )
  createEffect(
    () => props.background,
    b => scene.setBackground(b ?? null),
  )
  untrack(() => props.ref)?.(scene)
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  return (
    <SceneContext value={{ scene, parent: scene.root }}>
      {output ? (
        untrack(() => output(scene.texture))
      ) : (
        <texture
          src={scene.texture}
          width={props.width}
          height={props.height}
          onPointerDown={events ? scene.handlers.onPointerDown : undefined}
          onPointerMove={events ? scene.handlers.onPointerMove : undefined}
          onPointerUp={events ? scene.handlers.onPointerUp : undefined}
          onPointerLeave={events ? scene.handlers.onPointerLeave : undefined}
        />
      )}
      {props.children}
    </SceneContext>
  )
}

/** A transform node: children inherit its position/rotation/scale. */
export let Group: ParentComponent<TransformProps & PointerEventProps & { ref?: (node: SceneNode) => void }> = props => {
  let ctx = useContext(SceneContext)
  let node = createGroup()
  add(ctx.parent, node)
  syncNode(node, props)
  untrack(() => props.ref)?.(node)
  onCleanup(() => remove(node))
  return <SceneContext value={{ scene: ctx.scene, parent: node }}>{props.children}</SceneContext>
}

export type MeshProps = TransformProps & PointerEventProps & {
  geometry: Geometry
  material: Material
  /** Per-mesh uniforms for a custom material (setMeshParams as a prop).
   * Keys merge - a key that disappears keeps its old value; there is no
   * unset. Names must be declared by the material's shaders. For values
   * changing every frame prefer `ref` + setMeshParams from onFrame, the
   * same split as setTransform. */
  params?: ShaderParams
  /** Explicit draw-order key (setRenderOrder as a prop); default 0. */
  renderOrder?: number
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. Needs a `castShadow` DirectionalLight to show. */
  castShadow?: boolean
  ref?: (mesh: MeshNode) => void
}

// The mesh-side props Mesh and Sprite share (Sprite has no geometry).
function syncMesh(mesh: MeshNode, props: SpriteProps): void {
  createEffect(
    () => props.material,
    m => setMaterial(mesh, m),
    { defer: true },
  )
  createEffect(
    () => props.params,
    p => {
      if (p !== undefined) setMeshParams(mesh, p)
    },
  )
  createEffect(
    () => props.renderOrder,
    o => setRenderOrder(mesh, o ?? 0),
  )
  syncNode(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => remove(mesh))
}

/** One draw entry: geometry drawn with a material at a transform. */
export let Mesh: VoidComponent<MeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createMesh(props.geometry, props.material))
  add(ctx.parent, mesh)
  createEffect(
    () => props.geometry,
    g => setGeometry(mesh, g),
    { defer: true },
  )
  createEffect(
    () => props.castShadow,
    c => setCastShadow(mesh, c === true),
  )
  syncMesh(mesh, props)
  return null
}

export type SpriteProps = TransformProps & PointerEventProps & {
  /** A `sprite()` material (any material draws, only a sprite one turns). */
  material: Material
  /** Per-mesh uniforms, merge semantics - as on Mesh. */
  params?: ShaderParams
  /** Explicit draw-order key (setRenderOrder as a prop); default 0. */
  renderOrder?: number
  ref?: (mesh: MeshNode) => void
}

/** A camera-facing unit quad (createSprite as a component): no geometry
 * prop, `scale` is its world size, rotation is ignored. */
export let Sprite: VoidComponent<SpriteProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createSprite(props.material))
  add(ctx.parent, mesh)
  syncMesh(mesh, props)
  return null
}

export type InstancedMeshProps = TransformProps & PointerEventProps & {
  geometry: Geometry
  /** Must declare instanceAttributes (shaderMaterialClass). */
  material: Material
  /** Interleaved per-instance records (stride = the material's instance
   * attributes summed). Reactive; a later array larger than the buffer
   * grows it (capacity doubles into a replacement buffer). */
  records: Float32Array
  /** How many records draw; default all of the latest `records`. */
  count?: number
  /** LOCAL bounds covering every instance ([minX..maxZ]), fixed at
   * creation. Without them the mesh has no picking leaf, so pointer events
   * never target it. */
  bounds?: ArrayLike<number>
  /** Per-mesh uniforms, merge semantics - as on Mesh. */
  params?: ShaderParams
  /** Explicit draw-order key (setRenderOrder as a prop); default 0. */
  renderOrder?: number
  ref?: (mesh: InstancedMeshNode) => void
}

/** One draw entry covering N instances: geometry repeated per record of
 * `records` (createInstancedMesh as a component). The record buffer is
 * component-owned and freed on unmount. */
export let InstancedMesh: VoidComponent<InstancedMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() =>
    createInstancedMesh(props.geometry, props.material, props.records, props.count, { bounds: props.bounds }),
  )
  add(ctx.parent, mesh)
  createEffect(
    () => props.records,
    r => setInstances(mesh, r, untrack(() => props.count)),
    { defer: true },
  )
  createEffect(
    () => props.count,
    c => {
      if (c !== undefined) setInstanceCount(mesh, c)
    },
    { defer: true },
  )
  createEffect(
    () => props.geometry,
    g => setGeometry(mesh, g),
    { defer: true },
  )
  createEffect(
    () => props.material,
    m => setMaterial(mesh, m),
    { defer: true },
  )
  createEffect(
    () => props.params,
    p => {
      if (p !== undefined) setMeshParams(mesh, p)
    },
  )
  createEffect(
    () => props.renderOrder,
    o => setRenderOrder(mesh, o ?? 0),
  )
  syncNode(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => disposeInstances(mesh))
  return null
}

export type PerspectiveCameraProps = {
  /** Vertical field of view in DEGREES (default 60). */
  fov?: number
  near?: number
  far?: number
  position?: Vec3
  lookAt?: Vec3
  up?: Vec3
}

/**
 * Drives the scene's camera from props (the scene has a default camera, so
 * this component is optional). The camera is scene state, not a tree node:
 * to orbit it, update `position`/`lookAt`.
 */
export let PerspectiveCamera: VoidComponent<PerspectiveCameraProps> = props => {
  let ctx = useContext(SceneContext)
  createEffect(
    () => [props.fov, props.near, props.far, props.position, props.lookAt, props.up] as const,
    ([fov, near, far, position, lookAt, up]) =>
      ctx.scene.setCamera({ fov, near, far, position, target: lookAt, up }),
  )
  return null
}

export type HemisphereLightProps = { sky?: Vec3; ground?: Vec3; intensity?: number; ref?: (light: HemisphereLightNode) => void }

/** The scene's ambient term as a node (createHemisphereLight); one per
 * scene, the last mounted wins. */
export let HemisphereLight: VoidComponent<HemisphereLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() => createHemisphereLight({ sky: props.sky, ground: props.ground, intensity: props.intensity }))
  add(ctx.parent, light)
  createEffect(
    () => [props.sky, props.ground, props.intensity] as const,
    ([sky, ground, intensity]) => setLight(light, { sky, ground, intensity }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => remove(light))
  return null
}

export type DirectionalLightProps = TransformProps & {
  /** Travel direction in the node's local space; default [0, -1, 0]. */
  direction?: Vec3
  color?: Vec3
  intensity?: number
  /** Render the scene's shadow map from this light (one per scene). Its
   * shadow camera sits at the light's WORLD position, so give a casting
   * light a `position` above the scene. */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, camera frustum),
   * merged key by key. */
  shadow?: ShadowOptions
  ref?: (light: DirectionalLightNode) => void
}

/** A directional light node (createDirectionalLight): a parent Group's
 * rotation turns it; up to MAX_LIGHTS per scene, in mount order. */
export let DirectionalLight: VoidComponent<DirectionalLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() =>
    createDirectionalLight({
      direction: props.direction,
      color: props.color,
      intensity: props.intensity,
      castShadow: props.castShadow,
      shadow: props.shadow,
    }),
  )
  add(ctx.parent, light)
  syncNode(light, props)
  createEffect(
    () => [props.direction, props.color, props.intensity, props.castShadow, props.shadow] as const,
    ([direction, color, intensity, castShadow, shadow]) => setLight(light, { direction, color, intensity, castShadow, shadow }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => remove(light))
  return null
}
