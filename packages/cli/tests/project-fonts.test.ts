// The `solidrt.fonts` map's rules (src/lib/fonts.ts): a boolean names a
// font of the runtime, so it must be in the runtime's listed set, the stock
// roles for a project without a runtime of its own; an unknown set (a
// custom runtime not built here) lets any alias through.
import { test, expect } from "flux:test"
import { checkFontsMap, compiledInUnusedHint, STOCK_FONT_ROLES } from "../src/lib/fonts.ts"

test("the scaffold's map and an app's own faces pass", () => {
  expect(checkFontsMap({ sans: true, serif: true, mono: true }, STOCK_FONT_ROLES)).toBe(null)
  expect(checkFontsMap({ sans: "./assets/fonts/Inter.ttf", mono: false, display: "./assets/fonts/F.ttf" }, STOCK_FONT_ROLES)).toBe(null)
  expect(checkFontsMap({}, STOCK_FONT_ROLES)).toBe(null)
  expect(STOCK_FONT_ROLES).toEqual(["sans", "serif", "mono"])
})

test("a boolean on an alias the runtime does not bind is refused", () => {
  expect(checkFontsMap({ display: false }, STOCK_FONT_ROLES)).toContain('"display": false toggles a font of the runtime')
  expect(checkFontsMap({ momo: true }, STOCK_FONT_ROLES)).toContain("binds only sans/serif/mono")
  // A custom runtime's listed set: its own aliases pass, a stock role it
  // dropped does not, and an empty set binds none.
  expect(checkFontsMap({ display: false, sans: true }, ["sans", "display"])).toBe(null)
  expect(checkFontsMap({ mono: false }, ["sans", "display"])).toContain("binds only sans/display")
  expect(checkFontsMap({ sans: true }, [])).toContain("binds none")
  // Not built here, so the set is unknown: the alias goes through and
  // lattice checks it at run time.
  expect(checkFontsMap({ display: false }, null)).toBe(null)
})

test("the shape: a map of alias to path, true or false", () => {
  expect(checkFontsMap(["sans"], STOCK_FONT_ROLES)).toContain("must be a map")
  expect(checkFontsMap({ sans: 1 }, STOCK_FONT_ROLES)).toContain('"sans" must be a font file path, true or false')
  expect(checkFontsMap({ ["x".repeat(256)]: "./assets/fonts/F.ttf" }, STOCK_FONT_ROLES)).toContain("too long")
})

test("a project runtime's face the app drops or replaces is hinted", () => {
  let hint = compiledInUnusedHint(["serif", "mono", "display"], ["sans", "serif", "mono"])
  expect(hint).toContain("drops or replaces serif, mono, which the runtime still compiles in")
  expect(hint).toContain("their lines")
  expect(compiledInUnusedHint(["sans"], ["sans"])).toContain("its line")
  // Nothing claimed the runtime binds, or its set unknown: no hint.
  expect(compiledInUnusedHint(["display"], ["sans"])).toBe(null)
  expect(compiledInUnusedHint(["serif"], null)).toBe(null)
})
