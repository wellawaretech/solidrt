// Checks for the velocity tracker (velocity.ts): a constant speed reads
// exactly, a flick accelerating into the lift reads the speed at the lift,
// a stop's resampler bounce does not fling, a hard brake never reads
// backward, near-simultaneous samples read no jolt, a rested finger reads
// zero, the window and the clamp hold, a frame-batched stream (same-age
// sample pairs) reads as the unbatched one, a shift keeps the fit, and the
// fling gate. Explicit timestamps, so it is deterministic and needs no
// timers. Pure-module input only, so it runs headless on flux: `srt test
// packages/core`.

import { test } from "flux:test"
import { createVelocityTracker, flingVelocity, FLING_MIN_VELOCITY } from "../src/velocity.ts"

function fail(msg: string): void {
  throw new Error(msg)
}
let near = (a: number, b: number, eps = 1e-6) => Math.abs(a - b) <= eps

test("A constant speed reads exactly; a diagonal keeps both axes", () => {
  let t = createVelocityTracker()
  // 16 ms steps of 8 px: 500 px/s rightward, 250 px/s down.
  for (let i = 0; i <= 10; i++) t.push(i * 8, i * 4, 1000 + i * 16)
  let v = t.velocity(1000 + 160)
  if (!near(v.vx, 500) || !near(v.vy, 250)) fail(`constant speed reads 500,250, got ${v.vx},${v.vy}`)
  // Read a frame after the last sample (the up lands before the next
  // move would have): still the same line.
  v = t.velocity(1000 + 176)
  if (!near(v.vx, 500) || !near(v.vy, 250)) fail(`a read one frame later reads the same line, got ${v.vx},${v.vy}`)
})

test("A flick accelerating into the lift reads the speed at the lift", () => {
  let t = createVelocityTracker()
  // x = t + t^2 / 96: 1 px/ms at the window's start, 3 px/ms at the lift.
  // A line through the window reads its average, 2000 px/s.
  for (let i = 0; i <= 6; i++) t.push(i * 16 + (i * 16) ** 2 / 96, 0, i * 16)
  let v = t.velocity(96)
  if (!near(v.vx, 3000)) fail(`an accelerating flick reads its lift speed 3000, got ${v.vx}`)
})

test("A stop's resampler bounce does not fling", () => {
  let t = createVelocityTracker()
  // 625 px/s to 100, then the resampler's extrapolated step on the first
  // empty frame and its settle back on the second.
  for (let i = 0; i <= 10; i++) t.push(i * 10, 0, i * 16)
  t.push(110, 0, 11 * 16)
  t.push(100, 0, 12 * 16)
  let v = flingVelocity(t.velocity(12 * 16))
  if (v.vx !== 0) fail(`a stop's extrapolate-and-settle is not a fling, got ${v.vx}`)
})

test("The window: only the last 100 ms count", () => {
  let t = createVelocityTracker()
  // 200 ms slow (100 px/s), then 100 ms fast (1000 px/s): the fast part
  // alone is in the window.
  let x = 0
  let time = 0
  for (let i = 0; i < 12; i++) {
    t.push(x, 0, time)
    x += 1.6
    time += 16
  }
  time -= 16
  x -= 1.6
  for (let i = 0; i < 7; i++) {
    time += 16
    x += 16
    t.push(x, 0, time)
  }
  let v = t.velocity(time)
  if (!near(v.vx, 1000, 1e-6)) fail(`the window holds the last 100 ms only, got ${v.vx}`)
})

test("A hard brake never reads backward", () => {
  let t = createVelocityTracker()
  // Steps of 30, 25, 20, 15, 10, 5, 0 px: the quadratic alone overshoots
  // to a negative slope.
  let xs = [0, 30, 55, 75, 90, 100, 105, 105]
  xs.forEach((x, i) => t.push(x, 0, i * 16))
  let v = t.velocity(7 * 16)
  if (v.vx !== 0) fail(`a hard brake reads zero, not a reverse speed, got ${v.vx}`)
})

test("Near-simultaneous samples read no jolt", () => {
  let t = createVelocityTracker()
  // Two samples 0.05 ms apart are one instant: two distinct times, so the
  // line, not a curve pinned by the pair.
  t.push(0, 0, 0)
  t.push(10, 0, 16)
  t.push(10.5, 0, 16.05)
  let v = t.velocity(16.05)
  if (!(v.vx > 500 && v.vx < 800)) fail(`near-simultaneous samples read the line, got ${v.vx}`)
})

test("A rested finger reads zero; fewer than two samples read zero", () => {
  let t = createVelocityTracker()
  for (let i = 0; i <= 5; i++) t.push(i * 8, 0, i * 16)
  let v = t.velocity(5 * 16 + 60)
  if (v.vx !== 0 || v.vy !== 0) fail(`a lift 60 ms after the last move reads zero, got ${v.vx}`)
  v = t.velocity(5 * 16 + 40)
  if (v.vx === 0) fail("a lift 40 ms after the last move still reads the fit")
  // The runtime resamples a held finger every frame: same-position samples
  // do not restart the rest clock.
  for (let i = 6; i <= 9; i++) t.push(40, 0, i * 16)
  v = t.velocity(9 * 16 + 4)
  if (v.vx !== 0) fail(`a position unchanged for 68 ms reads zero under resampled holds, got ${v.vx}`)
  let one = createVelocityTracker()
  one.push(0, 0, 0)
  if (one.velocity(0).vx !== 0) fail("one sample reads zero")
  if (createVelocityTracker().velocity(0).vx !== 0) fail("no samples read zero")
})

test("The clamp", () => {
  let t = createVelocityTracker()
  // 400 px per 16 ms = 25000 px/s, at a 3:4 ratio.
  for (let i = 0; i <= 4; i++) t.push(i * 240, i * 320, i * 16)
  let v = t.velocity(64)
  if (!near(Math.hypot(v.vx, v.vy), 8000, 1e-6)) fail(`speed clamps to 8000, got ${Math.hypot(v.vx, v.vy)}`)
  if (!near(v.vx / v.vy, 0.75, 1e-9)) fail(`the clamp keeps the direction, got ${v.vx}/${v.vy}`)
})

test("Frame batching: same-age samples do not skew", () => {
  let plain = createVelocityTracker()
  let batched = createVelocityTracker()
  for (let i = 0; i <= 6; i++) {
    plain.push(i * 10, 0, i * 16)
    // Two samples per frame at the same time: the second is the resampled
    // correction, a pixel further.
    batched.push(i * 10, 0, i * 16)
    batched.push(i * 10 + 1, 0, i * 16)
  }
  let a = plain.velocity(96).vx
  let b = batched.velocity(96).vx
  if (!near(a, 625)) fail(`the plain stream reads 625, got ${a}`)
  if (!near(b, 625)) fail(`same-age pairs read the same slope, got ${b}`)
  // Every sample the same age: no slope.
  let flat = createVelocityTracker()
  flat.push(0, 0, 50)
  flat.push(100, 0, 50)
  if (flat.velocity(50).vx !== 0) fail("all samples at one time read zero")
})

test("A shift keeps the fit; a reset empties it", () => {
  let t = createVelocityTracker()
  for (let i = 0; i <= 5; i++) t.push(i * 8, i * 8, i * 16)
  t.shift(-1000, 250)
  let v = t.velocity(80)
  if (!near(v.vx, 500) || !near(v.vy, 500)) fail(`a shift keeps the slope, got ${v.vx},${v.vy}`)
  t.push(48 - 1000, 48 + 250, 96)
  v = t.velocity(96)
  if (!near(v.vx, 500) || !near(v.vy, 500)) fail(`a sample after a shift continues the line, got ${v.vx},${v.vy}`)
  t.reset()
  if (t.velocity(96).vx !== 0) fail("reset empties the tracker")
})

test("The ring: more samples than the capacity keep the newest", () => {
  let t = createVelocityTracker()
  for (let i = 0; i < 60; i++) t.push(i * 5, 0, i * 16)
  let v = t.velocity(59 * 16)
  if (!near(v.vx, 312.5)) fail(`a long stream reads its recent speed, got ${v.vx}`)
})

test("The fling gate", () => {
  let slow = flingVelocity({ vx: 30, vy: 30 })
  if (slow.vx !== 0 || slow.vy !== 0) fail("a 42 px/s release is not a fling")
  let fast = flingVelocity({ vx: 40, vy: 40 })
  if (fast.vx !== 40) fail("a 57 px/s release passes through")
  if (FLING_MIN_VELOCITY !== 50) fail("FLING_MIN_VELOCITY is Flutter's 50")
})
