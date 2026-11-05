// The oversample follow shared by <SpriteLayer>, <View2d> and
// <TileLayer>: the view's oversample follows the leaf's on-screen box, by
// the pure rule in oversample-math.ts around the measured inputs (window
// texel budget, device limits).
import { displayScale, windowSize } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import { checkOversampleStep, DEFAULT_OVERSAMPLE_STEP, pickOversample } from "../oversample-math.ts"

// The window's device pixel count: the outer texel budget a followed
// oversample target always stays within, `maxPixels` or not (see
// fitOversample).
function windowTexels(): number {
  let win = windowSize()
  let scale = displayScale()
  return win.width * scale * (win.height * scale)
}

// The follow: the decision itself (step, cap) is pure and lives in
// oversample-math.ts. `maxPixels` is the app's texel
// budget for the target, under the window's own (a target beyond the
// window buys nothing on screen). The props are validated here, where
// they enter, named for the component `what` (throws - the dev policy);
// the pure decision guards its own inputs again for direct callers.
export function followOversample(
  what: string,
  layer: { readonly oversample: number; setOversample(n: number): void },
  scale: number,
  targetW: number,
  targetH: number,
  max: number | undefined,
  maxPixels: number | undefined,
  step: number | undefined,
): void {
  if (max !== undefined && !(Number.isFinite(max) && max > 0)) {
    throw new Error(`${what}: maxOversample must be a positive number, got ${max}`)
  }
  if (maxPixels !== undefined && !(Number.isFinite(maxPixels) && maxPixels > 0)) {
    throw new Error(`${what}: maxPixels must be a positive number, got ${maxPixels}`)
  }
  if (step !== undefined) checkOversampleStep(what, step)
  let budget = maxPixels === undefined ? windowTexels() : Math.min(windowTexels(), maxPixels)
  // No shrink margin: the follow is a pure function of the current box,
  // step and bounds. A scale oscillating on a step boundary re-picks on
  // every swing, which the thrash sentinel reports with the explicit fixes.
  let n = pickOversample(layer.oversample, scale, targetW, targetH, budget, limits.maxTextureSize, max, step ?? DEFAULT_OVERSAMPLE_STEP, 0)
  if (n !== null) layer.setOversample(n)
}
