// The `solidrt.fonts` map's rules (src/lib/fonts.ts): a boolean names a
// font of the runtime, so without a runtime of the project's own it must be
// a stock role; a path or a boolean under any alias is fine with one.
import { test, expect } from "flux:test"
import { checkFontsMap, STOCK_FONT_ROLES } from "../src/lib/fonts.ts"

test("the scaffold's map and an app's own faces pass", () => {
  expect(checkFontsMap({ sans: true, serif: true, mono: true }, false)).toBe(null)
  expect(checkFontsMap({ sans: "./assets/fonts/Inter.ttf", mono: false, display: "./assets/fonts/F.ttf" }, false)).toBe(null)
  expect(checkFontsMap({}, false)).toBe(null)
  expect(STOCK_FONT_ROLES).toEqual(["sans", "serif", "mono"])
})

test("a boolean on an alias the stock runtime does not bind is refused without a runtime", () => {
  expect(checkFontsMap({ display: false }, false)).toContain('"display": false toggles a font of the runtime')
  expect(checkFontsMap({ momo: true }, false)).toContain("sans/serif/mono")
  // With a runtime of its own, the runtime's set is its lib.rs's: the
  // CLI lets the alias through and lattice checks it at run time.
  expect(checkFontsMap({ display: false }, true)).toBe(null)
})

test("the shape: a map of alias to path, true or false", () => {
  expect(checkFontsMap(["sans"], false)).toContain("must be a map")
  expect(checkFontsMap({ sans: 1 }, false)).toContain('"sans" must be a font file path, true or false')
  expect(checkFontsMap({ ["x".repeat(256)]: "./assets/fonts/F.ttf" }, false)).toContain("too long")
})
