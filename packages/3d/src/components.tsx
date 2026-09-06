// The Solid face: PascalCase components over context, syncing the retained
// scene (node/mesh/light/scene.ts) - no new intrinsic elements, no
// renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained nodes and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// node with `ref` and call setTransform from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the scene.

import { merge } from "@solidjs/signals"
import { createContext, createEffect, createSignal, displayScale, getBoundingBoxViewport, getLayoutBox, onCleanup, onFrame, onLayout, setFocus, untrack, useContext } from "@solidrt/core"
import type { Element, KeyEvent, ParentComponent, PointerEvent, TextureId, VoidComponent, WheelEvent } from "@solidrt/core"
import { createOrbitCamera } from "./orbit.ts"
import type { OrbitCamera as OrbitCameraHandle, OrbitCameraOptions, OrbitPose } from "./orbit.ts"
import { createFirstPersonCamera } from "./first-person.ts"
import type { FirstPersonCamera as FirstPersonCameraHandle, FirstPersonCameraOptions, FirstPersonPose } from "./first-person.ts"
import { add, createGroup, remove, setTransform, setTransition, setVisible } from "./node.ts"
import type { SceneNode, ScenePointerEvent, TransitionEndEvent } from "./node.ts"
import {
  addInstance,
  createInstancedMesh,
  createMesh,
  createRecordMesh,
  createSprite,
  disposeInstances,
  removeInstance,
  setCastShadow,
  setCulling,
  setGeometry,
  setInstanceStyle,
  setLayers,
  setMaterial,
  setMeshParams,
  setRecordCount,
  setRecords,
  setRenderOrder,
} from "./mesh.ts"
import type { InstancedMesh as InstancedMeshNode, InstanceNode, Mesh as MeshNode, RecordMesh as RecordMeshNode } from "./mesh.ts"
import { createDirectionalLight, createHemisphereLight, createPointLight, createSpotLight, setLight } from "./light.ts"
import type {
  DirectionalLight as DirectionalLightNode,
  HemisphereLight as HemisphereLightNode,
  PointLight as PointLightNode,
  ShadowOptions,
  SpotLight as SpotLightNode,
  SpotShadowOptions,
} from "./light.ts"
import { createScene } from "./scene.ts"
import type { CameraUpdate } from "./camera.ts"
import type { EnvironmentOptions, FogOptions, Scene as SceneHandle, SkyboxOptions, ToneMapping } from "./scene.ts"
import type { ShaderParams } from "@solidrt/core/gpu"
import type { NodeTransition } from "flux:spatial"
import type { Geometry } from "./geometry.ts"
import type { Material } from "./material.ts"
import type { Quat, Vec3 } from "./math.ts"

/** A camera control's input feed - the handlers shape the camera controls
 * expose, every field optional. A listener with key handlers makes the
 * leaf focusable and focused on pointer down, since keys route through
 * the focused node; onBlur tells it the held keys will get no key-up. */
export type SceneInputListener = {
  onPointerDown?(event: PointerEvent): void
  onPointerMove?(event: PointerEvent): void
  onPointerUp?(event: PointerEvent): void
  onWheel?(event: WheelEvent): void
  onKeyDown?(event: KeyEvent): void
  onKeyUp?(event: KeyEvent): void
  onBlur?(): void
}

/**
 * The channel between the element showing the scene and camera-control
 * components (`<OrbitCamera>`, `<FirstPersonCamera>`): the leaf feeds
 * pointer, wheel and key events in, controls listen. The built-in
 * `<Scene>` leaf is wired automatically; a custom `output` leaf spreads
 * `{...useScene().input.handlersFor(layout)}` beside its scene.handlersFor
 * spread, with the same `layout`, and declares itself `focusable` when a
 * key-driven control is in the scene (the spread's onPointerDown focuses
 * it; keys reach only the focused node).
 */
export type SceneInput = {
  /** Spreadable pointer + wheel + key handlers for the leaf. `layout`
   * reports the leaf's laid-out size - what viewport-relative controls
   * scale to - and is read per event, so a reactive layout just works.
   * `node` is the leaf itself, focused on pointer down while a key
   * listener is registered. */
  handlersFor(layout: () => { width: number; height: number }, node?: () => { id: number } | undefined): {
    onPointerDown(event: PointerEvent): void
    onPointerMove(event: PointerEvent): void
    onPointerUp(event: PointerEvent): void
    onWheel(event: WheelEvent): void
    onKeyDown(event: KeyEvent): void
    onKeyUp(event: KeyEvent): void
    onBlur(): void
  }
  /** Subscribe a control; returns the unsubscribe. */
  add(listener: SceneInputListener): () => void
  /** The showing leaf's laid-out size, null before one registered a
   * layout (handlersFor supplies it). */
  layout(): { width: number; height: number } | null
}

type SceneCtx = { scene: SceneHandle; parent: SceneNode; input: SceneInput }
let SceneContext = createContext<SceneCtx>()

/**
 * The enclosing scene, parent node and input channel - the imperative
 * escape hatch inside a component subtree (throws outside a `<Scene>`).
 */
export function useScene(): SceneCtx {
  return useContext(SceneContext)
}

// The scene context for a node component's children. Solid's provider is
// a root and two memos over the children, paid even when there are none,
// so it is built only when the JSX has children: the compiler emits the
// `children` getter only then, so the test is structural, not a read
// (probes/3d-instance-mount-bench.tsx has the per-row cost).
function provide(ctx: SceneCtx, parent: SceneNode, props: { children?: Element }): Element | undefined {
  if (!("children" in props)) return undefined
  return <SceneContext value={{ scene: ctx.scene, parent, input: ctx.input }}>{props.children}</SceneContext>
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
  /** How transform-prop changes animate (see setTransition); the mount
   * transform always snaps. */
  transition?: NodeTransition | string | null
  /** A declared transition settled on one component. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
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
  // One effect for the transform and the handlers: re-assigning six
  // handler fields on a transform write is free, an effect of its own is
  // not (a node component's mount cost is mostly its effects and its
  // context provider, see probes/3d-instance-mount-bench.tsx).
  createEffect(
    () =>
      [
        props.position,
        props.rotation,
        props.quaternion,
        props.scale,
        props.visible,
        props.onPointerDown,
        props.onPointerMove,
        props.onPointerUp,
        props.onPointerEnter,
        props.onPointerLeave,
        props.onTransitionEnd,
      ] as const,
    ([position, rotation, quaternion, scale, visible, down, move, up, enter, leave, end]) => {
      setTransform(node, { position, rotation, quaternion, scale })
      setVisible(node, visible !== false)
      node.onPointerDown = down
      node.onPointerMove = move
      node.onPointerUp = up
      node.onPointerEnter = enter
      node.onPointerLeave = leave
      node.onTransitionEnd = end
    },
  )
  // After the transform effect, so the mount transform snaps before writes
  // animate. Its own effect: merged above, every transform write would
  // re-send the config to the core.
  createEffect(
    () => props.transition,
    transition => setTransition(node, transition ?? null),
  )
}

export type SceneProps = {
  /**
   * Target pixels - give both, or neither. Omitted, the scene FILLS: the
   * built-in leaf is laid out at 100% of its parent's box (give it a sized
   * parent, as on the web) and the target tracks the leaf's on-screen size
   * in device pixels - display scale, designSize fits and ancestor
   * transforms included - so a bare `<Scene>` renders at native density on
   * any display, and viewport-relative camera controls scale with the box.
   * Fill or fixed is decided at mount. `output` needs explicit sizes (the
   * target cannot follow a leaf it does not own); the leaf's own
   * width/height are then layout, so render size and display size separate
   * (supersampling).
   */
  width?: number
  height?: number
  clearColor?: [number, number, number, number]
  /**
   * Drive the scene camera declaratively (scene.setCamera as a prop): a
   * partial CameraUpdate, absent keys keep their values - `ortho` included,
   * which `<PerspectiveCamera>` by its name never sets. The prop and the
   * `<PerspectiveCamera>` child write the same scene state, so use one
   * form, not both (last write wins). The 2d layers' `camera` prop, one
   * dimension up.
   */
  camera?: CameraUpdate
  /** The background drawn behind the meshes, inside the scene's own pass
   * (scene.setBackground): fragment GLSL (vUV/iResolution/fragColor plus
   * the vRay view ray, so a createShaderTexture backdrop ports verbatim
   * and a directional sky is a few lines) or a skybox `{ cube, intensity?,
   * rotation? }`. Reactive - swapping the source replaces the background,
   * a skybox's knobs update in place; undefined removes it. Three's
   * `scene.background = color` is `clearColor` here. */
  background?: string | SkyboxOptions
  /** The cube map reflective `lit` materials mirror (scene.setEnvironment):
   * `{ cube, intensity?, rotation? }`, typically the skybox's own cube
   * turned with it. Reactive; undefined removes it. */
  environment?: EnvironmentOptions
  /** Scene-wide fog (scene.setFog): linear `{ color, near, far }` or exp2
   * `{ color, density }`, optionally thinning above `height` by
   * `heightFalloff`; every standard material fades toward `color` by
   * distance from the camera. Reactive; undefined removes it. Match
   * `color` to clearColor or the background, which is not fogged. */
  fog?: FogOptions
  /** Output tone mapping (scene.setToneMapping): "none" (default) or
   * "aces". Reactive. */
  toneMapping?: ToneMapping
  /** Output exposure (scene.setExposure), default 1. Reactive. */
  exposure?: number
  /** The scene target's layer mask (scene.setLayers as a prop; default 1):
   * the scene draws the meshes whose `layers` intersect it. Reactive. */
  layers?: number
  /** `"texture"` exposes the target's depth as `scene.depthTexture` (a
   * depth-reading post effect's input). Fixed at creation; not with
   * `samples`. */
  depth?: true | "texture"
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
// Creation size of a fill-mode target: the first onLayout replaces it
// before the first paint, so it only has to be a valid texture size.
const FILL_INITIAL_SIZE = 1

export let Scene: ParentComponent<SceneProps> = props => {
  // Fill vs fixed target is decided at mount, like `output`: both
  // width and height (fixed pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("Scene: width and height come together - give both (fixed target) or neither (fill)")
    }
    if (props.width === undefined && props.output) {
      throw new Error("Scene: fill needs the built-in leaf - with output, give width and height")
    }
    return props.width === undefined
  })
  // The initial props seed createScene, so `ref` (below) hands out a
  // configured scene and the first frame draws it whole; the effects
  // after this follow changes only (`defer`).
  let scene = untrack(() => {
    let s = createScene(props.width ?? FILL_INITIAL_SIZE, props.height ?? FILL_INITIAL_SIZE, {
      clearColor: props.clearColor,
      label: props.label,
      samples: props.samples,
      depth: props.depth,
      background: props.background,
      environment: props.environment,
      fog: props.fog,
      toneMapping: props.toneMapping,
      exposure: props.exposure,
      layers: props.layers,
    })
    if (props.camera) s.setCamera(props.camera)
    return s
  })
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => {
      if ((w === undefined) !== (h === undefined) || (w === undefined) !== fill) {
        throw new Error("Scene: fill/fixed is mount-fixed - width/height cannot appear or disappear")
      }
      if (!fill) scene.setSize(w!, h!)
    },
  )
  createEffect(
    () => props.camera,
    camera => {
      if (camera) scene.setCamera(camera)
    },
    { defer: true },
  )
  createEffect(
    () => props.background,
    b => scene.setBackground(b ?? null),
    { defer: true },
  )
  createEffect(
    () => props.environment,
    e => scene.setEnvironment(e ?? null),
    { defer: true },
  )
  createEffect(
    () => props.fog,
    f => scene.setFog(f ?? null),
    { defer: true },
  )
  createEffect(
    () => props.toneMapping,
    t => scene.setToneMapping(t ?? "none"),
    { defer: true },
  )
  createEffect(
    () => props.exposure,
    e => scene.setExposure(e ?? 1),
    { defer: true },
  )
  createEffect(
    () => props.layers,
    l => scene.setLayers(l ?? 1),
    { defer: true },
  )
  untrack(() => props.ref)?.(scene)
  // Camera-control input (SceneInput): controls register through context,
  // the leaf dispatches to them. `hasInput` gates the built-in leaf's
  // handlers so a control-less scene with events={false} still costs no
  // pointer routing; ownedWrite because controls add() from their
  // component bodies.
  let listeners = new Set<SceneInputListener>()
  let [hasInput, setHasInput] = createSignal(false, { ownedWrite: true })
  // Whether any control wants keys: the leaf turns focusable and takes
  // focus on pointer down only then, so a pointer-only scene never steals
  // focus from a text field on a click.
  let [hasKeys, setHasKeys] = createSignal(false, { ownedWrite: true })
  let anyKeys = () => [...listeners].some(l => l.onKeyDown || l.onKeyUp)
  let leafLayout: (() => { width: number; height: number }) | null = null
  let input: SceneInput = {
    handlersFor(layout, node) {
      leafLayout = layout
      return {
        onPointerDown: e => {
          let n = node?.()
          if (n && untrack(hasKeys)) setFocus(n.id)
          listeners.forEach(l => l.onPointerDown?.(e))
        },
        onPointerMove: e => listeners.forEach(l => l.onPointerMove?.(e)),
        onPointerUp: e => listeners.forEach(l => l.onPointerUp?.(e)),
        onWheel: e => listeners.forEach(l => l.onWheel?.(e)),
        onKeyDown: e => listeners.forEach(l => l.onKeyDown?.(e)),
        onKeyUp: e => listeners.forEach(l => l.onKeyUp?.(e)),
        onBlur: () => listeners.forEach(l => l.onBlur?.()),
      }
    },
    add(listener) {
      listeners.add(listener)
      setHasInput(true)
      setHasKeys(anyKeys())
      return () => {
        listeners.delete(listener)
        setHasInput(listeners.size > 0)
        setHasKeys(anyKeys())
      }
    },
    layout: () => leafLayout?.() ?? null,
  }
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  let leafNode: { id: number } | undefined
  // The built-in leaf's laid-out box in ITS units - what pointer
  // localX/localY report in and every handlersFor scales against. Fixed
  // mode lays the leaf out at the target size; fill mode reads the box
  // back (getLayoutBox, the untransformed read, so a designSize fit or
  // ancestor transform never skews events). Zero before the first layout:
  // no event can arrive before one, and a degenerate layout passes
  // coordinates through unscaled. A custom output leaf registers its own
  // layout via handlersFor.
  let builtinLayout = fill
    ? () => (leafNode && getLayoutBox(leafNode)) || { width: 0, height: 0 }
    : () => ({ width: props.width!, height: props.height! })
  if (fill) {
    // Fill sizing: after every layout (and on display-scale changes, which
    // lay nothing out) the target follows the leaf's on-screen box in
    // device pixels - getBoundingBoxViewport composes designSize fits and
    // ancestor transforms, so the scene renders at true density wherever
    // it sits. onLayout runs before paint (no frame draws at a stale
    // size); setSize no-ops when nothing changed. apply runs as an
    // onLayout handler and as an effect apply, both untracked scopes, so
    // its scale read is wrapped in an explicit untrack (the effect's
    // compute is what tracks it).
    let apply = () =>
      untrack(() => {
        if (!leafNode) return
        let box = getBoundingBoxViewport(leafNode)
        if (!box) return
        let scale = displayScale()
        scene.setSize(Math.max(1, Math.round(box.width * scale)), Math.max(1, Math.round(box.height * scale)))
      })
    onLayout(apply)
    createEffect(() => displayScale(), apply)
  }
  // Mesh events on the built-in leaf: at target size the plain handlers,
  // in fill mode scaled from the laid-out box.
  let sceneHandlers = fill ? scene.handlersFor(builtinLayout) : scene.handlers
  let leaf = output ? null : input.handlersFor(builtinLayout, () => leafNode)
  return (
    <SceneContext value={{ scene, parent: scene.root, input }}>
      {output ? (
        untrack(() => output(scene.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leafNode = n)}
          src={scene.texture}
          width={fill ? "100%" : props.width}
          height={fill ? "100%" : props.height}
          onPointerDown={events || hasInput() ? (e: PointerEvent) => { if (events) sceneHandlers.onPointerDown(e); leaf!.onPointerDown(e) } : undefined}
          onPointerMove={events || hasInput() ? (e: PointerEvent) => { if (events) sceneHandlers.onPointerMove(e); leaf!.onPointerMove(e) } : undefined}
          onPointerUp={events || hasInput() ? (e: PointerEvent) => { if (events) sceneHandlers.onPointerUp(e); leaf!.onPointerUp(e) } : undefined}
          onPointerLeave={events ? sceneHandlers.onPointerLeave : undefined}
          onWheel={hasInput() ? leaf!.onWheel : undefined}
          focusable={hasKeys()}
          onKeyDown={hasKeys() ? leaf!.onKeyDown : undefined}
          onKeyUp={hasKeys() ? leaf!.onKeyUp : undefined}
          onBlur={hasKeys() ? leaf!.onBlur : undefined}
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
  return provide(ctx, node, props)
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
   * false. Needs a `castShadow` light to show. */
  castShadow?: boolean
  /** Layer membership bitmask (setLayers as a prop; default 1): a target
   * draws the mesh when its mask intersects this. Not inherited from
   * ancestor Groups. */
  layers?: number
  /** Frustum culling switch (setCulling as a prop; default true). */
  frustumCulled?: boolean
  /** World units the culled box grows by (setCulling as a prop; default 0). */
  cullMargin?: number
  ref?: (mesh: MeshNode) => void
}

// The mesh-side props every mesh component shares (Sprite has no
// geometry, so that one stays with the components that take it); the
// ref and the cleanup are each component's own (a populated mesh frees
// its buffers, a plain one is removed).
function syncMesh(mesh: MeshNode, props: Omit<SpriteProps, "ref">): void {
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
  createEffect(
    () => props.layers,
    l => setLayers(mesh, l ?? 1),
  )
  createEffect(
    () => [props.frustumCulled, props.cullMargin] as const,
    ([culled, margin]) => setCulling(mesh, { frustumCulled: culled !== false, cullMargin: margin ?? 0 }),
  )
  syncNode(mesh, props)
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
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => remove(mesh))
  return null
}

export type SpriteProps = TransformProps & PointerEventProps & {
  /** A `sprite()` material (any material draws, only a sprite one turns). */
  material: Material
  /** Per-mesh uniforms, merge semantics - as on Mesh. */
  params?: ShaderParams
  /** Explicit draw-order key (setRenderOrder as a prop); default 0. */
  renderOrder?: number
  /** Layer membership bitmask (setLayers as a prop; default 1). */
  layers?: number
  /** Frustum culling switch (setCulling as a prop; default true). */
  frustumCulled?: boolean
  /** World units the culled box grows by (setCulling as a prop; default 0). */
  cullMargin?: number
  ref?: (mesh: MeshNode) => void
}

/** A camera-facing unit quad (createSprite as a component): no geometry
 * prop, `scale` is its world size, rotation is ignored. */
export let Sprite: VoidComponent<SpriteProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createSprite(props.material))
  add(ctx.parent, mesh)
  syncMesh(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => remove(mesh))
  return null
}

// The props both populated meshes share with Mesh: everything but the
// geometry/material pair (documented per component) and the ref.
type PopulatedMeshProps = Omit<MeshProps, "geometry" | "material" | "ref">

export type InstancedMeshProps = PopulatedMeshProps & {
  geometry: Geometry
  /** An instanced material: a stock one with `instanced` (or
   * `instanceColors`, for a per-instance tint), or a class declaring
   * INSTANCE_MATRIX_ATTRIBUTES in slot 0 and any style layout in slot 1. */
  material: Material
  /** Instance slots reserved up front (default 64; fixed at creation).
   * Past it the buffers double into replacements - amortized, but size it
   * realistically to skip the copies. */
  capacity?: number
  /** LOCAL bounds covering the population ([minX..maxZ]), fixed at
   * creation: the mesh node's own box for the frustum test and the
   * transparent sort. Optional here - instances pick by themselves, and
   * without it the mesh culls by the union of their boxes. */
  bounds?: ArrayLike<number>
  /** Debug label for the record buffers. */
  label?: string
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. The stock instanced materials cast; a custom class needs a
   * `shadowVertex`. */
  castShadow?: boolean
  ref?: (mesh: InstancedMeshNode) => void
}

let InstancedMeshContext = createContext<InstancedMeshNode | null>(null)

/**
 * One draw entry covering N instance NODES (createInstancedMesh as a
 * component): `<Instance>` children populate it - each a scene node the
 * core places, so their `transition` props spring with zero per-frame
 * JS, and picking, pointer events, overlap and sweep name the instance
 * struck. `<Group>` children between the mesh and its instances are
 * squads (the records stay mesh-relative through them). The record
 * buffers are component-owned and freed on unmount.
 */
export let InstancedMesh: ParentComponent<InstancedMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() =>
    createInstancedMesh(props.geometry, props.material, { capacity: props.capacity, bounds: props.bounds, label: props.label }),
  )
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
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => disposeInstances(mesh))
  return (
    <SceneContext value={{ scene: ctx.scene, parent: mesh, input: ctx.input }}>
      <InstancedMeshContext value={mesh}>{props.children}</InstancedMeshContext>
    </SceneContext>
  )
}

export type InstanceProps = TransformProps & PointerEventProps & {
  /** The instance's style record (setInstanceStyle as a prop): the
   * floats of the material's slot-1 instance attributes - `[r, g, b, a]`
   * under a stock material's `instanceColors`. Reactive; absent, the
   * instance keeps the material's instanceStyle (white for a tint). */
  style?: ArrayLike<number>
  ref?: (instance: InstanceNode) => void
}

/**
 * One instance of the enclosing `<InstancedMesh>` (addInstance as a
 * component): a node placed under the nearest `<Group>` or `<Instance>`
 * inside the mesh, with the transform, transition and pointer props of
 * a `<Group>` plus `style`. Children (a `<Mesh>` headlight under a car
 * instance) mount under it. Throws outside an `<InstancedMesh>`.
 */
export let Instance: ParentComponent<InstanceProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = useContext(InstancedMeshContext)
  if (mesh === null) throw new Error("<Instance> must be inside an <InstancedMesh>")
  let instance = addInstance(mesh, undefined, ctx.parent)
  syncNode(instance, props)
  createEffect(
    () => props.style,
    s => {
      if (s !== undefined) setInstanceStyle(instance, s)
    },
  )
  untrack(() => props.ref)?.(instance)
  onCleanup(() => removeInstance(instance))
  return provide(ctx, instance, props)
}

export type RecordMeshProps = PopulatedMeshProps & {
  geometry: Geometry
  /** Must declare instanceAttributes (shaderMaterialClass), slot 0 only. */
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
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. Needs a `castShadow` light AND a material class declaring
   * `shadowVertex` (the depth pass with the instance placement) - the
   * shadow views skip a populated mesh without one. */
  castShadow?: boolean
  ref?: (mesh: RecordMeshNode) => void
}

/** One draw entry covering N records: geometry repeated per record of
 * `records` (createRecordMesh as a component, the JS-written population).
 * The record buffer is component-owned and freed on unmount. */
export let RecordMesh: VoidComponent<RecordMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() =>
    createRecordMesh(props.geometry, props.material, props.records, props.count, { bounds: props.bounds }),
  )
  add(ctx.parent, mesh)
  createEffect(
    () => props.records,
    r => setRecords(mesh, r, untrack(() => props.count)),
    { defer: true },
  )
  createEffect(
    () => props.count,
    c => {
      if (c !== undefined) setRecordCount(mesh, c)
    },
    { defer: true },
  )
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
 * to orbit it, update `position`/`lookAt`. The Scene `camera` prop drives
 * the same state (a partial CameraUpdate, ortho included) - use one form,
 * not both.
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

// Cap on the auto-orbit's per-frame dt in seconds, so the first tick after
// a suspended stretch (a resumed app, a reload) cannot leap the pose.
const MAX_ORBIT_DT = 0.1

export type OrbitCameraProps = OrbitCameraOptions & {
  /** The control's handle (pose()/set()/eye()/orbiting()/active() - also
   * the debug command shape). This handle's set() pushes the pose itself,
   * so a caller never touches update(). */
  ref?: (orbit: OrbitCameraHandle) => void
}

/**
 * createOrbitCamera as a Scene child: drives the enclosing scene's camera
 * and takes its input from the scene's leaf through context - no ref
 * plumbing, no handler spreads, no onFrame of your own (with a custom
 * `output`, spread `useScene().input.handlersFor(layout)` on your leaf).
 * The pose props (target, azimuth, elevation, distance) are initial
 * values - change the pose at runtime through `ref`'s set(); every other
 * prop is live, forwarded to the control as a getter and read where it
 * applies, so a clamp, a rate, an anchor callback or `viewport` follows
 * its prop, and a clamp change re-clamps the pose at once. `viewport`
 * defaults to the leaf's laid-out size plus the scene camera's fov, so
 * rotation is viewport-relative and two-finger pan works out of the box
 * (pass your own to override). Auto-orbit runs a frame loop only while
 * `active()` (orbiting with a non-zero `orbitSpeed`); a paused or
 * drag-only camera leaves the app demand-driven idle.
 */
export let OrbitCamera: VoidComponent<OrbitCameraProps> = props => {
  let ctx = useContext(SceneContext)
  let leafViewport = () => {
    let layout = ctx.input.layout()
    return layout === null ? null : { height: layout.height, fov: ctx.scene.camera().fov }
  }
  // The control keeps this object and reads each option where it applies,
  // so the props go through as getters (merge), never a spread: a spread
  // would snapshot every prop once and a change could only remount.
  let options: OrbitCameraOptions = merge(props, {
    get viewport() {
      return props.viewport ?? leafViewport
    },
  })
  let orbit = untrack(() => createOrbitCamera(ctx.scene, options))
  // Input pushes the pose synchronously (update(0)), so a drag needs no
  // frame loop and the next paint carries the new camera.
  onCleanup(
    ctx.input.add({
      onPointerDown: e => {
        orbit.handlers.onPointerDown(e)
        orbit.update(0)
      },
      onPointerMove: e => {
        orbit.handlers.onPointerMove(e)
        orbit.update(0)
      },
      onPointerUp: e => orbit.handlers.onPointerUp(e),
      onWheel: e => {
        orbit.handlers.onWheel(e)
        orbit.update(0)
      },
    }),
  )
  createEffect(
    () => orbit.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_ORBIT_DT)
        last = now
        orbit.update(dt)
      })
    },
  )
  // A clamp prop change re-clamps the pose at once (set({}) applies the
  // clamps, update(0) pushes the result) - the update() call a Three
  // OrbitControls app makes after setting minDistance, done by the prop.
  // Deferred: the creation already clamped and pushed the initial pose.
  // Untracked: the control reads the clamps through the getters while it
  // applies them, and the apply wants the values of that moment (the
  // compute above is what tracks them).
  createEffect(
    () => [options.minDistance, options.maxDistance, options.minElevation, options.maxElevation],
    () =>
      untrack(() => {
        orbit.set({})
        orbit.update(0)
      }),
    { defer: true },
  )
  untrack(() => props.ref)?.({
    ...orbit,
    set: (pose: OrbitPose) => {
      orbit.set(pose)
      orbit.update(0)
    },
  })
  return null
}

// Cap on the first-person loop's per-frame dt in seconds, the same guard
// as the orbit's: a resumed app must not teleport the walker.
const MAX_WALK_DT = 0.1

export type FirstPersonCameraProps = FirstPersonCameraOptions & {
  /** The control's handle (pose()/set()/eye()/forward()/active() - also
   * the debug command shape). This handle's set() pushes the pose itself,
   * so a caller never touches update(). */
  ref?: (camera: FirstPersonCameraHandle) => void
}

/**
 * createFirstPersonCamera as a Scene child: drives the enclosing scene's
 * camera and takes its input from the scene's leaf through context. The
 * leaf becomes focusable and takes focus on pointer down, which is what
 * routes WASD to the control (keys go to the focused node; a click on the
 * scene is the same gesture a web canvas needs). With a custom `output`,
 * spread `useScene().input.handlersFor(layout, node)` on your leaf and
 * mark it `focusable`. Pointer lock stays yours: call `lockPointer(true)`
 * from a click and `lockPointer(false)` from Escape. The pose props
 * (position, yaw, pitch) are initial values - change the pose at runtime
 * through `ref`'s set(); every other prop is live, forwarded to the
 * control as a getter and read where it applies: `fly={flying()}` swaps
 * walk and fly on the running control (pose and held keys carry over, no
 * remount), `moveSpeed`, `lookSpeed` and `clampPosition` follow their
 * props, and a pitch clamp change re-clamps at once. `viewport` defaults
 * to the leaf's laid-out size, so a drag is viewport-relative. A frame
 * loop runs only while `active()` - a key held or a stick deflected - so
 * a still scene stays demand-driven idle.
 */
export let FirstPersonCamera: VoidComponent<FirstPersonCameraProps> = props => {
  let ctx = useContext(SceneContext)
  let leafViewport = () => {
    let layout = ctx.input.layout()
    return layout === null ? null : { height: layout.height }
  }
  // Getters, not a spread: see OrbitCamera.
  let options: FirstPersonCameraOptions = merge(props, {
    get viewport() {
      return props.viewport ?? leafViewport
    },
  })
  let camera = untrack(() => createFirstPersonCamera(ctx.scene, options))
  // Look input applies synchronously inside the control's handlers, so a
  // mouse move under lock needs no frame loop; only movement integrates.
  onCleanup(ctx.input.add(camera.handlers))
  createEffect(
    () => camera.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_WALK_DT)
        last = now
        camera.update(dt)
      })
    },
  )
  // A pitch clamp change re-clamps the pose at once; deferred and
  // untracked as OrbitCamera's.
  createEffect(
    () => [options.minPitch, options.maxPitch],
    () =>
      untrack(() => {
        camera.set({})
        camera.update(0)
      }),
    { defer: true },
  )
  untrack(() => props.ref)?.({
    ...camera,
    set: (pose: FirstPersonPose) => {
      camera.set(pose)
      camera.update(0)
    },
  })
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
  /** Render a shadow map from this light (any directional light may;
   * each is a pass). Its shadow camera sits at the light's WORLD
   * position, so give a casting light a `position` above the scene. */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, camera frustum,
   * cascades, distance), merged key by key. */
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

export type SpotLightProps = TransformProps & {
  /** Aim direction in the node's local space; default [0, -1, 0], a
   * lamp pointing straight down. */
  direction?: Vec3
  color?: Vec3
  intensity?: number
  /** Falloff cutoff in world units (0 = no cutoff). */
  distance?: number
  /** Cone half-angle in DEGREES, (0, 90]; default 60 (degrees like
   * camera fov; Three's radians convert as `angle * 180 / PI`). */
  angle?: number
  /** 0..1 fraction of the cone fading to the rim; default 0. */
  penumbra?: number
  /** Falloff exponent; default 2 (inverse square). */
  decay?: number
  /** Render a shadow map from this light (a perspective map of its
   * cone; one shadow slot). */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, near), merged key
   * by key. */
  shadow?: SpotShadowOptions
  ref?: (light: SpotLightNode) => void
}

/** A spot light node (createSpotLight): a cone from the node's world
 * position along its local `direction` - give it a `position`, and aim
 * it with `direction` or a parent's rotation. Counts against MAX_LIGHTS
 * with the other non-ambient lights. */
export let SpotLight: VoidComponent<SpotLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() =>
    createSpotLight({
      direction: props.direction,
      color: props.color,
      intensity: props.intensity,
      distance: props.distance,
      angle: props.angle,
      penumbra: props.penumbra,
      decay: props.decay,
      castShadow: props.castShadow,
      shadow: props.shadow,
    }),
  )
  add(ctx.parent, light)
  syncNode(light, props)
  createEffect(
    () =>
      [props.direction, props.color, props.intensity, props.distance, props.angle, props.penumbra, props.decay, props.castShadow, props.shadow] as const,
    ([direction, color, intensity, distance, angle, penumbra, decay, castShadow, shadow]) =>
      setLight(light, { direction, color, intensity, distance, angle, penumbra, decay, castShadow, shadow }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => remove(light))
  return null
}

export type PointLightProps = TransformProps & {
  color?: Vec3
  intensity?: number
  /** Falloff cutoff in world units (0 = no cutoff). */
  distance?: number
  /** Falloff exponent; default 2 (inverse square). */
  decay?: number
  /** Render shadow maps from this light (six 90-degree face maps, one
   * per cube face; six shadow slots). Far = `distance`, so give a
   * casting bulb one. */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, near), merged key
   * by key. */
  shadow?: SpotShadowOptions
  ref?: (light: PointLightNode) => void
}

/** A point light node (createPointLight): light in every direction from
 * the node's world position - give it a `position`; rotation does not
 * matter. Counts against MAX_LIGHTS with the other non-ambient lights. */
export let PointLight: VoidComponent<PointLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() =>
    createPointLight({
      color: props.color,
      intensity: props.intensity,
      distance: props.distance,
      decay: props.decay,
      castShadow: props.castShadow,
      shadow: props.shadow,
    }),
  )
  add(ctx.parent, light)
  syncNode(light, props)
  createEffect(
    () => [props.color, props.intensity, props.distance, props.decay, props.castShadow, props.shadow] as const,
    ([color, intensity, distance, decay, castShadow, shadow]) => setLight(light, { color, intensity, distance, decay, castShadow, shadow }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => remove(light))
  return null
}
