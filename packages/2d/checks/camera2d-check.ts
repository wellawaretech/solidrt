// Checks for the 2d camera control (camera2d.ts): the contain clamp and
// its centering, anchored zoom under any pivot and rotation, the eased
// glides (a wheel notch, glideTo, fit, a rotation) landing exactly,
// follow through the framing (a dead zone, hard limits that keep a fast
// target in view, damping per axis, lookahead), the lanes (the offset,
// a shake that never enters the pose and never shows the outside of the
// world), damped bounds, drag inertia, Godot's limits-ignore-rotation
// rule, the deferred fit on an unknown viewport, the axes (a pan
// gesture's brackets, a bracketed and an unbracketed zoom delta, rates
// integrated by update) and the validation throws - hand-written cases
// plus a seeded sweep. Pure-module input only (camera2d.ts imports no GUI or
// runtime module), so it runs headless on flux, bundled from the repo
// root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/camera2d-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end, and the flux binary
// exits 1 on the uncaught throw, so a CI step can gate on the exit code.
// The device side (the pointer feed's recognizer over core's transform
// recognizer) needs the runtime's event bus and is exercised live by
// examples/camera.tsx.

import { flush } from "@solidjs/signals"
import { argv } from "flux:process"
import { createCamera2d } from "../src/camera2d.ts"
import type { Camera2d, Camera2dOptions } from "../src/camera2d.ts"
import { projectCamera } from "../src/camera.ts"
import { createShots, mixCamera2d } from "../src/shots.ts"
import type { CameraUpdate } from "../src/camera.ts"

let seed = Number(argv[0] ?? Math.floor(Math.random() * 0xffffffff))
console.log("seed", seed)

let s = seed >>> 0
function rand(): number {
  s = (Math.imul(s, 1664525) + 1013904223) >>> 0
  return s / 4294967296
}
function range(lo: number, hi: number): number {
  return lo + rand() * (hi - lo)
}

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}

// Absolute tolerance for coordinates: inputs span a few thousand pixels,
// so float64 noise stays far below this.
const EPS = 1e-6
// The frame time every glide is stepped at, and the ticks any glide,
// follow or fling must have landed within (the slowest ease is 3
// e-foldings/s; 5 s is generous).
const DT = 1 / 60
const SETTLE_TICKS = 300
const SWEEP = 500

let near = (a: number, b: number, eps = EPS) => Math.abs(a - b) <= eps

type Rig = { cam: Camera2d; view: { width: number; height: number }; options: Camera2dOptions; last: () => CameraUpdate | null; writes: () => number }

// The options object is handed to the control as-is and returned, so a
// case can change an option after creation the way a live prop does.
function make(opts: Partial<Camera2dOptions> = {}, view = { width: 800, height: 600 }): Rig {
  let last: CameraUpdate | null = null
  let writes = 0
  let options: Camera2dOptions = { viewport: () => view, ...opts }
  let cam = createCamera2d(
    {
      setCamera: (u) => {
        last = u
        writes++
      },
    },
    options,
  )
  return { cam, view, options, last: () => last, writes: () => writes }
}

// A wheel notch as the pointer feed delivers it: an unbracketed zoom
// delta of deltaY wheel units at the feed's exponent, an eased glide.
let notch = (cam: Camera2d, sx: number, sy: number, deltaY: number) => cam.zoomAt(sx, sy, Math.exp(-deltaY * 0.0015), { glide: true })

// Step until update reports rest; returns the ticks taken (SETTLE_TICKS+1
// means it never rested).
function settle(cam: Camera2d): number {
  for (let i = 0; i < SETTLE_TICKS; i++) {
    if (!cam.update(DT)) return i
  }
  return SETTLE_TICKS + 1
}

// ---- Default fit, contain centering, the fit-zoom pan no-op ----
{
  let { cam, last } = make({ world: { width: 1000, height: 500 } })
  let c = cam.camera()
  if (!near(c.zoom!, 0.8)) fail(`default fit zoom: expected 0.8, got ${c.zoom}`)
  if (!near(c.pivotX!, 400) || !near(c.pivotY!, 300)) fail(`pivot defaults to the viewport center, got ${c.pivotX},${c.pivotY}`)
  let r = cam.viewRect()
  if (!near(r.x, 0) || !near(r.width, 1000)) fail(`fit shows the whole world width, got x=${r.x} w=${r.width}`)
  if (!near(r.y, -125) || !near(r.height, 750)) fail(`the taller view centers the world vertically, got y=${r.y} h=${r.height}`)
  if (last() === null) fail("the initial pose reaches the target at creation")
  cam.panBy(100, 50)
  let after = cam.camera()
  if (!near(after.x!, c.x!) || !near(after.y!, c.y!)) fail(`panning at fit zoom is a no-op, moved to ${after.x},${after.y}`)
}

// ---- The contain clamp at a zoom, snap writes ----
{
  let { cam } = make({ world: { width: 1000, height: 500 }, zoom: 2 })
  cam.set({ x: 5000 })
  if (!near(cam.camera().x, 800)) fail(`set clamps x to the right edge (800), got ${cam.camera().x}`)
  cam.panBy(10000, 10000)
  let c = cam.camera()
  if (!near(c.x!, 200) || !near(c.y!, 150)) fail(`a huge pan lands on the top-left edge (200,150), got ${c.x},${c.y}`)
  cam.set({ y: -100 })
  if (!near(cam.camera().y, 150)) fail(`set clamps y to the top edge (150), got ${cam.camera().y}`)
}

// ---- Godot's rule: limits ignore rotation ----
{
  let { cam } = make({ world: { width: 1000, height: 500 }, zoom: 2 })
  cam.set({ x: 5000, y: 5000, rotation: 1 })
  let c = cam.camera()
  if (!near(c.x!, 800) || !near(c.y!, 350)) fail(`rotated view clamps as if unrotated (800,350), got ${c.x},${c.y}`)
  if (!near(c.rotation!, 1)) fail(`rotation survives the clamp, got ${c.rotation}`)
}

// ---- Anchored zoom under any pivot and rotation (unbounded so no clamp interferes) ----
for (let i = 0; i < SWEEP; i++) {
  let pivot = { x: range(0, 1), y: range(0, 1) }
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, pivot, x: range(-1000, 1000), y: range(-1000, 1000), zoom: range(0.2, 5), rotation: range(-Math.PI, Math.PI) })
  let sx = range(0, 800)
  let sy = range(0, 600)
  let factor = range(0.5, 2)
  let before = cam.camera()
  let wx = before.x! + range(-500, 500)
  let wy = before.y! + range(-500, 500)
  let z0 = before.zoom!
  let [px0, py0] = projectCamera(cam.camera(), wx, wy)
  cam.zoomAt(px0, py0, factor)
  let after = cam.camera()
  if (!near(after.zoom!, z0 * factor, 1e-9)) fail(`zoomAt scales the zoom by the factor: ${z0} * ${factor} != ${after.zoom}`)
  let [px1, py1] = projectCamera(after, wx, wy)
  if (!near(px1, px0, 1e-6) || !near(py1, py0, 1e-6)) {
    fail(`zoomAt keeps the world point under the screen point (pivot ${pivot.x},${pivot.y} rot ${before.rotation}): ${px0},${py0} -> ${px1},${py1}`)
  }
  cam.panBy(sx - px1, sy - py1)
  let [px2, py2] = projectCamera(cam.camera(), wx, wy)
  if (!near(px2, sx, 1e-6) || !near(py2, sy, 1e-6)) fail(`panBy slides the world by the screen delta under rotation: expected ${sx},${sy}, got ${px2},${py2}`)
}

// ---- The wheel glide: anchor pinned every tick, exact landing, rest ----
{
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, x: 300, y: 200, zoom: 1.5, rotation: 0.4 })
  let sx = 123
  let sy = 456
  let [wx, wy] = [cam.camera().x + 80, cam.camera().y - 40]
  let [ax, ay] = projectCamera(cam.camera(), wx, wy)
  notch(cam, ax, ay, -400)
  let target = 1.5 * Math.exp(400 * 0.0015)
  let ticks = 0
  for (; ticks < SETTLE_TICKS; ticks++) {
    if (!cam.update(DT)) break
    let [px, py] = projectCamera(cam.camera(), wx, wy)
    if (!near(px, ax, 1e-6) || !near(py, ay, 1e-6)) {
      fail(`wheel glide keeps the anchor pinned at tick ${ticks}: ${ax},${ay} -> ${px},${py}`)
      break
    }
  }
  if (ticks === 0 || ticks >= SETTLE_TICKS) fail(`wheel glide should run and then rest, ticks=${ticks}`)
  if (cam.camera().zoom !== target) fail(`wheel glide lands exactly on its target ${target}, got ${cam.camera().zoom}`)
  // Notches compound on the pending target.
  notch(cam, sx, sy, -100)
  notch(cam, sx, sy, -100)
  settle(cam)
  if (!near(cam.camera().zoom, target * Math.exp(200 * 0.0015), 1e-9)) fail(`two notches compound: got ${cam.camera().zoom}`)
  // A rotation write mid-glide leaves the glide running; an x write cancels it.
  notch(cam, sx, sy, 100)
  cam.set({ rotation: 0.9 })
  let z = cam.camera().zoom
  cam.update(DT)
  if (cam.camera().zoom === z) fail("set({ rotation }) must not cancel a glide")
  notch(cam, sx, sy, 100)
  cam.set({ x: 310 })
  z = cam.camera().zoom
  cam.update(DT)
  cam.update(DT)
  if (cam.camera().zoom !== z) fail("set({ x }) cancels a glide in flight")
}

// ---- damping: 0 applies a notch at once, 2 coasts longer ----
{
  let snap = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, damping: 0 })
  notch(snap.cam, 400, 300, -400)
  if (!near(snap.cam.camera().zoom, Math.exp(400 * 0.0015), 1e-9)) fail(`damping 0 applies a wheel notch at once, got ${snap.cam.camera().zoom}`)
  snap.cam.update(DT)
  if (snap.cam.update(DT)) fail("damping 0 starts no glide")
  let quick = make({ minZoom: 0.01, maxZoom: 100, zoom: 1 })
  notch(quick.cam, 400, 300, -400)
  let quickTicks = settle(quick.cam)
  let slow = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, damping: 2 })
  notch(slow.cam, 400, 300, -400)
  let slowTicks = settle(slow.cam)
  if (!(slowTicks > quickTicks * 1.5)) fail(`damping 2 coasts longer: ${slowTicks} vs ${quickTicks} ticks`)
}

// ---- glideTo: eased pose, exact landing, rest ----
{
  let { cam, writes } = make({ world: { width: 1000, height: 500 }, zoom: 2 })
  cam.glideTo(700, 300, 3)
  let w0 = writes()
  let ticks = settle(cam)
  let c = cam.camera()
  if (ticks === 0 || ticks > SETTLE_TICKS) fail(`glideTo should run and then rest, ticks=${ticks}`)
  if (c.x !== 700 || c.y !== 300 || c.zoom !== 3) fail(`glideTo lands exactly on (700,300,3), got ${c.x},${c.y},${c.zoom}`)
  if (writes() - w0 !== ticks) fail(`one setCamera per changed tick: ${writes() - w0} writes over ${ticks} ticks`)
  if (cam.update(DT)) fail("a landed glide writes nothing more")
  // A destination outside the world lands on the clamp: at zoom 3 the view
  // is 800/3 x 200 world px, so x tops out at 1000 - 400/3 and y at 400.
  cam.glideTo(5000, 5000)
  settle(cam)
  c = cam.camera()
  if (!near(c.x!, 1000 - 400 / 3) || !near(c.y!, 400)) fail(`glideTo clamps its destination to (866.67,400), got ${c.x},${c.y}`)
  let r = cam.viewRect()
  if (r.x + r.width > 1000 + EPS || r.y + r.height > 500 + EPS) fail(`glide destination stays inside the world, view ${JSON.stringify(r)}`)
}

// ---- Live options: bounds, zoom range and pivot read where applied ----
{
  let { cam, options, last } = make({ world: { width: 1000, height: 500 }, maxZoom: 10, zoom: 5, x: 500, y: 250 })
  if (!near(cam.camera().zoom, 5)) fail(`live options: initial zoom 5, got ${cam.camera().zoom}`)
  // A tighter maxZoom re-clamps on set({}) - the component's re-clamp entry.
  options.maxZoom = 2
  cam.set({})
  if (!near(cam.camera().zoom, 2)) fail(`live maxZoom re-clamps the zoom, got ${cam.camera().zoom}`)
  // A smaller world re-contains: the view (400x300 at zoom 2) is wider
  // than a 300x100 world on both axes, so the pose centers on it.
  options.world = { width: 300, height: 100 }
  cam.set({})
  let c = cam.camera()
  if (!near(c.x!, 150) || !near(c.y!, 50)) fail(`live world re-contains and centers, got ${c.x},${c.y}`)
  // Dropping the world lifts the clamp: the same write now lands as given.
  options.world = undefined
  cam.set({ x: -400, y: -400 })
  c = cam.camera()
  if (!near(c.x!, -400) || !near(c.y!, -400)) fail(`live world removal unclamps, got ${c.x},${c.y}`)
  // A pivot change reaches the pushed pose at once.
  options.pivot = { x: 0, y: 0 }
  cam.set({})
  let pushed = last()
  if (!pushed || pushed.pivotX !== 0 || pushed.pivotY !== 0) fail(`live pivot is pushed by set({}), got ${pushed?.pivotX},${pushed?.pivotY}`)
  // A live rate applies where it is read: panSpeed scales the pan nudge.
  let before = cam.camera().x
  options.panSpeed = 2
  cam.axes.nudge("pan", [0.1, 0])
  let travelled = before - cam.camera().x
  if (!near(travelled, (0.1 * 600 * 2) / cam.camera().zoom)) fail(`live panSpeed scales the nudge, travelled ${travelled}`)
  // A bad live value throws at the re-clamp, as at creation.
  options.maxZoom = -1
  let threw = false
  try {
    cam.set({})
  } catch {
    threw = true
  }
  if (!threw) fail("a bad live maxZoom throws at set({})")
}

// ---- fit(rect): snapping and gliding, maxZoom below the fit ----
{
  let { cam } = make({ world: { width: 1000, height: 500 }, maxZoom: 10 })
  cam.fit({ x: 100, y: 100, width: 200, height: 100 })
  let c = cam.camera()
  if (!near(c.zoom!, 4) || !near(c.x!, 200) || !near(c.y!, 150)) fail(`fit(rect) centers the rect at zoom 4 (200,150), got ${c.x},${c.y},${c.zoom}`)
  cam.fit(undefined, { glide: true })
  let ticks = settle(cam)
  c = cam.camera()
  if (ticks === 0 || !near(c.zoom!, 0.8) || !near(c.x!, 500) || !near(c.y!, 250)) fail(`fit({ glide }) eases back to the world fit, got ${c.x},${c.y},${c.zoom} after ${ticks}`)
  let capped = make({ world: { width: 1000, height: 500 }, maxZoom: 0.5 })
  let cc = capped.cam.camera()
  let r = capped.cam.viewRect()
  if (!near(cc.zoom!, 0.5) || !near(cc.x!, 500) || !near(cc.y!, 250)) fail(`maxZoom below the fit wins and the world floats centered, got ${cc.x},${cc.y},${cc.zoom}`)
  if (!near(r.width, 1600)) fail(`capped fit view is 1600 wide, got ${r.width}`)
}

// ---- Deferred fit: an unknown viewport neither throws nor clamps ----
{
  let view = { width: 0, height: 0 }
  let { cam, last } = make({ world: { width: 1000, height: 500 } }, view)
  if (last() === null || cam.camera().zoom !== 1) fail("unknown viewport: the pose still reaches the target, unfitted")
  view.width = 800
  view.height = 600
  if (!cam.update(DT)) fail("the viewport becoming known is a change")
  if (!near(cam.camera().zoom, 0.8)) fail(`the deferred fit runs once the viewport is known, got zoom ${cam.camera().zoom}`)
  // A resize keeps the world point under the pivot and re-clamps.
  cam.set({ zoom: 2, x: 700, y: 300 })
  view.width = 400
  cam.update(DT)
  let c = cam.camera()
  if (!near(c.pivotX!, 200) || !near(c.x!, 700)) fail(`resize keeps the world point at the moved pivot, got pivotX ${c.pivotX} x ${c.x}`)
}

// ---- Follow: tight, then through a dead zone; settles and rests ----
{
  let { cam } = make({ world: { width: 1000, height: 500 }, zoom: 2, x: 500, y: 250 })
  cam.follow(600, 250)
  let ticks = settle(cam)
  let c = cam.camera()
  if (ticks === 0 || ticks > SETTLE_TICKS) fail(`tight follow should run and rest, ticks=${ticks}`)
  if (!near(c.x!, 600, 1e-3) || !near(c.y!, 250, 1e-3)) fail(`tight follow lands on the target (600,250), got ${c.x},${c.y}`)
  if (cam.update(DT)) fail("a settled follow writes nothing")
  cam.follow(600, 250)
  if (cam.update(DT)) fail("re-following a reached target writes nothing")
}
{
  let { cam } = make({ world: { width: 1000, height: 500 }, zoom: 2, x: 500, y: 250, follow: { deadZone: { width: 0.5, height: 0.5 } } })
  // Zone half-width 200 px; the target at screen x 800 overshoots by 200 px
  // = 100 world px, so the camera stops at 600 with the target on the edge.
  cam.follow(700, 250)
  settle(cam)
  let c = cam.camera()
  if (!near(c.x!, 600, 1e-3) || !near(c.y!, 250, 1e-3)) fail(`dead-zone follow parks the target on the zone edge (camera 600,250), got ${c.x},${c.y}`)
  cam.follow(650, 250)
  let moved = settle(cam)
  if (moved !== 0 || !near(cam.camera().x, 600, 1e-3)) fail(`a target inside the dead zone does not move the camera, moved ${moved} ticks to ${cam.camera().x}`)
  cam.unfollow()
  cam.follow(100, 250)
  settle(cam)
  if (!near(cam.camera().x, 200, 1e-3)) fail(`follow honors the world clamp (200), got ${cam.camera().x}`)
}

// ---- A wheel zoom survives a per-frame follow of a moving target ----
{
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, zoom: 2, x: 500, y: 250 })
  cam.follow(500, 250)
  settle(cam)
  notch(cam, 400, 300, -200)
  let target = 2 * Math.exp(200 * 0.0015)
  // The target moves a pixel a tick for as long as any glide may take;
  // the follow never rests meanwhile (a moving target keeps it active),
  // so the span is fixed rather than settled.
  let last = 500
  for (let tick = 0; tick < SETTLE_TICKS; tick++) {
    last = 500 + tick
    cam.follow(last, 250)
    cam.update(DT)
  }
  if (cam.camera().zoom !== target) fail(`wheel zoom lands on its target ${target} under a moving follow, got ${cam.camera().zoom}`)
  // The target stops: the follow, which trailed it by its ease, settles on it.
  settle(cam)
  if (!near(cam.camera().x, last, 1e-3)) fail(`follow tracked the moving target to ${last}, got ${cam.camera().x}`)
  // A pose glide still yields to the follow.
  cam.glideTo(100, 100)
  cam.follow(last, 250)
  settle(cam)
  if (!near(cam.camera().x, last, 1e-3)) fail(`follow cancels a pose glide, got ${cam.camera().x}`)
}

// ---- Framing: hard limits keep a fast target in view, per-axis damping, lookahead ----
{
  // A point running at 3000 px/s under a lazy follow: with hard limits
  // of half the view it never gets more than a quarter of the width from
  // the pivot; without them the same follow lets it run away.
  let run = (opts: Partial<Camera2dOptions>) => {
    let { cam } = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, x: 400, y: 300, ...opts })
    let point = 400
    let worst = 0
    for (let i = 0; i < 60; i++) {
      point += 3000 * DT
      cam.follow(point, 300)
      cam.update(DT)
      worst = Math.max(worst, point - cam.camera().x)
    }
    return worst
  }
  let limited = run({ follow: { damping: 4, hardLimits: { width: 0.5, height: 0.5 } } })
  if (limited > 200 + 1e-6) fail(`hard limits keep the target within a quarter of the width (200 px), worst ${limited}`)
  let free = run({ follow: { damping: 4 } })
  if (!(free > 200)) fail(`without hard limits a lazy follow trails past 200 px, worst ${free}`)
  // Per-axis damping: a tight x and a lazy y close different fractions
  // of their gaps over the same ticks.
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, x: 400, y: 300, follow: { damping: { x: 0.25, y: 4 } } })
  cam.follow(600, 400)
  for (let i = 0; i < 10; i++) cam.update(DT)
  let fx = (cam.camera().x - 400) / 200
  let fy = (cam.camera().y - 300) / 100
  if (!(fx > fy * 4)) fail(`per-axis damping: x closed ${fx}, y ${fy}`)
  // Lookahead: following a point moving right at 600 px/s with half a
  // second of lookahead, the camera runs AHEAD of the point.
  let ahead = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, x: 400, y: 300, follow: { lookahead: { time: 0.5 } } })
  let point = 400
  for (let i = 0; i < 90; i++) {
    point += 600 * DT
    ahead.cam.follow(point, 300)
    ahead.cam.update(DT)
  }
  if (!(ahead.cam.camera().x > point)) fail(`lookahead frames ahead of a moving point: camera ${ahead.cam.camera().x}, point ${point}`)
  // The point stops: the prediction collapses and the follow lands on it.
  for (let i = 0; i < SETTLE_TICKS; i++) {
    ahead.cam.follow(point, 300)
    if (!ahead.cam.update(DT)) break
  }
  if (!near(ahead.cam.camera().x, point, 1e-3)) fail(`after the point rests the follow lands on it, got ${ahead.cam.camera().x} want ${point}`)
}

// ---- Lanes: the offset shows the pose point off the pivot; a shake never enters the pose or shows the outside ----
{
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, x: 400, y: 300, offset: [0, -0.25] })
  if (!near(cam.pose().y, 300) || !near(cam.camera().y, 450)) fail(`the offset lane shifts the final camera, not the pose: pose ${cam.pose().y}, camera ${cam.camera().y}`)
  let [sx, sy] = projectCamera(cam.camera(), 400, 300)
  if (!near(sx, 400) || !near(sy, 150)) fail(`the pose point shows a quarter of the height above the pivot, got ${sx},${sy}`)
  // Under a world the view fills, the FINAL camera is what the bounds
  // contain: the pose sits the lane back from the centered camera.
  let bounded = make({ world: { width: 800, height: 600 }, zoom: 1, offset: [0, -0.25] })
  if (!near(bounded.cam.camera().y, 300) || !near(bounded.cam.pose().y, 150)) fail(`bounds contain pose plus lanes: camera ${bounded.cam.camera().y}, pose ${bounded.cam.pose().y}`)
  // A shake at the world's left edge: the pose never changes, the camera
  // moves inward only (the bounds clip the outward half), and it never
  // shows the outside.
  let edge = make({ world: { width: 1000, height: 500 }, zoom: 2, x: 200, y: 250 })
  edge.cam.shake(0.1, 0.5, { direction: [1, 0] })
  flush()
  if (!edge.cam.active()) fail("a shake wakes active()")
  let moved = false
  let ticks = 0
  for (; ticks < 100; ticks++) {
    if (!edge.cam.update(DT)) break
    let c = edge.cam.camera()
    if (c.x < 200 - 1e-9) fail(`a shake never shows the outside of the world, camera x ${c.x} at tick ${ticks}`)
    if (edge.cam.pose().x !== 200) fail(`a shake at the edge never enters the pose, pose x ${edge.cam.pose().x} at tick ${ticks}`)
    if (c.x > 200 + 1e-9) moved = true
  }
  if (!moved) fail("a shake displaces the camera from the pose")
  if (ticks >= 100) fail("a shake ends")
  flush()
  if (edge.cam.active()) fail("an ended shake rests")
  if (!near(edge.cam.camera().x, edge.cam.pose().x)) fail("after the shake the camera is the pose again")
  // Away from the edge the pose is untouched throughout.
  let mid = make({ world: { width: 1000, height: 500 }, zoom: 2, x: 500, y: 250 })
  mid.cam.shake(0.05, 0.3, { direction: [0, 1] })
  for (let i = 0; i < 40; i++) mid.cam.update(DT)
  if (mid.cam.pose().x !== 500 || mid.cam.pose().y !== 250) fail(`a shake inside the world leaves the pose alone: ${JSON.stringify(mid.cam.pose())}`)
}

// ---- Damped bounds: a fling eases into the limit; a direct write clamps at once ----
{
  let { cam } = make({ world: { width: 1000, height: 500, damping: 1 }, zoom: 2, x: 300, y: 250 })
  cam.release([4000, 0])
  let crossed = false
  for (let i = 0; i < SETTLE_TICKS; i++) {
    if (!cam.update(DT)) break
    if (cam.camera().x < 200 - 1e-9) crossed = true
  }
  if (!crossed) fail("a damped bound lets a fling cross the limit briefly")
  if (!near(cam.camera().x, 200, 1e-3)) fail(`a damped bound settles on the limit, got ${cam.camera().x}`)
  flush()
  if (cam.active()) fail("a settled damped bound rests")
  cam.panBy(400, 0)
  if (cam.camera().x !== 200) fail(`a direct write clamps at once even with damped bounds, got ${cam.camera().x}`)
  let hard = make({ world: { width: 1000, height: 500 }, zoom: 2, x: 300, y: 250 })
  hard.cam.release([4000, 0])
  for (let i = 0; i < SETTLE_TICKS; i++) {
    if (!hard.cam.update(DT)) break
    if (hard.cam.camera().x < 200 - 1e-9) fail("undamped bounds never show the outside")
  }
}

// ---- A pose glide inherits a pending anchor glide's zoom (a double tap that also glides) ----
{
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, x: 400, y: 300 })
  cam.zoomAt(200, 150, 2, { glide: true })
  cam.glideTo(100, 100)
  settle(cam)
  if (cam.camera().zoom !== 2 || !near(cam.camera().x, 100)) fail(`glideTo keeps a pending zoom's target: ${JSON.stringify(cam.camera())}`)
  cam.zoomAt(200, 150, 2, { glide: true })
  cam.glideTo(100, 100, 3)
  settle(cam)
  if (cam.camera().zoom !== 3) fail(`an explicit glideTo zoom wins, got ${cam.camera().zoom}`)
}

// ---- A rotation glide: eased, exact landing ----
{
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, zoom: 1, x: 400, y: 300 })
  cam.glideTo(400, 300, 1, Math.PI / 2)
  cam.update(DT)
  let r = cam.camera().rotation
  if (!(r > 0 && r < Math.PI / 2)) fail(`a rotation glide eases, got ${r} after one tick`)
  settle(cam)
  if (cam.camera().rotation !== Math.PI / 2) fail(`a rotation glide lands exactly, got ${cam.camera().rotation}`)
}

// ---- Inertia: a flick keeps gliding and decays to rest; a rested or disabled release does not ----
{
  // A drag of ten frames at 20 px, then the lift with the velocity the
  // gesture measured (the recognizer's, not the camera's: it has no
  // estimator).
  let drag = (cam: Camera2d, perTick: number, velocity: [number, number]) => {
    for (let i = 0; i < 10; i++) {
      cam.panBy(perTick, 0)
      cam.update(DT)
    }
    cam.release(velocity)
  }
  let rests = (cam: Camera2d) => settle(cam) === 0
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, x: 0, y: 0, zoom: 1 })
  drag(cam, 20, [1200, 0])
  let atRelease = cam.camera().x
  let ticks = settle(cam)
  let travelled = atRelease - cam.camera().x
  // 1200 px/s at 3 e-foldings/s: the fling covers ~400 px.
  if (ticks <= 1 || ticks > SETTLE_TICKS) fail(`a flick flings and then rests, ticks=${ticks}`)
  if (!(travelled > 300 && travelled < 450)) fail(`fling distance ~400 px, got ${travelled}`)
  drag(cam, 0.5, [0, 0])
  if (!rests(cam)) fail("a release at rest (the recognizer read no fling) does not fling")
  let still = make({ minZoom: 0.01, maxZoom: 100, inertia: false })
  drag(still.cam, 20, [1200, 0])
  if (!rests(still.cam)) fail("inertia: false never flings")
  // A press landing on a fling stops it.
  drag(cam, 20, [1200, 0])
  cam.update(DT)
  cam.interrupt()
  if (cam.update(DT)) fail("interrupt() stops a fling")
  // Following swallows the release.
  cam.follow(0, 0)
  settle(cam)
  drag(cam, 20, [1200, 0])
  settle(cam)
  if (!near(cam.camera().x, 0, 1e-3)) fail(`a release while following eases back instead of flinging, got x ${cam.camera().x}`)
}

// ---- Pivot at the top-left: the scrolling camera ----
{
  let { cam } = make({ world: { width: 1000, height: 500 }, zoom: 1, pivot: { x: 0, y: 0 } })
  cam.set({ x: -50, y: 0 })
  let c = cam.camera()
  if (!near(c.x!, 0) || !near(c.y!, -50)) fail(`top-left pivot: x clamps to 0 and the short axis centers at -50, got ${c.x},${c.y}`)
  cam.glideTo(100, 100)
  settle(cam)
  c = cam.camera()
  if (c.x !== 100 || !near(c.y!, -50)) fail(`glideTo under a top-left pivot lands x=100, y=-50, got ${c.x},${c.y}`)
}

// ---- Validation ----
{
  let throws = (what: string, f: () => void) => {
    try {
      f()
      fail(`${what} must throw`)
    } catch (err) {
      if (!(err instanceof Error) || !err.message.startsWith("createCamera2d")) fail(`${what}: unexpected error ${err}`)
    }
  }
  throws("world.width 0", () => make({ world: { width: 0, height: 10 } }))
  throws("minZoom > maxZoom", () => make({ minZoom: 3, maxZoom: 2 }))
  throws("deadZone 2", () => make({ follow: { deadZone: { width: 2, height: 0 } } }))
  throws("hardLimits -1", () => make({ follow: { hardLimits: { width: -1, height: 0 } } }))
  throws("offset NaN", () => make({ offset: [NaN, 0] }))
  throws("world.damping -1", () => make({ world: { width: 10, height: 10, damping: -1 } }))
  throws("shake duration 0", () => make().cam.shake(0.1, 0))
  throws("zoom 0", () => make({ zoom: 0 }))
  throws("set NaN", () => make().cam.set({ x: NaN }))
  throws("zoomAt factor 0", () => make().cam.zoomAt(0, 0, 0))
  throws("release without a pair", () => make().cam.release(5 as never))
  throws("fit without world or rect", () => make().cam.fit())
}

// ---- The axes: brackets, bracketed vs unbracketed zoom, rates ----
{
  let { cam, view } = make({ minZoom: 0.01, maxZoom: 100, x: 400, y: 300, zoom: 1 })
  // A pan gesture: its begin stops a glide, its deltas are viewport heights
  // of content travel (the world follows the finger, so the camera point
  // moves the other way), its end flings.
  cam.glideTo(900, 900)
  cam.axes.begin("pan")
  cam.update(DT)
  if (cam.camera().x !== 400) fail(`a pan begin stops the glide, x=${cam.camera().x}`)
  cam.axes.nudge("pan", [0.1, 0])
  if (!near(cam.camera().x, 400 - 0.1 * view.height)) fail(`a pan delta of 0.1 heights slides the camera 60 px, got ${cam.camera().x}`)
  for (let i = 0; i < 5; i++) {
    cam.axes.nudge("pan", [0.05, 0])
    cam.update(DT)
  }
  let beforeRelease = cam.camera().x
  // An end without a velocity rests; one with a velocity (viewport
  // heights per second of finger travel) flings the content that way.
  cam.axes.end("pan")
  if (settle(cam) !== 0) fail("a pan end without velocity rests")
  cam.axes.begin("pan")
  cam.axes.end("pan", [2, 0])
  cam.update(DT)
  if (settle(cam) === 0 || cam.camera().x >= beforeRelease) fail("a pan end flings with the release velocity")
  // A bracketed zoom delta (a pinch) applies at once about its focal; an
  // unbracketed one (a wheel notch) eases there.
  cam.set({ x: 400, y: 300, zoom: 1 })
  cam.axes.begin("zoom")
  cam.axes.nudge("zoom", 1, [0.5, 0.5])
  cam.axes.end("zoom")
  if (cam.camera().zoom !== 2) fail(`a bracketed zoom delta of one octave doubles at once, got ${cam.camera().zoom}`)
  cam.axes.nudge("zoom", 1, [0.5, 0.5])
  if (cam.camera().zoom !== 2) fail(`an unbracketed zoom delta glides, not snaps, got ${cam.camera().zoom}`)
  settle(cam)
  if (!near(cam.camera().zoom, 4, 1e-9)) fail(`the unbracketed zoom lands at 4, got ${cam.camera().zoom}`)
  // A roll delta in turns.
  cam.axes.nudge("roll", 0.25)
  if (!near(cam.camera().rotation, Math.PI / 2)) fail(`a roll delta of a quarter turn, got ${cam.camera().rotation}`)
  // Rates: a full deflection slides one viewport height per second and
  // wakes active(); removing the source rests it.
  cam.set({ x: 400, y: 300, zoom: 1, rotation: 0 })
  settle(cam)
  if (cam.active()) fail("a resting camera is not active")
  let remove = cam.axes.add("pan", () => [1, 0])
  flush()
  if (!cam.active()) fail("a rate source wakes active()")
  cam.update(0.5)
  if (!near(cam.camera().x, 400 - 0.5 * view.height)) fail(`a pan rate of 1 over half a second slides half a height, got ${cam.camera().x}`)
  remove()
  flush()
  if (cam.active()) fail("removing the rate source rests active()")
  let stopZoom = cam.axes.add("zoom", () => 1)
  cam.update(1)
  stopZoom()
  if (!near(cam.camera().zoom, 2, 1e-9)) fail(`a zoom rate of 1 doubles per second, got ${cam.camera().zoom}`)
}

// ---- Shots: the zoom blends in log space; a shot's control drives the view through the blender ----
{
  let a = { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 }
  let b = { x: 100, y: 50, zoom: 4, rotation: 1, pivotX: 10, pivotY: 20 }
  let m = mixCamera2d(a, b, 0.5)
  if (!near(m.zoom, 2) || !near(m.x, 50) || !near(m.rotation, 0.5) || !near(m.pivotX, 5)) fail(`mixCamera2d: zoom in log space, the rest linear, got ${JSON.stringify(m)}`)
  let last = null as CameraUpdate | null
  let shots = createShots({ setCamera: u => (last = u) }, { blend: 0.25 })
  let wide = createCamera2d(shots.shot("wide"), { viewport: () => ({ width: 800, height: 600 }), zoom: 1, x: 400, y: 300 })
  let close = createCamera2d(shots.shot("close", { priority: 1 }), { viewport: () => ({ width: 800, height: 600 }), zoom: 4, x: 100, y: 100 })
  if (last !== null) fail("a shot's control pushes into the recorder, not the view, until live")
  shots.activate("wide")
  if (last === null || (last as CameraUpdate).zoom !== 1) fail("the live shot's camera reaches the view")
  wide.panBy(-100, 0)
  if (!near((last as CameraUpdate).x!, 500)) fail(`a live shot's control drives the view through the blender, got ${(last as CameraUpdate).x}`)
  shots.activate("close")
  for (let i = 0; i < 8; i++) shots.update(DT)
  let z = (last as CameraUpdate).zoom!
  if (!(z > 1 && z < 4)) fail(`mid-blend zoom between the shots, got ${z}`)
  for (let i = 0; i < 20; i++) shots.update(DT)
  if ((last as CameraUpdate).zoom !== 4 || !near((last as CameraUpdate).x!, 100)) fail(`the blend lands on the close shot, got ${JSON.stringify(last)}`)
  void close
}

console.log(failures === 0 ? "CAMERA2D-OK" : `CAMERA2D-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} camera2d check(s) failed (seed ${seed})`)
