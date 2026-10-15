// file(path).write(data) where data is a string or Uint8Array, read back as
// bytes to keep the focus on write itself.
import { expect, test } from "flux:test"
import { dir, file } from "flux:fs"

test("writes a string", async () => {
  await file("s.txt").write("content here")
  expect(await file("s.txt").bytes()).toEqual(new TextEncoder().encode("content here"))
})

test("writes a Uint8Array", async () => {
  await file("bytes.bin").write(new Uint8Array([1, 2, 3, 4]))
  expect(await file("bytes.bin").bytes()).toEqual(new Uint8Array([1, 2, 3, 4]))
})

test("overwrites an existing file", async () => {
  await file("over.txt").write("old contents")
  await file("over.txt").write("new")
  expect(await file("over.txt").bytes()).toEqual(new TextEncoder().encode("new"))
})

test("an atomic write replaces the file whole and leaves no temporary behind", async () => {
  await file("doc.txt").write("old contents")
  await file("doc.txt").write("new", { atomic: true })
  expect(await file("doc.txt").text()).toBe("new")
  await file("fresh.txt").write(new Uint8Array([7, 8]), { atomic: true })
  expect(await file("fresh.txt").bytes()).toEqual(new Uint8Array([7, 8]))
  let names = (await dir(".").entries()).map(e => e.name)
  expect(names.filter(n => n.endsWith(".tmp"))).toEqual([])
})

test("an atomic write into a missing directory rejects and leaves nothing", async () => {
  await expect(file("no-such-dir/doc.txt").write("x", { atomic: true })).rejects.toThrow()
  expect(await file("no-such-dir/doc.txt").exists()).toBe(false)
})

test("rejects an invalid data type", async () => {
  // @ts-expect-error a number is no data; write has to say so at runtime too
  expect(() => file("never.txt").write(123)).toThrow("data must be string or Uint8Array")
  expect(await file("never.txt").exists()).toBe(false)
})
