// The pure fill-size decision (src/components/fill-math.ts): the device
// box through renderScale and maxPixels. Plain numbers in and out, so it
// runs headless on flux - the app-side checks (tests/fill-knobs.test.tsx)
// pin the follow, this pins the arithmetic at its edges: the budget's one
// factor on both axes, rounding under it, the one-pixel floor, and the
// validation that names the component.

import { test, expect } from "flux:test"
import { checkDensityProps, fillSize } from "../src/components/fill-math.ts"

// A 16:9 device box.
const W = 1920
const H = 1080

test("renderScale multiplies both axes; the device size is the unscaled box", () => {
  expect(fillSize(W, H, {})).toEqual({ width: W, height: H, deviceWidth: W, deviceHeight: H })
  expect(fillSize(W, H, { renderScale: 2 })).toEqual({ width: 2 * W, height: 2 * H, deviceWidth: W, deviceHeight: H })
  expect(fillSize(W, H, { renderScale: 0.5 })).toEqual({ width: W / 2, height: H / 2, deviceWidth: W, deviceHeight: H })
})

test("maxPixels scales both axes by one factor, so the aspect holds", () => {
  // A quarter of the pixels: each axis halves exactly.
  let quarter = fillSize(W, H, { maxPixels: (W * H) / 4 })
  expect(quarter).toEqual({ width: W / 2, height: H / 2, deviceWidth: W, deviceHeight: H })
  // After the scale: 2x then capped back to the box's own count.
  let back = fillSize(W, H, { renderScale: 2, maxPixels: W * H })
  expect(back).toEqual({ width: W, height: H, deviceWidth: W, deviceHeight: H })
  // A budget above the size changes nothing.
  expect(fillSize(W, H, { maxPixels: W * H + 1 })).toEqual({ width: W, height: H, deviceWidth: W, deviceHeight: H })
})

test("rounding keeps the product within a row and a column of the budget", () => {
  let budget = 1000000
  let s = fillSize(W, H, { maxPixels: budget })
  expect(s.width * s.height).toBeLessThanOrEqual(budget + s.width + s.height)
  expect(s.width * s.height).toBeGreaterThan(budget - s.width - s.height)
  // The aspect survives the rounding to within a pixel on the short side.
  expect(Math.abs(s.width / s.height - W / H) * s.height).toBeLessThan(1)
})

test("an axis never rounds below one pixel", () => {
  expect(fillSize(0.2, 0.2, {})).toEqual({ width: 1, height: 1, deviceWidth: 1, deviceHeight: 1 })
  expect(fillSize(W, H, { maxPixels: 1 })).toEqual({ width: 1, height: 1, deviceWidth: W, deviceHeight: H })
  expect(fillSize(W, H, { renderScale: 0.0001 }).height).toBe(1)
})

test("the knobs are validated by name", () => {
  checkDensityProps("Scene", {})
  checkDensityProps("Scene", { renderScale: 1.5, maxPixels: 2e6 })
  for (let bad of [0, -1, NaN, Infinity]) {
    expect(() => checkDensityProps("Scene", { renderScale: bad })).toThrow("Scene: renderScale")
    expect(() => checkDensityProps("View3d", { maxPixels: bad })).toThrow("View3d: maxPixels")
  }
})
