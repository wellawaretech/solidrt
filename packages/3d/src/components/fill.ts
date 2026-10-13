// Fill mode, shared by <Scene> and <View3d>: a target that follows a leaf's
// on-screen box. The leaf is the built-in <texture>, or the one an `output`
// callback refs; core's followBox measures it after every layout (before
// any paint) and on display-scale changes, and fill-math.ts decides the
// size.
import { createSignal, followBox } from "@solidrt/core"
import type { SceneHandlers } from "../scene.ts"
import { checkDensityProps, fillSize } from "./fill-math.ts"
import type { DensityProps } from "./fill-math.ts"

/**
 * What an `output` callback composes the leaf with: `ref` it, spread its
 * `handlers`, size a post chain from `size`. In fill mode the target
 * follows the ref'd leaf exactly as it follows the built-in one; in
 * fixed mode `ref` only scales `handlers` to the leaf's layout box.
 */
export type OutputLeaf = {
  /** Hand over the leaf you render (`ref={leaf.ref}`): a layout leaf,
   * the one whose box the target follows and whose layout the handlers
   * scale by. A d-* leaf has no box to follow. */
  ref: (node: { id: number }) => void
  /** The pointer handlers for the ref'd leaf, scaled to its layout box
   * per event (so a leaf laid out at any size, under any designSize fit
   * or ancestor transform, routes right): one spread carries the node
   * events, the owner's own and its pointer feed. Without a ref they
   * assume a leaf laid out at the target size (`scene.handlers`). */
  handlers: SceneHandlers
  /** The target's size in pixels, reactive: a post chain over the
   * texture sizes its own targets from it (`createShaderTextureMemo`
   * takes the read directly and resizes in place), applied before the
   * frame that follows a resize paints (core flushes pending writes
   * after the layout handlers run). 1 x 1 until the first layout in fill
   * mode; the props in fixed mode. `useScene().viewportSize` is the
   * same read for the children. */
  size: () => { width: number; height: number }
  /** The leaf's on-screen size in device pixels, reactive: what the
   * target would be at `renderScale` 1 and no `maxPixels`, so a post
   * pass that should run at display density (its sampler doubling as the
   * downsample of a supersampled scene) sizes from this, not `size`.
   * Equals `size` before the first layout, and in fixed mode. */
  deviceSize: () => { width: number; height: number }
}

/** What createFill resizes: a scene or a view. */
export type FillTarget = { setSize(width: number, height: number): void; size(): { width: number; height: number } }

// Creation size of a fill-mode target: the first onLayout replaces it
// before the first paint, so it only has to be a valid texture size.
export const FILL_INITIAL_SIZE = 1

/**
 * Make `target` follow the leaf `leaf()` returns (undefined before it
 * mounts) through followBox and fillSize. Returns the reactive size
 * reads an OutputLeaf exposes. `what` names the component in warnings
 * and throws; `output` says the leaf is the app's, for the warning when
 * it never arrives.
 */
export function createFill(
  what: string,
  target: FillTarget,
  leaf: () => { id: number } | undefined,
  props: () => DensityProps,
  output: boolean,
): Pick<OutputLeaf, "size" | "deviceSize"> {
  let initial = target.size()
  let sameSize = (a: { width: number; height: number }, b: { width: number; height: number }) => a.width === b.width && a.height === b.height
  let [size, setSize] = createSignal({ width: initial.width, height: initial.height }, { equals: sameSize })
  let [deviceSize, setDeviceSize] = createSignal({ width: initial.width, height: initial.height }, { equals: sameSize })
  followBox(
    what,
    leaf,
    box => {
      let next = fillSize(box.device.width, box.device.height, props())
      // setSize no-ops when nothing changed.
      target.setSize(next.width, next.height)
      setSize({ width: next.width, height: next.height })
      setDeviceSize({ width: next.deviceWidth, height: next.deviceHeight })
    },
    {
      // The tracked read of the knobs, validated where they enter.
      deps: () => {
        let p = props()
        checkDensityProps(what, p)
        return [p.renderScale, p.maxPixels]
      },
      missing: output ? "fill with output follows the leaf you ref, and none arrived - put ref={leaf.ref} on the leaf your output returns, or give width and height." : undefined,
    },
  )
  return { size, deviceSize }
}
