// The project's `uses` under sol test: the manifest carries the declared
// set, and what the project switched off leaves the engine's capability
// list, with the module calls behind it refusing or playing nothing.

import { test, expect } from "@solidrt/test"
import { open } from "flux:camera"
import { feedbackSupport } from "flux:haptics"

test("a name the project switched off leaves Flux.capabilities", () => {
  expect(Flux.capabilities.includes("camera")).toBe(false)
  expect(Flux.capabilities.includes("microphone")).toBe(true)
})

test("a facility the project switched off reads unavailable, though the test session records", () => {
  expect(Flux.availability("camera")).toBe("unavailable")
  expect(Flux.availability("haptics")).toBe("unavailable")
  expect(Flux.availability("link")).toBe("available")
})

test("opening a switched-off camera rejects with NotSupportedError", async () => {
  let name = await open().then(
    () => "opened",
    (e: Error) => e.name,
  )
  expect(name).toBe("NotSupportedError")
})

test("switched-off haptics reports no actuator, though the test session records", () => {
  expect(feedbackSupport().actuator).toBe(false)
})

test("a permission the project switched off rejects its query with a TypeError", async () => {
  let name = await navigator.permissions.query({ name: "camera" }).then(
    () => "resolved",
    (e: Error) => e.name,
  )
  expect(name).toBe("TypeError")
  let microphone = await navigator.permissions.query({ name: "microphone" })
  expect(["granted", "denied", "prompt"]).toContain(microphone.state)
})
