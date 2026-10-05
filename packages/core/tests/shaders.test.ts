// The screen-size clamp (src/shaders.ts): the JS scale against a
// brute-force reading of the contract, the GLSL twin kept in step by
// text (the two bodies must say the same thing, and this test fails when
// one is edited without the other), and the validation throws. Pure-module
// input only, so it runs headless on flux: `sol test packages/core`.

import { test } from "flux:test"
import { checkScreenSize, SCREEN_SIZE_GLSL, screenSizeScale } from "../src/shaders"

function fail(msg: string): void {
  throw new Error(msg)
}

function range(lo: number, hi: number): number {
  return lo + Math.random() * (hi - lo)
}

test("screenSizeScale: off, collapsed, within bounds, floored, capped, and constant at equal bounds", () => {
  if (screenSizeScale(4, 2, 1, 0, 0) !== 1) fail("no bounds is world size")
  if (screenSizeScale(0, 2, 1, 12, 12) !== 1) fail("a collapsed quad stays collapsed")
  if (screenSizeScale(40, 20, 1, 12, 100) !== 1) fail("a quad within its bounds is unscaled")
  // 4 x 2 at one pixel per unit is 2 px on its smaller axis: 6x lifts it
  // to the 12 px floor; at half a pixel per unit, 12x.
  if (screenSizeScale(4, 2, 1, 12, 0) !== 6) fail("the smaller axis is lifted to the floor")
  if (screenSizeScale(4, 2, 0.5, 12, 0) !== 12) fail("the floor scales with the inverse pixels per unit")
  // 40 x 20 at one pixel per unit is 20 px: a 10 px ceiling halves it.
  if (screenSizeScale(40, 20, 1, 0, 10) !== 0.5) fail("the smaller axis is brought down to the ceiling")
  // Equal bounds: whatever the zoom, the smaller axis is 16 px.
  for (let px of [0.01, 0.5, 1, 7, 100]) {
    let s = screenSizeScale(3, 9, px, 16, 16)
    if (Math.abs(3 * px * s - 16) > 1e-9) fail(`equal bounds not constant at ${px} px per unit: ${3 * px * s}`)
  }
  if (screenSizeScale(-4, 2, 1, 12, 0) !== 6) fail("a negative size reads as its magnitude")
})

test("screenSizeScale matches the contract read literally, over random quads", () => {
  const SWEEPS = 20000
  for (let i = 0; i < SWEEPS; i++) {
    let w = range(0.01, 50)
    let h = range(0.01, 50)
    let px = range(0.05, 20)
    let minPx = Math.random() < 0.3 ? 0 : range(0.1, 60)
    let maxPx = Math.random() < 0.3 ? 0 : range(minPx > 0 ? minPx : 0.1, 120)
    let s = screenSizeScale(w, h, px, minPx, maxPx)
    let drawn = Math.min(w, h) * px * s
    let want = Math.min(w, h) * px
    if (minPx > 0 && want < minPx) want = minPx
    if (maxPx > 0 && want > maxPx) want = maxPx
    if (Math.abs(drawn - want) > 1e-9 * Math.max(1, want)) {
      fail(`${w}x${h} at ${px} px/unit in [${minPx}, ${maxPx}] draws ${drawn}, want ${want}`)
    }
    // Uniform: the larger axis scales by the same factor.
    if (Math.abs(Math.max(w, h) * px * s - (Math.max(w, h) / Math.min(w, h)) * want) > 1e-6 * Math.max(1, want)) fail("the scale is not uniform")
  }
})

test("the GLSL twin states the same clamp", () => {
  // Not a GLSL compiler: a textual pin on the three decisions the JS makes
  // (the smaller axis, the zero-is-off bounds, the collapsed guard), so an
  // edit to one half fails here until the other follows.
  for (let line of [
    "min(abs(size.x), abs(size.y)) * pxPerUnit",
    "if (smallest <= 0.0) return 1.0;",
    "if (screenPx.x > 0.0) target = max(target, screenPx.x);",
    "if (screenPx.y > 0.0) target = min(target, screenPx.y);",
    "return target / smallest;",
  ]) {
    if (!SCREEN_SIZE_GLSL.includes(line)) fail(`GLSL twin lost: ${line}`)
  }
})

test("checkScreenSize: finite and non-negative bounds, the ceiling never below a floor that is on", () => {
  checkScreenSize("t", 0, 0)
  checkScreenSize("t", 16, 0)
  checkScreenSize("t", 0, 16)
  checkScreenSize("t", 16, 16)
  checkScreenSize("t", 8, 16)
  for (let [lo, hi] of [
    [-1, 0],
    [0, -1],
    [Number.NaN, 0],
    [0, Number.POSITIVE_INFINITY],
    [16, 8],
  ] as const) {
    let threw = false
    try {
      checkScreenSize("t", lo, hi)
    } catch {
      threw = true
    }
    if (!threw) fail(`[${lo}, ${hi}] accepted`)
  }
})
