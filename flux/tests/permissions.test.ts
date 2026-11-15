// Flux.permission: the user's standing answer on a device permission, in
// this engine. The names are "camera" and "microphone"; the state is one of
// three strings, read live; anything else throws a TypeError. The same
// shape as Flux.availability.
import { expect, test } from "flux:test"

const STATES = ["granted", "denied", "prompt"]

test("the camera reads one of the three states", () => {
  expect(STATES.includes(Flux.permission("camera"))).toBe(true)
})

test("the microphone reads one of the three states", () => {
  expect(STATES.includes(Flux.permission("microphone"))).toBe(true)
})

test("an unknown name throws a TypeError", () => {
  expect(() => Flux.permission("teleport")).toThrow(TypeError)
  expect(() => Flux.permission("teleport")).toThrow("Flux.permission: unknown permission teleport")
})

test("a name that is not a string throws a TypeError", () => {
  expect(() => (Flux.permission as unknown as () => string)()).toThrow(TypeError)
})

test("Flux stays frozen with permission on it", () => {
  expect(Object.isFrozen(Flux)).toBe(true)
  expect(typeof Flux.permission).toBe("function")
})
