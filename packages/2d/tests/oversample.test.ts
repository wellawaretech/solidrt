// Tests for the oversample auto-pick math (oversample-math.ts): fit
// rounding and bounds, the shrink hysteresis and maxOversample cap in
// pickOversample. Pure-module input only (oversample-math.ts imports nothing), so it runs
// headless on flux: `sol test packages/2d`. The random inputs come from
// Math.random, which `sol test` seeds: the same on every run, and `--seed
// <n>` tries others.

import { test } from "flux:test"
import { fitOversampleWithin, pickOversample } from "../src/oversample-math.ts"

function range(lo: number, hi: number): number {
  return lo + Math.random() * (hi - lo)
}

function fail(msg: string): void {
  throw new Error(msg)
}

// Generous bounds that never bind, for the cases probing something else.
const BIG_BUDGET = 1e12
const BIG_MAX_SIZE = 1e6

test("fitOversampleWithin: rounding and the three bounds", () => {
  let fit = (scale: number, w = 100, h = 100, budget = BIG_BUDGET, maxSize = BIG_MAX_SIZE) =>
    fitOversampleWithin(scale, w, h, budget, maxSize)

  if (fit(1) !== 1) fail(`fit(1) = ${fit(1)}, want 1`)
  if (fit(2.5) !== 3) fail(`fit(2.5) = ${fit(2.5)}, want 3 (ceiling)`)
  if (fit(0.3) !== 1) fail(`fit(0.3) = ${fit(0.3)}, want 1 (floor at 1)`)
  // FIT_EPSILON: a scale that is 3 up to float noise picks 3, not 4.
  if (fit(3 + 1e-7) !== 3) fail(`fit(3 + 1e-7) = ${fit(3 + 1e-7)}, want 3 (epsilon)`)
  if (fit(3.1) !== 4) fail(`fit(3.1) = ${fit(3.1)}, want 4`)
  // Budget bounds the SCALE, not the rounded factor: 320 x 200 against a
  // 2560 x 1440 window fits at sqrt(budget / texels) = 7.589..., which a
  // larger scale clamps to and the ceiling rounds to 8 (the doc's example).
  {
    let n = fit(20, 320, 200, 2560 * 1440)
    if (n !== 8) fail(`budget-bound fit = ${n}, want 8`)
  }
  // Device bound: a 1000 x 1000 target on a 4096 device caps at 4.
  {
    let n = fit(10, 1000, 1000, BIG_BUDGET, 4096)
    if (n !== 4) fail(`device-bound fit = ${n}, want 4`)
  }
})

test("pickOversample: hysteresis, cap, validation", () => {
  let pick = (current: number, scale: number, cap?: number) =>
    pickOversample(current, scale, 100, 100, BIG_BUDGET, BIG_MAX_SIZE, cap)

  // Growth is immediate.
  if (pick(1, 2.2) !== 3) fail(`grow pick(1, 2.2) = ${pick(1, 2.2)}, want 3`)
  // Same factor: keep (null), not a redundant set.
  if (pick(3, 2.9) !== null) fail(`pick(3, 2.9) = ${pick(3, 2.9)}, want null (same factor)`)
  // Shrink hysteresis: at current 3 the lower boundary is 2, the margin holds
  // until scale drops below 2 - 0.25 = 1.75.
  if (pick(3, 1.9) !== null) fail(`pick(3, 1.9) = ${pick(3, 1.9)}, want null (hysteresis holds)`)
  if (pick(3, 1.7) !== 2) fail(`pick(3, 1.7) = ${pick(3, 1.7)}, want 2 (past the margin)`)
  // The cap overrides the hysteresis: lowering maxOversample is an explicit ask.
  if (pick(3, 2.9, 1) !== 1) fail(`pick(3, 2.9, cap 1) = ${pick(3, 2.9, 1)}, want 1 (cap overrides)`)
  // A cap above the pick changes nothing.
  if (pick(1, 2.2, 8) !== 3) fail(`pick(1, 2.2, cap 8) = ${pick(1, 2.2, 8)}, want 3`)
  // Cap validation throws (dev policy).
  for (let bad of [0, 1.5, -1]) {
    let threw = false
    try {
      pick(1, 1, bad)
    } catch {
      threw = true
    }
    if (!threw) fail(`pick with cap ${bad} did not throw`)
  }
})


