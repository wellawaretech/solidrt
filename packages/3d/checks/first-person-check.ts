// Checks for the first-person control (first-person.ts) as a pure axes
// consumer: the look and move verbs, the look/move/rise deltas in the
// vocabulary's units, the rates integrated by update(dt) with the unit
// clamp on diagonals, walk vs fly, clampPosition, the pitch clamps,
// glideTo (exact landing, clampPosition every frame, dropped by input and
// set()) and active(). Pure-module input only, headless on flux, from the
// repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/first-person-check.ts | target/release/flux -
//
// Deterministic; prints FAIL lines and throws at the end.

import { flush } from "@solidjs/signals"
import { createFirstPersonCamera } from "../src/first-person.ts"
import type { FirstPersonCameraOptions } from "../src/first-person.ts"
import type { CameraUpdate } from "../src/camera.ts"
import type { Vec3 } from "../src/math.ts"

let failures = 0
let fail = (msg: string) => {
  failures++
  console.log(`FAIL ${msg}`)
}
let near = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) <= eps
let nearV = (a: Vec3, b: Vec3, eps = 1e-9) => near(a[0], b[0], eps) && near(a[1], b[1], eps) && near(a[2], b[2], eps)
// The frame time every glide is stepped at, and the ticks a glide must
// have landed within.
const DT = 1 / 60
const SETTLE_TICKS = 300

function make(options: FirstPersonCameraOptions = {}) {
  let last: CameraUpdate | null = null
  let cam = createFirstPersonCamera({ setCamera: u => (last = u) }, options)
  return { cam, last: () => last }
}

// ---- Verbs ----
{
  let { cam, last } = make({ position: [0, 1.6, 0] })
  cam.lookBy(Math.PI / 2, 0)
  if (!nearV(cam.forward(), [-1, 0, 0])) fail(`yaw positive turns left (faces -x), got ${cam.forward()}`)
  cam.moveBy(0, 2)
  if (!nearV(cam.eye(), [-2, 1.6, 0])) fail(`moveBy forward walks along the heading, got ${cam.eye()}`)
  cam.moveBy(1, 0)
  if (!nearV(cam.eye(), [-2, 1.6, -1])) fail(`moveBy right walks along the heading's right, got ${cam.eye()}`)
  cam.moveBy(0, 0, 1)
  if (!near(cam.eye()[1], 1.6)) fail("up is ignored while walking")
  if (!nearV(last()!.position as Vec3, cam.eye())) fail("the verbs push the pose")
  if (!cam.update(0.016) || cam.update(0.016)) fail("update reports a verb's change once")
}

// ---- Deltas ----
{
  let { cam } = make()
  // Half a turn per element height (DRAG_TURNS 0.5); a drag right turns
  // right (yaw negative), a drag down looks down (pitch negative).
  cam.axes.nudge("look", [0.5, 0.1])
  let p = cam.pose()
  if (!near(p.yaw, -Math.PI / 2) || !near(p.pitch, -0.1 * Math.PI)) fail(`look delta in turns: ${JSON.stringify(p)}`)
  cam.set({ yaw: 0, pitch: 0, position: [0, 1.6, 0] })
  // A move delta is a step in world units, forward being -y.
  cam.axes.nudge("move", [0, -1])
  if (!nearV(cam.eye(), [0, 1.6, -1])) fail(`a move delta steps a world unit forward (-z at yaw 0), got ${cam.eye()}`)
  cam.axes.nudge("rise", 1)
  if (!near(cam.eye()[1], 1.6)) fail("rise is inert while walking")
}

// ---- Rates, the unit clamp, fly, clampPosition ----
{
  let opts: FirstPersonCameraOptions = { position: [0, 1.6, 0], moveSpeed: 2 }
  let { cam } = make(opts)
  if (cam.active()) fail("a still walker rests")
  let stop = cam.axes.add("move", () => [1, -1])
  flush()
  if (!cam.active()) fail("a held direction wakes active()")
  cam.update(1)
  let e = cam.eye()
  // [1, -1] clamps to the unit diagonal: two seconds of speed 2 is 2 units
  // of travel, not 2.83.
  if (!near(Math.hypot(e[0], e[2]), 2)) fail(`a diagonal walks at full speed, not faster: travelled ${Math.hypot(e[0], e[2])}`)
  stop()
  flush()
  if (cam.active()) fail("releasing rests")
  cam.set({ position: [0, 1.6, 0], pitch: -Math.PI / 4 })
  let ahead = cam.axes.add("move", () => [0, -1])
  cam.update(1)
  if (!near(cam.eye()[1], 1.6) || !near(cam.eye()[2], -2)) fail(`walking ignores the pitch, got ${cam.eye()}`)
  opts.fly = true
  cam.set({ position: [0, 1.6, 0] })
  cam.update(1)
  if (!near(cam.eye()[1], 1.6 - 2 * Math.sin(Math.PI / 4)) || !near(cam.eye()[2], -2 * Math.cos(Math.PI / 4))) fail(`flying follows the view, got ${cam.eye()}`)
  ahead()
  let rise = cam.axes.add("rise", () => 1)
  cam.set({ position: [0, 1.6, 0] })
  cam.update(0.5)
  if (!near(cam.eye()[1], 2.6)) fail(`rise climbs at moveSpeed in fly mode, got ${cam.eye()[1]}`)
  rise()
  // A held boost doubles the rate (the default boostSpeed), a nudge is
  // still one world unit.
  let boost = cam.axes.add("boost", () => 1)
  let sprint = cam.axes.add("move", () => [0, -1])
  cam.set({ position: [0, 1.6, 0], pitch: 0 })
  cam.update(1)
  if (!near(cam.eye()[2], -4)) fail(`a held boost walks at boostSpeed x moveSpeed, got ${cam.eye()}`)
  sprint()
  cam.axes.nudge("move", [0, -1])
  if (!near(cam.eye()[2], -5)) fail(`a boost leaves a move delta a one-unit step, got ${cam.eye()}`)
  boost()
  opts.fly = false
  opts.clampPosition = (next, current) => (Math.abs(next[2]) > 3 ? current : next)
  cam.set({ position: [0, 1.6, 0], pitch: 0 })
  let walk = cam.axes.add("move", () => [0, -1])
  cam.update(1)
  cam.update(1)
  cam.update(1)
  walk()
  if (!near(cam.eye()[2], -2)) fail(`clampPosition rejects the third step, got ${cam.eye()}`)
  let look = cam.axes.add("look", () => [0, -1])
  cam.update(1)
  look()
  if (!near(cam.pose().pitch, Math.min(0.4 * 2 * Math.PI, Math.PI / 2 - 0.01))) fail(`a look rate of 1 over a second turns 0.4 turns, clamped at the pole: ${cam.pose().pitch}`)
}

// ---- Clamps and validation ----
{
  let { cam } = make({ minPitch: -0.3, maxPitch: 0.3 })
  cam.lookBy(0, 2)
  if (cam.pose().pitch !== 0.3) fail(`maxPitch clamps, got ${cam.pose().pitch}`)
  let throws = (what: string, f: () => void) => {
    try {
      f()
      fail(`${what} must throw`)
    } catch (err) {
      if (!(err instanceof Error)) fail(`${what}: unexpected ${err}`)
    }
  }
  throws("moveBy NaN", () => cam.moveBy(NaN, 0))
  throws("glideTo NaN", () => cam.glideTo({ yaw: NaN }))
  throws("a target without setCamera", () => createFirstPersonCamera({} as never))
}

// ---- glideTo: eased pose, exact landing, clampPosition per frame, rest ----
{
  let opts: FirstPersonCameraOptions = { position: [0, 1.6, 0], maxPitch: 0.3 }
  let { cam, last } = make(opts)
  cam.glideTo({ position: [4, 1.6, -2], yaw: 1, pitch: 1 })
  if (!nearV(cam.eye(), [0, 1.6, 0])) fail("glideTo does not jump")
  flush()
  if (!cam.active()) fail("a glide wakes active()")
  let ticks = 0
  for (; ticks < SETTLE_TICKS; ticks++) if (!cam.update(DT)) break
  if (ticks === 0 || ticks >= SETTLE_TICKS) fail(`glideTo should run and then rest, ticks=${ticks}`)
  let p = cam.pose()
  if (!nearV(p.position, [4, 1.6, -2]) || p.yaw !== 1 || p.pitch !== 0.3) fail(`glideTo lands exactly on the clamped goal, got ${JSON.stringify(p)}`)
  if (!nearV(last()!.position as Vec3, [4, 1.6, -2])) fail("the glide pushes the pose")
  flush()
  if (cam.active()) fail("a landed glide rests")
  // clampPosition is consulted every frame: a wall at z = -1 stops the walk.
  opts.clampPosition = next => (next[2] < -1 ? [next[0], next[1], -1] : next)
  cam.set({ position: [0, 1.6, 0] })
  cam.glideTo({ position: [0, 1.6, -5] })
  for (let i = 0; i < SETTLE_TICKS; i++) {
    if (!cam.update(DT)) break
    if (cam.eye()[2] < -1 - 1e-9) {
      fail(`a glide's frames go through clampPosition, z ${cam.eye()[2]} at tick ${i}`)
      break
    }
  }
  if (!near(cam.eye()[2], -1)) fail(`a glide lands where clampPosition allows, got ${cam.eye()}`)
  // Any input drops a glide; so does a set() of a pose field.
  opts.clampPosition = undefined
  cam.set({ position: [0, 1.6, 0], yaw: 0 })
  cam.glideTo({ yaw: 2 })
  cam.update(DT)
  cam.axes.nudge("look", [0, 0])
  let yaw = cam.pose().yaw
  cam.update(DT)
  if (cam.update(DT) || cam.pose().yaw !== yaw) fail("a nudge drops a glide")
  cam.glideTo({ yaw: 2 })
  cam.set({ pitch: 0.1 })
  cam.update(DT)
  if (cam.update(DT) || cam.pose().yaw !== yaw) fail("set() of a pose field drops a glide")
}

console.log(failures === 0 ? "FIRST-PERSON-OK" : `FIRST-PERSON-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} first-person check(s) failed`)
