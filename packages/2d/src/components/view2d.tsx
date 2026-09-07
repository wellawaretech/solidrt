import { createEffect, displayScale, getBoundingBoxViewport, onCleanup, onLayout, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import type { CameraUpdate } from "../camera.ts"
import { feedPointer } from "../views.ts"
import type { ViewHandle, ViewOptions } from "../views.ts"
import { LayerContext, PointerContext, ViewportContext } from "./context.ts"
import type { LayerPointerProps } from "./sprite-layer.tsx"
import { applyOversample } from "./auto-oversample.ts"

export type View2dProps = LayerPointerProps &
  Pick<ViewOptions, "clearColor" | "label"> & {
    /** View pixels, live (view.setSize). Fixed only, for now: a fill mode
     * like SpriteLayer's is a later, additive step. */
    width: number
    height: number
    /** Partial camera update on the view's own camera (view.setCamera):
     * absent keys keep their values. Live. A `<Camera2d>` child writes the
     * same state - use one form, not both. */
    camera?: CameraUpdate
    /**
     * Target texels per view pixel. Absent, the component picks it every
     * layout from the built-in leaf's on-screen size, as `<SpriteLayer>`
     * does; with `output` there is no built-in leaf, so set it yourself.
     */
    oversample?: number
    /** Cap on the auto-picked oversample (see SpriteLayerProps). */
    maxOversample?: number
    ref?: (view: ViewHandle) => void
    /**
     * Compose the leaf yourself: called once (untracked) with view.texture,
     * and its return renders in place of the built-in `<texture>` leaf.
     * Sprite pointer events through the view then need its handlers on
     * your leaf: `<texture src={texture} {...view.handlers} />` (the view
     * from `ref`).
     */
    output?: (texture: TextureId) => Element
    /** Pointer events (default on): the built-in leaf carries the view's
     * handlers. `false` detaches them. */
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
  // The initial props seed createView, so `ref` hands out a configured
  // view and the first frame draws it whole; the effects follow changes
  // only (`defer`).
  let view = untrack(() =>
    layer.createView({
      width: props.width,
      height: props.height,
      camera: props.camera,
      oversample: props.oversample,
      clearColor: props.clearColor,
      label: props.label,
    }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => view.setSize(w, h),
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
  untrack(() => props.ref)?.(view)
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
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  let leaf: { id: number } | undefined
  // Auto oversample from the built-in leaf's window box, in device pixels,
  // per view pixel: the same pick as SpriteLayer's fixed mode.
  let pick = () =>
    untrack(() => {
      if (!leaf || props.oversample !== undefined) return
      let box = getBoundingBoxViewport(leaf)
      if (!box) return
      let scale = displayScale() * Math.max(box.width / props.width, box.height / props.height)
      applyOversample(view, scale, props.width, props.height, props.maxOversample)
    })
  onLayout(pick)
  createEffect(() => [displayScale(), props.maxOversample], pick)
  return (
    <ViewportContext value={view}>
      <PointerContext value={pointer}>
      {output ? (
        untrack(() => output(view.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leaf = n)}
          src={view.texture}
          width={props.width}
          height={props.height}
          onPointerDown={events ? view.handlers.onPointerDown : undefined}
          onPointerMove={events ? view.handlers.onPointerMove : undefined}
          onPointerUp={events ? view.handlers.onPointerUp : undefined}
          onPointerLeave={events ? view.handlers.onPointerLeave : undefined}
          onWheel={events ? view.handlers.onWheel : undefined}
        />
      )}
      {props.children}
      </PointerContext>
    </ViewportContext>
  )
}
