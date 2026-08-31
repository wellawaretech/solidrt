// The pure oversample math: arithmetic on plain numbers, no GPU or window
// imports, so the headless check (checks/oversample-check.ts) drives it
// directly. oversample.ts wraps it with the device limits; components.tsx
// adds the measured inputs (window box, display scale, camera).

/** Tolerance under a whole number when rounding a display scale up, so a
 * scale that is 3 up to float noise picks 3, not 4. */
const FIT_EPSILON = 1e-6

/** How far below the current factor's lower boundary the scale must fall
 * before an auto-picked oversample shrinks. Growth is immediate (an
 * undersampled layer is visibly soft); the margin keeps a scale
 * oscillating around an integer (a breathing ancestor transform, float
 * noise in a measured box) from re-baking the layer on every swing. */
const SHRINK_MARGIN = 0.25

/** fitOversample against an explicit device size ceiling (see
 * fitOversample in oversample.ts, which supplies limits.maxTextureSize). */
export function fitOversampleWithin(scale: number, targetW: number, targetH: number, budget: number, maxSize: number): number {
  // The budget bounds the scale, not the rounded factor: the ceiling may
  // still round a fit that fills the window up by one (a 320 x 200 design
  // on a 2560 x 1440 panel fits at 7.2 and needs 8).
  let byBudget = Math.sqrt(budget / (targetW * targetH))
  let n = Math.max(1, Math.ceil(Math.min(scale, byBudget) - FIT_EPSILON))
  let byDevice = Math.floor(maxSize / Math.max(targetW, targetH))
  return Math.max(1, Math.min(n, byDevice))
}

/**
 * The auto-pick decision: the oversample a layer at `current` should move
 * to for a measured `scale`, or null for "keep current" (already there, or
 * the shrink hysteresis holding). `cap` is the maxOversample prop,
 * validated here (throws - the dev policy).
 */
export function pickOversample(
  current: number,
  scale: number,
  targetW: number,
  targetH: number,
  budget: number,
  maxSize: number,
  cap: number | undefined,
): number | null {
  if (cap !== undefined && !(Number.isInteger(cap) && cap >= 1)) {
    throw new Error(`maxOversample must be a positive integer, got ${cap}`)
  }
  let n = fitOversampleWithin(scale, targetW, targetH, budget, maxSize)
  // The cap overrides the shrink hysteresis: lowering maxOversample below
  // the current factor is an explicit ask, not measurement noise.
  let capped = false
  if (cap !== undefined && n > cap) {
    n = cap
    capped = true
  }
  if (!capped && n < current && scale > current - 1 - SHRINK_MARGIN) return null
  return n === current ? null : n
}

/**
 * The device-pixels-per-world-pixel factor of a tile world measured through
 * a rotated camera, before the display scale: the measured box is the AABB
 * of the ROTATED world view, which swells by up to sqrt(2) as the camera
 * turns, but texels per world pixel do not change with rotation - so the
 * swell is divided back out for the known rotation, leaving the camera zoom
 * and any ancestor fit.
 */
export function tileWorldScale(boxW: number, boxH: number, worldW: number, worldH: number, rotation: number): number {
  let cos = Math.abs(Math.cos(rotation))
  let sin = Math.abs(Math.sin(rotation))
  let rotW = worldW * cos + worldH * sin
  let rotH = worldW * sin + worldH * cos
  return Math.max(boxW / rotW, boxH / rotH)
}
