// The velocity tracker every recognizer reads at a lift: the pointer's
// speed from the positions of its last VELOCITY_WINDOW_MS as a
// least-squares quadratic read at the newest sample (Flutter's
// VelocityTracker, Android's LSQ2), not the last two samples. The final
// sample before a lift is often stationary, and frame batching makes the
// last delta a whole frame old, so a two-point estimate reads anything from
// zero to a jolt; a fit over the window reads the finger's actual speed.
// Quadratic, not a line: a flick accelerates into the lift, and a line
// reads the window's average speed, well under the speed at the lift. The
// line stays as the guard: a quadratic overshoots under a hard brake and
// can point backward, so an axis where the two disagree in direction
// reads zero. One
// estimator whatever recognizer asks (createPan, createTransform, the
// pointer feed), so the 2d camera and a control after it never re-derive
// their own.
//
// Samples are positions in whatever frame the caller measures (the
// recognizers push the node's parent frame, so the velocity applies 1:1
// as their deltas do; a swipe classifier pushes window pixels, the
// finger's own travel), each with the `timeStamp` of the pointer event it
// came from. The tracker reads no clock: the read at a lift takes the
// up's `timeStamp`, and a move's stamp is the time of the position it
// carries, so the age of the last one is how long the pointer had been
// there, at any frame rate.

// Fit horizon: Flutter's and Android's VelocityTracker window.
const VELOCITY_WINDOW_MS = 100
// Largest speed reported, px/s (Flutter's kMaxFlingVelocity): a sensor
// glitch cannot launch the content across the world.
const VELOCITY_MAX = 8000
// A position unchanged for this long at the read means the finger rested
// before lifting: the velocity is zero, whatever the window still holds.
// Measured from when the pointer reached the position it is at, not from
// the newest sample's age: a same-position re-delivery must not restart
// the clock.
const VELOCITY_REST_MS = 50
// A gap this long (ms) between two consecutive samples means the pointer
// stopped in between (Flutter's and Android's assume-stopped time): what
// came before the gap is an earlier motion and stays out of the fit. A
// slow drag delivers a sample only when the finger moves, so its history
// is full of such pauses.
const VELOCITY_STOP_GAP_MS = 40
// Travel (px) the fitted samples must cover to be a motion at all. A finger
// leaving the panel shifts its contact by a pixel or two in its last
// samples (measured: up to 2.7 px over 8 to 25 ms on a tablet), which a
// fit over so short a span reads as 60 to 450 px/s. The pan slop's
// distance: under it nothing was dragged.
const VELOCITY_MIN_TRAVEL = 8
// Samples closer in time than this (ms) are one instant to the fit: two
// samples of one frame carry the same stamp, and a curve pinned by such a
// pair reads a jolt.
const VELOCITY_MIN_STEP_MS = 1
// Ring capacity: a 100 ms window holds 12 samples at 120 Hz; the rest is
// room for a burst of same-frame samples.
const VELOCITY_SAMPLES = 20
/** Release speeds under this (px/s, Flutter's kMinFlingVelocity) are not a
 * fling: a recognizer delivers zero for them, so a finger that slowed to a
 * stop leaves the content where it is. */
export const FLING_MIN_VELOCITY = 50

export type Velocity = { vx: number; vy: number }

export interface VelocityTracker {
  /** Record a position at time `at`, ms: the `timeStamp` of the event
   * that carried it. Real positions only: a `predicted` move is not where
   * the pointer was, and is left out. */
  push(x: number, y: number, at: number): void
  /** Translate every sample: a recognizer rebasing its reference point
   * (a finger joining or leaving a transform) keeps the history
   * continuous instead of dropping it. */
  shift(dx: number, dy: number): void
  reset(): void
  /** The speed at time `at` (the lift's `timeStamp`), px/s per axis: a
   * least-squares quadratic over the window read at its newest sample,
   * zero after a rest or when the motion since the last pause is a
   * twitch, clamped to VELOCITY_MAX. */
  velocity(at: number): Velocity
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
  return {
    push(x, y, at) {
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
    velocity(at) {
      if (count < 2) return ZERO
      let newest = (head - 1 + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
      // The rest rule's clock: when the pointer reached the position it is
      // at, the oldest of the newest samples that share it.
      let movedAt = ts[newest]!
      for (let i = 1; i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
        if (xs[k] !== xs[newest] || ys[k] !== ys[newest]) break
        movedAt = ts[k]!
      }
      if (at - movedAt > VELOCITY_REST_MS) return ZERO
      // Sums over the window, time and position measured from the newest
      // sample so the fit's slope at t = 0 is the answer: the power sums
      // of t, each axis's moments against them, and how many distinct
      // times (VELOCITY_MIN_STEP_MS apart) the window holds.
      let s0 = 0
      let s1 = 0
      let s2 = 0
      let s3 = 0
      let s4 = 0
      let x0 = 0
      let x1 = 0
      let x2 = 0
      let y0 = 0
      let y1 = 0
      let y2 = 0
      let times = 0
      let prev = Infinity
      // The sample after this one in time, and the fit's oldest position.
      let later = ts[newest]!
      let farX = 0
      let farY = 0
      for (let i = 0; i < count; i++) {
        let k = (head - 1 - i + VELOCITY_SAMPLES) % VELOCITY_SAMPLES
        if (at - ts[k]! > VELOCITY_WINDOW_MS) break
        if (later - ts[k]! > VELOCITY_STOP_GAP_MS) break
        later = ts[k]!
        let t = ts[k]! - ts[newest]!
        if (prev - t >= VELOCITY_MIN_STEP_MS) {
          times++
          prev = t
        }
        let x = xs[k]! - xs[newest]!
        let y = ys[k]! - ys[newest]!
        farX = x
        farY = y
        let tt = t * t
        s0 += 1
        s1 += t
        s2 += tt
        s3 += tt * t
        s4 += tt * tt
        x0 += x
        x1 += t * x
        x2 += tt * x
        y0 += y
        y1 += t * y
        y2 += tt * y
      }
      // Every sample the same age (one batched frame): no slope to read.
      if (times < 2) return ZERO
      if (Math.hypot(farX, farY) < VELOCITY_MIN_TRAVEL) return ZERO
      // One axis's slope at t = 0, px per ms: the quadratic
      // c0 + c1 t + c2 t^2 (c1 by Cramer's rule) once three distinct times
      // pin it, the line before that; zero where the quadratic points
      // against the line.
      let det = s0 * (s2 * s4 - s3 * s3) - s1 * (s1 * s4 - s2 * s3) + s2 * (s1 * s3 - s2 * s2)
      let slope = (m0: number, m1: number, m2: number) => {
        let line = (s0 * m1 - s1 * m0) / (s0 * s2 - s1 * s1)
        if (times < 3) return line
        let curve = (s0 * (m1 * s4 - s3 * m2) - m0 * (s1 * s4 - s3 * s2) + s2 * (s1 * m2 - m1 * s2)) / det
        return curve * line < 0 ? 0 : curve
      }
      // Slopes are px per ms; the result is px per second.
      let vx = slope(x0, x1, x2) * 1000
      let vy = slope(y0, y1, y2) * 1000
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
