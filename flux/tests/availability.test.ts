// Flux.availability: whether a platform facility is there now, in this
// engine. The test host records links, shares and haptics, so it reports
// those available whatever the machine has; the device facilities stay
// unknown until their enumeration lands. The platform names are no longer
// in the build's list.
import { expect, test } from "flux:test"

test("the test host reports its recorded facilities available", () => {
  expect(Flux.availability("link")).toBe("available")
  expect(Flux.availability("share")).toBe("available")
  expect(Flux.availability("haptics")).toBe("available")
})

test("a device facility is unknown", () => {
  expect(Flux.availability("camera")).toBe("unknown")
})

test("an unknown facility name throws a TypeError", () => {
  expect(() => Flux.availability("teleport")).toThrow(TypeError)
  expect(() => Flux.availability("teleport")).toThrow("Flux.availability: unknown facility teleport")
})

test("Flux.capabilities holds no platform facility", () => {
  for (let name of ["link", "share", "haptics", "dialog", "clipboard-image"]) {
    expect(Flux.capabilities.includes(name)).toBe(false)
  }
})

test("Flux stays frozen with availability on it", () => {
  expect(Object.isFrozen(Flux)).toBe(true)
  expect(typeof Flux.availability).toBe("function")
})
