// navigator.permissions: the web's Permissions API, simplified. The names
// are "camera" and "microphone"; the state is one of three strings, read
// live; anything else rejects with a TypeError, as the web rejects a name
// it does not support.
import { expect, test } from "flux:test"

const STATES = ["granted", "denied", "prompt"]

test("a camera query resolves to a status with its name and a state", async () => {
  let status = await navigator.permissions.query({ name: "camera" })
  expect(status.name).toBe("camera")
  expect(STATES.includes(status.state)).toBe(true)
})

test("a microphone query resolves to a status with its name and a state", async () => {
  let status = await navigator.permissions.query({ name: "microphone" })
  expect(status.name).toBe("microphone")
  expect(STATES.includes(status.state)).toBe(true)
})

test("an unknown name rejects with a TypeError", async () => {
  let query = navigator.permissions.query({ name: "teleport" } as unknown as PermissionDescriptor)
  await expect(query).rejects.toThrow(TypeError)
  await expect(navigator.permissions.query({ name: "teleport" } as unknown as PermissionDescriptor)).rejects.toThrow(
    "navigator.permissions.query: unsupported permission name teleport",
  )
})

test("a query without a descriptor rejects with a TypeError", async () => {
  let query = (navigator.permissions.query as unknown as () => Promise<PermissionStatus>)()
  await expect(query).rejects.toThrow(TypeError)
})

test("onchange starts as null and takes a function", async () => {
  let status = await navigator.permissions.query({ name: "camera" })
  expect(status.onchange).toBe(null)
  let listener = () => {}
  status.onchange = listener
  expect(status.onchange).toBe(listener)
  status.onchange = null
  expect(status.onchange).toBe(null)
})
