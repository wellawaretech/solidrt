// The discrete recognizers and the arena relation: the arena's
// pend/decide/defer, createPan's lift velocity, createSwipe's classifier
// and axis rule, and createDoubleTap's window, slop, bounce and steal.
// Synthetic pointer events. The pan, swipe and double-tap recognizers
// import no runtime module (createTransform does: the pointerFrame
// terminator), so this runs on the bare flux binary: `srt test
// packages/core`.
//
// Time is an input here: the recognizers read the timeStamp of the events
// they are handed and no clock, so every test states its times and none
// waits. What does wait on a timer (the long-press hold, the double-tap
// window passing with no second tap) is about time passing and is parked
// in checks/gesture-timers.test.ts for the app layer
// (okf/plans/test-harness.md, D5).

import { test } from "flux:test"
import { createRoot } from "@solidjs/signals"
import { arena } from "../src/arena.ts"
import { createPan } from "../src/pan.ts"
import { classifySwipe, createSwipe } from "../src/swipe.ts"
import { createDoubleTap } from "../src/double-tap.ts"
import type { PointerEvent } from "../src/types"
import type { Velocity } from "../src/velocity.ts"

let fail = (msg: string): void => {
  throw new Error(msg)
}

// A synthetic event at time `at` (ms): every frame the same, the pointer
// at (x, y).
let ev = (pointerId: number, x: number, y: number, at: number, button = 0): PointerEvent =>
  ({ timeStamp: at, predicted: false, clientX: x, clientY: y, localX: x, localY: y, parentX: x, parentY: y, movementX: 0, movementY: 0, currentTarget: 1, target: 1, pointerId, pointerType: "touch", button, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, stopPropagation() {} }) as PointerEvent

type Handlers = { onPointerDown(e: PointerEvent): void; onPointerMove(e: PointerEvent): void; onPointerUp(e: PointerEvent): void }

// A drag starting at time `at`: down at (x, y), `steps` moves of (dx, dy)
// every `every` ms, then the up `pause` ms after the last move.
function drag(h: Handlers, id: number, at: number, x: number, y: number, dx: number, dy: number, steps: number, every: number, pause = 0) {
  h.onPointerDown(ev(id, x, y, at))
  for (let i = 0; i < steps; i++) {
    at += every
    x += dx
    y += dy
    h.onPointerMove(ev(id, x, y, at))
  }
  h.onPointerUp(ev(id, x, y, at + pause))
}

// Every recognizer registers its cleanup with an owner.
let inRoot = <T>(f: () => T): T => createRoot(dispose => f())

test("the arena relation: pend, defer, decide", () => {
  let a = { cancel() {} }
  let b = { cancel() {} }
  let fired: string[] = []
  if (arena.defer(1, () => fired.push("x"))) fail("defer with nothing pending stores nothing")
  arena.pend(1, a)
  if (!arena.defer(1, () => fired.push("a1"))) fail("defer with a pending decision stores the firing")
  arena.decide(1, b, false)
  if (fired.length !== 0) fail("a decision by a non-pending owner changes nothing")
  arena.decide(1, a, false)
  if (fired.join() !== "a1") fail(`the last pending owner losing fires, got ${fired}`)
  if (arena.defer(1, () => fired.push("late"))) fail("nothing pending after the decision")
  arena.pend(2, a)
  arena.defer(2, () => fired.push("a2"))
  arena.decide(2, a, true)
  if (fired.join() !== "a1") fail("a win drops the deferred firing")
  arena.pend(3, a)
  arena.pend(3, b)
  arena.defer(3, () => fired.push("a3"))
  arena.decide(3, a, false)
  if (fired.join() !== "a1") fail("one of two pending owners losing keeps the firing waiting")
  arena.decide(3, b, false)
  if (fired.join() !== "a1,a3") fail("the second owner losing fires")
})

test("createPan: the lift velocity", () => {
  let ends: Velocity[] = []
  let pan = inRoot(() => createPan({ onPanEnd: v => ends.push(v) }))
  // 10 px every 16 ms: 625 px/s rightward. The first move only activates
  // the pan, so the fit has an even run to read.
  drag(pan.handlers, 10, 1000, 0, 0, 10, 0, 12, 16)
  let v = ends[0]
  if (!v || Math.abs(v.vx - 625) > 1e-6 || v.vy !== 0) fail(`a 625 px/s drag lifts at that, got ${JSON.stringify(v)}`)
  // A finger that rested 80 ms before lifting: zero.
  drag(pan.handlers, 11, 2000, 0, 0, 10, 0, 12, 16, 80)
  v = ends[1]
  if (!v || v.vx !== 0 || v.vy !== 0) fail(`a rested lift reads zero, got ${JSON.stringify(v)}`)
  // A crawl under the fling minimum: zero.
  drag(pan.handlers, 12, 3000, 0, 0, 0.4, 0, 30, 16)
  v = ends[2]
  if (!v || v.vx !== 0) fail(`a 25 px/s crawl is not a fling, got ${JSON.stringify(v)}`)
})

test("createPan: a predicted move moves the pan and stays out of its velocity", () => {
  let ends: Velocity[] = []
  let moved = 0
  let pan = inRoot(() => createPan({ onPanMove: dx => (moved += dx), onPanEnd: v => ends.push(v) }))
  // 10 px every 16 ms, then the runtime bridges a late delivery with a
  // predicted step, the finger turns out to have stopped, and its real
  // position follows with the time it got there.
  pan.handlers.onPointerDown(ev(20, 0, 0, 1000))
  for (let i = 1; i <= 6; i++) pan.handlers.onPointerMove(ev(20, i * 10, 0, 1000 + i * 16))
  pan.handlers.onPointerMove({ ...ev(20, 70, 0, 1000 + 7 * 16), predicted: true })
  if (moved !== 60) fail(`the pan follows the predicted step, moved ${moved}`)
  pan.handlers.onPointerMove(ev(20, 60, 0, 1000 + 6 * 16))
  if (moved !== 50) fail(`the pan settles on the real position, moved ${moved}`)
  // A lift 60 ms after the finger got there is a rest, though the predicted
  // move is only 44 ms old.
  pan.handlers.onPointerUp(ev(20, 60, 0, 1000 + 6 * 16 + 60))
  let v = ends[0]
  if (!v || v.vx !== 0 || v.vy !== 0) fail(`the rest is read from the real position, got ${JSON.stringify(v)}`)
})

test("classifySwipe", () => {
  let far = { dx: 100, dy: 0 }
  if (classifySwipe({ vx: 500, vy: 0 }, far) !== "Right") fail("a fast rightward lift swipes Right")
  if (classifySwipe({ vx: -500, vy: 100 }, far) !== "Left") fail("11 degrees off the axis still swipes")
  if (classifySwipe({ vx: 400, vy: 400 }, far) !== null) fail("a diagonal lift is no swipe")
  if (classifySwipe({ vx: 0, vy: -500 }, { dx: 0, dy: -30 }) !== "Up") fail("an upward lift swipes Up")
  if (classifySwipe({ vx: 500, vy: 0 }, { dx: 10, dy: 0 }) !== null) fail("too little travel is no swipe")
  if (classifySwipe({ vx: 200, vy: 0 }, far) !== null) fail("too slow is no swipe")
  if (classifySwipe({ vx: 500, vy: 0 }, far, ["Left"]) !== null) fail("a direction not allowed is no swipe")
  if (classifySwipe({ vx: 100, vy: 600 }, { dx: 100, dy: 0 }) !== "Down") fail("direction follows the velocity, not the travel")
})

test("createSwipe: fast, slow, off-axis, axis rule", () => {
  let log: string[] = []
  let swipe = inRoot(() =>
    createSwipe({
      directions: ["Left", "Right"],
      onSwipeStart: () => log.push("start"),
      onSwipe: (d, v) => log.push(`swipe ${d} ${v.vx > 0 ? "+" : "-"}`),
      onSwipeEnd: () => log.push("end"),
    }),
  )
  drag(swipe.handlers, 20, 1000, 100, 100, -12, 0, 10, 16)
  if (log.join("|") !== "start|swipe Left -|end") fail(`a fast leftward drag swipes Left then ends, got ${log.join("|")}`)
  log.length = 0
  // Slow: the same 120 px over a second.
  drag(swipe.handlers, 21, 2000, 100, 100, -12, 0, 10, 100)
  if (log.join("|") !== "start|end") fail(`a slow drag ends without a swipe, got ${log.join("|")}`)
  log.length = 0
  // Vertical travel never activates a horizontal-only swipe.
  drag(swipe.handlers, 22, 4000, 100, 100, 0, -12, 10, 16)
  if (log.length !== 0) fail(`a vertical drag never starts a horizontal swipe, got ${log.join("|")}`)
  // Diagonal: activates (both axes past slop) but classifies as nothing.
  drag(swipe.handlers, 23, 5000, 100, 100, -10, 10, 10, 16)
  if (log.join("|") !== "start|end") fail(`a diagonal drag ends without a swipe, got ${log.join("|")}`)
})

// A double-tap recognizer, and a tap through it: down at `at`, up 30 ms
// later.
function doubleTapRig() {
  let taps: string[] = []
  let dt = inRoot(() => createDoubleTap({ onDoubleTap: at => taps.push(`double ${at.clientX}`) }))
  let tap = (id: number, x: number, at: number) => {
    dt.handlers.onPointerDown(ev(id, x, 0, at))
    dt.handlers.onPointerUp(ev(id, x, 0, at + 30))
  }
  return { dt, taps, tap }
}

test("createDoubleTap: a second down 100 ms after the first up double-taps, on the down", () => {
  let { dt, taps, tap } = doubleTapRig()
  tap(40, 10, 1000)
  dt.handlers.onPointerDown(ev(41, 12, 0, 1130))
  if (taps.join("|") !== "double 12") fail(`tap-tap at 100 ms double-taps on the second down, got ${taps.join("|")}`)
  dt.handlers.onPointerUp(ev(41, 12, 0, 1160))
})

test("createDoubleTap: the window's edges, to the millisecond", () => {
  // The first up is at 1030; the second down counts from 40 ms to 300 ms
  // after it.
  for (let [gap, doubles] of [[39, false], [40, true], [300, true], [301, false]] as const) {
    let { dt, taps, tap } = doubleTapRig()
    tap(42, 10, 1000)
    dt.handlers.onPointerDown(ev(43, 10, 0, 1030 + gap))
    if ((taps.length === 1) !== doubles) fail(`a second down ${gap} ms after the first up ${doubles ? "double-taps" : "does not double-tap"}, got ${taps.join("|")}`)
    dt.handlers.onPointerUp(ev(43, 10, 0, 1040 + gap))
    dt.cancel()
  }
})

test("createDoubleTap: a bounce 15 ms after the first up is not a second tap", () => {
  let { dt, taps, tap } = doubleTapRig()
  tap(44, 10, 1000)
  dt.handlers.onPointerDown(ev(45, 10, 0, 1045))
  dt.handlers.onPointerUp(ev(45, 10, 0, 1046))
  if (taps.length !== 0) fail("a bounce is not a double-tap")
  dt.cancel()
})

test("createDoubleTap: a second down 150 px away is a fresh first tap", () => {
  let { dt, taps, tap } = doubleTapRig()
  tap(46, 0, 1000)
  dt.handlers.onPointerDown(ev(47, 150, 0, 1130))
  dt.handlers.onPointerUp(ev(47, 150, 0, 1160))
  if (taps.length !== 0) fail("a second tap 150 px away is not a double-tap")
  dt.cancel()
})

test("createDoubleTap: a hold longer than the tap maximum is not a first tap", () => {
  let { dt, taps } = doubleTapRig()
  dt.handlers.onPointerDown(ev(48, 0, 0, 1000))
  dt.handlers.onPointerUp(ev(48, 0, 0, 1501))
  dt.handlers.onPointerDown(ev(49, 0, 0, 1551))
  dt.handlers.onPointerUp(ev(49, 0, 0, 1552))
  if (taps.length !== 0) fail("a long hold then a tap is not a double-tap")
  dt.cancel()
})

test("createDoubleTap: the second tap's steal cancels a press claimed on its down", () => {
  let { dt, taps, tap } = doubleTapRig()
  let cancelled = 0
  let inner = { cancel: () => cancelled++ }
  tap(50, 0, 1000)
  arena.claim(51, inner)
  dt.handlers.onPointerDown(ev(51, 0, 0, 1130))
  if (cancelled !== 1 || taps.join("|") !== "double 0") fail(`the second down steals from the press, got cancelled ${cancelled} taps ${taps.join("|")}`)
  if (arena.claim(51, inner)) fail("the double-tap holds the second pointer until its lift")
  dt.handlers.onPointerUp(ev(51, 0, 0, 1160))
  if (!arena.claim(51, inner)) fail("the lift releases the second pointer")
  arena.release(51, inner)
})
