import { callRef, createEffect, onCleanup, untrack } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import type { ShaderParams } from "@solidrt/core/gpu"
import type { Atlas } from "../atlas.ts"
import type { Material } from "../material.ts"
import { createSpriteLayer } from "../layer.ts"
import { feedPointer } from "../views.ts"
import type { LayerPointerEvent, LayerTapEvent, LayerWheelEvent, SpriteLayer as LayerHandle } from "../layer.ts"
import type { CameraUpdate } from "../camera.ts"
import type { ViewHandle } from "../views.ts"
import { LayerContext, PointerContext, ViewportContext, ViewportSizeContext } from "./context.ts"
import { createViewFill, FILL_INITIAL_SIZE } from "./fill.ts"
import type { OutputLeaf } from "./fill.ts"

/**
 * A view's own pointer events, the root of the walk: every event arrives
 * after the hit sprite and its Groups (`event.sprite` set) or as the only
 * stop over empty space (`event.sprite` null), unless a handler stopped
 * it. Deselect on a miss, start a marquee, drive a camera - the
 * `<Camera2d>` child listens at the same root.
 */
export type LayerPointerProps = {
  onPointerDown?: (event: LayerPointerEvent) => void
  onPointerMove?: (event: LayerPointerEvent) => void
  onPointerUp?: (event: LayerPointerEvent) => void
  onPointerCancel?: (event: LayerPointerEvent) => void
  onWheel?: (event: LayerWheelEvent) => void
  onTap?: (event: LayerTapEvent) => void
}

export type SpriteLayerProps = LayerPointerProps & {
  /**
   * View pixels of the layer's own view - give both, or neither. Omitted,
   * the view FILLS: the built-in leaf is laid out at 100% of its parent's
   * box (give it a sized parent, as on the web) and the view sizes to that
   * box, so view pixels are the leaf's own coordinates and sprites place
   * in element units - the auto oversample still supplies display density
   * on top. Fill or fixed is decided at mount, and matches @solidrt/3d's
   * `<Scene>` one dimension down. With a function `output` the view
   * follows the leaf you `ref` (OutputLeaf). Fixed, the leaf's own
   * width/height are layout, so render size and display size separate by
   * hand.
   */
  width?: number
  height?: number
  /** The atlases the sprites draw from (createAtlas records), bound as
   * one draw; every `<Sprite frame>` comes from one of them. Fixed at
   * mount. Give these (a layer of the component's own, disposed with it)
   * or `layer`, not both. */
  atlases?: Atlas[]
  /**
   * An existing layer (createSpriteLayer) to adopt instead of making one:
   * the component shows and populates it - the view and the leaf are
   * still its own, `<Sprite>`, `<Group>` and `<Text2d>` children mount
   * into it - and leaves its lifetime to its maker, so the component
   * face sits over an imperative layer. The creation props (atlases,
   * capacity, material, orderBy) describe a layer the component makes,
   * so with `layer` they have nothing to apply to and throw.
   */
  layer?: LayerHandle
  /** Initial record reservation (grows on demand); default 1024. */
  capacity?: number
  /** The layer's material (see SpriteLayerOptions.material): its blend,
   * its look, its per-sprite style record; default `unlit()`, `unlit({
   * blend: "add" })` for glows and additive particles. Fixed at mount. */
  material?: Material
  /** Uniform values written to every view of the layer (layer.setParams
   * as a prop, over the material's own): the material's prelude
   * uniforms. Live, with merge semantics - a key that disappears keeps
   * its last value; for per-frame values prefer `ref` + setParams from
   * onFrame. */
  params?: ShaderParams
  /** The own view's clear color. */
  clearColor?: [number, number, number, number]
  /** Pan/zoom/rotate the own view over the world (in-shader); a
   * shared-params write, never per-sprite. A `<Camera2d>` child writes
   * the same state - use one form, not both. */
  camera?: CameraUpdate
  /**
   * Layer tint, [r, g, b, a] in 0..1, multiplied over every sprite's own
   * tint in every view (day/night, a dimmed parallax plane, a fade-in).
   * One shared uniform write per view, cheap to animate - unlike
   * TileLayer's, which re-bakes.
   */
  tint?: [number, number, number, number]
  /**
   * Target texels per view pixel. Absent, the component picks it every
   * layout from the built-in leaf's on-screen size (display scale, any
   * designSize fit, layout scaling), so the view resamples properly at any
   * scale; with a function `output` there is no built-in leaf, so set it
   * yourself.
   */
  oversample?: number
  /**
   * Cap on the auto-picked oversample (integer >= 1): the view stays
   * adaptive up to it, never above - the lever that bounds target memory on
   * high-scale displays without giving up adaptivity. Ignored when
   * `oversample` is set (explicit already opts out of the pick).
   */
  maxOversample?: number
  /**
   * Cap on the own view's target texel count under the auto pick, the
   * budget form of `maxOversample`: one cost across devices, where an
   * integer cap means a different cost on every panel (the window's own
   * pixel count caps the pick always). Ignored when `oversample` is set.
   * The same name and meaning as the 3d `<Scene>`'s `maxPixels`, which
   * caps a fractional render scale where 2d keeps the integer oversample.
   */
  maxPixels?: number
  /**
   * Draw sprites in key order instead of slot order, produced by core at
   * every publish: `"y"` orders by world y (back-to-front for a
   * perspective scene, zero JS per frame even under native transitions),
   * `"renderOrder"` by the per-sprite `renderOrder` field (explicit layering:
   * raise-on-drag, click-to-front). Creation-fixed (see
   * SpriteLayerOptions.orderBy).
   */
  orderBy?: "y" | "renderOrder"
  /**
   * Stagger (ms) on the layer's root: the enters and exits of the sprites
   * and groups straight under the layer that begin in one frame are spaced
   * by `index * stagger` (a `<Group>` declaring its own wins for what is
   * under it). Live: a change applies to the next frame's cascade.
   */
  stagger?: number
  /**
   * The rate the layer's native motion runs at (layer.setTimeScale as a
   * prop): 0 pauses every sprite and group transition while the app's UI
   * keeps moving, 1 is app time; a `<Group timeScale={1}>` inside keeps
   * running. Undefined or null is app time. Live.
   */
  timeScale?: number | null
  label?: string
  /** The layer: sprites, tint, pick, createView. */
  ref?: (layer: LayerHandle) => void
  /** The layer's own view (texture, camera, handlers) for imperative use
   * outside the tree; inside, `useSpriteLayer().viewport` is the same.
   * Not called with `output={false}` - there is no own view. */
  viewRef?: (view: ViewHandle) => void
  /**
   * The own view's leaf. Absent: a built-in `<texture>` leaf carrying the
   * view's handlers. A function: compose the leaf yourself - called once
   * (untracked) with the view's texture id and the leaf contract, its
   * return renders in place of the built-in leaf; `ref={leaf.ref}` on the
   * leaf you render makes a fill-mode view follow it,
   * `{...leaf.handlers}` routes sprite pointer events through it scaled
   * to its layout, and `leaf.size()` sizes a post chain's targets.
   * `false`: NO view of its
   * own - the layer shows only through its `<View2d>` children, each a
   * full view with its own camera (split-screen: two side by side, a
   * game with its minimap and nothing else). The view props (width,
   * height, camera, clearColor, oversample, maxOversample, events,
   * viewRef, the pointer props) then have nothing to apply to and throw.
   */
  output?: ((texture: TextureId, leaf: OutputLeaf) => Element) | false
  /**
   * Pointer events (default on): the built-in leaf carries the view's
   * handlers, so sprites, groups, the view's own handlers and a
   * `<Camera2d>` child receive input. `false` detaches them - the leaf
   * then costs no pointer routing at all. Built-in leaf only: with a
   * function `output` the leaf is yours and so is the spread, and the
   * prop throws.
   */
  events?: boolean
  /**
   * The pointer feed of the layer's own view (createPointerFeed), fed
   * from the view's root - the events the sprites let through: a sprite
   * that claims its press keeps a camera bound to the feed out of that
   * drag - so an input map over it drives the `<Camera2d>` child; inside,
   * `useSpriteLayer().pointer` is this feed. Fixed at creation.
   */
  pointer?: PointerFeed
}

// The props that describe a layer the component creates - meaningless,
// hence rejected, on an adopted one (`layer`), which was created already.
const CREATION_PROPS = ["atlases", "capacity", "material", "orderBy"] as const

// The props that configure the layer's own view - meaningless, hence
// rejected, on a layer without one (output={false}).
const VIEW_PROPS = [
  "width",
  "height",
  "camera",
  "clearColor",
  "oversample",
  "maxOversample",
  "maxPixels",
  "events",
  "pointer",
  "viewRef",
  "onPointerDown",
  "onPointerMove",
  "onPointerUp",
  "onPointerCancel",
  "onWheel",
  "onTap",
] as const

/**
 * Owns a sprite layer (over `atlases`; or adopts an existing one through
 * `layer`, owning only what follows) and, unless `output={false}`, its
 * own view, composited as an ordinary `<texture>` leaf so the output
 * takes layout, transforms, blendMode, and pointer events like any
 * element - or hand `output` the texture id and compose it yourself. A
 * layout component: it cannot sit inside a d-* subtree; `output` with a
 * `<d-texture>` is the detached form. Children (`<Sprite>`) render
 * nothing themselves - they populate the retained layer through context;
 * `<View2d>` children are further views of the same sprites.
 */
export let SpriteLayer: ParentComponent<SpriteLayerProps> = props => {
  let layer = untrack(() => {
    let adopted = props.layer
    if (adopted) {
      for (let name of CREATION_PROPS) {
        if (props[name] !== undefined) throw new Error(`SpriteLayer: layer adopts an existing layer - ${name} was fixed when it was created`)
      }
      return adopted
    }
    if (props.atlases === undefined) throw new Error("SpriteLayer: give atlases (a layer of its own) or layer (an existing one)")
    return createSpriteLayer(props.atlases, {
      capacity: props.capacity,
      material: props.material,
      params: props.params,
      tint: props.tint,
      orderBy: props.orderBy,
      stagger: props.stagger,
      timeScale: props.timeScale ?? undefined,
      label: props.label,
    })
  })
  createEffect(
    () => props.tint,
    tint => {
      if (tint !== undefined) layer.setTint(tint)
    },
  )
  createEffect(
    () => props.params,
    params => {
      if (params !== undefined) layer.setParams(params)
    },
    { defer: true },
  )
  createEffect(
    () => props.stagger,
    ms => layer.setStagger(ms ?? null),
    { defer: true },
  )
  createEffect(
    () => props.timeScale,
    scale => layer.setTimeScale(scale ?? null),
    { defer: true },
  )
  callRef(() => props.ref, layer)
  let output = untrack(() => props.output)
  if (output === false) {
    untrack(() => {
      for (let name of VIEW_PROPS) {
        if (props[name] !== undefined) throw new Error(`SpriteLayer: output={false} has no view of its own - ${name} belongs on a <View2d> child`)
      }
    })
    return (
      <LayerContext value={layer}>
        <ViewportContext value={null}>{props.children}</ViewportContext>
      </LayerContext>
    )
  }
  // Fill vs fixed view is decided at mount, like `output`: both width
  // and height (fixed view pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("SpriteLayer: width and height come together - give both (fixed view) or neither (fill)")
    }
    return props.width === undefined
  })
  // The layer's own view: the initial props seed it, the effects follow
  // changes only (`defer`).
  let view = untrack(() =>
    layer.createView({
      width: props.width ?? FILL_INITIAL_SIZE,
      height: props.height ?? FILL_INITIAL_SIZE,
      camera: props.camera,
      oversample: props.oversample,
      clearColor: props.clearColor,
      label: props.label,
    }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => {
      if ((w === undefined) !== (h === undefined) || (w === undefined) !== fill) {
        throw new Error("SpriteLayer: fill/fixed is mount-fixed - width/height cannot appear or disappear")
      }
      if (!fill) view.setSize(w!, h!)
    },
    { defer: true },
  )
  createEffect(
    () => props.camera,
    camera => {
      if (camera) view.setCamera(camera)
    },
    { defer: true },
  )
  createEffect(
    () => props.oversample,
    n => {
      if (n !== undefined) view.setOversample(n)
    },
    { defer: true },
  )
  callRef(() => props.viewRef, view)
  // An adopted layer outlives the component, so its own view must not: a
  // layer of the component's own takes the view with it (createSpriteLayer's
  // owner-scoped dispose), an adopted one frees nothing of the component's.
  if (untrack(() => props.layer)) onCleanup(() => view.dispose())
  let pointer = untrack(() => props.pointer) ?? null
  if (pointer) onCleanup(feedPointer(view, pointer))
  // The view's own handlers at the root of the walk; the props are read
  // per event, so a handler prop may change without re-registering.
  onCleanup(
    view.listen({
      onPointerDown: e => props.onPointerDown?.(e),
      onPointerMove: e => props.onPointerMove?.(e),
      onPointerUp: e => props.onPointerUp?.(e),
      onPointerCancel: e => props.onPointerCancel?.(e),
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
  let events = untrack(() => props.events) !== false
  // `events` wires the built-in leaf; with output the leaf and its spread
  // are the app's, so the prop has nothing to apply to and throws (the
  // dev validation policy), like the view props under output={false}.
  if (output && untrack(() => props.events) !== undefined) {
    throw new Error("SpriteLayer: events applies to the built-in leaf - with output, spread leaf.handlers on your leaf, or leave them off")
  }
  // A feed on a built-in leaf carrying no handlers listens at a root no
  // event reaches: a contradiction, so it throws instead of feeding
  // nothing.
  if (pointer !== null && !events && !output) {
    throw new Error(
      "SpriteLayer: a pointer feed with events={false} receives nothing - the feed listens at the view's root, which only the leaf's handlers reach; keep events on, or compose the leaf with output and spread leaf.handlers on it",
    )
  }
  let leafNode: { id: number } | undefined
  // The view follows the leaf (fill.ts): its size from the leaf's layout
  // box in fill mode, its auto oversample from the leaf's on-screen box in
  // both - the built-in leaf, or the one `output` refs.
  let follow = createViewFill(
    "SpriteLayer",
    view,
    () => leafNode,
    fill ? null : () => ({ width: props.width!, height: props.height! }),
    () => ({ oversample: props.oversample, maxOversample: props.maxOversample, maxPixels: props.maxPixels }),
    output !== undefined,
  )
  // Sprite events on the leaf: the built-in fixed leaf is laid out at the
  // view size, so the plain handlers (no layout read per event); otherwise
  // scaled from the laid-out box per event.
  let viewHandlers = fill || output ? view.handlersFor(follow.layout) : view.handlers
  let leaf: OutputLeaf = { ref: (n: { id: number }) => (leafNode = n), handlers: viewHandlers, size: follow.size, deviceSize: follow.deviceSize }
  return (
    <LayerContext value={layer}>
      <ViewportContext value={view}>
        <ViewportSizeContext value={follow.viewportSize}>
        <PointerContext value={pointer}>
          {output ? (
            untrack(() => output(view.texture, leaf))
          ) : (
            <texture
              ref={leaf.ref}
              src={view.texture}
              width={fill ? "100%" : props.width}
              height={fill ? "100%" : props.height}
              contain={fill ? "size" : undefined}
              onPointerDown={events ? viewHandlers.onPointerDown : undefined}
              onPointerMove={events ? viewHandlers.onPointerMove : undefined}
              onPointerUp={events ? viewHandlers.onPointerUp : undefined}
              onPointerCancel={events ? viewHandlers.onPointerCancel : undefined}
              onPointerLeave={events ? viewHandlers.onPointerLeave : undefined}
              onWheel={events ? viewHandlers.onWheel : undefined}
            />
          )}
          {props.children}
        </PointerContext>
        </ViewportSizeContext>
      </ViewportContext>
    </LayerContext>
  )
}
