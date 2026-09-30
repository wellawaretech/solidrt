// The recognizer tests that wait on a timer: createLongPress's hold, slop
// and steal, and createDoubleTap's window passing with no second tap (a
// press beside the double-tap deferring its fire until then; a mock press
// over arena.defer). Synthetic pointer events.
//
// Parked here, outside tests/, so `srt test` does not run it: these are
// tests of time passing in SolidRT logic, which makes them app tests
// (okf/plans/test-harness.md, D5). They come back under srt:test, stepped
// by frames. Until then they wait on real timers, assert with tolerances
// and run by hand: `srt test packages/core/checks/gesture-timers.test.ts`.
// Everything about the recognizers that takes its time from the events is
// in tests/gesture.test.ts.

import { test } from "flux:test"
import { createRoot } from "@solidjs/signals"
import { arena } from "../src/arena.ts"
import { createLongPress } from "../src/long-press.ts"
import { createDoubleTap } from "../src/double-tap.ts"
import type { PointerEvent } from "../src/types"

let fail = (msg: string): void => {
  throw new Error(msg)
}
let sleep = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms))

// A synthetic event, stamped with the wall: the waits here are real, so
// the stamps a recognizer compares have to pass as the timers do.
let ev = (pointerId: number, x: number, y: number, button = 0): PointerEvent =>
  ({ timeStamp: performance.now(), clientX: x, clientY: y, localX: x, localY: y, parentX: x, parentY: y, movementX: 0, movementY: 0, currentTarget: 1, target: 1, pointerId, pointerType: "touch", button, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, stopPropagation() {} }) as PointerEvent

// Every recognizer registers its cleanup with an owner.
let inRoot = <T>(f: () => T): T => createRoot(dispose => f())

test("createLongPress: timer, slop, steal", async () => {
  let log: string[] = []
  let lp = inRoot(() =>
    createLongPress({
      onLongPress: at => log.push(`press ${at.clientX},${at.clientY}`),
      onLongPressMove: (dx, dy) => log.push(`move ${dx},${dy}`),
      onLongPressEnd: () => log.push("end"),
    }),
  )
  lp.handlers.onPointerDown(ev(30, 50, 60))
  await sleep(400)
  if (log.length !== 0) fail("a long-press has not fired at 400 ms")
  await sleep(200)
  if (log.join("|") !== "press 50,60") fail(`a long-press fires by 600 ms with the down's point, got ${log.join("|")}`)
  lp.handlers.onPointerMove(ev(30, 58, 60))
  lp.handlers.onPointerMove(ev(30, 58, 70))
  lp.handlers.onPointerUp(ev(30, 58, 70))
  if (log.join("|") !== "press 50,60|move 8,0|move 0,10|end") fail(`moves stream after the fire, then the lift ends, got ${log.join("|")}`)
  log.length = 0
  // Travel past the slop before the timer disarms it.
  lp.handlers.onPointerDown(ev(31, 0, 0))
  await sleep(100)
  lp.handlers.onPointerMove(ev(31, 12, 0))
  await sleep(550)
  lp.handlers.onPointerUp(ev(31, 12, 0))
  if (log.length !== 0) fail(`12 px of travel cancels the long-press, got ${log.join("|")}`)
  // A lift before the timer disarms it.
  lp.handlers.onPointerDown(ev(32, 0, 0))
  await sleep(100)
  lp.handlers.onPointerUp(ev(32, 0, 0))
  await sleep(550)
  if (log.length !== 0) fail("a lift before the timer fires nothing")
  // A press holding the pointer is cancelled by the steal at the timer;
  // a pan that already won keeps the finger.
  let cancelled = 0
  let press = { cancel: () => cancelled++ }
  arena.claim(33, press)
  lp.handlers.onPointerDown(ev(33, 0, 0))
  await sleep(600)
  if (log.join("|") !== "press 0,0" || cancelled !== 1) fail(`the timer's steal cancels the press, got ${log.join("|")} cancelled ${cancelled}`)
  lp.handlers.onPointerUp(ev(33, 0, 0))
  log.length = 0
  let pan = { cancel() {} }
  lp.handlers.onPointerDown(ev(34, 0, 0))
  arena.steal(34, pan)
  await sleep(600)
  if (log.length !== 0) fail("a pan that won the finger keeps the long-press out")
  arena.release(34, pan)
  lp.handlers.onPointerUp(ev(34, 0, 0))
})

// A double-tap recognizer with a press on the same node, as createPress
// does it: claim at the down, defer the fire at the up. `presses` holds
// when each single fired, in ms since `restart()`.
function doubleTapRig() {
  let taps: string[] = []
  let dt = inRoot(() => createDoubleTap({ onDoubleTap: at => taps.push(`double ${at.clientX}`) }))
  let presses: number[] = []
  let t0 = performance.now()
  let active: number | null = null
  let press = {
    cancel() {
      active = null
    },
  }
  let pressDown = (e: PointerEvent) => {
    if (arena.claim(e.pointerId, press)) active = e.pointerId
  }
  let pressUp = (e: PointerEvent) => {
    if (active !== e.pointerId) return
    active = null
    arena.release(e.pointerId, press)
    if (!arena.defer(e.pointerId, () => presses.push(performance.now() - t0))) presses.push(performance.now() - t0)
  }
  let tap = async (id: number, x: number) => {
    dt.handlers.onPointerDown(ev(id, x, 0))
    pressDown(ev(id, x, 0))
    await sleep(30)
    dt.handlers.onPointerUp(ev(id, x, 0))
    pressUp(ev(id, x, 0))
  }
  let restart = () => {
    t0 = performance.now()
  }
  return { dt, taps, presses, tap, restart }
}

test("createDoubleTap: two taps 100 ms apart are one double and no single", async () => {
  let { taps, presses, tap } = doubleTapRig()
  await tap(40, 10)
  await sleep(100)
  await tap(41, 12)
  await sleep(400)
  if (taps.join("|") !== "double 12") fail(`tap-tap at 100 ms double-taps on the second down, got ${taps.join("|")}`)
  if (presses.length !== 0) fail(`the single tap never fires under a double-tap, got ${presses}`)
})

test("createDoubleTap: two taps 400 ms apart are two singles, the first late by the window", async () => {
  let { taps, presses, tap, restart } = doubleTapRig()
  restart()
  await tap(42, 10)
  await sleep(400)
  let firstAt = presses[0]
  await tap(43, 10)
  await sleep(400)
  if (taps.length !== 0) fail(`taps 400 ms apart are two singles, got ${taps.join("|")}`)
  if (presses.length !== 2) fail(`both singles fire, got ${presses}`)
  if (!(firstAt !== undefined && firstAt > 300 && firstAt < 420)) fail(`the first single fires when the window passes (~330 ms), got ${firstAt}`)
})
