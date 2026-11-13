// The `solidrt.windowSize` rules (src/lib/window-size.ts): an object with
// exactly "width" and "height", each a whole number of at least 1, and its
// rendering as the client's --size value.
import { test, expect } from "flux:test"
import { checkWindowSize, windowSizeFlag } from "../src/lib/window-size.ts"

test("valid sizes pass", () => {
  expect(checkWindowSize({ width: 900, height: 600 })).toBe(null)
  expect(checkWindowSize({ height: 1, width: 1 })).toBe(null)
  expect(checkWindowSize({ width: 7680, height: 4320 })).toBe(null)
})

test("a side that is missing or not a whole number of at least 1 is refused", () => {
  expect(checkWindowSize({ width: 900 })).toContain('"height" is missing')
  expect(checkWindowSize({ height: 600 })).toContain('"width" is missing')
  expect(checkWindowSize({ width: 0.5, height: 600 })).toContain('"width" must be a whole number of at least 1 (got 0.5)')
  expect(checkWindowSize({ width: 900, height: 0 })).toContain('"height" must be a whole number of at least 1 (got 0)')
  expect(checkWindowSize({ width: -900, height: 600 })).toContain('"width" must be a whole number of at least 1 (got -900)')
  expect(checkWindowSize({ width: "900", height: 600 })).toContain('"width" must be a whole number of at least 1 (got "900")')
})

test("the shape: an object with only width and height", () => {
  expect(checkWindowSize("900x600")).toContain('"windowSize" must be an object with "width" and "height"')
  expect(checkWindowSize([900, 600])).toContain('"windowSize" must be an object with "width" and "height"')
  expect(checkWindowSize(900)).toContain('"windowSize" must be an object with "width" and "height"')
  expect(checkWindowSize({ width: 900, height: 600, depth: 1 })).toContain('"solidrt.windowSize": unknown key "depth"')
})

test("the size renders as the --size value", () => {
  expect(windowSizeFlag({ width: 900, height: 600 })).toBe("900x600")
  expect(windowSizeFlag(undefined)).toBe(undefined)
})
