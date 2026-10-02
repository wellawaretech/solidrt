// Line delivery needs a real stdin, which belongs to the test runner; that
// path is checked by hand with the flux binary and a pipe. These cover the
// surface and the validation.
import { expect, test } from "flux:test"
import { isTTY, on, once, setRawMode, write } from "flux:tty"

test("exposes the terminal surface", () => {
  expect([typeof isTTY, typeof on, typeof once, typeof setRawMode, typeof write]).toEqual([
    "boolean",
    "function",
    "function",
    "function",
    "function",
  ])
})

test("rejects an unknown event", () => {
  // @ts-expect-error not an event; on has to say so at runtime too
  expect(() => on("keypress", () => {})).toThrow(/^Unknown tty event: keypress \(expected "line", "key" or "close"\)$/)
})
