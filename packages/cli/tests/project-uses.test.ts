// The `solidrt.uses` rules (src/lib/uses.ts): every name is on unless
// switched off, the manifest carries the declared set sorted, and the old
// spellings (the "capabilities" key, the "vibration" name) are told their
// new names.
import { test, expect } from "flux:test"
import { checkUses, resolveUses, usesEntry, USES } from "../src/lib/uses.ts"

test("every name is on unless switched off", () => {
  expect(resolveUses({})).toEqual([...USES])
  expect(resolveUses({ uses: { camera: true } })).toEqual([...USES])
  expect(resolveUses({ uses: { camera: false } })).toEqual(["microphone", "haptics", "internet"])
  expect(resolveUses({ uses: { camera: false, microphone: false, haptics: false, internet: false } })).toEqual([])
})

test("the manifest carries the declared set sorted, and nothing without a project", () => {
  expect(usesEntry({})).toEqual({ uses: ["camera", "haptics", "internet", "microphone"] })
  expect(usesEntry({ uses: { haptics: false } })).toEqual({ uses: ["camera", "internet", "microphone"] })
  expect(usesEntry({ uses: { camera: false, microphone: false, haptics: false, internet: false } })).toEqual({ uses: [] })
  expect(usesEntry(null)).toEqual({})
  expect(JSON.stringify(usesEntry({ uses: { microphone: false } }))).toBe('{"uses":["camera","haptics","internet"]}')
})

test("a well-formed or unset declaration passes", () => {
  expect(checkUses({})).toBe(null)
  expect(checkUses({ uses: null })).toBe(null)
  expect(checkUses({ uses: { camera: true, microphone: false, haptics: true, internet: true } })).toBe(null)
})

test("the old key and the old name are told their new names", () => {
  expect(checkUses({ capabilities: { camera: false } })).toBe('"solidrt": "capabilities" is now named "uses"; rename the key')
  expect(checkUses({ uses: { vibration: true } })).toBe('"solidrt.uses": "vibration" is now named "haptics"; rename the key')
})

test("the shape: a map of a known name to true or false", () => {
  expect(checkUses({ uses: ["camera"] })).toBe('"solidrt": "uses" must be a map of name to true/false')
  expect(checkUses({ uses: true })).toBe('"solidrt": "uses" must be a map of name to true/false')
  expect(checkUses({ uses: { bluetooth: true } })).toBe('"solidrt.uses": "bluetooth" is not one of camera, microphone, haptics, internet')
  expect(checkUses({ uses: { camera: "yes" } })).toBe('"solidrt.uses": "camera" must be true or false')
})
