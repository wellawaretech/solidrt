// Tests for the oversample auto-pick math (oversample-math.ts): fit
// rounding to the step and bounds, the shrink margin, the bounds and the
// maxOversample cap in pickOversample, the rounded target size.
// Pure-module input only (oversample-math.ts imports nothing), so it runs
// headless on flux: `sol test packages/2d`. The random inputs come from
// Math.random, which `sol test` seeds: the same on every run, and `--seed
// <n>` tries others.

import { test } from "flux:test"
import { DEFAULT_OVERSAMPLE_STEP, fitOversampleWithin, pickOversample, scaledSize } from "../src/oversample-math.ts"

function range(lo: number, hi: number): number {
  return lo + Math.random() * (hi - lo)
}

function fail(msg: string): void {
  throw new Error(msg)
}

// Generous bounds that never bind, for the cases probing something else.
const BIG_BUDGET = 1e12
const BIG_MAX_SIZE = 1e6
// How far under the scale a pick may sit: the fit's FIT_EPSILON (in
// steps) rounds a scale just past a multiple down to it.
const UNDER_TOLERANCE = 1e-6
// The tile layer's density margin (DENSITY_SHRINK_MARGIN in tiles.ts,
// which this pure test does not import).
const DENSITY_MARGIN = 0.25

test("fitOversampleWithin: rounding to the step and the three bounds", () => {
  let fit = (scale: number, step = 1, w = 100, h = 100, budget = BIG_BUDGET, maxSize = BIG_MAX_SIZE) =>
    fitOversampleWithin(scale, w, h, budget, maxSize, step)

  // Step 1, the whole numbers pixel art picks at.
  if (fit(1) !== 1) fail(`fit(1) = ${fit(1)}, want 1`)
  if (fit(2.5) !== 3) fail(`fit(2.5) = ${fit(2.5)}, want 3 (rounded up)`)
  if (fit(0.3) !== 1) fail(`fit(0.3) = ${fit(0.3)}, want 1 (floor at 1)`)
  // FIT_EPSILON: a scale that is 3 up to float noise picks 3, not 4.
  if (fit(3 + 1e-7) !== 3) fail(`fit(3 + 1e-7) = ${fit(3 + 1e-7)}, want 3 (epsilon)`)
  if (fit(3.1) !== 4) fail(`fit(3.1) = ${fit(3.1)}, want 4`)
  // The default step: every common display scale picks itself, anything
  // else at most a step over.
  for (let display of [1.25, 1.5, 1.75, 2.625, 2.75, 3.5]) {
    let n = fit(display, DEFAULT_OVERSAMPLE_STEP)
    if (n !== display) fail(`fit(${display}) at the default step = ${n}, want ${display}`)
  }
  if (fit(3.6, DEFAULT_OVERSAMPLE_STEP) !== 3.625) fail(`fit(3.6) at the default step = ${fit(3.6, DEFAULT_OVERSAMPLE_STEP)}, want 3.625`)
  for (let i = 0; i < 100; i++) {
    let scale = range(1, 8)
    let n = fit(scale, DEFAULT_OVERSAMPLE_STEP)
    if (n < scale - UNDER_TOLERANCE || n > scale + DEFAULT_OVERSAMPLE_STEP) fail(`fit(${scale}) at the default step = ${n}, want within a step above`)
  }
  // Budget bounds the SCALE, not the rounded factor: 320 x 200 against a
  // 2560 x 1440 window fits at sqrt(budget / texels) = 7.589..., which a
  // larger scale clamps to and step 1 rounds to 8 (the doc's example).
  {
    let n = fit(20, 1, 320, 200, 2560 * 1440)
    if (n !== 8) fail(`budget-bound fit = ${n}, want 8`)
  }
  // Device bound: a 1000 x 1000 target on a 4096 device caps at 4, the
  // largest multiple of the step that fits: 4.0 at an eighth.
  {
    let n = fit(10, 1, 1000, 1000, BIG_BUDGET, 4096)
    if (n !== 4) fail(`device-bound fit = ${n}, want 4`)
    let stepped = fit(10, DEFAULT_OVERSAMPLE_STEP, 1000, 1000, BIG_BUDGET, 4096)
    if (stepped !== 4) fail(`device-bound fit at the default step = ${stepped}, want 4`)
  }
  // The step is validated (dev policy).
  for (let bad of [0, -1, Infinity, NaN]) {
    let threw = false
    try {
      fit(2, bad)
    } catch {
      threw = true
    }
    if (!threw) fail(`fit with step ${bad} did not throw`)
  }
})

test("pickOversample: the shrink margin, bounds overriding it, cap, validation", () => {
  let pick = (current: number, scale: number, opts: { cap?: number; step?: number; margin?: number; budget?: number } = {}) =>
    pickOversample(current, scale, 100, 100, opts.budget ?? BIG_BUDGET, BIG_MAX_SIZE, opts.cap, opts.step ?? 1, opts.margin ?? 0)

  // Growth is immediate.
  if (pick(1, 2.2) !== 3) fail(`grow pick(1, 2.2) = ${pick(1, 2.2)}, want 3`)
  // Same factor: keep (null), not a redundant set.
  if (pick(3, 2.9) !== null) fail(`pick(3, 2.9) = ${pick(3, 2.9)}, want null (same factor)`)
  // No margin (the view follow): a pure function of the scale, so it
  // shrinks at once, at any step.
  if (pick(3, 1.9) !== 2) fail(`pick(3, 1.9) = ${pick(3, 1.9)}, want 2 (no margin)`)
  let eighth = DEFAULT_OVERSAMPLE_STEP
  if (pick(1, 1.5, { step: eighth }) !== 1.5) fail(`grow pick(1, 1.5) at an eighth = ${pick(1, 1.5, { step: eighth })}, want 1.5`)
  if (pick(1.625, 1.5, { step: eighth }) !== 1.5) fail(`pick(1.625, 1.5) at an eighth = ${pick(1.625, 1.5, { step: eighth })}, want 1.5 (no margin)`)
  // The density's margin: at current 3 the lower boundary is 2, and the
  // margin holds until the scale drops below 2 - 0.25 = 1.75.
  if (pick(3, 1.9, { margin: DENSITY_MARGIN }) !== null) fail(`pick(3, 1.9, margin) = ${pick(3, 1.9, { margin: DENSITY_MARGIN })}, want null (margin holds)`)
  if (pick(3, 1.7, { margin: DENSITY_MARGIN }) !== 2) fail(`pick(3, 1.7, margin) = ${pick(3, 1.7, { margin: DENSITY_MARGIN })}, want 2 (past the margin)`)
  // A bound overrides the margin: a budget of 200 x 200 texels allows 2,
  // and the pick drops to it though the scale has not moved.
  {
    let n = pick(4, 4, { margin: DENSITY_MARGIN, budget: 200 * 200 })
    if (n !== 2) fail(`budget-bound pick(4, 4, margin) = ${n}, want 2 (the bound overrides)`)
  }
  // So does the cap: lowering maxOversample is an explicit ask.
  if (pick(3, 2.9, { cap: 1, margin: DENSITY_MARGIN }) !== 1) fail(`pick(3, 2.9, cap 1) = ${pick(3, 2.9, { cap: 1, margin: DENSITY_MARGIN })}, want 1 (cap overrides)`)
  // A fractional cap is a cap like any other.
  if (pick(1, 2.2, { cap: 1.5 }) !== 1.5) fail(`pick(1, 2.2, cap 1.5) = ${pick(1, 2.2, { cap: 1.5 })}, want 1.5`)
  // A cap above the pick changes nothing.
  if (pick(1, 2.2, { cap: 8 }) !== 3) fail(`pick(1, 2.2, cap 8) = ${pick(1, 2.2, { cap: 8 })}, want 3`)
  // Cap validation throws (dev policy).
  for (let bad of [0, -1, NaN]) {
    let threw = false
    try {
      pick(1, 1, { cap: bad })
    } catch {
      threw = true
    }
    if (!threw) fail(`pick with cap ${bad} did not throw`)
  }
})

test("scaledSize: rounded, never below one texel", () => {
  if (scaledSize(1333, 1.5) !== 2000) fail(`scaledSize(1333, 1.5) = ${scaledSize(1333, 1.5)}, want 2000`)
  if (scaledSize(100, 2) !== 200) fail(`scaledSize(100, 2) = ${scaledSize(100, 2)}, want 200`)
  if (scaledSize(1, 0.25) !== 1) fail(`scaledSize(1, 0.25) = ${scaledSize(1, 0.25)}, want 1`)
})
