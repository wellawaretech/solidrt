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

// The auto-pick apply: the decision itself (cap, shrink hysteresis,
// validation) is pure and lives in oversample-math.ts.
export function applyOversample(
  layer: { readonly oversample: number; setOversample(n: number): void },
  scale: number,
  targetW: number,
  targetH: number,
  max: number | undefined,
): void {
  let n = pickOversample(layer.oversample, scale, targetW, targetH, windowTexels(), limits.maxTextureSize, max)
  if (n !== null) layer.setOversample(n)
}
