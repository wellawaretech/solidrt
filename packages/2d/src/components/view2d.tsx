import { callRef, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import type { CameraUpdate } from "../camera.ts"
import { feedPointer } from "../views.ts"
import type { ViewHandle, ViewOptions } from "../views.ts"
import { LayerContext, PointerContext, ViewportContext, ViewportSizeContext } from "./context.ts"
import type { LayerPointerProps } from "./sprite-layer.tsx"
import { createViewFill, FILL_INITIAL_SIZE } from "./fill.ts"
import type { DensityProps, OutputLeaf } from "./fill.ts"

export type View2dProps = LayerPointerProps &
  DensityProps &
  Pick<ViewOptions, "clearColor" | "label"> & {
    /** View pixels, live (view.setSize) - give both, or neither. Omitted,
     * the view FILLS exactly as a `<SpriteLayer>` does: the leaf lays out
     * at 100% of its sized parent, the view sizes to that box, the auto
     * oversample supplies display density, and the leaf `output` refs is
     * followed the same way. Fill or fixed is decided at mount. */
    width?: number
    height?: number
    /** Partial camera update on the view's own camera (view.setCamera):
     * absent keys keep their values. Live. A `<Camera2d>` child writes the
     * same state - use one form, not both. */
    camera?: CameraUpdate
    ref?: (view: ViewHandle) => void
    /**
     * Compose the leaf yourself: called once (untracked) with view.texture
     * and the leaf contract (see SpriteLayer's `output`), and its return
     * renders in place of the built-in `<texture>` leaf. `ref={leaf.ref}`
     * makes a fill-mode view follow your leaf, `{...leaf.handlers}` routes
     * sprite pointer events through the view scaled to the leaf's layout.
     */
    output?: (texture: TextureId, leaf: OutputLeaf) => Element
    /** Pointer events (default on): the built-in leaf carries the view's
     * handlers. `false` detaches them. Built-in leaf only: with `output`
     * the prop throws, as SpriteLayer's. */
    events?: boolean
    /** The pointer feed of this view (createPointerFeed), fed from the
     * view's root - the events the sprites let through - so a map bound
     * to it drives the `<Camera2d>` inside this view and no other; inside,
     * `useSpriteLayer().pointer` is this feed. Fixed at creation. */
    pointer?: PointerFeed
  }

/**
 * A view of the enclosing layer from a camera of its own (layer.createView
 * as a component): the same sprites, one more target and one entry, so
 * it costs no per-frame JS. The `<SpriteLayer>`'s own leaf is one of
 * these too; a `<SpriteLayer output={false}>` shows ONLY through its
 * `<View2d>` children (split-screen: two side by side). Composites as an
 * ordinary `<texture>` leaf at the view size, or through `output`. A
 * `<Camera2d>`
 * child drives the VIEW's camera from the map it is given, and the view's
 * `pointer` feed carries the view's gestures to that map: inside,
 * `useSpriteLayer()` reports the view as `viewport` and the feed as
 * `pointer`. `<Sprite>` and
 * `<Group>` children mount to the layer as they would outside - a view
 * mirrors the layer's sprites, it has none of its own. Sprites under the
 * view's leaf get their ordinary pointer handlers, with the view's camera
 * undone; the view's own `onPointer*`/`onTap` are the last stop of that
 * walk (`event.sprite` null over empty space).
 */
export let View2d: ParentComponent<View2dProps> = props => {
  let layer = useContext(LayerContext)
  // Fill vs fixed view is decided at mount, as SpriteLayer's: both width
  // and height (fixed view pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("View2d: width and height come together - give both (fixed view) or neither (fill)")
    }
    return props.width === undefined
  })
  // The initial props seed createView, so `ref` hands out a configured
  // view and the first frame draws it whole; the effects follow changes
  // only (`defer`).
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
        throw new Error("View2d: fill/fixed is mount-fixed - width/height cannot appear or disappear")
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
  callRef(() => props.ref, view)
  onCleanup(() => view.dispose())
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
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  // `events` wires the built-in leaf; with output the spread is the
  // app's, so the prop throws (see SpriteLayer).
  if (output && untrack(() => props.events) !== undefined) {
    throw new Error("View2d: events applies to the built-in leaf - with output, spread leaf.handlers on your leaf, or leave them off")
  }
  // A feed on a built-in leaf carrying no handlers listens at a root no
  // event reaches: a contradiction, so it throws (the dev validation
  // policy) instead of feeding nothing.
  if (pointer !== null && !events && !output) {
    throw new Error(
      "View2d: a pointer feed with events={false} receives nothing - the feed listens at the view's root, which only the leaf's handlers reach; keep events on, or compose the leaf with output and spread leaf.handlers on it",
    )
  }
  let leafNode: { id: number } | undefined
  // The view follows the leaf (fill.ts), exactly as SpriteLayer's own.
  let follow = createViewFill(
    "View2d",
    view,
    () => leafNode,
    fill ? null : () => ({ width: props.width!, height: props.height! }),
    () => ({ oversample: props.oversample, maxOversample: props.maxOversample, maxPixels: props.maxPixels }),
    output !== undefined,
  )
  let viewHandlers = fill || output ? view.handlersFor(follow.layout) : view.handlers
  let leaf: OutputLeaf = { ref: (n: { id: number }) => (leafNode = n), handlers: viewHandlers, size: follow.size, deviceSize: follow.deviceSize }
  // The fill leaf is size-contained, as SpriteLayer's: it adds nothing to
  // its ancestors, so the view's pixels never pin them.
  return (
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
  )
}
