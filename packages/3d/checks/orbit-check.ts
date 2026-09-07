// Checks for the orbit control (orbit.ts) as a pure axes consumer: the
// pose verbs, the rotate/zoom/pan deltas in the vocabulary's units, the
// anchored zoom (one anchor per pinch gesture, per notch for the wheel),
// the pivot re-seat on a rotate begin, the rates integrated by update(dt)
// with active() gating, the clamps and update()'s change report. Pure-
// module input only (orbit.ts imports `@solidrt/core/input`, no runtime
// module), so it runs headless on flux, bundled from the repo root:
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

function make(options: OrbitCameraOptions = {}) {
  let last: CameraUpdate | null = null
  let writes = 0
  let orbit = createOrbitCamera(
    {
      setCamera: u => {
        last = u
        writes++
      },
      camera: () => ({ fov: FOV }),
    },
    options,
  )
  return { orbit, last: () => last, writes: () => writes }
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

// ---- Deltas in the vocabulary's units ----
{
  let { orbit } = make({ distance: 4, azimuth: 0, elevation: 0 })
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
  throws("a target without camera()", () => createOrbitCamera({ setCamera() {} } as never))
  throws("nudge with a bad delta", () => orbit.axes.nudge("zoom", [1, 2] as never))
}

console.log(failures === 0 ? "ORBIT-OK" : `ORBIT-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} orbit check(s) failed`)
