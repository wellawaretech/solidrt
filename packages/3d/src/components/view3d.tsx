import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, TextureId } from "@solidrt/core"
import { SceneContext, createSceneInput } from "./context.tsx"
import type { ViewHandle, ViewOptions } from "../scene.ts"
import type { CameraUpdate } from "../camera.ts"

export type View3dProps = Pick<ViewOptions, "clearColor" | "label" | "overrideMaterial" | "fog" | "depth" | "samples" | "filter" | "wrap" | "into"> & {
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
   * Spread `useScene().input.handlersFor(layout)` on it so the camera
   * controls inside receive its input (inside a `<View3d>`, useScene's
   * `input` is this view's). A tiled view's id is a draw target, not a
   * texture: show `into` with srcX/srcY, as the built-in leaf does.
   */
  output?: (texture: TextureId) => Element
  ref?: (view: ViewHandle) => void
}

/**
 * A second rendering of the enclosing scene from a camera of its own
 * (scene.createView as a component): the same meshes, materials, lights
 * and shadow maps, another target and one entry per mesh, so it costs no
 * per-frame JS. Composites as an ordinary `<texture>` leaf at the target
 * size (a tile of `into` shown through srcX/srcY), or through `output`.
 * Camera-control children (`<OrbitCamera>`, `<FirstPersonCamera>`,
 * `<PerspectiveCamera>`) drive the VIEW's camera and take their input
 * from the view's leaf: inside, `useScene()` reports the view as `camera`
 * and the view leaf's channel as `input`. Node children (`<Mesh>`) mount
 * to the scene as they would outside - a view mirrors the scene's meshes,
 * it has none of its own. Mesh pointer events stay the scene leaf's
 * (picking is the scene camera's); a view leaf carries none.
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
  let { input, hasInput, hasKeys } = createSceneInput()
  let output = untrack(() => props.output)
  let leafNode: { id: number } | undefined
  // The built-in leaf is laid out at the target size, so pointer
  // coordinates and the controls' viewport are target pixels.
  let leaf = output ? null : input.handlersFor(() => ({ width: props.width, height: props.height }), () => leafNode)
  return (
    <SceneContext value={{ scene: ctx.scene, parent: ctx.parent, camera: view, input }}>
      {output ? (
        untrack(() => output(view.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leafNode = n)}
          src={tiled ? props.into : view.texture}
          width={props.width}
          height={props.height}
          srcX={tiled ? (props.x ?? 0) : undefined}
          srcY={tiled ? (props.y ?? 0) : undefined}
          srcW={tiled ? props.width : undefined}
          srcH={tiled ? props.height : undefined}
          onPointerDown={hasInput() ? leaf!.onPointerDown : undefined}
          onPointerMove={hasInput() ? leaf!.onPointerMove : undefined}
          onPointerUp={hasInput() ? leaf!.onPointerUp : undefined}
          onWheel={hasInput() ? leaf!.onWheel : undefined}
          focusable={hasKeys()}
          onKeyDown={hasKeys() ? leaf!.onKeyDown : undefined}
          onKeyUp={hasKeys() ? leaf!.onKeyUp : undefined}
          onBlur={hasKeys() ? leaf!.onBlur : undefined}
        />
      )}
      {props.children}
    </SceneContext>
  )
}
