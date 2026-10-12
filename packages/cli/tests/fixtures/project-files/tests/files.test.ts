// The project's files under sol test, from a flux test: a binary or text
// import is inlined as the app bundle inlines it, `assets/...` reads the
// project's assets folder (read-only) while the working directory is the
// test's sandbox; `sol test packages/cli` runs it with the rest.

import { test, expect } from "flux:test"
import { file } from "flux:fs"
import bytes from "../assets/bytes.bin" with { type: "binary" }
import greeting from "../assets/greeting.txt" with { type: "text" }

test("a binary import is the file's bytes", () => {
  expect(bytes).toBeInstanceOf(Uint8Array)
  expect([...bytes]).toEqual([0, 1, 254, 255])
})

test("a text import is the file's text", () => {
  expect(greeting).toBe("hello from assets\n")
})

test("assets/ reads the project's assets folder", async () => {
  expect(await file("assets/greeting.txt").exists()).toBe(true)
  expect(await file("assets/greeting.txt").text()).toBe("hello from assets\n")
})

test("assets are read-only", async () => {
  await expect(file("assets/scratch.txt").write("x")).rejects.toThrow("read-only")
})

test("the working directory is an empty sandbox", async () => {
  expect(await file("package.json").exists()).toBe(false)
  await file("note.txt").write("x")
  expect(await file("note.txt").exists()).toBe(true)
})

test("the sandbox is emptied before every test", async () => {
  expect(await file("note.txt").exists()).toBe(false)
})
