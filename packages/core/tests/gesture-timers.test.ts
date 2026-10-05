// The recognizer tests that wait on a timer: createLongPress's hold, slop
// and steal, and createDoubleTap's window passing with no second tap (a
// press beside the double-tap deferring its fire until then; a mock press
// over arena.defer). Synthetic pointer events.
//
// App tests: time passes by the frames a test asks for, so the waits are
// steps and the thresholds are asserted to the millisecond (at 1000 fps a
// frame is one). Everything about the recognizers that takes its time from
// the events alone is in gesture.test.ts, on the flux binary.

import { test } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createRoot } from "@solidjs/signals"
import { arena } from "../src/arena.ts"
import { createLongPress } from "../src/long-press.ts"
import { createDoubleTap } from "../src/double-tap.ts"
import type { PointerEvent } from "../src/types"

// A frame per millisecond: the thresholds below are whole milliseconds.
const MS_FRAMES = { fps: 1000 }

let fail = (msg: string): void => {
  throw new Error(msg)
}

// A synthetic event stamped `at`, the app time it happens at.
let ev = (at: number, pointerId: number, x: number, y: number, button = 0): PointerEvent =>
  ({ timeStamp: at, clientX: x, clientY: y, localX: x, localY: y, parentX: x, parentY: y, movementX: 0, movementY: 0, currentTarget: 1, target: 1, pointerId, pointerType: "touch", button, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, stopPropagation() {} }) as PointerEvent

// Every recognizer registers its cleanup with an owner.
let inRoot = <T>(f: () => T): T => createRoot(dispose => f())

test("createLongPress: timer, slop, steal", async app => {
  let log: string[] = []
  let lp = inRoot(() =>
    createLongPress({
      onLongPress: at => log.push(`press ${at.clientX},${at.clientY}`),
      onLongPressMove: (dx, dy) => log.push(`move ${dx},${dy}`),
      onLongPressEnd: () => log.push("end"),
      onLongPressCancel: () => log.push("cancel"),
    }),
  )
  lp.handlers.onPointerDown(ev(app.time, 30, 50, 60))
  await app.advance(499)
  if (log.length !== 0) fail("a long-press has not fired at 499 ms")
  await app.advance(1)
  if (log.join("|") !== "press 50,60") fail(`a long-press fires at 500 ms with the down's point, got ${log.join("|")}`)
  lp.handlers.onPointerMove(ev(app.time, 30, 58, 60))
  lp.handlers.onPointerMove(ev(app.time, 30, 58, 70))
  lp.handlers.onPointerUp(ev(app.time, 30, 58, 70))
  if (log.join("|") !== "press 50,60|move 8,0|move 0,10|end") fail(`moves stream after the fire, then the lift ends, got ${log.join("|")}`)
  log.length = 0
  // Travel past the slop before the timer disarms it.
  lp.handlers.onPointerDown(ev(app.time, 31, 0, 0))
  await app.advance(100)
  lp.handlers.onPointerMove(ev(app.time, 31, 12, 0))
  await app.advance(550)
  lp.handlers.onPointerUp(ev(app.time, 31, 12, 0))
  if (log.length !== 0) fail(`12 px of travel cancels the long-press, got ${log.join("|")}`)
  // A lift before the timer disarms it.
  lp.handlers.onPointerDown(ev(app.time, 32, 0, 0))
  await app.advance(100)
  lp.handlers.onPointerUp(ev(app.time, 32, 0, 0))
  await app.advance(550)
  if (log.length !== 0) fail("a lift before the timer fires nothing")
  // A press holding the pointer is cancelled by the steal at the timer;
  // a pan that already won keeps the finger.
  let cancelled = 0
  let press = { cancel: () => cancelled++ }
  arena.claim(33, press)
  lp.handlers.onPointerDown(ev(app.time, 33, 0, 0))
  await app.advance(500)
  if (log.join("|") !== "press 0,0" || cancelled !== 1) fail(`the timer's steal cancels the press, got ${log.join("|")} cancelled ${cancelled}`)
  lp.handlers.onPointerUp(ev(app.time, 33, 0, 0))
  log.length = 0
  let pan = { cancel() {} }
  lp.handlers.onPointerDown(ev(app.time, 34, 0, 0))
  arena.steal(34, pan)
  await app.advance(500)
  if (log.length !== 0) fail("a pan that won the finger keeps the long-press out")
  arena.release(34, pan)
  lp.handlers.onPointerUp(ev(app.time, 34, 0, 0))
  // A fired press whose pointer the system cancels ends with
  // onLongPressCancel, never onLongPressEnd; a cancel before the timer
  // disarms it.
  lp.handlers.onPointerDown(ev(app.time, 35, 0, 0))
  await app.advance(500)
  lp.handlers.onPointerCancel(ev(app.time, 35, 0, 0))
  lp.handlers.onPointerUp(ev(app.time, 35, 0, 0))
  if (log.join("|") !== "press 0,0|cancel") fail(`a cancelled fired press ends with onLongPressCancel, got ${log.join("|")}`)
  log.length = 0
  lp.handlers.onPointerDown(ev(app.time, 36, 0, 0))
  await app.advance(100)
  lp.handlers.onPointerCancel(ev(app.time, 36, 0, 0))
  await app.advance(550)
  if (log.length !== 0) fail(`a cancel before the timer fires nothing, got ${log.join("|")}`)
}, MS_FRAMES)

// A double-tap recognizer with a press on the same node, as createPress
// does it: claim at the down, defer the fire at the up. `presses` holds
// the app time each single fired at.
function doubleTapRig(app: TestApp) {
  let taps: string[] = []
  let dt = inRoot(() => createDoubleTap({ onDoubleTap: at => taps.push(`double ${at.clientX}`) }))
  let presses: number[] = []
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
    if (!arena.defer(e.pointerId, () => presses.push(app.time))) presses.push(app.time)
  }
  // A tap held 30 ms.
  let tap = async (id: number, x: number) => {
    dt.handlers.onPointerDown(ev(app.time, id, x, 0))
    pressDown(ev(app.time, id, x, 0))
    await app.advance(30)
    dt.handlers.onPointerUp(ev(app.time, id, x, 0))
    pressUp(ev(app.time, id, x, 0))
  }
  return { dt, taps, presses, tap }
}

test("createDoubleTap: two taps 100 ms apart are one double and no single", async app => {
  let { taps, presses, tap } = doubleTapRig(app)
  await tap(40, 10)
  await app.advance(100)
  await tap(41, 12)
  await app.advance(400)
  if (taps.join("|") !== "double 12") fail(`tap-tap at 100 ms double-taps on the second down, got ${taps.join("|")}`)
  if (presses.length !== 0) fail(`the single tap never fires under a double-tap, got ${presses}`)
}, MS_FRAMES)

test("createDoubleTap: two taps 400 ms apart are two singles, the first late by the window", async app => {
  let { taps, presses, tap } = doubleTapRig(app)
  await tap(42, 10)
  await app.advance(299)
  if (presses.length !== 0) fail(`the first single waits for the window, got ${presses}`)
  await app.advance(101)
  await tap(43, 10)
  await app.advance(400)
  if (taps.length !== 0) fail(`taps 400 ms apart are two singles, got ${taps.join("|")}`)
  // The first up is at 30 ms and the window is 300 ms; the second up is at
  // 460 ms.
  if (presses.join() !== "330,760") fail(`each single fires when its window passes, got ${presses}`)
}, MS_FRAMES)
