// The input map's hold interaction over a signal source. An app test:
// `hold` keeps time with setTimeout, and time passes by the frames the test
// asks for. The interactions that take their time from their source (tap,
// doubleTap, chord) are in input-map.test.ts, on the flux binary.

import { test } from "../src/test.ts"
import { createSignal, flush } from "@solidjs/signals"
import { createInputMap, hold } from "../src/input.ts"
import type { InputSource } from "../src/input.ts"

// The hold time under test, and one frame short of it and one past it at
// the 1000 fps the test steps at.
const HOLD_MS = 30

function fail(msg: string): void {
  throw new Error(msg)
}

// A button source behind a signal: its edges reach the effect the
// interaction runs.
function signalled(label: string) {
  let [value, set] = createSignal(false, { ownedWrite: true })
  let source: InputSource<"button"> = { kind: "button", label, id: `custom:${label}`, rate: value }
  return { source, set: (v: boolean) => set(v) }
}

test("interactions: hold", async app => {
  let input = createInputMap({ charge: "button" })
  let a = signalled("a")
  input.bind("charge", hold(a.source, HOLD_MS))
  let charges = 0
  input.onPress("charge", () => charges++)
  // A short press: not a hold.
  a.set(true)
  flush()
  await app.advance(HOLD_MS - 1)
  a.set(false)
  flush()
  await app.advance(HOLD_MS)
  if (charges !== 0 || input.pressed("charge")) fail("a press released before the time does not hold")
  // A long press: a hold from the time on, until the release.
  a.set(true)
  flush()
  await app.advance(HOLD_MS - 1)
  if (input.pressed("charge") || charges !== 0) fail("one frame short of the time, hold does not read pressed")
  await app.advance(1)
  if (!input.pressed("charge") || charges !== 1) fail("at the time, hold reads pressed")
  a.set(false)
  flush()
  await app.frame()
  if (input.pressed("charge")) fail("release ends the hold")
}, { fps: 1000 })
