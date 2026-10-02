// file(path): text()/bytes()/json()/exists()/stat()/write()/remove().
// Bodies are read from disk on each call (re-readable, unlike a Response).
// Each test lays its fixture down with file(path).write() in the sandbox,
// then reads it back.
import { expect, test } from "flux:test"
import { file } from "flux:fs"

test("reads text and exposes the path", async () => {
  await file("hello.txt").write("hello world")
  let f = file("hello.txt")
  expect(f.path).toBe("hello.txt")
  expect(await f.text()).toBe("hello world")
})

test("reads bytes as a Uint8Array", async () => {
  await file("bytes.bin").write(new Uint8Array([10, 20, 30]))
  let b = await file("bytes.bin").bytes()
  expect(b instanceof Uint8Array).toBe(true)
  expect(b).toEqual(new Uint8Array([10, 20, 30]))
})

test("reads JSON", async () => {
  await file("data.json").write(JSON.stringify({ n: 7, s: "x" }))
  expect(await file("data.json").json()).toEqual({ n: 7, s: "x" })
})

test("exists is true for a file and false for a missing one", async () => {
  await file("present.txt").write("x")
  expect(await file("present.txt").exists()).toBe(true)
  expect(await file("missing.txt").exists()).toBe(false)
})

test("stat reports the size and the type", async () => {
  await file("sized.txt").write("12345")
  let s = await file("sized.txt").stat()
  expect(s.size).toBe(5)
  expect(s.type).toBe("file")
})

test("text is re-readable", async () => {
  await file("again.txt").write("again")
  let f = file("again.txt")
  // A file body is not consume-once, so a second read succeeds.
  expect(await f.text()).toBe("again")
  expect(await f.text()).toBe("again")
})

test("remove deletes and tolerates a missing file", async () => {
  let f = file("gone.txt")
  await f.write("bye")
  await f.remove()
  expect(await f.exists()).toBe(false)
  await f.remove()
})

test("stat on a missing file rejects", async () => {
  await expect(file("nope.txt").stat()).rejects.toThrow()
})
