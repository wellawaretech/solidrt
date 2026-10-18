import { callRef, createEffect, createMemo, createRenderEffect, createSignal, getLayoutBox, onCleanup, quality, untrack } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createFill, FILL_INITIAL_SIZE } from "./fill.ts"
import type { OutputLeaf } from "./fill.ts"
import type { DensityProps } from "./fill-math.ts"
import { createScene } from "../scene.ts"
import { feedPointer } from "../scene-pointer.ts"
import type { BlendSpace, BloomOptions, EnvironmentOptions, FogOptions, ResolveInput, Scene as SceneHandle, SkyboxOptions, ToneMapping } from "../scene.ts"
import type { ShadowQuality } from "../scene-shadows.ts"
import { resolveQuality } from "../quality.ts"
import type { QualityInput } from "../quality.ts"
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
  onPointerCancel?: (event: ScenePointerEvent) => void
  onWheel?: (event: SceneWheelEvent) => void
  onTap?: (event: SceneTapEvent) => void
}

export type SceneProps = ScenePointerProps &
  DensityProps & {
  /**
   * Target pixels - give both, or neither. Omitted, the scene FILLS: the
   * built-in leaf is laid out at 100% of its parent's box (give it a sized
   * parent, as on the web) and the target tracks the leaf's on-screen size
   * in device pixels - display scale, designSize fits and ancestor
   * transforms included - so a bare `<Scene>` renders at native density on
   * any display, and viewport-relative camera controls scale with the box;
   * `renderScale` and `maxPixels` scale and cap that size (`maxPixels`
   * defaults to the quality row's cap: 1 Mpx at low, none above). With `output`
   * the target follows the leaf you `ref` (OutputLeaf). Fill or fixed is
   * decided at mount. Fixed, the leaf's own width/height are layout, so
   * render size and display size separate by hand.
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
   * (see SceneOptions.stagger). Live (scene.setStagger). */
  stagger?: number
  /** The rate the scene's native motion runs at (scene.setTimeScale as a
   * prop): 0 pauses the world while the app's UI keeps moving, 1 is app
   * time; a `<Group timeScale={1}>` inside keeps running. Undefined or
   * null is app time. Live. */
  timeScale?: number | null
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
  /** The scene's quality (scene.setQuality as a prop; see
   * SceneOptions.quality): a tier name or a row object over the app tier
   * (core's `quality()`). Absent, the scene follows the app tier. Every
   * quality knob - the shadow kernel, a light's map size and cascades,
   * `lodBias`, the LOD cross-fade, `samples`, a fill scene's `maxPixels`,
   * a probe's size - defaults from the row; each knob's own prop is the
   * override. Reactive, the app tier too. */
  quality?: QualityInput
  /** The LOD size multiplier (scene.setLodBias as a prop): below 1 hands
   * over to far levels sooner. Default the quality row's. Reactive. */
  lodBias?: number
  /** Scene-wide shadow settings: `quality` (scene.setShadowQuality) is
   * the soft shadow kernel's tier, "hard" | "low" | "medium" | "high" -
   * the tap count every light with a `radius` above 1 or a size spends,
   * "hard" the one tap for all of them. Default the quality row's.
   * Reactive. */
  shadows?: { quality?: ShadowQuality }
  /** The buffer's blend space (SceneOptions.blendSpace), default
   * "linear"; fixed at creation. "display" blends sRGB-encoded values
   * in an rgba8 buffer that is itself the displayed image - the mode a
   * splat capture is faithful in - and rejects resolve, bloom,
   * toneMapping and exposure. */
  blendSpace?: BlendSpace
  /** The scene target's layer mask (scene.setLayers as a prop; default 1):
   * the scene draws the meshes whose `layers` intersect it. Reactive. */
  layers?: number
  /** `"texture"` exposes the target's depth as `scene.depthTexture` (a
   * depth-reading post effect's input). Fixed at creation; not with
   * `samples`. */
  depth?: true | "texture"
  label?: string
  /** Multisample count (1, 2, 4 or 8): anti-aliased mesh edges. Default
   * the quality row's (1 / 1 / 4; 1 with `depth: "texture"`). Reactive
   * (scene.setSamples: the target's storage is reallocated in place). */
  samples?: 1 | 2 | 4 | 8
  ref?: (scene: SceneHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the scene's
   * texture id and the leaf contract, and its return renders in place of
   * the built-in `<texture>` leaf - a `<d-texture>`, a leaf carrying
   * paint/pointer/layout props, or a post-effect chain (a shader target
   * sampling the id; created in the callback it disposes with the Scene).
   * Return null to render no leaf. `ref={leaf.ref}` on the leaf you render
   * makes a fill-mode target follow it, and `{...leaf.handlers}` routes
   * pointer events through it scaled to its layout - one spread carries
   * the node events, the scene's own and the pointer feed, which listens
   * at the scene's root; `leaf.size()` sizes a post chain's targets.
   */
  output?: (texture: TextureId, leaf: OutputLeaf) => Element
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
   * routing at all. Built-in leaf only: with `output` the leaf is yours
   * and so is the spread, and the prop throws.
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
export let Scene: ParentComponent<SceneProps> = props => {
  // Fill vs fixed target is decided at mount, like `output`: both
  // width and height (fixed pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("Scene: width and height come together - give both (fixed target) or neither (fill)")
    }
    // Fixed pixels and a scale over them contradict: the fixed contract
    // is that width/height ARE the target.
    if (props.width !== undefined && (props.renderScale !== undefined || props.maxPixels !== undefined)) {
      throw new Error("Scene: renderScale and maxPixels scale a fill-mode target - with width and height the target is those pixels")
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
      shadows: props.shadows,
      quality: props.quality,
      lodBias: props.lodBias,
      blendSpace: props.blendSpace,
      resolve: props.resolve,
      bloom: props.bloom,
      layers: props.layers,
      stagger: props.stagger,
      timeScale: props.timeScale ?? undefined,
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
    () => [props.width, props.height, props.renderScale, props.maxPixels] as const,
    ([w, h, renderScale, maxPixels]) => {
      if ((w === undefined) !== (h === undefined) || (w === undefined) !== fill) {
        throw new Error("Scene: fill/fixed is mount-fixed - width/height cannot appear or disappear")
      }
      if (!fill && (renderScale !== undefined || maxPixels !== undefined)) {
        throw new Error("Scene: renderScale and maxPixels scale a fill-mode target - with width and height the target is those pixels")
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
    () => props.shadows?.quality,
    q => scene.setShadowQuality(q ?? null),
    { defer: true },
  )
  createEffect(
    () => props.quality,
    q => scene.setQuality(q ?? null),
    { defer: true },
  )
  createEffect(
    () => props.lodBias,
    b => scene.setLodBias(b ?? null),
    { defer: true },
  )
  createEffect(
    () => props.samples,
    s => {
      if (s !== undefined) scene.setSamples(s)
    },
    { defer: true },
  )
  // The row in force, reactive, for the fill cap and the children's
  // context: the scene's own resolution is the same function over the
  // same inputs.
  let row = createMemo(() => resolveQuality(props.quality, quality()))
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
  createEffect(
    () => props.stagger,
    ms => scene.setStagger(ms ?? null),
    { defer: true },
  )
  createEffect(
    () => props.timeScale,
    scale => scene.setTimeScale(scale ?? null),
    { defer: true },
  )
  callRef(() => props.ref, scene)
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
      onPointerCancel: e => props.onPointerCancel?.(e),
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  // `events` wires the built-in leaf; with output the leaf and its spread
  // are the app's, so the prop has nothing to apply to and throws (the
  // dev validation policy), like the view props under output={false} in
  // @solidrt/2d.
  if (output && untrack(() => props.events) !== undefined) {
    throw new Error("Scene: events applies to the built-in leaf - with output, spread leaf.handlers on your leaf, or leave them off")
  }
  // A feed on a built-in leaf carrying no handlers listens at a root no
  // event reaches: a contradiction, so it throws instead of feeding
  // nothing.
  if (pointer !== null && !events && !output) {
    throw new Error(
      "Scene: a pointer feed with events={false} receives nothing - the feed listens at the scene's root, which only the leaf's handlers reach; keep events on, or compose the leaf with output and spread leaf.handlers on it",
    )
  }
  let leafNode: { id: number } | undefined
  // The leaf's laid-out box in ITS units - what pointer localX/localY
  // report in and every handlersFor scales against: getLayoutBox, the
  // untransformed read, so a designSize fit or ancestor transform never
  // skews events. Zero before the first layout in fill mode (no event can
  // arrive before one, and a degenerate layout passes coordinates through
  // unscaled); the target size in fixed mode until an output leaf is
  // ref'd.
  let leafLayout = () => (leafNode && getLayoutBox(leafNode)) || (fill ? { width: 0, height: 0 } : { width: props.width!, height: props.height! })
  // Fill sizing: the target follows the leaf's on-screen box (fill.ts),
  // the built-in leaf or the one `output` refs.
  let fillReads = fill
    ? createFill("Scene", scene, () => leafNode, () => ({ renderScale: props.renderScale, maxPixels: props.maxPixels ?? row().maxPixels ?? undefined }), output !== undefined)
    : null
  // Pointer events on the leaf: the built-in fixed leaf is laid out at the
  // target size, so the plain handlers (no layout read per event);
  // otherwise scaled from the laid-out box per event.
  let sceneHandlers = fill || output ? scene.handlersFor(leafLayout) : scene.handlers
  let fixedSize = () => ({ width: props.width!, height: props.height! })
  let leaf: OutputLeaf = {
    ref: (n: { id: number }) => (leafNode = n),
    handlers: sceneHandlers,
    size: fillReads ? fillReads.size : fixedSize,
    deviceSize: fillReads ? fillReads.deviceSize : fixedSize,
  }
  // The fill leaf is size-contained: the target follows the leaf's box, so
  // the target's pixels must never come back as the leaf's intrinsic size
  // (the flex automatic minimum would pin every ancestor to the target, and
  // an indefinite height would come out square). The leaf adds nothing to
  // its ancestors, exactly as an empty view at 100% would.
  return (
    <SceneContext value={{ scene, parent: scene.root, viewport: scene, pointer, viewportSize: leaf.size, quality: row }}>
      {output ? (
        untrack(() => output(scene.texture, leaf))
      ) : (
        <texture
          ref={leaf.ref}
          src={scene.texture}
          width={fill ? "100%" : props.width}
          height={fill ? "100%" : props.height}
          contain={fill ? "size" : undefined}
          onPointerDown={events ? sceneHandlers.onPointerDown : undefined}
          onPointerMove={events ? sceneHandlers.onPointerMove : undefined}
          onPointerUp={events ? sceneHandlers.onPointerUp : undefined}
          onPointerCancel={events ? sceneHandlers.onPointerCancel : undefined}
          onPointerLeave={events ? sceneHandlers.onPointerLeave : undefined}
          onWheel={events ? sceneHandlers.onWheel : undefined}
        />
      )}
      {props.children}
    </SceneContext>
  )
}
