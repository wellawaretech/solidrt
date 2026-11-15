// Flux.capabilities: the build's list, defined once after every plugin
// listed its names, and frozen, as is the Flux object itself.
import { expect, test } from "flux:test"

test("Flux.capabilities and Flux are frozen", () => {
  expect(Object.isFrozen(Flux.capabilities)).toBe(true)
  expect(Object.isFrozen(Flux)).toBe(true)
})

test("Flux.capabilities takes no new name", () => {
  // Module code is strict, so a write to the frozen array throws.
  expect(() => (Flux.capabilities as string[]).push("x")).toThrow(TypeError)
  expect(Flux.capabilities.includes("x")).toBe(false)
})

test("Flux.capabilities still lists the base names", () => {
  expect(Flux.capabilities.includes("fs")).toBe(true)
})
