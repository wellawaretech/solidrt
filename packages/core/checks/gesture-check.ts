// Checks for the discrete recognizers and the arena relation: the arena's
// pend/decide/defer, createPan's lift velocity, createSwipe's classifier
// and axis rule, createLongPress's timer, slop and steal, createDoubleTap's
// window, slop and bounce, and a press beside a double-tap deferring its
// fire (a mock press over arena.defer). Synthetic pointer events over real
// timers (the recognizers keep time with setTimeout and performance.now()),
// so the timings are asserted with tolerances. The pan, swipe, long-press
// and double-tap recognizers import no runtime module (createTransform
// does: the pointerFrame terminator), so this runs headless on flux,
// bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/core/checks/gesture-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, so a CI step can
// gate on the exit code.

import { createRoot } from "@solidjs/signals"
import { arena } from "../src/arena.ts"
import { createPan } from "../src/pan.ts"
import { classifySwipe, createSwipe } from "../src/swipe.ts"
import { createLongPress } from "../src/long-press.ts"
import { createDoubleTap } from "../src/double-tap.ts"
import type { PointerEvent } from "../src/types"
import type { Velocity } from "../src/velocity.ts"

let failures = 0
let fail = (msg: string) => {
  failures++
  console.log(`FAIL ${msg}`)
}
let sleep = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms))

// A synthetic event: every frame the same, the pointer at (x, y).
let ev = (pointerId: number, x: number, y: number, button = 0): PointerEvent =>
  ({ clientX: x, clientY: y, localX: x, localY: y, parentX: x, parentY: y, movementX: 0, movementY: 0, currentTarget: 1, target: 1, pointerId, pointerType: "touch", button, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, stopPropagation() {} }) as PointerEvent

type Handlers = { onPointerDown(e: PointerEvent): void; onPointerMove(e: PointerEvent): void; onPointerUp(e: PointerEvent): void }

// A drag: down at (x, y), `steps` moves of (dx, dy) every `every` ms, then
// the up `pause` ms after the last move.
async function drag(h: Handlers, id: number, x: number, y: number, dx: number, dy: number, steps: number, every: number, pause = 0) {
  h.onPointerDown(ev(id, x, y))
  for (let i = 0; i < steps; i++) {
    await sleep(every)
    x += dx
    y += dy
    h.onPointerMove(ev(id, x, y))
  }
  if (pause > 0) await sleep(pause)
  h.onPointerUp(ev(id, x, y))
}

// Every recognizer registers its cleanup with an owner.
let inRoot = <T>(f: () => T): T => createRoot(dispose => f())

async function checks() {
  // ---- The arena relation: pend, defer, decide ----
  {
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
  }

  // ---- createPan: the lift velocity ----
  {
    let ends: Velocity[] = []
    let pan = inRoot(() => createPan({ onPanEnd: v => ends.push(v) }))
    // 10 px every 16 ms: 625 px/s rightward.
    await drag(pan.handlers, 10, 0, 0, 10, 0, 12, 16)
    let v = ends[0]
    if (!v || !(v.vx > 450 && v.vx < 800) || Math.abs(v.vy) > 50) fail(`a 625 px/s drag lifts near that, got ${JSON.stringify(v)}`)
    // A finger that rested 80 ms before lifting: zero.
    await drag(pan.handlers, 11, 0, 0, 10, 0, 12, 16, 80)
    v = ends[1]
    if (!v || v.vx !== 0 || v.vy !== 0) fail(`a rested lift reads zero, got ${JSON.stringify(v)}`)
    // A crawl under the fling minimum: zero.
    await drag(pan.handlers, 12, 0, 0, 0.4, 0, 30, 16)
    v = ends[2]
    if (!v || v.vx !== 0) fail(`a 25 px/s crawl is not a fling, got ${JSON.stringify(v)}`)
  }

  // ---- classifySwipe ----
  {
    let far = { dx: 100, dy: 0 }
    if (classifySwipe({ vx: 500, vy: 0 }, far) !== "Right") fail("a fast rightward lift swipes Right")
    if (classifySwipe({ vx: -500, vy: 100 }, far) !== "Left") fail("11 degrees off the axis still swipes")
    if (classifySwipe({ vx: 400, vy: 400 }, far) !== null) fail("a diagonal lift is no swipe")
    if (classifySwipe({ vx: 0, vy: -500 }, { dx: 0, dy: -30 }) !== "Up") fail("an upward lift swipes Up")
    if (classifySwipe({ vx: 500, vy: 0 }, { dx: 10, dy: 0 }) !== null) fail("too little travel is no swipe")
    if (classifySwipe({ vx: 200, vy: 0 }, far) !== null) fail("too slow is no swipe")
    if (classifySwipe({ vx: 500, vy: 0 }, far, ["Left"]) !== null) fail("a direction not allowed is no swipe")
    if (classifySwipe({ vx: 100, vy: 600 }, { dx: 100, dy: 0 }) !== "Down") fail("direction follows the velocity, not the travel")
  }

  // ---- createSwipe: fast, slow, off-axis, axis rule ----
  {
    let log: string[] = []
    let swipe = inRoot(() =>
      createSwipe({
        directions: ["Left", "Right"],
        onSwipeStart: () => log.push("start"),
        onSwipe: (d, v) => log.push(`swipe ${d} ${v.vx > 0 ? "+" : "-"}`),
        onSwipeEnd: () => log.push("end"),
      }),
    )
    await drag(swipe.handlers, 20, 100, 100, -12, 0, 10, 16)
    if (log.join("|") !== "start|swipe Left -|end") fail(`a fast leftward drag swipes Left then ends, got ${log.join("|")}`)
    log.length = 0
    // Slow: the same 120 px over a second.
    await drag(swipe.handlers, 21, 100, 100, -12, 0, 10, 100)
    if (log.join("|") !== "start|end") fail(`a slow drag ends without a swipe, got ${log.join("|")}`)
    log.length = 0
    // Vertical travel never activates a horizontal-only swipe.
    await drag(swipe.handlers, 22, 100, 100, 0, -12, 10, 16)
    if (log.length !== 0) fail(`a vertical drag never starts a horizontal swipe, got ${log.join("|")}`)
    // Diagonal: activates (both axes past slop) but classifies as nothing.
    await drag(swipe.handlers, 23, 100, 100, -10, 10, 10, 16)
    if (log.join("|") !== "start|end") fail(`a diagonal drag ends without a swipe, got ${log.join("|")}`)
  }

  // ---- createLongPress: timer, slop, steal ----
  {
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
  }

  // ---- createDoubleTap: window, slop, bounce; the deferred press ----
  {
    let taps: string[] = []
    let dt = inRoot(() => createDoubleTap({ onDoubleTap: at => taps.push(`double ${at.clientX}`) }))
    // A press on the same node, as createPress does it: claim at the
    // down, defer the fire at the up.
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
    // Two taps 100 ms apart: one double, no single.
    await tap(40, 10)
    await sleep(100)
    await tap(41, 12)
    await sleep(400)
    if (taps.join("|") !== "double 12") fail(`tap-tap at 100 ms double-taps on the second down, got ${taps.join("|")}`)
    if (presses.length !== 0) fail(`the single tap never fires under a double-tap, got ${presses}`)
    // Two taps 400 ms apart: two singles, the first late by the window.
    taps.length = 0
    t0 = performance.now()
    await tap(42, 10)
    await sleep(400)
    let firstAt = presses[0]
    await tap(43, 10)
    await sleep(400)
    if (taps.length !== 0) fail(`taps 400 ms apart are two singles, got ${taps.join("|")}`)
    if (presses.length !== 2) fail(`both singles fire, got ${presses}`)
    if (!(firstAt !== undefined && firstAt > 300 && firstAt < 420)) fail(`the first single fires when the window passes (~330 ms), got ${firstAt}`)
    // A bounce 20 ms after the first up is not a second tap.
    taps.length = 0
    presses.length = 0
    await tap(44, 10)
    await sleep(15)
    dt.handlers.onPointerDown(ev(45, 10, 0))
    dt.handlers.onPointerUp(ev(45, 10, 0))
    await sleep(400)
    if (taps.length !== 0) fail("a bounce is not a double-tap")
    // A second down 150 px away is a fresh first tap.
    await tap(46, 0)
    await sleep(100)
    dt.handlers.onPointerDown(ev(47, 150, 0))
    dt.handlers.onPointerUp(ev(47, 150, 0))
    await sleep(400)
    if (taps.length !== 0) fail("a second tap 150 px away is not a double-tap")
    // A hold longer than the tap maximum is not a first tap.
    dt.handlers.onPointerDown(ev(48, 0, 0))
    await sleep(600)
    dt.handlers.onPointerUp(ev(48, 0, 0))
    await sleep(50)
    dt.handlers.onPointerDown(ev(49, 0, 0))
    dt.handlers.onPointerUp(ev(49, 0, 0))
    await sleep(400)
    if (taps.length !== 0) fail("a long hold then a tap is not a double-tap")
    // The second tap's steal cancels a press claimed on its down.
    let cancelled = 0
    let inner = { cancel: () => cancelled++ }
    dt.handlers.onPointerDown(ev(50, 0, 0))
    await sleep(30)
    dt.handlers.onPointerUp(ev(50, 0, 0))
    await sleep(100)
    arena.claim(51, inner)
    dt.handlers.onPointerDown(ev(51, 0, 0))
    if (cancelled !== 1 || taps.join("|") !== "double 0") fail(`the second down steals from the press, got cancelled ${cancelled} taps ${taps.join("|")}`)
    if (arena.claim(51, inner)) fail("the double-tap holds the second pointer until its lift")
    dt.handlers.onPointerUp(ev(51, 0, 0))
    if (!arena.claim(51, inner)) fail("the lift releases the second pointer")
    arena.release(51, inner)
  }
}

checks().then(
  () => {
    if (failures > 0) throw new Error(`gesture check: ${failures} failure(s)`)
    console.log("gesture check: ok")
  },
  err => {
    throw err
  },
)
