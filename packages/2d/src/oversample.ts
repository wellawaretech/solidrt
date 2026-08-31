// The layer oversample: how many target texels one layer pixel is drawn
// as. A layer renders at `oversample` times its size and is composited down
// to its box, so a fractional or HiDPI display scale resamples properly: the
// atlas is still sampled nearest inside (texels stay square blocks), the
// composite samples linear (each block edge softens over one device pixel
// instead of snapping to uneven widths). okf/backlog/2d-layer-display-scale.md.
// The pure math lives in oversample-math.ts (checkable headless); this
// module binds it to the device limits and adds the thrash sentinel.
import { limits } from "@solidrt/core/gpu"
import { fitOversampleWithin } from "./oversample-math.ts"

/** Validate an oversample against the target it scales; throws (dev policy). */
export function checkOversample(verb: string, n: number, width: number, height: number): void {
  if (!(Number.isInteger(n) && n >= 1)) {
    throw new Error(`${verb}: oversample must be a positive integer, got ${n}`)
  }
  let max = limits.maxTextureSize
  if (width * n > max || height * n > max) {
    throw new Error(`${verb}: ${width} x ${height} at oversample ${n} exceeds maxTextureSize ${max}`)
  }
}

/**
 * The oversample for a layer shown at `scale` device pixels per layer pixel:
 * the ceiling of the scale, so no device pixel goes without a target texel.
 * Bounded by `budget` texels for the `targetW x targetH` target (the
 * window's own device pixel count: a target beyond it buys nothing on
 * screen, and a layer stretched far past the window inside a scroller must
 * not ask for one) and by what the target can grow to on this device.
 */
export function fitOversample(scale: number, targetW: number, targetH: number, budget: number): number {
  return fitOversampleWithin(scale, targetW, targetH, budget, limits.maxTextureSize)
}

/** Oversample changes inside the window before the thrash warning fires. A
 * healthy layer settles in a change or two (mount, then the post-resize
 * scale); reaching this count means something re-picks every frame. */
const THRASH_CHANGES = 4
/** The window those changes must land in, ms. */
const THRASH_WINDOW_MS = 1000

/**
 * The thrash sentinel: each layer calls the returned hook on every ACTUAL
 * oversample change, and bunched changes warn once per layer. Every change
 * resizes and redraws the layer's targets (a tile layer re-bakes every
 * resident chunk), so a scale sweeping an integer boundary every frame is
 * this package's most expensive silent mistake - worth a console line the
 * moment it happens rather than a profiling session later.
 */
export function thrashSentinel(what: string): () => void {
  let stamps: number[] = []
  let warned = false
  return () => {
    if (warned) return
    let now = performance.now()
    stamps.push(now)
    while (stamps.length > 0 && now - stamps[0]! > THRASH_WINDOW_MS) stamps.shift()
    if (stamps.length < THRASH_CHANGES) return
    warned = true
    console.warn(
      `Oversample thrash: ${what} changed oversample ${THRASH_CHANGES} times within a second. ` +
        `Every change resizes and redraws the layer's targets (a tile layer re-bakes every ` +
        `resident chunk). The usual cause is an animated transform or camera sweeping the ` +
        `measured scale across integer boundaries; pin \`oversample\`, or bound the sweep ` +
        `with \`maxOversample\`. Warned once per layer.`,
    )
  }
}
