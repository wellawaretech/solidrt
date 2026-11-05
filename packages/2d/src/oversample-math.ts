// The pure oversample math: arithmetic on plain numbers, no GPU or window
// imports, so the headless check (tests/oversample.test.ts) drives it
// directly. oversample.ts wraps it with the device limits; components/
// adds the measured inputs (window box, display scale, camera).

/** The auto-pick's default step (`oversampleStep`): the pick is the scale
 * rounded up to a multiple of it. Every common display scale (1.25, 1.5,
 * 1.75, 2.625, 2.75, 3.5) is a multiple of an eighth, so the target
 * matches the display and the composite is 1:1; elsewhere the pick is at
 * most an eighth over, and a scale sweeping a range re-picks eight times
 * per unit, not on every layout. */
export const DEFAULT_OVERSAMPLE_STEP = 1 / 8

/** Tolerance, in steps, under a multiple of the step when rounding a scale
 * up, so a scale that is 3 up to float noise picks 3, not the next step. */
const FIT_EPSILON = 1e-6

/** A size in view pixels at oversample `n`, in target texels: rounded, and
 * at least one. */
export function scaledSize(size: number, n: number): number {
  return Math.max(1, Math.round(size * n))
}

/** Validate the auto-pick's step (`what` names the caller); throws (dev
 * policy). */
export function checkOversampleStep(what: string, step: number): void {
  if (!(Number.isFinite(step) && step > 0)) {
    throw new Error(`${what}: oversampleStep must be a positive number, got ${step}`)
  }
}

/** fitOversample against an explicit device size ceiling (see
 * fitOversample in oversample.ts, which supplies limits.maxTextureSize). */
export function fitOversampleWithin(scale: number, targetW: number, targetH: number, budget: number, maxSize: number, step: number): number {
  checkOversampleStep("fitOversample", step)
  // The budget bounds the scale, not the rounded factor: the rounding may
  // still take a fit that fills the window up by a step (a 320 x 200
  // design on a 2560 x 1440 panel fits at 7.2 and picks 8 at step 1).
  let byBudget = Math.sqrt(budget / (targetW * targetH))
  let n = Math.max(1, Math.ceil(Math.min(scale, byBudget) / step - FIT_EPSILON) * step)
  let byDevice = Math.floor(maxSize / Math.max(targetW, targetH) / step) * step
  return Math.max(1, Math.min(n, byDevice))
}

/**
 * The pick decision: the oversample a target at `current` should move to
 * for a measured `scale`, or null for "keep current" (already there, or
 * the shrink margin holding). `cap` is the maxOversample prop, validated
 * here (throws - the dev policy); `step` the oversampleStep (the tile
 * layer's bake density picks at step 1). `shrinkMargin` is how far, in
 * scale units, the scale must fall below the current factor's lower
 * boundary (`current - step`) before the pick shrinks; growth is always
 * immediate. At 0 the pick is a pure function of its inputs, what the
 * view follow uses; the tile layer's density holds a margin
 * (DENSITY_SHRINK_MARGIN in tiles.ts).
 */
export function pickOversample(
  current: number,
  scale: number,
  targetW: number,
  targetH: number,
  budget: number,
  maxSize: number,
  cap: number | undefined,
  step: number,
  shrinkMargin: number,
): number | null {
  if (cap !== undefined && !(Number.isFinite(cap) && cap > 0)) {
    throw new Error(`maxOversample must be a positive number, got ${cap}`)
  }
  let n = fitOversampleWithin(scale, targetW, targetH, budget, maxSize, step)
  // A bound overrides the shrink margin: the budget, the device size or
  // the cap lowering the pick is an explicit change (maxPixels, a smaller
  // window, maxOversample), not measurement noise.
  let bound = n < fitOversampleWithin(scale, targetW, targetH, Infinity, Infinity, step)
  if (cap !== undefined && n > cap) {
    n = cap
    bound = true
  }
  if (!bound && n < current && scale > current - step - shrinkMargin) return null
  return n === current ? null : n
}
