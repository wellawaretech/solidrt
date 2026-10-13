// The view-leaf follow shared by <SpriteLayer> and <View2d>: in fill mode
// the view's size follows the leaf's layout box (view pixels are the leaf's
// own units; display density rides on the oversample), and in both modes
// the auto oversample follows the leaf's on-screen box. The leaf is the
// built-in <texture>, or the one an `output` callback refs; core's
// followBox measures it after every layout (before any paint) and on
// display-scale changes. The 3d packages' fill.ts is the same shape one
// dimension up, with a fractional render scale where 2d keeps the integer
// oversample.
import { createSignal, followBox, getLayoutBox } from "@solidrt/core"
import type { LayerHandlers } from "../layer.ts"
import { applyOversample } from "./auto-oversample.ts"

/** The density knobs a view leaf takes (see SpriteLayerProps): texels per
 * view pixel, as an integer oversample, a cap on it and a budget. The 3d
 * packages' DensityProps are the same knobs with a fractional scale. */
export type DensityProps = {
  oversample?: number
  maxOversample?: number
  maxPixels?: number
}

/**
 * What an `output` callback composes the leaf with: `ref` it, spread its
 * `handlers`, size a post chain from `size`. In fill mode the view
 * follows the ref'd leaf exactly as it follows the built-in one (its size
 * from the layout box, its oversample from the on-screen box); in fixed
 * mode `ref` scales `handlers` to the leaf's layout box and feeds the
 * oversample pick. The same contract as @solidrt/3d's OutputLeaf.
 */
export type OutputLeaf = {
  /** Hand over the leaf you render (`ref={leaf.ref}`): a layout leaf. A
   * d-* leaf has no box to follow. */
  ref: (node: { id: number }) => void
  /** The view's pointer handlers for the ref'd leaf, scaled to its layout
   * box per event: one spread carries the sprite events, the view's own
   * and its pointer feed. Without a ref they assume a leaf laid out at
   * the view size (`view.handlers`). */
  handlers: LayerHandlers
  /** The view target's size in texels (view pixels times the oversample),
   * reactive: a post chain over the texture sizes its own targets from it
   * (`createShaderTextureMemo` takes the read directly and resizes in
   * place), applied before the frame that follows a resize or a re-pick
   * paints (core flushes pending writes after the layout handlers run).
   * 1 x 1 until the first layout in fill mode. The view's own size,
   * reactive, is `useSpriteLayer().viewportSize`. */
  size: () => { width: number; height: number }
  /** The leaf's on-screen size in device pixels, reactive: what a post
   * pass that should run at display density sizes from. Equals `size`
   * before the first layout. */
  deviceSize: () => { width: number; height: number }
}

/** What createViewFill drives: a view's size and oversample. */
export type FillView = {
  readonly oversample: number
  size(): { width: number; height: number }
  setSize(width: number, height: number): void
  setOversample(n: number): void
}

/** What createViewFill returns: the leaf's reactive reads, the view's own
 * size reactive, and the layout box for handlersFor. */
export type ViewFill = Pick<OutputLeaf, "size" | "deviceSize"> & {
  /** The view's size in view pixels, reactive (view.size() as a signal). */
  viewportSize: () => { width: number; height: number }
  /** The leaf's laid-out box in its units; the fixed size without a leaf
   * in fixed mode, zero before the first layout in fill mode. */
  layout: () => { width: number; height: number }
}

// Creation size of a fill-mode view: the first onLayout replaces it
// before the first paint, so it only has to be a valid target size.
export const FILL_INITIAL_SIZE = 1

/**
 * Keep `view` in step with the leaf `leaf()` returns (undefined before it
 * mounts). `fixed` is the view size in fixed mode, null in fill mode,
 * where the size follows the leaf's layout box (the untransformed read,
 * so a designSize fit or ancestor transform never enters the world
 * space). One followBox applies the size and then the oversample pick;
 * the display scale and the caps re-run it between layouts. `what` names
 * the component in warnings and throws; `output` says the leaf is the
 * app's.
 */
export function createViewFill(
  what: string,
  view: FillView,
  leaf: () => { id: number } | undefined,
  fixed: (() => { width: number; height: number }) | null,
  props: () => DensityProps,
  output: boolean,
): ViewFill {
  let sameSize = (a: { width: number; height: number }, b: { width: number; height: number }) => a.width === b.width && a.height === b.height
  let initial = view.size()
  let [size, setSize] = createSignal({ width: initial.width * view.oversample, height: initial.height * view.oversample }, { equals: sameSize })
  let [deviceSize, setDeviceSize] = createSignal({ width: initial.width, height: initial.height }, { equals: sameSize })
  let [viewportSize, setViewportSize] = createSignal({ width: initial.width, height: initial.height }, { equals: sameSize })
  // The layout box the handlers scale against (getLayoutBox, the
  // untransformed read, so a designSize fit or ancestor transform never
  // skews events): the fixed size without a leaf in fixed mode, zero
  // before the first layout in fill mode (no event can arrive before one,
  // and a degenerate layout passes coordinates through unscaled).
  let layout = () => {
    let node = leaf()
    return (node && getLayoutBox(node)) || (fixed ? fixed() : { width: 0, height: 0 })
  }
  followBox(
    what,
    leaf,
    box => {
      // Fill: the view's pixels are the leaf's own units (the box can be
      // fractional, the view size never is). setSize no-ops when nothing
      // changed.
      if (!fixed) view.setSize(Math.max(1, Math.round(box.layout.width)), Math.max(1, Math.round(box.layout.height)))
      let current = view.size()
      let p = props()
      // Auto oversample: device pixels per view pixel.
      if (p.oversample === undefined) {
        let scale = Math.max(box.device.width / current.width, box.device.height / current.height)
        applyOversample(what, view, scale, current.width, current.height, p.maxOversample, p.maxPixels)
      }
      setViewportSize({ width: current.width, height: current.height })
      setSize({ width: current.width * view.oversample, height: current.height * view.oversample })
      setDeviceSize({ width: Math.max(1, Math.round(box.device.width)), height: Math.max(1, Math.round(box.device.height)) })
    },
    {
      deps: () => {
        let p = props()
        return [p.maxOversample, p.maxPixels]
      },
      missing: output && !fixed ? "fill with output follows the leaf you ref, and none arrived - put ref={leaf.ref} on the leaf your output returns, or give width and height." : undefined,
    },
  )
  return { size, deviceSize, viewportSize, layout }
}
