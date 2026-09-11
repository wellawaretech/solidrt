import { createEffect, createRenderEffect, createSignal, displayScale, getBoundingBoxViewport, getLayoutBox, onCleanup, onLayout, untrack } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createScene } from "../scene.ts"
import { feedPointer } from "../scene-pointer.ts"
import type { BloomOptions, EnvironmentOptions, FogOptions, ResolveInput, Scene as SceneHandle, SkyboxOptions, ToneMapping } from "../scene.ts"
import type { ScenePointerEvent, SceneTapEvent, SceneWheelEvent } from "../node.ts"
import type { CameraUpdate } from "../camera.ts"

/**
 * The scene's own pointer events, the root of the walk: every event the
 * nodes let through arrives here last - `event.mesh` is the hit it
 * bubbled from, or null over empty space (a tap there with `mesh` null is
 * "deselect"), `x`/`y` scene pixels. scene.listen as props; an
 * `<OrbitCamera>` child's feed listens at the same root.
 */
export type ScenePointerProps = {
  onPointerDown?: (event: ScenePointerEvent) => void
  onPointerMove?: (event: ScenePointerEvent) => void
  onPointerUp?: (event: ScenePointerEvent) => void
  onWheel?: (event: SceneWheelEvent) => void
  onTap?: (event: SceneTapEvent) => void
}

export type SceneProps = ScenePointerProps & {
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
  /** Stagger (ms) for the nodes straight under the scene root: their
   * enters and exits beginning in one frame are spaced by `index *
   * stagger`, the whole-scene form of `<Group transition={{ stagger }}>`
   * (see SceneOptions.stagger). Set at creation. */
  stagger?: number
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
   * Pointer events then need the scene's handlers on your leaf:
   * `<texture src={texture} {...useScene().scene.handlers} />` - one
   * spread carries the node events, the scene's own and the pointer
   * feed, which listens at the scene's root.
   */
  output?: (texture: TextureId) => Element
  /**
   * The scene's resolve (SceneOptions.resolve): the source, `{ source,
   * textures }`, or a function of the buffer id `scene.hdrTexture` so a
   * radiance-reading chain (a bloom) is built over it right there and its
   * result bound. Fixed at creation, like `output`.
   */
  resolve?: ResolveInput
  /** The stock bloom (scene.setBloom as a prop): `{ threshold?,
   * intensity?, radius? }`, reactive; undefined turns it off. */
  bloom?: BloomOptions
  /**
   * Pointer events (default on): the built-in leaf carries scene.handlers,
   * so nodes, the scene's own handlers and a camera control's feed
   * receive input. `false` detaches them - the leaf then costs no pointer
   * routing at all.
   */
  events?: boolean
  /**
   * The pointer feed of the scene's leaf (createPointerFeed), fed from
   * the scene's root - the events the nodes let through: a mesh that
   * claims its press (stopPropagation on its down) keeps a camera control
   * bound to the feed out of that drag. The feed's gestures - drag,
   * pinch, two-finger pan, wheel, mouse motion under lock - come from
   * this scene, and an input map binds them to the camera controls
   * inside; `useScene().pointer` is this feed. Fixed at creation. Without
   * one the leaf feeds no gestures and the controls move only through
   * their handles.
   */
  pointer?: PointerFeed
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
  // A budget error the scene reports from its sync (a microtask, outside
  // the tree) is rethrown inside it, so the app's error boundary shows
  // it: the handler parks the error and bumps a signal, the render effect
  // below takes and throws it. Taking it first means the boundary's reset
  // finds nothing to throw and returns to the scene, which kept rendering
  // with what fits.
  let pendingError: Error | null = null
  let [errorTick, setErrorTick] = createSignal(0)
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
      resolve: props.resolve,
      bloom: props.bloom,
      layers: props.layers,
      stagger: props.stagger,
      onError: e => {
        pendingError = e
        setErrorTick(errorTick() + 1)
      },
    })
    if (props.camera) s.setCamera(props.camera)
    return s
  })
  createRenderEffect(
    () => {
      errorTick()
      let e = pendingError
      pendingError = null
      if (e !== null) throw e
      return null
    },
    () => {},
  )
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
    () => props.bloom,
    b => scene.setBloom(b ?? null),
    { defer: true },
  )
  createEffect(
    () => props.layers,
    l => scene.setLayers(l ?? 1),
    { defer: true },
  )
  untrack(() => props.ref)?.(scene)
  // The pointer feed the app handed in listens at the scene's root, and
  // children reach it through useScene().pointer. Fixed at creation, like
  // output.
  let pointer = untrack(() => props.pointer) ?? null
  if (pointer) onCleanup(feedPointer(scene, pointer))
  // The scene's own handlers at the root of the walk; the props are read
  // per event, so a handler prop may change without re-registering.
  onCleanup(
    scene.listen({
      onPointerDown: e => props.onPointerDown?.(e),
      onPointerMove: e => props.onPointerMove?.(e),
      onPointerUp: e => props.onPointerUp?.(e),
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
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
  // Pointer events on the built-in leaf: at target size the plain
  // handlers, in fill mode scaled from the laid-out box.
  let sceneHandlers = fill ? scene.handlersFor(builtinLayout) : scene.handlers
  return (
    <SceneContext value={{ scene, parent: scene.root, viewport: scene, pointer }}>
      {output ? (
        untrack(() => output(scene.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leafNode = n)}
          src={scene.texture}
          width={fill ? "100%" : props.width}
          height={fill ? "100%" : props.height}
          onPointerDown={events ? sceneHandlers.onPointerDown : undefined}
          onPointerMove={events ? sceneHandlers.onPointerMove : undefined}
          onPointerUp={events ? sceneHandlers.onPointerUp : undefined}
          onPointerLeave={events ? sceneHandlers.onPointerLeave : undefined}
          onWheel={events ? sceneHandlers.onWheel : undefined}
        />
      )}
      {props.children}
    </SceneContext>
  )
}
