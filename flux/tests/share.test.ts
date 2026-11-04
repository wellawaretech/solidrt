// navigator.share. The test host records shares instead of showing a
// sheet, so a call resolves at once, and it defines navigator.share and
// lists the capability whatever the machine has.
import { expect, test } from "flux:test"
import { file } from "flux:fs"

test("the test host lists the share capability and defines navigator.share", () => {
  expect(Flux.capabilities.includes("share")).toBe(true)
  expect(typeof navigator.share).toBe("function")
})

test("a share of text, a url and a file resolves", async () => {
  expect(await navigator.share!({ title: "Board", text: "My board", url: "https://example.com/board" })).toBeUndefined()
  expect(await navigator.share!({ files: [file("share/board.png")] })).toBeUndefined()
})

test("a share with nothing to share throws a TypeError", () => {
  expect(() => navigator.share!({})).toThrow(TypeError)
  expect(() => navigator.share!({ files: [] })).toThrow("navigator.share: nothing to share: set title, text, url or files")
  expect(() => navigator.share!(undefined as unknown as ShareData)).toThrow(TypeError)
})

test("files must be flux:fs file handles", () => {
  expect(() => navigator.share!({ files: ["board.png" as unknown as ReturnType<typeof file>] })).toThrow(
    "navigator.share: files must be flux:fs file handles, as file() returns",
  )
})

test("a field of the wrong type throws", () => {
  expect(() => navigator.share!({ text: 42 as unknown as string })).toThrow("navigator.share: text must be a string")
})
