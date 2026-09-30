// The input map's hold interaction over a signal source. Parked here,
// outside tests/, so `srt test` does not run it: `hold` keeps time with
// setTimeout, which makes this a test of time passing in SolidRT logic, an
// app test (okf/plans/test-harness.md, D5). It comes back under srt:test,
// stepped by frames. Until then it waits on real timers and runs by hand:
// `srt test packages/core/checks/input-map-hold.test.ts`. The
// interactions that take their time from their source (tap, doubleTap,
// chord) are in tests/input-map.test.ts.

import { test } from "flux:test"
import { createSignal, flush } from "@solidjs/signals"
import { createInputMap, hold } from "../src/input.ts"
import type { InputSource } from "../src/input.ts"

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

let tick = () => new Promise<void>(resolve => setTimeout(resolve, 0))
let sleep = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms))

test("interactions: hold", async () => {
  let input = createInputMap({ charge: "button" })
  let a = signalled("a")
  input.bind("charge", hold(a.source, 30))
  let charges = 0
  input.onPress("charge", () => charges++)
  // A short press: not a hold.
  a.set(true)
  flush()
  a.set(false)
  flush()
  await tick()
  if (charges !== 0 || input.pressed("charge")) fail("a short press does not hold")
  // A long press: a hold, until the release.
  a.set(true)
  flush()
  await sleep(50)
  if (!input.pressed("charge") || charges !== 1) fail("held past the time, hold reads pressed")
  a.set(false)
  flush()
  await tick()
  if (input.pressed("charge")) fail("release ends the hold")
})
