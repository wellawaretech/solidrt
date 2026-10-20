import { callRef, createEffect, getLayoutBox, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createFill, FILL_INITIAL_SIZE } from "./fill.ts"
import type { OutputLeaf } from "./fill.ts"
import type { DensityProps } from "./fill-math.ts"
import { feedPointer } from "../scene-pointer.ts"
import type { ViewHandle, ViewOptions } from "../scene.ts"
import type { CameraUpdate } from "../camera.ts"
import type { ScenePointerProps } from "./scene.tsx"

export type View3dProps = Pick<ViewOptions, "clearColor" | "label" | "overrideMaterial" | "fog" | "depth" | "samples" | "filter" | "wrap" | "into" | "resolve" | "bloom"> &
  ScenePointerProps &
  DensityProps & {
  /** Target pixels, live: the view resizes, or its tile moves - give
   * both, or neither. Omitted, the view FILLS exactly as a `<Scene>`
   * does: the leaf lays out at 100% of its sized parent and the target
   * follows its on-screen box in device pixels, `renderScale` and
   * `maxPixels` over it (`maxPixels` defaults to the scene's quality
   * row's cap), the leaf `output` refs followed the same way.
   * Fill or fixed is decided at mount; a tile of `into` has its rect, so
   * it gives both. */
  width?: number
  height?: number
  /** The tile's top-left in `into` (view.setRect as props), live; a tile
   * only - without `into` they throw. */
  x?: number
  y?: number
  /** The view's layer mask (view.setLayers as a prop; default 1). Live. */
  layers?: number
  /** Occlusion culling in this view (view.setOcclusion as a prop; default
   * true). Live. */
  occlusion?: boolean
  /** Partial camera update on the view's own camera (view.setCamera):
   * absent keys keep their values. Live. The `<PerspectiveCamera>` child
   * writes the same state - use one form, not both. */
  camera?: CameraUpdate
  /**
   * Compose the leaf yourself: called once (untracked) with view.texture
   * and the leaf contract (see Scene's `output`), and its return renders
   * in place of the built-in `<texture>` leaf. `ref={leaf.ref}` makes a
   * fill-mode view follow your leaf, `{...leaf.handlers}` routes pointer
   * events through the view scaled to the leaf's layout; the view's
   * pointer feed listens at its root. A tiled view's id is a draw target,
   * not a texture: show `into` with srcX/srcY, as the built-in leaf does.
   */
  output?: (texture: TextureId, leaf: OutputLeaf) => Element
  /** Pointer events (default on): the built-in leaf carries the view's
   * handlers. `false` detaches them. Built-in leaf only: with `output`
   * the prop throws, as Scene's. */
  events?: boolean
  /** The pointer feed of this view's leaf (see Scene's `pointer`), fed
   * from the view's root - the events the nodes let through - so a map
   * bound to it drives the camera controls inside this view and no other;
   * inside, `useScene().pointer` is this feed. Fixed at creation. */
  pointer?: PointerFeed
  ref?: (view: ViewHandle) => void
}

/**
 * A second rendering of the enclosing scene from a camera of its own
 * (scene.createView as a component): the same meshes, materials, lights
 * and shadow maps, another target and one entry per mesh, so it costs no
 * per-frame JS. Composites as an ordinary `<texture>` leaf at the target
 * size (a tile of `into` shown through srcX/srcY), or through `output`.
 * Camera-control children (`<OrbitCamera>`, `<FirstPersonCamera>`,
 * `<PerspectiveCamera>`) drive the VIEW's camera: inside, `useScene()`
 * reports the view as `viewport` and the view leaf's feed as `pointer`,
 * so a map over that feed moves this view alone. Node children (`<Mesh>`) mount
 * to the scene as they would outside - a view mirrors the scene's meshes,
 * it has none of its own. Nodes under the view's leaf get their ordinary
 * pointer handlers, picked through the view's camera; the view's own
 * `onPointer*`/`onWheel`/`onTap` are the last stop of that walk
 * (`event.mesh` null over empty space).
 */
export let View3d: ParentComponent<View3dProps> = props => {
  let ctx = useContext(SceneContext)
  // A tile of `into` or a target of its own is decided at mount, like
  // Scene's fill/fixed: `into` cannot appear or disappear.
  let tiled = untrack(() => props.into !== undefined)
  // Fill vs fixed target is decided at mount, as Scene's: both width and
  // height (fixed pixels) or neither (fill). A tile has its rect.
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("View3d: width and height come together - give both (fixed target) or neither (fill)")
    }
    if (props.width === undefined && tiled) {
      throw new Error("View3d: a tile of `into` has its rect - give width and height")
    }
    if (!tiled && (props.x !== undefined || props.y !== undefined)) {
      throw new Error("View3d: x and y place a tile in `into` - a view with a target of its own has no tile to place")
    }
    if (props.width !== undefined && (props.renderScale !== undefined || props.maxPixels !== undefined)) {
      throw new Error("View3d: renderScale and maxPixels scale a fill-mode target - with width and height the target is those pixels")
    }
    return props.width === undefined
  })
  // The initial props seed createView, so `ref` hands out a configured
  // view and the first frame draws it whole; the effects follow changes
  // only (`defer`).
  let view = untrack(() => {
    let v = ctx.scene.createView({
      width: props.width ?? FILL_INITIAL_SIZE,
      height: props.height ?? FILL_INITIAL_SIZE,
      x: props.x,
      y: props.y,
      layers: props.layers,
      occlusion: props.occlusion,
      clearColor: props.clearColor,
      label: props.label,
      overrideMaterial: props.overrideMaterial,
      fog: props.fog,
      depth: props.depth,
      samples: props.samples,
      filter: props.filter,
      wrap: props.wrap,
      into: props.into,
      resolve: props.resolve,
      bloom: props.bloom,
    })
    if (props.camera) v.setCamera(props.camera)
    return v
  })
  createEffect(
    () => props.camera,
    camera => {
      if (camera) view.setCamera(camera)
    },
    { defer: true },
  )
  createEffect(
    () => [props.x, props.y, props.width, props.height, props.renderScale, props.maxPixels] as const,
    ([x, y, width, height, renderScale, maxPixels]) => {
      if ((width === undefined) !== (height === undefined) || (width === undefined) !== fill) {
        throw new Error("View3d: fill/fixed is mount-fixed - width/height cannot appear or disappear")
      }
      if (!tiled && (x !== undefined || y !== undefined)) {
        throw new Error("View3d: x and y place a tile in `into` - a view with a target of its own has no tile to place")
      }
      // Fixed pixels and a scale over them contradict: the fixed contract
      // is that width/height ARE the target.
      if (!fill && (renderScale !== undefined || maxPixels !== undefined)) {
        throw new Error("View3d: renderScale and maxPixels scale a fill-mode target - with width and height the target is those pixels")
      }
      if (fill) return
      if (tiled) view.setRect({ x: x ?? 0, y: y ?? 0, width: width!, height: height! })
      else view.setSize(width!, height!)
    },
    { defer: true },
  )
  createEffect(
    () => props.layers,
    l => view.setLayers(l ?? 1),
    { defer: true },
  )
  createEffect(
    () => props.occlusion,
    o => view.setOcclusion(o ?? true),
    { defer: true },
  )
  // A bloom prop that changes claims the view's own (undefined = off);
  // one never given leaves the view following the scene's.
  createEffect(
    () => props.bloom,
    b => view.setBloom(b ?? null),
    { defer: true },
  )
  callRef(() => props.ref, view)
  onCleanup(() => view.dispose())
  // The pointer feed the app handed in (see Scene) listens at the view's
  // root; children reach it through useScene().pointer.
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
  // app's, so the prop throws (see Scene).
  if (output && untrack(() => props.events) !== undefined) {
    throw new Error("View3d: events applies to the built-in leaf - with output, spread leaf.handlers on your leaf, or leave them off")
  }
  // A feed on a built-in leaf carrying no handlers listens at a root no
  // event reaches: a contradiction, so it throws (the dev validation
  // policy) instead of feeding nothing.
  if (pointer !== null && !events && !output) {
    throw new Error(
      "View3d: a pointer feed with events={false} receives nothing - the feed listens at the view's root, which only the leaf's handlers reach; keep events on, or compose the leaf with output and spread leaf.handlers on it",
    )
  }
  let leafNode: { id: number } | undefined
  // The leaf's laid-out box in ITS units, what handlersFor scales against
  // (see Scene): zero before the first layout in fill mode, the target
  // size in fixed mode until an output leaf is ref'd.
  let leafLayout = () => (leafNode && getLayoutBox(leafNode)) || (fill ? { width: 0, height: 0 } : { width: props.width!, height: props.height! })
  // Fill sizing: the view follows the leaf's on-screen box (fill.ts).
  // The scene's quality row caps a fill view that sets no `maxPixels`.
  let fillReads = fill
    ? createFill("View3d", view, () => leafNode, () => ({ renderScale: props.renderScale, maxPixels: props.maxPixels ?? ctx.quality().maxPixels ?? undefined }), output !== undefined)
    : null
  // Pointer events on the leaf: the built-in fixed leaf is laid out at the
  // target size, so the plain handlers; otherwise scaled from the
  // laid-out box per event.
  let viewHandlers = fill || output ? view.handlersFor(leafLayout) : view.handlers
  let fixedSize = () => ({ width: props.width!, height: props.height! })
  let leaf: OutputLeaf = {
    ref: (n: { id: number }) => (leafNode = n),
    handlers: viewHandlers,
    size: fillReads ? fillReads.size : fixedSize,
    deviceSize: fillReads ? fillReads.deviceSize : fixedSize,
  }
  // The fill leaf is size-contained, as Scene's: it adds nothing to its
  // ancestors, so the target's pixels never pin them.
  return (
    <SceneContext value={{ scene: ctx.scene, parent: ctx.parent, viewport: view, pointer, viewportSize: leaf.size, quality: ctx.quality }}>
      {output ? (
        untrack(() => output(view.texture, leaf))
      ) : (
        <texture
          ref={leaf.ref}
          src={tiled ? props.into : view.texture}
          width={fill ? "100%" : props.width}
          height={fill ? "100%" : props.height}
          contain={fill ? "size" : undefined}
          srcX={tiled ? (props.x ?? 0) : undefined}
          srcY={tiled ? (props.y ?? 0) : undefined}
          srcW={tiled ? props.width : undefined}
          srcH={tiled ? props.height : undefined}
          onPointerDown={events ? viewHandlers.onPointerDown : undefined}
          onPointerMove={events ? viewHandlers.onPointerMove : undefined}
          onPointerUp={events ? viewHandlers.onPointerUp : undefined}
          onPointerCancel={events ? viewHandlers.onPointerCancel : undefined}
          onPointerLeave={events ? viewHandlers.onPointerLeave : undefined}
          onWheel={events ? viewHandlers.onWheel : undefined}
        />
      )}
      {props.children}
    </SceneContext>
  )
}
