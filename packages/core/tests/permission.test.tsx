// permission(name) under the test host: a reactive read of the user's
// standing answers, fed by the sticky "permission" event. Until a test sets
// it, the test host reads the machine's own state, so the default is only
// asserted to be one of the three; a test's setPermission reaches a reader
// through the event and the engine's own read at once.

import { test, expect } from "@solidrt/test"
import { permission } from "@solidrt/core"

const STATES = ["granted", "denied", "prompt"]

test("a reader of permission follows the test's setPermission", async app => {
  await app.mount(() => <text label="camera">{permission("camera")}</text>)
  expect(STATES).toContain(app.find({ label: "camera" }).text)
  app.setPermission("camera", "denied")
  await app.settle()
  expect(app.find({ label: "camera" }).text).toBe("denied")
  expect(Flux.permission("camera")).toBe("denied")
})

test("an unknown name throws a TypeError, from the read and from the test verb", async app => {
  // The tracked read subscribes; after the settle the replayed map is there.
  await app.mount(() => <text>{permission("microphone")}</text>)
  await app.settle()
  expect(() => permission("teleport")).toThrow(TypeError)
  expect(() => app.setPermission("teleport", "granted")).toThrow(TypeError)
  expect(() => app.setPermission("camera", "maybe" as "prompt")).toThrow(TypeError)
})
