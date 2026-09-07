import { createEffect, displayScale, getBoundingBoxViewport, getLayoutBox, onCleanup, onLayout, untrack } from "@solidrt/core"
import type { Element, ParentComponent, TextureId } from "@solidrt/core"
import { createSpriteLayer } from "../layer.ts"
import type { LayerPointerEvent, LayerTapEvent, LayerWheelEvent, SpriteLayer as LayerHandle } from "../layer.ts"
import type { CameraUpdate } from "../camera.ts"
import { LayerContext } from "./context.ts"
import { applyOversample } from "./auto-oversample.ts"

/**
 * The layer's own pointer events, the root of the walk: every event
 * arrives after the hit sprite and its Groups (`event.sprite` set) or as
 * the only stop over empty space (`event.sprite` null), unless a handler
 * stopped it. Deselect on a miss, start a marquee, drive a camera - the
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
   * Layer pixels - give both, or neither. Omitted, the layer FILLS: the
   * built-in leaf is laid out at 100% of its parent's box (give it a sized
   * parent, as on the web) and the layer sizes to that box, so layer
   * pixels are the leaf's own coordinates and sprites place in element
   * units - the auto oversample still supplies display density on top.
   * Fill or fixed is decided at mount, and matches @solidrt/3d's `<Scene>`
   * one dimension down. `output` needs explicit sizes (the layer cannot
   * follow a leaf it does not own); the leaf's own width/height are then
   * layout, so render size and display size separate.
   */
  width?: number
  height?: number
  /** The atlas texture every sprite samples (create with createAtlas). */
  atlas: TextureId
  /** Initial record reservation (grows on demand); default 1024. */
  capacity?: number
  clearColor?: [number, number, number, number]
  /** Pan/zoom/rotate over the world (in-shader); a shared-params write,
   * never per-sprite. */
  camera?: CameraUpdate
  /**
   * Layer tint, [r, g, b, a] in 0..1, multiplied over every sprite's own
   * tint (day/night, a dimmed parallax plane, a fade-in). One shared
   * uniform write, cheap to animate - unlike TileLayer's, which re-bakes.
   */
  tint?: [number, number, number, number]
  /**
   * Target texels per layer pixel. Absent, the component picks it every
   * layout from the built-in leaf's on-screen size (display scale, any
   * designSize fit, layout scaling), so the layer resamples properly at any
   * scale; with `output` there is no built-in leaf, so set it yourself.
   */
  oversample?: number
  /**
   * Cap on the auto-picked oversample (integer >= 1): the layer stays
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
  label?: string
  ref?: (layer: LayerHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the layer's
   * texture id, and its return renders in place of the built-in `<texture>`
   * leaf. Sprite pointer events then need the layer's handlers on your
   * leaf: `<texture src={texture} {...useSpriteLayer().layer.handlers} />`.
   */
  output?: (texture: TextureId) => Element
  /**
   * Pointer events (default on): the built-in leaf carries layer.handlers,
   * so sprites, groups, the layer's own handlers and a `<Camera2d>` child
   * receive input. `false` detaches them - the leaf then costs no pointer
   * routing at all.
   */
  events?: boolean
}

// Creation size of a fill-mode layer: the first onLayout replaces it
// before the first paint, so it only has to be a valid target size.
const FILL_INITIAL_SIZE = 1

/**
 * Owns a sprite layer and composites it as an ordinary `<texture>` leaf, so
 * the output takes layout, transforms, blendMode, and pointer events like
 * any element - or hand `output` the texture id and compose it yourself.
 * A layout component: it cannot sit inside a d-* subtree; `output` with a
 * `<d-texture>` is the detached form.
 * Children (`<Sprite>`) render nothing themselves - they populate the
 * retained layer through context.
 */
export let SpriteLayer: ParentComponent<SpriteLayerProps> = props => {
  // Fill vs fixed layer is decided at mount, like `output`: both width
  // and height (fixed layer pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("SpriteLayer: width and height come together - give both (fixed layer) or neither (fill)")
    }
    if (props.width === undefined && props.output) {
      throw new Error("SpriteLayer: fill needs the built-in leaf - with output, give width and height")
    }
    return props.width === undefined
  })
  let layer = untrack(() =>
    createSpriteLayer(props.width ?? FILL_INITIAL_SIZE, props.height ?? FILL_INITIAL_SIZE, props.atlas, {
      capacity: props.capacity,
      clearColor: props.clearColor,
      tint: props.tint,
      orderBy: props.orderBy,
      label: props.label,
    }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => {
      if ((w === undefined) !== (h === undefined) || (w === undefined) !== fill) {
        throw new Error("SpriteLayer: fill/fixed is mount-fixed - width/height cannot appear or disappear")
      }
      if (!fill) layer.setSize(w!, h!)
    },
  )
  createEffect(
    () => props.camera,
    camera => {
      if (camera) layer.setCamera(camera)
    },
  )
  createEffect(
    () => props.oversample,
    n => {
      if (n !== undefined) layer.setOversample(n)
    },
  )
  createEffect(
    () => props.tint,
    tint => {
      if (tint !== undefined) layer.setTint(tint)
    },
  )
  untrack(() => props.ref)?.(layer)
  // The layer's own handlers at the root of the walk; the props are read
  // per event, so a handler prop may change without re-registering.
  onCleanup(
    layer.listen({
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
  // The layer's pixel size: the props in fixed mode, the built-in leaf's
  // laid-out box in fill mode (getLayoutBox, the untransformed read, so a
  // designSize fit or ancestor transform never enters the world space -
  // display density rides on the oversample pick below). Null before the
  // first layout.
  let layerSize = (): { width: number; height: number } | null => {
    if (!fill) return { width: props.width!, height: props.height! }
    let box = leaf && getLayoutBox(leaf)
    if (!box || box.width <= 0 || box.height <= 0) return null
    return { width: Math.max(1, Math.round(box.width)), height: Math.max(1, Math.round(box.height)) }
  }
  // Fill: the layer follows the leaf's box. Registered before the pick, so
  // one onLayout applies size then oversample in order; setSize no-ops
  // when nothing changed.
  if (fill) {
    onLayout(() => {
      let size = layerSize()
      if (size) layer.setSize(size.width, size.height)
    })
  }
  // Auto oversample: the built-in leaf's window box, in device pixels, per
  // layer pixel. Picked after every layout, and again when the display scale
  // changes: the first layout runs before the resize event that reports the
  // scale, and a scale change alone lays nothing out. pick runs as an
  // onLayout handler and as an effect apply, both untracked scopes, so its
  // prop reads are wrapped in an explicit untrack.
  let pick = () =>
    untrack(() => {
      if (!leaf || props.oversample !== undefined) return
      let box = getBoundingBoxViewport(leaf)
      let size = layerSize()
      if (!box || !size) return
      let scale = displayScale() * Math.max(box.width / size.width, box.height / size.height)
      applyOversample(layer, scale, size.width, size.height, props.maxOversample)
    })
  onLayout(pick)
  createEffect(() => [displayScale(), props.maxOversample], pick)
  // Sprite events on the built-in leaf: at layer size the plain handlers,
  // in fill mode scaled from the laid-out box (the box can be fractional,
  // the layer size never is).
  let layerHandlers = fill
    ? layer.handlersFor(() => (leaf && getLayoutBox(leaf)) || { width: 0, height: 0 })
    : layer.handlers
  return (
    <LayerContext value={layer}>
      {output ? (
        untrack(() => output(layer.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leaf = n)}
          src={layer.texture}
          width={fill ? "100%" : props.width}
          height={fill ? "100%" : props.height}
          onPointerDown={events ? layerHandlers.onPointerDown : undefined}
          onPointerMove={events ? layerHandlers.onPointerMove : undefined}
          onPointerUp={events ? layerHandlers.onPointerUp : undefined}
          onPointerLeave={events ? layerHandlers.onPointerLeave : undefined}
          onWheel={events ? layerHandlers.onWheel : undefined}
        />
      )}
      {props.children}
    </LayerContext>
  )
}
