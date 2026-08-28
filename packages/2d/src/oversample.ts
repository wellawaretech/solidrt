// The layer oversample: how many target texels one layer pixel is drawn
// as. A layer renders at `oversample` times its size and is composited down
// to its box, so a fractional or HiDPI display scale resamples properly: the
// atlas is still sampled nearest inside (texels stay square blocks), the
// composite samples linear (each block edge softens over one device pixel
// instead of snapping to uneven widths). okf/backlog/2d-layer-display-scale.md.
import { limits } from "@solidrt/core/gpu"

/** Ceiling on an auto-picked oversample: a small layer in a huge box would
 * otherwise ask for a target the size of the window squared. Eight covers a
 * 4x design fit on a 2x display. */
export const MAX_OVERSAMPLE = 8

/** Tolerance under a whole number when rounding a display scale up, so a
 * scale that is 3 up to float noise picks 3, not 4. */
const FIT_EPSILON = 1e-6

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
 * the ceiling of the scale, so no device pixel goes without a target texel,
 * capped by MAX_OVERSAMPLE and by what a `targetW x targetH` target can grow
 * to on this device.
 */
export function fitOversample(scale: number, targetW: number, targetH: number): number {
  let n = Math.max(1, Math.ceil(scale - FIT_EPSILON))
  let byDevice = Math.floor(limits.maxTextureSize / Math.max(targetW, targetH))
  return Math.max(1, Math.min(n, MAX_OVERSAMPLE, byDevice))
}
