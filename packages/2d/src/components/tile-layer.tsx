import { callRef, createEffect, onCleanup, untrack } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import type { ResourcePool, ShaderParams } from "@solidrt/core/gpu"
import type { Atlas } from "../atlas.ts"
import type { Material } from "../material.ts"
import type { CameraUpdate } from "../camera.ts"
import type { Frame } from "../frames.ts"
import { createTileLayer } from "../tiles.ts"
import type { TileChunk, TileLayer as TileLayerHandle, TileRect } from "../tiles.ts"
import { feedPointer } from "../views.ts"
import type { ViewHandle } from "../views.ts"
import { PointerContext, ViewportContext, ViewportSizeContext } from "./context.ts"
import { createViewFill, FILL_INITIAL_SIZE } from "./fill.ts"
import type { DensityProps, OutputLeaf } from "./fill.ts"
import type { LayerPointerProps } from "./sprite-layer.tsx"

/**
 * The tile layer's camera - the same CameraUpdate vocabulary as the sprite
 * layer (see camera.ts for the pivot/rotation semantics and the
 * heading-upward convention), so one signal drives a whole rotating scene
 * across both layers. Here the camera is the view's in-shader mapping and
 * the view the residency gate keeps chunks resident around, never a
 * re-bake; projectCamera/unprojectCamera are the same mapping as plain
 * functions.
 */
export type TileCamera = CameraUpdate

export type TileLayerProps = LayerPointerProps &
  DensityProps & {
  /** Cell pixel size - creation-fixed. Required unless `layer` is given. */
  tileW?: number
  tileH?: number
  /** The tilesets the cells draw from (createAtlas records), bound as one
   * bake; every frame set names one of them. Fixed at mount. Give these
   * (a layer of the component's own, disposed with it) or `layer`, not
   * both. */
  atlases?: Atlas[]
  /**
   * An existing layer (createTileLayer) to show instead of making one: a
   * second view of the same world under a camera of its own (a minimap
   * beside the main view; the layer's chunks are resident where any view
   * wants them). Its lifetime, its hooks (`onEnter`, `onLeave`) and its
   * cells stay its maker's, so the creation props and the hook props
   * throw with it.
   */
  layer?: TileLayerHandle
  /** The frames table `setTiles`' index cells name (a tileset, `grid()`'s
   * array); see TileLayerOptions. Fixed at creation. */
  frames?: Frame[]
  /** Per-chunk clear color (the name says the scope: chunks holding no
   * cell render nothing, so a full-bleed ground color is the view's
   * `clearColor`, or the container behind the leaf). */
  chunkClearColor?: [number, number, number, number]
  /** The material the cells bake with (see TileLayerOptions.material):
   * its blend, its look, its per-cell style record; default `unlit()`.
   * Fixed at mount. */
  material?: Material
  /** Uniform values written to every resident chunk (layer.setParams as
   * a prop, over the material's own). Live, merge semantics; each change
   * re-bakes every resident chunk - slow state, not per frame. */
  params?: ShaderParams
  /**
   * Pin the bake density, target texels per world pixel (live for a
   * layer of the component's own). Absent, the LAYER picks it from its
   * views: every view asks for its oversample times its camera zoom and
   * the layer bakes at the largest, so a main view and a minimap of one
   * world never contend (TileLayerOptions.density). `maxDensity` caps
   * the pick and `mipmap` (default true) keeps a mip chain on the
   * chunks, so a view below the density minifies without aliasing; both
   * creation-fixed. With `layer` all three belong to its maker.
   */
  density?: number
  maxDensity?: number
  mipmap?: boolean
  /** Chunk edge in tiles (default ~512px worth); see TileLayerOptions. */
  chunkTiles?: number
  /**
   * Layer tint, [r, g, b, a] in 0..1, multiplied over every cell's own
   * tint (day/night, a dimmed parallax plane). A change re-renders every
   * resident chunk GPU-side (no record uploads) - drive it from slow
   * state, not per frame.
   */
  tint?: [number, number, number, number]
  /** The world's cell bounds, wrap periods, overhang margin, residency
   * band, texture budget, retention and fill (TileLayerOptions). Fixed
   * at creation. */
  bounds?: TileRect
  wrap?: { cols: number; rows: number }
  overhang?: number
  enter?: number
  leave?: number
  priority?: number
  maxTexels?: number
  retain?: "records" | "none"
  fill?: (chunk: TileChunk) => void
  /** The resource pool the layer's chunks take their buffers and targets
   * from (TileLayerOptions.pool); the layer's own by default. */
  pool?: ResourcePool
  /** The layer's residency hooks (TileLayer.onEnter / onLeave), for a
   * layer of the component's own. */
  onEnter?: (chunk: TileChunk) => void
  onLeave?: (chunk: TileChunk) => void
  /** The layer's label for a layer of the component's own (its chunk
   * resources in the inventory), and the view's `label` either way. */
  label?: string
  /** The layer: cells, tint, createView. */
  ref?: (layer: TileLayerHandle) => void
  /**
   * View pixels of the component's view - give both, or neither. Omitted,
   * the view FILLS: the built-in leaf is laid out at 100% of its parent's
   * box (give it a sized parent, as on the web) and the view sizes to
   * that box, so view pixels are the leaf's own coordinates - the auto
   * oversample still supplies display density on top. Fill or fixed is
   * decided at mount, as `<SpriteLayer>`'s. With a function `output` the
   * view follows the leaf you `ref` (OutputLeaf).
   */
  width?: number
  height?: number
  /**
   * Pan/zoom/rotate over the world (view.setCamera as a prop): a shared
   * params write on the view and the view the gate keeps chunks resident
   * around, never a re-bake. Partial, absent keys keep their values. A
   * `<Camera2d>` child writes the same state - use one form, not both.
   */
  camera?: TileCamera
  /** The view's clear color: the ground every chunk draws over, so a
   * full-bleed backdrop belongs here. */
  clearColor?: [number, number, number, number]
  /** The component's view (texture, camera, handlers) for imperative use
   * outside the tree; inside, `useSpriteLayer().viewport` is the same. */
  viewRef?: (view: ViewHandle) => void
  /**
   * The view's leaf. Absent: a built-in `<texture>` leaf carrying the
   * view's handlers. A function: compose the leaf yourself - called once
   * (untracked) with the view's texture id and the leaf contract, its
   * return renders in place of the built-in leaf; `ref={leaf.ref}` on
   * the leaf you render makes a fill-mode view follow it,
   * `{...leaf.handlers}` routes the view's pointer events through it
   * scaled to its layout, and `leaf.size()` sizes a post chain's targets.
   */
  output?: (texture: TextureId, leaf: OutputLeaf) => Element
  /**
   * Pointer events (default on): the built-in leaf carries the view's
   * handlers, so the view's own handlers and a `<Camera2d>` child receive
   * input. `false` detaches them. Built-in leaf only: with a function
   * `output` the leaf is yours and so is the spread, and the prop throws.
   */
  events?: boolean
  /**
   * The pointer feed of the view (createPointerFeed), fed from the
   * view's root - every event over the world, since a tile view hits no
   * sprite - so an input map over it drives the `<Camera2d>` child;
   * inside, `useSpriteLayer().pointer` is this feed. Fixed at creation.
   */
  pointer?: PointerFeed
}

// The props that describe a layer the component creates - meaningless,
// hence rejected, on an adopted one (`layer`), which was created already.
const CREATION_PROPS = ["tileW", "tileH", "atlases", "frames", "material", "chunkTiles", "bounds", "wrap", "overhang", "enter", "leave", "priority", "maxTexels", "retain", "fill", "pool", "density", "maxDensity", "mipmap", "chunkClearColor"] as const

/**
 * Owns a tile layer (over `atlases`; or adopts an existing one through
 * `layer`, owning only what follows) and one view of it (layer.createView),
 * composited as an ordinary `<texture>` leaf so the output takes layout,
 * transforms, blendMode and pointer events like any element - or hand
 * `output` the texture id and compose it yourself. The leaf fills its
 * parent's box when unsized, exactly as `<SpriteLayer>`'s own view does.
 * The camera is the view's: a camera move is one params write and the
 * gate's cue, never a re-bake, and under `wrap` the view draws every
 * image of a resident chunk inside it, so the seam is two quads over one
 * texture. A second `<TileLayer layer={handle}>` is a second view of the
 * same world (a minimap); a `<Camera2d>` child drives this view's camera.
 * A layout component: it cannot sit inside a d-* subtree; `output` with
 * a `<d-texture>` is the detached form. Tiles are data, not children:
 * write them through `ref` with `setTile` and `setTiles` - there is no
 * `<Tile>` component on purpose (a component per tile would re-introduce
 * the per-element cost the bake removes).
 */
export let TileLayer: ParentComponent<TileLayerProps> = props => {
  let own = untrack(() => props.layer === undefined)
  let layer = untrack(() => {
    if (props.layer !== undefined) {
      for (let name of CREATION_PROPS) {
        if (props[name] !== undefined) throw new Error(`TileLayer: layer adopts an existing layer - ${name} was fixed when it was created`)
      }
      if (props.onEnter !== undefined || props.onLeave !== undefined) throw new Error("TileLayer: with `layer` the residency hooks belong to the layer's maker (layer.onEnter / onLeave)")
      return props.layer
    }
    if (props.tileW === undefined || props.tileH === undefined || props.atlases === undefined) throw new Error("TileLayer: tileW, tileH and atlases are required without `layer`")
    return createTileLayer(props.tileW, props.tileH, props.atlases, {
      chunkClearColor: props.chunkClearColor,
      material: props.material,
      params: props.params,
      density: props.density,
      maxDensity: props.maxDensity,
      mipmap: props.mipmap,
      chunkTiles: props.chunkTiles,
      tint: props.tint,
      frames: props.frames,
      bounds: props.bounds,
      wrap: props.wrap,
      overhang: props.overhang,
      enter: props.enter,
      leave: props.leave,
      priority: props.priority,
      maxTexels: props.maxTexels,
      retain: props.retain,
      fill: props.fill,
      pool: props.pool,
      label: props.label,
    })
  })
  if (own) {
    layer.onEnter = chunk => untrack(() => props.onEnter)?.(chunk)
    layer.onLeave = chunk => untrack(() => props.onLeave)?.(chunk)
    // A pin given later pins; taking it away hands the density back to
    // the views' pick (the creation option is the initial pin).
    createEffect(
      () => props.density,
      n => layer.setDensity(n ?? null),
      { defer: true },
    )
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
  }
  callRef(() => props.ref, layer)
  // Fill vs fixed view is decided at mount, as SpriteLayer's: both width
  // and height (fixed view pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("TileLayer: width and height come together - give both (fixed view) or neither (fill)")
    }
    return props.width === undefined
  })
  // The view: the initial props seed it, the effects follow changes only
  // (`defer`).
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
        throw new Error("TileLayer: fill/fixed is mount-fixed - width/height cannot appear or disappear")
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
  // An adopted layer outlives the component, so its view must not: a
  // layer of the component's own takes the view with it.
  if (!own) onCleanup(() => view.dispose())
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
  // `events` wires the built-in leaf; with output the leaf and its spread
  // are the app's, so the prop has nothing to apply to and throws (the
  // dev validation policy).
  if (output && untrack(() => props.events) !== undefined) {
    throw new Error("TileLayer: events applies to the built-in leaf - with output, spread leaf.handlers on your leaf, or leave them off")
  }
  // A feed on a built-in leaf carrying no handlers listens at a root no
  // event reaches: a contradiction, so it throws instead of feeding
  // nothing.
  if (pointer !== null && !events && !output) {
    throw new Error(
      "TileLayer: a pointer feed with events={false} receives nothing - the feed listens at the view's root, which only the leaf's handlers reach; keep events on, or compose the leaf with output and spread leaf.handlers on it",
    )
  }
  let leafNode: { id: number } | undefined
  // The view follows the leaf (fill.ts): its size from the leaf's layout
  // box in fill mode, its auto oversample from the leaf's on-screen box in
  // both - the built-in leaf, or the one `output` refs.
  let follow = createViewFill(
    "TileLayer",
    view,
    () => leafNode,
    fill ? null : () => ({ width: props.width!, height: props.height! }),
    () => ({ oversample: props.oversample, maxOversample: props.maxOversample, maxPixels: props.maxPixels }),
    output !== undefined,
  )
  let viewHandlers = fill || output ? view.handlersFor(follow.layout) : view.handlers
  let leaf: OutputLeaf = { ref: (n: { id: number }) => (leafNode = n), handlers: viewHandlers, size: follow.size, deviceSize: follow.deviceSize }
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
