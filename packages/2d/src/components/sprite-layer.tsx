import { createEffect, displayScale, getBoundingBoxViewport, getLayoutBox, onCleanup, onLayout, untrack } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import { createSpriteLayer } from "../layer.ts"
import { feedPointer } from "../views.ts"
import type { LayerPointerEvent, LayerTapEvent, LayerWheelEvent, SpriteLayer as LayerHandle } from "../layer.ts"
import type { CameraUpdate } from "../camera.ts"
import type { ViewHandle } from "../views.ts"
import { LayerContext, PointerContext, ViewportContext } from "./context.ts"
import { applyOversample } from "./auto-oversample.ts"

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
   * `<Scene>` one dimension down. A function `output` needs explicit
   * sizes (the view cannot follow a leaf it does not own); the leaf's own
   * width/height are then layout, so render size and display size
   * separate.
   */
  width?: number
  height?: number
  /** The atlas texture every sprite samples (create with createAtlas). */
  atlas: TextureId
  /** Initial record reservation (grows on demand); default 1024. */
  capacity?: number
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
   * (untracked) with the view's texture id, its return renders in place
   * of the built-in leaf; sprite pointer events then need the view's
   * handlers on your leaf: `<texture src={texture}
   * {...useSpriteLayer().viewport.handlers} />`. `false`: NO view of its
   * own - the layer shows only through its `<View2d>` children, each a
   * full view with its own camera (split-screen: two side by side, a
   * game with its minimap and nothing else). The view props (width,
   * height, camera, clearColor, oversample, maxOversample, events,
   * viewRef, the pointer props) then have nothing to apply to and throw.
   */
  output?: ((texture: TextureId) => Element) | false
  /**
   * Pointer events (default on): the built-in leaf carries the view's
   * handlers, so sprites, groups, the view's own handlers and a
   * `<Camera2d>` child receive input. `false` detaches them - the leaf
   * then costs no pointer routing at all.
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

// Creation size of a fill-mode view: the first onLayout replaces it
// before the first paint, so it only has to be a valid target size.
const FILL_INITIAL_SIZE = 1

// The props that configure the layer's own view - meaningless, hence
// rejected, on a layer without one (output={false}).
const VIEW_PROPS = [
  "width",
  "height",
  "camera",
  "clearColor",
  "oversample",
  "maxOversample",
  "events",
  "pointer",
  "viewRef",
  "onPointerDown",
  "onPointerMove",
  "onPointerUp",
  "onWheel",
  "onTap",
] as const

/**
 * Owns a sprite layer and, unless `output={false}`, its own view,
 * composited as an ordinary `<texture>` leaf so the output takes layout,
 * transforms, blendMode, and pointer events like any element - or hand
 * `output` the texture id and compose it yourself. A layout component:
 * it cannot sit inside a d-* subtree; `output` with a `<d-texture>` is
 * the detached form. Children (`<Sprite>`) render nothing themselves -
 * they populate the retained layer through context; `<View2d>` children
 * are further views of the same sprites.
 */
export let SpriteLayer: ParentComponent<SpriteLayerProps> = props => {
  let layer = untrack(() =>
    createSpriteLayer(props.atlas, {
      capacity: props.capacity,
      tint: props.tint,
      orderBy: props.orderBy,
      stagger: props.stagger,
      label: props.label,
    }),
  )
  createEffect(
    () => props.tint,
    tint => {
      if (tint !== undefined) layer.setTint(tint)
    },
  )
  createEffect(
    () => props.stagger,
    ms => layer.setStagger(ms ?? null),
    { defer: true },
  )
  untrack(() => props.ref)?.(layer)
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
    if (props.width === undefined && output) {
      throw new Error("SpriteLayer: fill needs the built-in leaf - with output, give width and height")
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
  untrack(() => props.viewRef)?.(view)
  let pointer = untrack(() => props.pointer) ?? null
  if (pointer) onCleanup(feedPointer(view, pointer))
  // The view's own handlers at the root of the walk; the props are read
  // per event, so a handler prop may change without re-registering.
  onCleanup(
    view.listen({
      onPointerDown: e => props.onPointerDown?.(e),
      onPointerMove: e => props.onPointerMove?.(e),
      onPointerUp: e => props.onPointerUp?.(e),
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
  let events = untrack(() => props.events) !== false
  // A feed on a built-in leaf carrying no handlers listens at a root no
  // event reaches: a contradiction, so it throws (the dev validation
  // policy) instead of feeding nothing.
  if (pointer !== null && !events && !output) {
    throw new Error(
      "SpriteLayer: a pointer feed with events={false} receives nothing - the feed listens at the view's root, which only the leaf's handlers reach; keep events on, or compose the leaf with output and spread useSpriteLayer().viewport.handlers on it",
    )
  }
  let leaf: { id: number } | undefined
  // The view's pixel size: the props in fixed mode, the built-in leaf's
  // laid-out box in fill mode (getLayoutBox, the untransformed read, so a
  // designSize fit or ancestor transform never enters the world space -
  // display density rides on the oversample pick below). Null before the
  // first layout.
  let viewSize = (): { width: number; height: number } | null => {
    if (!fill) return { width: props.width!, height: props.height! }
    let box = leaf && getLayoutBox(leaf)
    if (!box || box.width <= 0 || box.height <= 0) return null
    return { width: Math.max(1, Math.round(box.width)), height: Math.max(1, Math.round(box.height)) }
  }
  // Fill: the view follows the leaf's box. Registered before the pick, so
  // one onLayout applies size then oversample in order; setSize no-ops
  // when nothing changed.
  if (fill) {
    onLayout(() => {
      let size = viewSize()
      if (size) view.setSize(size.width, size.height)
    })
  }
  // Auto oversample: the built-in leaf's window box, in device pixels, per
  // view pixel. Picked after every layout, and again when the display scale
  // changes: the first layout runs before the resize event that reports the
  // scale, and a scale change alone lays nothing out. pick runs as an
  // onLayout handler and as an effect apply, both untracked scopes, so its
  // prop reads are wrapped in an explicit untrack.
  let pick = () =>
    untrack(() => {
      if (!leaf || props.oversample !== undefined) return
      let box = getBoundingBoxViewport(leaf)
      let size = viewSize()
      if (!box || !size) return
      let scale = displayScale() * Math.max(box.width / size.width, box.height / size.height)
      applyOversample(view, scale, size.width, size.height, props.maxOversample)
    })
  onLayout(pick)
  createEffect(() => [displayScale(), props.maxOversample], pick)
  // Sprite events on the built-in leaf: at view size the plain handlers,
  // in fill mode scaled from the laid-out box (the box can be fractional,
  // the view size never is).
  let viewHandlers = fill
    ? view.handlersFor(() => (leaf && getLayoutBox(leaf)) || { width: 0, height: 0 })
    : view.handlers
  return (
    <LayerContext value={layer}>
      <ViewportContext value={view}>
        <PointerContext value={pointer}>
          {output ? (
            untrack(() => output(view.texture))
          ) : (
            <texture
              ref={(n: { id: number }) => (leaf = n)}
              src={view.texture}
              width={fill ? "100%" : props.width}
              height={fill ? "100%" : props.height}
              onPointerDown={events ? viewHandlers.onPointerDown : undefined}
              onPointerMove={events ? viewHandlers.onPointerMove : undefined}
              onPointerUp={events ? viewHandlers.onPointerUp : undefined}
              onPointerLeave={events ? viewHandlers.onPointerLeave : undefined}
              onWheel={events ? viewHandlers.onWheel : undefined}
            />
          )}
          {props.children}
        </PointerContext>
      </ViewportContext>
    </LayerContext>
  )
}
