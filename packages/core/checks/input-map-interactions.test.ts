// The input map's interactions (hold, tap, doubleTap, chord) over signal
// sources. Parked here, outside tests/, so `srt test` does not run it: the
// interactions keep time with setTimeout and performance.now() together,
// which makes this a test of time passing in SolidRT logic, an app test
// (okf/plans/test-harness.md, D5). It comes back under srt:test, stepped
// by frames. Until then it waits on real timers and runs by hand:
// `srt test packages/core/checks/input-map-interactions.test.ts`.

import { test } from "flux:test"
import { createSignal, flush } from "@solidjs/signals"
import { chord, createInputMap, doubleTap, hold, tap } from "../src/input.ts"
import type { InputSource, Vec2 } from "../src/input.ts"

function fail(msg: string): void {
  throw new Error(msg)
}
let throws = (what: string, f: () => void) => {
  let threw = false
  try {
    f()
  } catch (err) {
    threw = true
    if (!(err instanceof Error)) fail(`${what}: unexpected ${err}`)
  }
  if (!threw) fail(`${what} must throw`)
}

// A value source of one kind with a settable value.
function valued<K extends "axis" | "vec2" | "button">(kind: K, label: string, initial: number | Vec2 | boolean) {
  let value = initial
  let source: InputSource<K> = { kind, label, id: `custom:${label}`, rate: () => value as never }
  return { source, set: (v: number | Vec2 | boolean) => (value = v) }
}

// A value source behind a signal: its edges reach the effects an
// interaction runs.
function signalled<K extends "axis" | "vec2" | "button">(kind: K, label: string, initial: number | Vec2 | boolean) {
  let [value, set] = createSignal(initial, { ownedWrite: true })
  let source: InputSource<K> = { kind, label, id: `custom:${label}`, rate: () => value() as never }
  return { source, set: (v: number | Vec2 | boolean) => set(v) }
}

let tick = () => new Promise<void>(resolve => setTimeout(resolve, 0))
let sleep = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms))

test("interactions: hold, tap, doubleTap, chord", async () => {
  let input = createInputMap({ charge: "button", dash: "button", dodge: "button", both: "button" })
  let a = signalled("button", "a", false)
  let b = signalled("button", "b", false)
  input.bind("charge", hold(a.source, 30))
  input.bind("dash", tap(a.source, 30))
  input.bind("dodge", doubleTap(a.source, 60, 30))
  input.bind("both", chord(a.source, b.source))
  let counts = { charge: 0, dash: 0, dodge: 0, both: 0 }
  for (let name of Object.keys(counts) as (keyof typeof counts)[]) input.onPress(name, () => counts[name]++)
  // A short press: a tap, not a hold.
  a.set(true)
  flush()
  a.set(false)
  flush()
  await tick()
  if (counts.dash !== 1 || counts.charge !== 0) fail(`a short press taps (${counts.dash}) and does not hold (${counts.charge})`)
  if (input.pressed("dash")) fail("a tap releases on its own")
  // A long press: a hold, not a tap.
  a.set(true)
  flush()
  await sleep(50)
  if (!input.pressed("charge")) fail("held past the time, hold reads pressed")
  a.set(false)
  flush()
  await tick()
  if (input.pressed("charge") || counts.dash !== 1) fail("release ends the hold and a long press is not a tap")
  // Two quick taps: a double tap, and two taps. The gap since the
  // taps above is let pass first, so the pair is the one counted.
  await sleep(80)
  for (let i = 0; i < 2; i++) {
    a.set(true)
    flush()
    a.set(false)
    flush()
    await tick()
  }
  if (counts.dodge !== 1) fail(`two quick taps double-tap once, got ${counts.dodge}`)
  if (counts.dash !== 3) fail(`each tap counts, got ${counts.dash}`)
  // Two slow taps: no double tap.
  await sleep(80)
  a.set(true)
  flush()
  a.set(false)
  flush()
  await sleep(80)
  a.set(true)
  flush()
  a.set(false)
  flush()
  await tick()
  if (counts.dodge !== 1) fail("taps too far apart do not double-tap")
  // The chord: both, not either.
  a.set(true)
  flush()
  if (input.pressed("both")) fail("a chord needs every source")
  b.set(true)
  flush()
  if (!input.pressed("both") || counts.both !== 1) fail("a chord presses when the last source lands")
  a.set(false)
  flush()
  if (input.pressed("both")) fail("a chord releases when any source lifts")
  throws("hold an axis", () => hold(valued("axis", "z", 0).source as never))
  throws("hold a negative time", () => hold(a.source, -1))
  throws("chord of one", () => chord(a.source))
})
