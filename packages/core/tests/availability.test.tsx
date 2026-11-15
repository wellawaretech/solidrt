// availability(name) under the test host: a reactive read of the platform
// facts, fed by the sticky "availability" event. The test host reads the
// recorded facilities as available, and a test's setAvailability reaches a
// reader through the event.

import { test, expect } from "@solidrt/test"
import { availability, capabilities } from "@solidrt/core"

test("a reader of availability follows the test's setAvailability", async app => {
  await app.mount(() => <text label="dialog">{availability("dialog")}</text>)
  expect(app.find({ label: "dialog" }).text).toBe("available")
  app.setAvailability("dialog", "unavailable")
  await app.settle()
  expect(app.find({ label: "dialog" }).text).toBe("unavailable")
  expect(Flux.availability("dialog")).toBe("unavailable")
  app.setAvailability("dialog", "unknown")
  await app.settle()
  expect(app.find({ label: "dialog" }).text).toBe("unknown")
})

test("capabilities.haptics is the haptics availability", async app => {
  await app.mount(() => <text label="haptics">{String(capabilities.haptics)}</text>)
  expect(app.find({ label: "haptics" }).text).toBe("true")
  app.setAvailability("haptics", "unavailable")
  await app.settle()
  expect(app.find({ label: "haptics" }).text).toBe("false")
})

test("an unknown name throws a TypeError, from the read and from the test verb", async app => {
  await app.mount(() => <text>names</text>)
  expect(() => availability("teleport")).toThrow(TypeError)
  expect(() => app.setAvailability("teleport", "available")).toThrow(TypeError)
  expect(() => app.setAvailability("dialog", "maybe" as "unknown")).toThrow(TypeError)
})
