// The auto oversample apply shared by <SpriteLayer> and <TileLayer>: the
// measured inputs (window texel budget, device limits) around the pure
// decision in oversample-math.ts.
import { displayScale, windowSize } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import { pickOversample } from "../oversample-math.ts"

// The window's device pixel count: the texel budget an auto-picked
// oversample target stays within (see fitOversample).
function windowTexels(): number {
  let win = windowSize()
  let scale = displayScale()
  return win.width * scale * (win.height * scale)
}

// The auto-pick apply: the decision itself (cap, shrink hysteresis) is
// pure and lives in oversample-math.ts. `maxPixels` is the app's texel
// budget for the target, under the window's own (a target beyond the
// window buys nothing on screen). The props are validated here, where
// they enter, named for the component `what` (throws - the dev policy);
// the pure decision guards its own inputs again for direct callers.
export function applyOversample(
  what: string,
  layer: { readonly oversample: number; setOversample(n: number): void },
  scale: number,
  targetW: number,
  targetH: number,
  max: number | undefined,
  maxPixels: number | undefined,
): void {
  if (max !== undefined && !(Number.isInteger(max) && max >= 1)) {
    throw new Error(`${what}: maxOversample must be a positive integer, got ${max}`)
  }
  if (maxPixels !== undefined && !(Number.isFinite(maxPixels) && maxPixels > 0)) {
    throw new Error(`${what}: maxPixels must be a positive number, got ${maxPixels}`)
  }
  let budget = maxPixels === undefined ? windowTexels() : Math.min(windowTexels(), maxPixels)
  let n = pickOversample(layer.oversample, scale, targetW, targetH, budget, limits.maxTextureSize, max)
  if (n !== null) layer.setOversample(n)
}
