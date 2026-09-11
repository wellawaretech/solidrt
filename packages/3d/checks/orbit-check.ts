// Checks for the orbit control (orbit.ts) as a pure axes consumer: the
// pose verbs, the rotate/zoom/pan deltas in the vocabulary's units, the
// anchored zoom (one anchor per pinch gesture, per notch for the wheel),
// the pivot re-seat on a rotate begin, the rates integrated by update(dt)
// with active() gating, the damped wheel notch (anchor pinned every tick,
// notches compounding, exact landing, dropped by input and set()),
// glideTo, fit against the aspect, clampPose on every write path, the
// clamps and update()'s change report. Pure-module input only (orbit.ts
// imports `@solidrt/core/input`, no runtime module), so it runs headless
// on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/orbit-check.ts | target/release/flux -
//
// Deterministic; prints FAIL lines and throws at the end.

import { flush } from "@solidjs/signals"
import { createOrbitCamera } from "../src/orbit.ts"
import type { OrbitCameraOptions } from "../src/orbit.ts"
import type { CameraUpdate } from "../src/camera.ts"
import type { Vec3 } from "../src/math.ts"

let failures = 0
let fail = (msg: string) => {
  failures++
  console.log(`FAIL ${msg}`)
}
let near = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) <= eps
let nearV = (a: Vec3, b: Vec3, eps = 1e-9) => near(a[0], b[0], eps) && near(a[1], b[1], eps) && near(a[2], b[2], eps)
const FOV = 60
// The frame time every glide is stepped at, and the ticks any glide must
// have landed within (9 e-foldings/s lands in under a second).
const DT = 1 / 60
const SETTLE_TICKS = 300

function make(options: OrbitCameraOptions = {}, size = { width: 800, height: 600 }) {
  let last: CameraUpdate | null = null
  let writes = 0
  let rig = { ortho: null as { left: number; right: number; top: number; bottom: number } | null }
  let orbit = createOrbitCamera(
    {
      setCamera: u => {
        last = u
        writes++
      },
      camera: () => ({ fov: FOV, ortho: rig.ortho }),
      size: () => size,
    },
    options,
  )
  return { orbit, rig, last: () => last, writes: () => writes }
}

// Step until update reports rest; returns the ticks taken (SETTLE_TICKS+1
// means it never rested).
function settle(orbit: ReturnType<typeof make>["orbit"]): number {
  for (let i = 0; i < SETTLE_TICKS; i++) {
    if (!orbit.update(DT)) return i
  }
  return SETTLE_TICKS + 1
}

// ---- Verbs push at once; update reports the change once ----
{
  let { orbit, last, writes } = make({ distance: 4 })
  if (writes() !== 1) fail(`creation pushes once, got ${writes()}`)
  orbit.rotateBy(0.5, 0.25)
  let p = orbit.pose()
  if (!near(p.azimuth, 0.5) || !near(p.elevation, 0.25) || writes() !== 2) fail(`rotateBy adds and pushes: ${JSON.stringify(p)}, writes ${writes()}`)
  if (!orbit.update(0.016)) fail("update reports a pose changed by a verb")
  if (orbit.update(0.016)) fail("a second update reports no change")
  orbit.zoomBy(2)
  if (!near(orbit.pose().distance, 2)) fail(`zoomBy(2) halves the distance, got ${orbit.pose().distance}`)
  orbit.set({ azimuth: 0, elevation: 0, distance: 4, target: [0, 0, 0] })
  orbit.panBy(1, 0.5)
  if (!nearV(orbit.pose().target, [1, 0.5, 0])) fail(`panBy at azimuth 0 slides the target along x/y, got ${orbit.pose().target}`)
  if (!nearV(last()!.position as Vec3, [1, 0.5, 4])) fail(`the eye follows the pan, got ${last()!.position}`)
  orbit.set({ azimuth: 0, elevation: 0, distance: 4, target: [0, 0, 0] })
  orbit.setPivot([0.5, 0.7, 1])
  if (!near(orbit.pose().distance, 3) || !nearV(orbit.pose().target, [0, 0, 1])) fail(`setPivot re-seats on the view axis at the point's depth, got ${JSON.stringify(orbit.pose())}`)
  if (!nearV(last()!.position as Vec3, [0, 0, 4])) fail("setPivot leaves the eye where it is")
  orbit.setPivot([0, 0, 9])
  if (!near(orbit.pose().distance, 3)) fail("a point behind the eye is ignored")
}

// ---- Deltas in the vocabulary's units (damping off: at once) ----
{
  let { orbit } = make({ distance: 4, azimuth: 0, elevation: 0, damping: 0 })
  // One element height of drag travel is one full turn (DRAG_TURNS 1),
  // a drag right turning azimuth negative, a drag down raising the eye.
  orbit.axes.nudge("rotate", [0.25, 0.1])
  let p = orbit.pose()
  if (!near(p.azimuth, -Math.PI / 2) || !near(p.elevation, 0.1 * 2 * Math.PI)) fail(`rotate delta in turns: ${JSON.stringify(p)}`)
  orbit.set({ azimuth: 0, elevation: 0 })
  // Zoom in octaves, positive in.
  orbit.axes.nudge("zoom", 1)
  if (!near(orbit.pose().distance, 2)) fail(`a zoom delta of one octave halves the distance, got ${orbit.pose().distance}`)
  orbit.axes.nudge("zoom", -1)
  // Pan in element heights: one height is the frustum height at the target.
  let frustum = 2 * Math.tan((FOV * Math.PI) / 360) * 4
  orbit.axes.nudge("pan", [0.5, 0])
  if (!near(orbit.pose().target[0], -0.5 * frustum)) fail(`a pan delta of half a height slides the scene half the frustum width, got ${orbit.pose().target[0]}`)
  orbit.set({ target: [0, 0, 0] })
  orbit.axes.nudge("pan", [0, 0.5])
  if (!near(orbit.pose().target[1], 0.5 * frustum)) fail(`a pan down lifts the target (the scene follows the finger), got ${orbit.pose().target[1]}`)
}

// ---- Anchored zoom: one anchor per pinch, per notch for the wheel ----
{
  let anchors = 0
  let { orbit } = make({
    distance: 4,
    target: [0, 0, 0],
    zoomAnchor: () => {
      anchors++
      return [1, 0, 0]
    },
  })
  orbit.axes.begin("zoom")
  orbit.axes.nudge("zoom", 0.5, [0.2, 0.2])
  orbit.axes.nudge("zoom", 0.5, [0.2, 0.2])
  orbit.axes.end("zoom")
  if (anchors !== 1) fail(`a pinch anchors once per gesture, got ${anchors}`)
  // The anchor stays where it projects: the target slid toward it by the
  // distance ratio (2 over 4).
  if (!nearV(orbit.pose().target, [0.5, 0, 0]) || !near(orbit.pose().distance, 2)) fail(`anchored zoom slides the target toward the anchor: ${JSON.stringify(orbit.pose())}`)
  orbit.axes.nudge("zoom", 0.1, [0.2, 0.2])
  orbit.axes.nudge("zoom", 0.1, [0.2, 0.2])
  if (anchors !== 3) fail(`wheel notches anchor per notch, got ${anchors}`)
  orbit.axes.nudge("zoom", 0.1)
  if (anchors !== 3) fail("a delta without a focal asks for no anchor")
  // A rotate begin re-seats the pivot through rotateAnchor.
  let pivoted = make({ distance: 4, target: [0, 0, 0], rotateAnchor: () => [0, 0, 2] })
  pivoted.orbit.axes.begin("rotate")
  if (!near(pivoted.orbit.pose().distance, 2)) fail(`rotateAnchor re-seats the pivot on a rotate begin, got ${pivoted.orbit.pose().distance}`)
  pivoted.orbit.axes.end("rotate")
}

// ---- Rates, active(), auto-orbit pause ----
{
  let { orbit } = make({ distance: 4, orbitSpeed: 1 })
  if (!orbit.active()) fail("an auto-orbit is active")
  orbit.set({ orbiting: false })
  flush()
  if (orbit.active()) fail("a paused auto-orbit rests")
  let stop = orbit.axes.add("rotate", () => [1, 0])
  flush()
  if (!orbit.active()) fail("a rate wakes active()")
  orbit.update(1)
  // ROTATE_RATE 0.5 turns/s, a stick right turning azimuth negative.
  if (!near(orbit.pose().azimuth, -Math.PI)) fail(`a rotate rate of 1 over a second is half a turn, got ${orbit.pose().azimuth}`)
  stop()
  let stopZoom = orbit.axes.add("zoom", () => 1)
  orbit.update(1)
  stopZoom()
  if (!near(orbit.pose().distance, 2)) fail(`a zoom rate of 1 halves the distance per second, got ${orbit.pose().distance}`)
  flush()
  if (orbit.active()) fail("no rates and no auto-orbit rests")
  // The auto-orbit pauses while a gesture is open.
  orbit.set({ orbiting: true, azimuth: 0 })
  orbit.axes.begin("rotate")
  orbit.update(1)
  if (orbit.pose().azimuth !== 0) fail("the auto-orbit pauses during a gesture")
  orbit.axes.end("rotate")
  orbit.update(1)
  if (!near(orbit.pose().azimuth, 1)) fail(`the auto-orbit resumes after the gesture, got ${orbit.pose().azimuth}`)
}

// ---- Clamps and validation ----
{
  let { orbit } = make({ distance: 4, minDistance: 2, maxDistance: 8, minElevation: -0.5, maxElevation: 0.5 })
  orbit.zoomBy(8)
  if (orbit.pose().distance !== 2) fail(`minDistance clamps the zoom, got ${orbit.pose().distance}`)
  orbit.rotateBy(0, 3)
  if (orbit.pose().elevation !== 0.5) fail(`maxElevation clamps the rotation, got ${orbit.pose().elevation}`)
  let throws = (what: string, f: () => void) => {
    try {
      f()
      fail(`${what} must throw`)
    } catch (err) {
      if (!(err instanceof Error)) fail(`${what}: unexpected ${err}`)
    }
  }
  throws("zoomBy 0", () => orbit.zoomBy(0))
  throws("rotateBy NaN", () => orbit.rotateBy(NaN, 0))
  throws("glideTo NaN", () => orbit.glideTo({ azimuth: NaN }))
  throws("fit with five values", () => orbit.fit([0, 0, 0, 1, 1]))
  throws("a target without camera()", () => createOrbitCamera({ setCamera() {} } as never))
  throws("a target without size()", () => createOrbitCamera({ setCamera() {}, camera: () => ({ fov: 60, ortho: null }) } as never))
  throws("nudge with a bad delta", () => orbit.axes.nudge("zoom", [1, 2] as never))
}

// ---- The damped wheel notch: a glide, anchor pinned, compounding ----
{
  let { orbit, writes } = make({ distance: 4, target: [0, 0, 0], zoomAnchor: () => [1, 0, 0] })
  let before = writes()
  orbit.axes.nudge("zoom", 0.5, [0.2, 0.2])
  if (orbit.pose().distance !== 4 || writes() !== before) fail("an unbracketed zoom delta does not jump the pose")
  flush()
  if (!orbit.active()) fail("a damped notch wakes active()")
  let goal = 4 * Math.pow(2, -0.5)
  let ticks = 0
  for (; ticks < SETTLE_TICKS; ticks++) {
    if (!orbit.update(DT)) break
    let p = orbit.pose()
    // The anchor [1, 0, 0] stays pinned: the target sits on the line from
    // the anchor to the reference target, scaled by the distance ratio.
    if (!near(p.target[0], 1 - p.distance / 4)) {
      fail(`the eased target stays anchored at tick ${ticks}: distance ${p.distance}, target ${p.target}`)
      break
    }
  }
  if (ticks === 0 || ticks >= SETTLE_TICKS) fail(`a notch glides and then rests, ticks=${ticks}`)
  if (orbit.pose().distance !== goal) fail(`the glide lands exactly on the notch's distance ${goal}, got ${orbit.pose().distance}`)
  flush()
  if (orbit.active()) fail("a landed glide rests")
  // Notches compound on the pending distance.
  orbit.axes.nudge("zoom", 0.5)
  orbit.axes.nudge("zoom", 0.5)
  settle(orbit)
  if (!near(orbit.pose().distance, 4 * Math.pow(2, -1.5))) fail(`two notches compound: got ${orbit.pose().distance}`)
  // A finger landing holds the glide; set() of a pose field drops it,
  // set({}) and set({ orbiting }) leave it running.
  orbit.axes.nudge("zoom", 1)
  orbit.update(DT)
  let held = orbit.pose().distance
  orbit.axes.begin("rotate")
  orbit.update(DT)
  if (orbit.pose().distance !== held) fail("a gesture begin holds a glide where it is")
  orbit.axes.end("rotate")
  orbit.axes.nudge("zoom", 1)
  orbit.set({ orbiting: false })
  orbit.set({})
  if (!orbit.update(DT) || orbit.pose().distance === held) fail("set({}) and set({ orbiting }) leave a glide running")
  orbit.set({ distance: 3 })
  orbit.update(DT)
  if (orbit.update(DT) || orbit.pose().distance !== 3) fail("set({ distance }) snaps and drops the glide")
  // A damped rotate step likewise glides; a rate drops it and applies at once.
  orbit.axes.nudge("rotate", [0.25, 0])
  if (orbit.pose().azimuth !== 0) fail("an unbracketed rotate delta glides too")
  settle(orbit)
  if (!near(orbit.pose().azimuth, -Math.PI / 2)) fail(`the rotate glide lands on the delta, got ${orbit.pose().azimuth}`)
  // damping scales the settle time: twice the damping, about twice the ticks.
  let slow = make({ distance: 4, damping: 2 })
  slow.orbit.axes.nudge("zoom", 1)
  let slowTicks = settle(slow.orbit)
  let quick = make({ distance: 4 })
  quick.orbit.axes.nudge("zoom", 1)
  let quickTicks = settle(quick.orbit)
  if (!(slowTicks > quickTicks * 1.5)) fail(`damping 2 coasts longer: ${slowTicks} vs ${quickTicks} ticks`)
}

// ---- glideTo: eased pose, clamps on the goal, exact landing, rest ----
{
  let { orbit, last } = make({ distance: 4, maxDistance: 10 })
  orbit.glideTo({ azimuth: 1, elevation: 0.5, distance: 20, target: [1, 2, 3] })
  if (orbit.pose().distance !== 4) fail("glideTo does not jump")
  flush()
  if (!orbit.active()) fail("a glide wakes active()")
  let ticks = settle(orbit)
  if (ticks === 0 || ticks > SETTLE_TICKS) fail(`glideTo should run and then rest, ticks=${ticks}`)
  let p = orbit.pose()
  if (p.azimuth !== 1 || p.elevation !== 0.5 || p.distance !== 10 || !nearV(p.target, [1, 2, 3])) fail(`glideTo lands exactly on the clamped goal, got ${JSON.stringify(p)}`)
  if (!nearV(last()!.target as Vec3, [1, 2, 3])) fail("the glide pushes the pose")
  flush()
  if (orbit.active()) fail("a landed glide rests")
  // A verb drops a glide; the verb applies at once.
  orbit.glideTo({ azimuth: 0 })
  orbit.rotateBy(0.25, 0)
  orbit.update(DT)
  if (orbit.update(DT) || !near(orbit.pose().azimuth, 1.25)) fail("a verb drops a glide and applies at once")
  // The auto-orbit carries the goal with it.
  let spinning = make({ distance: 4, orbitSpeed: 1, azimuth: 0 })
  spinning.orbit.glideTo({ distance: 2 })
  for (let i = 0; i < 60; i++) spinning.orbit.update(DT)
  if (!near(spinning.orbit.pose().azimuth, 1, 1e-6)) fail(`the auto-orbit runs through a glide, got ${spinning.orbit.pose().azimuth}`)
}

// ---- fit: the bounding sphere against the tighter fov ----
{
  let bounds = [-1, -2, -3, 3, 2, 1]
  let center: Vec3 = [1, 0, -1]
  let radius = Math.hypot(4, 4, 4) / 2
  let vertical = (FOV * Math.PI) / 360
  let expect = (aspect: number) => radius / Math.sin(Math.min(vertical, Math.atan(Math.tan(vertical) * aspect)))
  let wide = make({ distance: 4, azimuth: 0.3, elevation: 0.2 }, { width: 800, height: 400 })
  wide.orbit.fit(bounds)
  let p = wide.orbit.pose()
  if (!nearV(p.target, center) || !near(p.distance, expect(2)) || p.azimuth !== 0.3 || p.elevation !== 0.2) fail(`fit on a wide view frames by the vertical fov: ${JSON.stringify(p)}`)
  wide.orbit.update(DT)
  if (wide.orbit.update(DT)) fail("fit without glide snaps and starts no motion")
  let tall = make({ distance: 4 }, { width: 400, height: 800 })
  tall.orbit.fit(bounds)
  if (!near(tall.orbit.pose().distance, expect(0.5))) fail(`fit on a tall view frames by the horizontal fov: ${tall.orbit.pose().distance}`)
  if (!(tall.orbit.pose().distance > wide.orbit.pose().distance)) fail("a tall view needs more distance than a wide one")
  // Glide form: eases there; clamps apply.
  let glide = make({ distance: 4, maxDistance: 3 }, { width: 800, height: 400 })
  glide.orbit.fit(bounds, { glide: true })
  if (!nearV(glide.orbit.pose().target, [0, 0, 0])) fail("fit with glide does not jump")
  settle(glide.orbit)
  if (!nearV(glide.orbit.pose().target, center) || glide.orbit.pose().distance !== 3) fail(`fit with glide lands on the clamped frame: ${JSON.stringify(glide.orbit.pose())}`)
  // Under ortho only the target moves.
  let ortho = make({ distance: 4 })
  ortho.rig.ortho = { left: -1, right: 1, top: 1, bottom: -1 }
  ortho.orbit.fit(bounds)
  if (!nearV(ortho.orbit.pose().target, center) || ortho.orbit.pose().distance !== 4) fail("fit under ortho re-centres and keeps the distance")
  // A point has no extent: re-centre, keep the distance.
  ortho.rig.ortho = null
  ortho.orbit.fit([2, 2, 2, 2, 2, 2])
  if (!nearV(ortho.orbit.pose().target, [2, 2, 2]) || ortho.orbit.pose().distance !== 4) fail("fit on a point re-centres and keeps the distance")
}

// ---- clampPose: every write path, the whole pose in, fields out ----
{
  let calls = 0
  // The floor: the eye stays at or above y = 0.5, an elevation floor that
  // tightens with the distance.
  let floor = 0.5
  let { orbit } = make({
    distance: 2,
    target: [0, 0, 0],
    elevation: 0,
    clampPose: pose => {
      calls++
      let minElevation = Math.asin(Math.min(1, (floor - pose.target[1]) / pose.distance))
      return pose.elevation < minElevation ? { elevation: minElevation } : undefined
    },
  })
  if (calls !== 1) fail(`clampPose runs on the initial pose, got ${calls} calls`)
  if (!near(orbit.pose().elevation, Math.asin(0.25))) fail(`the initial pose is clamped through the hook, got ${orbit.pose().elevation}`)
  let eyeY = () => orbit.eye()[1]
  orbit.zoomBy(0.5)
  if (!(eyeY() >= floor - 1e-9)) fail(`a zoom out re-clamps the elevation on the write, eye y ${eyeY()}`)
  orbit.rotateBy(0, -1)
  if (!(eyeY() >= floor - 1e-9)) fail(`a rotate is clamped, eye y ${eyeY()}`)
  orbit.panBy(0, -3)
  if (!(eyeY() >= floor - 1e-9)) fail(`a pan is clamped, eye y ${eyeY()}`)
  orbit.set({ elevation: -1 })
  if (!(eyeY() >= floor - 1e-9)) fail(`set() is clamped, eye y ${eyeY()}`)
  // A glide toward an illegal pose lands on the clamped goal and never
  // shows an illegal frame on the way.
  orbit.set({ distance: 2, target: [0, 0, 0], elevation: 1 })
  orbit.glideTo({ elevation: -1, distance: 8 })
  for (let i = 0; i < SETTLE_TICKS; i++) {
    if (!orbit.update(DT)) break
    if (!(eyeY() >= floor - 1e-9)) {
      fail(`a glide's frames are clamped, eye y ${eyeY()} at tick ${i}`)
      break
    }
  }
  if (!(eyeY() >= floor - 1e-9) || orbit.pose().distance !== 8) fail(`a glide lands on the clamped goal: ${JSON.stringify(orbit.pose())}`)
  // The hook can move the target too (the clampTarget use).
  let boxed = make({ distance: 4, clampPose: pose => ({ target: [Math.min(1, pose.target[0]), pose.target[1], pose.target[2]] }) })
  boxed.orbit.panBy(5, 0)
  if (boxed.orbit.pose().target[0] !== 1) fail(`clampPose bounds a pan's target, got ${boxed.orbit.pose().target}`)
  let poison = false
  let bad = make({ distance: 4, clampPose: () => (poison ? { distance: NaN } : undefined) })
  poison = true
  try {
    bad.orbit.zoomBy(2)
    fail("a non-finite clampPose result must throw")
  } catch (err) {
    if (!(err instanceof Error)) fail(`clampPose NaN: unexpected ${err}`)
  }
}

console.log(failures === 0 ? "ORBIT-OK" : `ORBIT-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} orbit check(s) failed`)
