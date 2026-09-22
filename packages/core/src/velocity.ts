// The velocity tracker every recognizer reads at a lift: the pointer's
// speed from the positions of its last VELOCITY_WINDOW_MS as a
// least-squares line, not the last two samples. The final sample before a
// lift is often stationary, and frame batching makes the last delta a
// whole frame old, so a two-point estimate reads anything from zero to a
// jolt; a fit over the window reads the finger's actual speed. One
// estimator whatever recognizer asks (createPan, createTransform, the
// pointer feed), so the 2d camera and a control after it never re-derive
// their own.
//
// Samples are positions in whatever frame the caller measures (the
// recognizers push the node's parent frame, so the velocity applies 1:1
// as their deltas do; a swipe classifier pushes window pixels, the
// finger's own travel) with performance.now() at handler time, the
// precedent in transform.ts: PointerEvent carries no timestamp.

// Fit horizon: Flutter's and Android's VelocityTracker window.
const VELOCITY_WINDOW_MS = 100
// Largest speed reported, px/s (Flutter's kMaxFlingVelocity): a sensor
// glitch cannot launch the content across the world.
const VELOCITY_MAX = 8000
// A position unchanged for this long at the read means the finger rested
// before lifting: the velocity is zero, whatever the window still holds.
// Measured from the last sample that MOVED, not the newest sample's age:
// the runtime's resampler delivers an extrapolated step and its correction
// up to two frames after the finger stopped, and any same-position
// re-delivery must not restart the clock either.
const VELOCITY_REST_MS = 50
// Ring capacity: a 100 ms window holds 12 samples at 120 Hz; the rest is
// room for a burst of same-frame samples.
const VELOCITY_SAMPLES = 20
/** Release speeds under this (px/s, Flutter's kMinFlingVelocity) are not a
 * fling: a recognizer delivers zero for them, so a finger that slowed to a
 * stop leaves the content where it is. */
export const FLING_MIN_VELOCITY = 50

export type Velocity = { vx: number; vy: number }

export interface VelocityTracker {
  /** Record a position; `at` defaults to performance.now(). */
  push(x: number, y: number, at?: number): void
  /** Translate every sample: a recognizer rebasing its reference point
   * (a finger joining or leaving a transform) keeps the history
   * continuous instead of dropping it. */
  shift(dx: number, dy: number): void
  reset(): void
  /** The speed at `at` (default now), px/s per axis: a least-squares fit
   * over the window, zero after a rest, clamped to VELOCITY_MAX. */
  velocity(at?: number): Velocity
}

const ZERO: Velocity = { vx: 0, vy: 0 }

/** Zero under FLING_MIN_VELOCITY, else the velocity itself. */
export let flingVelocity = (v: Velocity): Velocity => (Math.hypot(v.vx, v.vy) < FLING_MIN_VELOCITY ? ZERO : v)

export function createVelocityTracker(): VelocityTracker {
  let xs = new Float64Array(VELOCITY_SAMPLES)
  let ys = new Float64Array(VELOCITY_SAMPLES)
  let ts = new Float64Array(VELOCITY_SAMPLES)
  // Ring: `count` samples ending at index `head - 1`.
  let head = 0
  let count = 0
  // When the position last changed (the rest rule's clock).
  let movedAt = -Infinity
  return {
    push(x, y, at = performance.now()) {
      if (count === 0) movedAt = at
      else {
        let last = (head - 1 + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
        if (xs[last] !== x || ys[last] !== y) movedAt = at
      }
      xs[head] = x
      ys[head] = y
      ts[head] = at
      head = (head + 1) % VELOCITY_SAMPLES
      if (count < VELOCITY_SAMPLES) count++
    },
    shift(dx, dy) {
      for (let i = 0; i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
        xs[k] = xs[k]! + dx
        ys[k] = ys[k]! + dy
      }
    },
    reset() {
      head = 0
      count = 0
    },
    velocity(at = performance.now()) {
      if (count < 2) return ZERO
      if (at - movedAt > VELOCITY_REST_MS) return ZERO
      // Means over the window, then the slope of each axis against time.
      let n = 0
      let tm = 0
      let xm = 0
      let ym = 0
      for (let i = 0; i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
        if (at - ts[k]! > VELOCITY_WINDOW_MS) break
        n++
        tm += ts[k]!
        xm += xs[k]!
        ym += ys[k]!
      }
      if (n < 2) return ZERO
      tm /= n
      xm /= n
      ym /= n
      let tt = 0
      let tx = 0
      let ty = 0
      for (let i = 0; i < n; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
        let dt = ts[k]! - tm
        tt += dt * dt
        tx += dt * (xs[k]! - xm)
        ty += dt * (ys[k]! - ym)
      }
      // Every sample the same age (one batched frame): no slope to read.
      if (tt === 0) return ZERO
      // Slopes are px per ms; the result is px per second.
      let vx = (tx / tt) * 1000
      let vy = (ty / tt) * 1000
      let speed = Math.hypot(vx, vy)
      if (speed > VELOCITY_MAX) {
        let f = VELOCITY_MAX / speed
        vx *= f
        vy *= f
      }
      return { vx, vy }
    },
  }
}
