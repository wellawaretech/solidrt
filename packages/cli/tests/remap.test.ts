// Stack-frame remapping (src/server/remap.ts): bundle positions as QuickJS
// prints them, rewritten to source positions through a sourcemap. The map
// here is written by hand, so every expected position is known: generated
// line 1 column 0 is app.tsx 1:0, and generated line 2 column 4 is app.tsx
// 10:6 (both 0-based columns, as a sourcemap stores them; a stack frame
// counts columns from 1).
import { test, expect } from "flux:test"
import { remapPositions } from "../src/server/remap.ts"

// "AAAA" is the segment [0, 0, 0, 0]; "IASM" is [4, 0, 9, 6], relative to it.
let map = (source: string) => JSON.stringify({ version: 3, sources: [source], names: [], mappings: "AAAA;IASM" })
let maps = { main: map("src/app.tsx") }

test("remapPositions: a frame's line and column map to the source, columns counted from 1", () => {
  expect(remapPositions("    at boom (main:2:5)", maps)).toBe("    at boom (src/app.tsx:10:7)")
})

test("remapPositions: a position without a column reads column 1", () => {
  expect(remapPositions("error at main:1", maps)).toBe("error at src/app.tsx:1:1")
})

test("remapPositions: every position in the text is rewritten", () => {
  expect(remapPositions("at a (main:1:1)\nat b (main:2:5)", maps)).toBe("at a (src/app.tsx:1:1)\nat b (src/app.tsx:10:7)")
})

test("remapPositions: a position the map has no entry for passes through", () => {
  expect(remapPositions("at boom (main:3:1)", maps)).toBe("at boom (main:3:1)")
  expect(remapPositions("at boom (main:2:2)", maps)).toBe("at boom (main:2:2)")
})

test("remapPositions: each module is remapped against its own map", () => {
  let both = { main: map("src/app.tsx"), worker: map("isolates/worker.ts") }
  expect(remapPositions("at a (main:2:5), at b (worker:2:5)", both)).toBe("at a (src/app.tsx:10:7), at b (isolates/worker.ts:10:7)")
  expect(remapPositions("at c (other:2:5)", both)).toBe("at c (other:2:5)")
})

test("remapPositions: a module name inside a longer name is left alone", () => {
  let audio = { audio: map("isolates/audio.ts") }
  expect(remapPositions("at a (workers/audio:2:5)", audio)).toBe("at a (workers/audio:2:5)")
  expect(remapPositions("at a (audio:2:5)", audio)).toBe("at a (isolates/audio.ts:10:7)")
})

test("remapPositions: without maps, or with a malformed one, the text is unchanged", () => {
  expect(remapPositions("at boom (main:2:5)", null)).toBe("at boom (main:2:5)")
  expect(remapPositions("at boom (broken:2:5)", { broken: "not json" })).toBe("at boom (broken:2:5)")
})
