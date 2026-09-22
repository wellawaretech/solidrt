// Checks for the shared camera-control math (src/camera-control.ts): the
// ease, the framing zones (dead zone, the soft band, hard limits, per-axis
// damping, the settle), the lookahead filter, the lanes (offset plus
// summed shakes, decay to zero, never in a pose) and the validation
// throws. Pure-module input only, so it runs headless on flux, bundled
// from the repo root:
//
//   bunx srt bundle -f --stdout packages/core/checks/camera-control-check.ts | target/release/flux -
//
// Deterministic; prints FAIL lines and throws at the end.

import { checkFollowOptions, checkShake, createActivity, createLanes, createLookahead, createShotBlend, easeStep, followRate, FOLLOW_EASE, frame, GLIDE_EASE } from "../src/camera-control"
import { flush } from "@solidjs/signals"

let failures = 0
let fail = (msg: string) => {
  failures++
  console.log(`FAIL ${msg}`)
}
let near = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) <= eps
const DT = 1 / 60
// The settle threshold the checks frame with, a viewport fraction.
const EPS = 0.001

// ---- The ease: frame-rate independent, closes the gap ----
{
  let two = 1 - (1 - easeStep(GLIDE_EASE, DT)) * (1 - easeStep(GLIDE_EASE, DT))
  if (!near(easeStep(GLIDE_EASE, 2 * DT), two)) fail("two half-steps compose to one full step")
  if (!near(easeStep(FOLLOW_EASE, 1), 1 - Math.exp(-FOLLOW_EASE))) fail("easeStep is 1 - e^(-rate dt)")
  if (followRate(undefined, "x") !== FOLLOW_EASE) fail("default follow rate is FOLLOW_EASE")
  if (followRate({ damping: 2 }, "y") !== FOLLOW_EASE / 2) fail("damping 2 halves the rate")
  if (followRate({ damping: { x: 0.5 } }, "x") !== FOLLOW_EASE * 2 || followRate({ damping: { x: 0.5 } }, "y") !== FOLLOW_EASE) fail("per-axis damping reads its axis, others default")
  if (followRate({ damping: 0 }, "x") !== Infinity) fail("damping 0 is at once")
}

// ---- Framing: the dead zone ignores, the band eases, the limits clamp ----
{
  // No zones: the point is chased the whole way, eased.
  let f = frame([0.3, -0.2], undefined, DT, EPS)
  let k = easeStep(FOLLOW_EASE, DT)
  if (!near(f.x, 0.3 * k) || !near(f.y, -0.2 * k) || f.settled) fail(`no zones: eased chase of the whole offset, got ${JSON.stringify(f)}`)
  // Inside the dead zone: nothing, and settled.
  let dead = frame([0.1, 0.05], { deadZone: { width: 0.4, height: 0.4 } }, DT, EPS)
  if (dead.x !== 0 || dead.y !== 0 || !dead.settled) fail(`inside the dead zone nothing moves: ${JSON.stringify(dead)}`)
  // Just outside it: only the overshoot past the edge is chased.
  let edge = frame([0.3, 0], { deadZone: { width: 0.4, height: 0.4 } }, DT, EPS)
  if (!near(edge.x, 0.1 * k) || edge.y !== 0) fail(`the overshoot past the dead edge is eased, got ${JSON.stringify(edge)}`)
  // Beyond the hard limits: the part past the limit applies whole, the
  // band's share eased, on top.
  let hard = frame([0.45, 0], { deadZone: { width: 0.2, height: 0.2 }, hardLimits: { width: 0.6, height: 0.6 } }, DT, EPS)
  if (!near(hard.x, 0.15 + 0.2 * k)) fail(`past the hard limit: whole beyond plus eased band, got ${hard.x}`)
  // Damping 0 snaps the band too.
  let snap = frame([0.45, 0], { deadZone: { width: 0.2, height: 0.2 }, damping: 0 }, DT, EPS)
  if (!near(snap.x, 0.35) || !snap.settled) fail(`damping 0 snaps to the dead edge and settles, got ${JSON.stringify(snap)}`)
  // Per-axis: a lazy y.
  let axes = frame([0.2, 0.2], { damping: { x: 1, y: 4 } }, DT, EPS)
  if (!near(axes.x, 0.2 * k) || !near(axes.y, 0.2 * easeStep(FOLLOW_EASE / 4, DT))) fail(`per-axis damping, got ${JSON.stringify(axes)}`)
  // Sign: a point left of and above the pivot corrects negative.
  let neg = frame([-0.3, -0.3], { deadZone: { width: 0.2, height: 0.2 } }, DT, EPS)
  if (!(neg.x < 0 && neg.y < 0)) fail("a negative offset corrects negative")
  // Under epsilon the remainder snaps and settles.
  let tiny = frame([0.0005, 0], undefined, DT, EPS)
  if (!near(tiny.x, 0.0005) || !tiny.settled) fail(`under epsilon the framing snaps and settles, got ${JSON.stringify(tiny)}`)
  // Hard limits never narrower than the dead zone: with no band left, a
  // point past the dead edge is clamped to it at once.
  let inverted = frame([0.3, 0], { deadZone: { width: 0.4, height: 0.4 }, hardLimits: { width: 0.2, height: 0.2 } }, DT, EPS)
  if (!near(inverted.x, 0.1) || !inverted.settled) fail(`hard limits inside the dead zone widen to it and clamp whole, got ${JSON.stringify(inverted)}`)
}

// ---- Framing over frames: a fast point never passes the hard limit ----
{
  let opts = { deadZone: { width: 0.2, height: 0.2 }, hardLimits: { width: 0.6, height: 0.6 }, damping: 4 }
  // The point runs right at 2 viewport widths per second; the camera
  // follows lazily, but the offset stays within the limit every frame.
  let cam = 0
  let point = 0
  let worst = 0
  for (let i = 0; i < 120; i++) {
    point += 2 * DT
    let f = frame([point - cam, 0], opts, DT, EPS)
    cam += f.x
    worst = Math.max(worst, Math.abs(point - cam))
  }
  if (worst > 0.3 + 1e-9) fail(`a fast point never passes the hard limit (0.3), worst offset ${worst}`)
  // Without limits the lazy follow lets it run away.
  cam = 0
  point = 0
  worst = 0
  for (let i = 0; i < 120; i++) {
    point += 2 * DT
    let f = frame([point - cam, 0], { damping: 4 }, DT, EPS)
    cam += f.x
    worst = Math.max(worst, Math.abs(point - cam))
  }
  if (!(worst > 0.3)) fail(`without hard limits a lazy follow trails past 0.3, worst ${worst}`)
  // A resting point settles: the follow reports settled once inside.
  let settledAt = -1
  cam = 0
  for (let i = 0; i < 600; i++) {
    let f = frame([point - cam, 0], opts, DT, EPS)
    cam += f.x
    if (f.settled) {
      settledAt = i
      break
    }
  }
  if (settledAt < 0) fail("a resting point lets the framing settle")
}

// ---- Lookahead: velocity times time, smoothed ----
{
  let look = createLookahead()
  let out: number[] = [0, 0]
  // A point moving at 60 units/s for a few frames predicts 0.5 s ahead.
  for (let i = 0; i < 5; i++) look.predict([i, 0], { time: 0.5 }, DT, out)
  if (!near(out[0]!, 4 + 60 * 0.5, 1e-6)) fail(`raw lookahead predicts velocity * time, got ${out[0]}`)
  // Smoothing lags the prediction: the same motion under 0.5 s of
  // smoothing predicts less on the first frames.
  let smooth = createLookahead()
  for (let i = 0; i < 5; i++) smooth.predict([i, 0], { time: 0.5, smoothing: 0.5 }, DT, out)
  if (!(out[0]! < 4 + 30 && out[0]! > 4)) fail(`smoothed lookahead lags, got ${out[0]}`)
  // No time: the point itself.
  look.predict([9, 9], undefined, DT, out)
  if (out[0] !== 9 || out[1] !== 9) fail("no lookahead returns the point")
  // reset forgets the velocity.
  look.reset()
  look.predict([100, 0], { time: 1 }, DT, out)
  if (out[0] !== 100) fail(`after reset the first prediction is the point, got ${out[0]}`)
  // Three dimensions work the same.
  let three = createLookahead()
  let out3: number[] = [0, 0, 0]
  for (let i = 0; i < 3; i++) three.predict([0, 0, i], { time: 1 }, DT, out3)
  if (!near(out3[2]!, 2 + 60, 1e-6)) fail(`3d lookahead, got ${out3[2]}`)
}

// ---- Lanes: offset plus shakes, decay, never in a pose ----
{
  let lanes = createLanes()
  let t = lanes.total([0.1, -0.2])
  if (t[0] !== 0.1 || t[1] !== -0.2 || lanes.active()) fail("the offset alone is the total, no shake active")
  if (lanes.total(undefined)[0] !== 0) fail("no offset reads zero")
  lanes.shake(0.05, 0.5, { direction: [1, 0] })
  if (!lanes.active()) fail("a shake makes the lanes active")
  let peak = 0
  let ticks = 0
  for (; ticks < 100; ticks++) {
    lanes.step(DT)
    if (!lanes.active()) break
    let v = lanes.total(undefined)
    if (v[1] !== 0) fail("a shake along x moves nothing on y")
    peak = Math.max(peak, Math.abs(v[0]))
  }
  if (ticks < 29 || ticks > 31) fail(`a 0.5 s shake at 60 Hz ends in ~30 ticks, got ${ticks}`)
  if (!(peak > 0 && peak <= 0.05 + 1e-9)) fail(`the shake peaks under its strength, got ${peak}`)
  if (lanes.total([0.1, 0])[0] !== 0.1) fail("after the shake the total is the offset again")
  // Two shakes sum: the total may exceed either strength alone.
  lanes.shake(0.05, 0.5, { direction: [1, 0] })
  lanes.shake(0.05, 0.5, { direction: [1, 0] })
  let summed = 0
  for (let i = 0; i < 30; i++) {
    lanes.step(DT)
    summed = Math.max(summed, Math.abs(lanes.total(undefined)[0]))
  }
  if (!(summed > 0.05)) fail(`two shakes sum, peak ${summed}`)
  lanes.clear()
  if (lanes.active()) fail("clear stops every shake")
}

// ---- Validation ----
{
  let throws = (what: string, f: () => void) => {
    try {
      f()
      fail(`${what} must throw`)
    } catch (err) {
      if (!(err instanceof Error) || !err.message.startsWith("check")) fail(`${what}: unexpected error ${err}`)
    }
  }
  throws("deadZone 2", () => checkFollowOptions("check", { deadZone: { width: 2, height: 0 } }))
  throws("hardLimits -1", () => checkFollowOptions("check", { hardLimits: { width: 0.5, height: -1 } }))
  throws("damping -1", () => checkFollowOptions("check", { damping: -1 }))
  throws("damping.y NaN", () => checkFollowOptions("check", { damping: { y: NaN } }))
  throws("lookahead time -1", () => checkFollowOptions("check", { lookahead: { time: -1 } }))
  throws("shake strength -1", () => checkShake("check", -1, 1))
  throws("shake duration 0", () => checkShake("check", 0.1, 0))
  throws("shake frequency 0", () => checkShake("check", 0.1, 1, { frequency: 0 }))
  throws("shake direction bad", () => checkShake("check", 0.1, 1, { direction: [1] as never }))
  checkFollowOptions("check", undefined)
  checkFollowOptions("check", { deadZone: { width: 0.2, height: 0.2 }, hardLimits: { width: 0.8, height: 0.8 }, damping: { x: 1, y: 2 }, lookahead: { time: 0.2, smoothing: 0.1 } })
}

// ---- The activity gate: the plain flag survives unflushed writes ----
{
  let busy = false
  let rates = false
  let gate = createActivity(() => busy, () => rates)
  busy = true
  gate.notify()
  busy = false
  gate.notify()
  flush()
  if (gate.active()) fail("a true then a false without a flush between lands on false")
  rates = true
  // A plain variable is not reactive: the gate reads it on the next
  // notify/flush cycle only through busy; rates must be a signal read in
  // a control (axes.active() is), so here only busy is checked.
  busy = true
  gate.notify()
  flush()
  if (!gate.active()) fail("a busy control is active")
}

// ---- Shots: live by priority, pushes at rest, blends smoothly, lands exactly ----
{
  type Cam = { v: number }
  let pushed: number[] = []
  let shots = createShotBlend<Cam>(c => pushed.push(c.v), (a, b, t) => ({ v: a.v + (b.v - a.v) * t }), { v: 0 }, { blend: 0.5 })
  let a = shots.shot("a")
  let b = shots.shot("b", { priority: 1 })
  a.setCamera({ v: 10 })
  if (pushed.length !== 0 || shots.live() !== null) fail("no shot enabled: nothing pushed")
  shots.activate("a")
  if (shots.live() !== "a" || pushed.at(-1) !== 10) fail(`the first live shot pushes its latest at once, got ${pushed.at(-1)}`)
  a.setCamera({ v: 12 })
  if (pushed.at(-1) !== 12 || shots.update(DT)) fail("a live shot's push goes straight through at rest, no update needed")
  // b wins by priority and blends from the current output over 0.5 s.
  b.setCamera({ v: 100 })
  shots.activate("b")
  if (shots.live() !== "b") fail("the higher priority is live")
  flush()
  if (!shots.active()) fail("a blend wakes active()")
  if (!near(shots.camera().v, 12)) fail(`a blend starts at the current output, got ${shots.camera().v}`)
  let prev = 12
  let ticks = 0
  for (; ticks < 100; ticks++) {
    if (!shots.update(DT)) break
    let v = shots.camera().v
    if (v < prev - 1e-9 || v > 100 + 1e-9) fail(`a blend is monotonic and bounded, ${prev} -> ${v}`)
    prev = v
  }
  if (ticks < 29 || ticks > 31) fail(`a 0.5 s blend takes ~30 ticks, took ${ticks}`)
  if (shots.camera().v !== 100) fail(`a blend lands exactly, got ${shots.camera().v}`)
  flush()
  if (shots.active()) fail("a landed blend rests")
  // The live shot moves during a blend: the blend chases its latest.
  shots.activate("a", { blend: 0.5 })
  if (shots.live() !== "b") fail("priority: a cannot beat b while b is enabled")
  shots.deactivate("b")
  if (shots.live() !== "a") fail("deactivating the live shot falls back by priority")
  for (let i = 0; i < 15; i++) shots.update(DT)
  let mid = shots.camera().v
  if (!(mid < 100 && mid > 12)) fail(`mid-blend output between the two, got ${mid}`)
  // A switch mid-blend starts from the output of that moment: no jump.
  shots.activate("b")
  if (!near(shots.camera().v, mid, 1e-9)) fail(`a switch mid-blend starts from the current output, got ${shots.camera().v} vs ${mid}`)
  // A zero blend cuts.
  shots.deactivate("b", { blend: 0 })
  if (shots.camera().v !== 12) fail(`a zero blend cuts to the live shot, got ${shots.camera().v}`)
  let throws = (what: string, f: () => void) => {
    try {
      f()
      fail(`${what} must throw`)
    } catch (err) {
      if (!(err instanceof Error) || !err.message.startsWith("createShotBlend")) fail(`${what}: unexpected error ${err}`)
    }
  }
  // remove frees the name and falls back by priority.
  shots.activate("b", { blend: 0 })
  shots.remove("b", { blend: 0 })
  if (shots.live() !== "a" || shots.camera().v !== 12) fail(`remove falls back to the next shot, live ${shots.live()}`)
  shots.shot("b")
  throws("duplicate shot", () => shots.shot("a"))
  throws("unknown shot", () => shots.activate("zzz"))
  throws("negative blend", () => shots.activate("a", { blend: -1 }))
}

if (failures > 0) throw new Error(`${failures} camera-control check(s) failed`)
console.log("camera-control checks passed")
