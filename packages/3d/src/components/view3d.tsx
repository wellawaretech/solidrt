import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, PointerFeed, TextureId } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { feedPointer } from "../scene-pointer.ts"
import type { ViewHandle, ViewOptions } from "../scene.ts"
import type { CameraUpdate } from "../camera.ts"
import type { ScenePointerProps } from "./scene.tsx"

export type View3dProps = Pick<ViewOptions, "clearColor" | "label" | "overrideMaterial" | "fog" | "depth" | "samples" | "filter" | "wrap" | "into"> &
  ScenePointerProps & {
  /** Target pixels, live: the view resizes, or its tile moves. Fixed
   * only, for now: a fill mode like Scene's is a later, additive step. */
  width: number
  height: number
  /** The tile's top-left in `into` (view.setRect as props), live; without
   * `into` they mean nothing. */
  x?: number
  y?: number
  /** The view's layer mask (view.setLayers as a prop; default 1). Live. */
  layers?: number
  /** Partial camera update on the view's own camera (view.setCamera):
   * absent keys keep their values. Live. The `<PerspectiveCamera>` child
   * writes the same state - use one form, not both. */
  camera?: CameraUpdate
  /**
   * Compose the leaf yourself: called once (untracked) with view.texture,
   * and its return renders in place of the built-in `<texture>` leaf.
   * Pointer events through the view then need its handlers on your leaf:
   * `<texture src={texture} {...view.handlers} />` (the view from `ref`);
   * the view's pointer feed listens at its root. A tiled view's id is a
   * draw target, not a texture: show `into` with srcX/srcY, as the
   * built-in leaf does.
   */
  output?: (texture: TextureId) => Element
  /** Pointer events (default on): the built-in leaf carries the view's
   * handlers. `false` detaches them. */
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
  // The initial props seed createView, so `ref` hands out a configured
  // view and the first frame draws it whole; the effects follow changes
  // only (`defer`).
  let view = untrack(() => {
    let v = ctx.scene.createView({
      width: props.width,
      height: props.height,
      x: props.x,
      y: props.y,
      layers: props.layers,
      clearColor: props.clearColor,
      label: props.label,
      overrideMaterial: props.overrideMaterial,
      fog: props.fog,
      depth: props.depth,
      samples: props.samples,
      filter: props.filter,
      wrap: props.wrap,
      into: props.into,
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
    () => [props.x, props.y, props.width, props.height] as const,
    ([x, y, width, height]) => {
      if (tiled) view.setRect({ x: x ?? 0, y: y ?? 0, width, height })
      else view.setSize(width, height)
    },
    { defer: true },
  )
  createEffect(
    () => props.layers,
    l => view.setLayers(l ?? 1),
    { defer: true },
  )
  untrack(() => props.ref)?.(view)
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
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  return (
    <SceneContext value={{ scene: ctx.scene, parent: ctx.parent, viewport: view, pointer }}>
      {output ? (
        untrack(() => output(view.texture))
      ) : (
        <texture
          src={tiled ? props.into : view.texture}
          width={props.width}
          height={props.height}
          srcX={tiled ? (props.x ?? 0) : undefined}
          srcY={tiled ? (props.y ?? 0) : undefined}
          srcW={tiled ? props.width : undefined}
          srcH={tiled ? props.height : undefined}
          onPointerDown={events ? view.handlers.onPointerDown : undefined}
          onPointerMove={events ? view.handlers.onPointerMove : undefined}
          onPointerUp={events ? view.handlers.onPointerUp : undefined}
          onPointerLeave={events ? view.handlers.onPointerLeave : undefined}
          onWheel={events ? view.handlers.onWheel : undefined}
        />
      )}
      {props.children}
    </SceneContext>
  )
}
