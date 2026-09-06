// Checks for the 2d camera's motion (camera-motion.ts): the contain clamp
// and its centering, anchored zoom under any pivot and rotation, the eased
// glides (wheel, glideTo, fit) landing exactly, follow through a dead zone,
// drag inertia, Godot's limits-ignore-rotation rule, the deferred fit on an
// unknown viewport, and the validation throws - hand-written cases plus a
// seeded sweep. Pure-module input only (camera-motion.ts imports no GUI),
// so it runs headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/camera2d-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end, and the flux binary
// exits 1 on the uncaught throw, so a CI step can gate on the exit code.
// The input glue (createCamera2d's handlers over core's transform
// recognizer) needs the runtime's event bus and is exercised live by
// examples/camera.tsx.

import { argv } from "flux:process"
import { createCameraMotion } from "../src/camera-motion.ts"
import type { Camera2dMotion, Camera2dMotionOptions } from "../src/camera-motion.ts"
import { projectCamera } from "../src/camera.ts"
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

type Rig = { cam: Camera2dMotion; view: { width: number; height: number }; last: () => CameraUpdate | null; writes: () => number }

function make(opts: Partial<Camera2dMotionOptions> = {}, view = { width: 800, height: 600 }): Rig {
  let last: CameraUpdate | null = null
  let writes = 0
  let cam = createCameraMotion(
    {
      setCamera: (u) => {
        last = u
        writes++
      },
    },
    { viewport: () => view, ...opts },
  )
  return { cam, view, last: () => last, writes: () => writes }
}

// Step until update reports rest; returns the ticks taken (SETTLE_TICKS+1
// means it never rested).
function settle(cam: Camera2dMotion): number {
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
  if (!near(cam.camera().x!, 800)) fail(`set clamps x to the right edge (800), got ${cam.camera().x}`)
  cam.panBy(10000, 10000)
  let c = cam.camera()
  if (!near(c.x!, 200) || !near(c.y!, 150)) fail(`a huge pan lands on the top-left edge (200,150), got ${c.x},${c.y}`)
  cam.set({ y: -100 })
  if (!near(cam.camera().y!, 150)) fail(`set clamps y to the top edge (150), got ${cam.camera().y}`)
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
  let [wx, wy] = [cam.camera().x! + 80, cam.camera().y! - 40]
  let [ax, ay] = projectCamera(cam.camera(), wx, wy)
  cam.wheel(ax, ay, -400)
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
  cam.wheel(sx, sy, -100)
  cam.wheel(sx, sy, -100)
  settle(cam)
  if (!near(cam.camera().zoom!, target * Math.exp(200 * 0.0015), 1e-9)) fail(`two notches compound: got ${cam.camera().zoom}`)
  // A rotation write mid-glide leaves the glide running; an x write cancels it.
  cam.wheel(sx, sy, 100)
  cam.set({ rotation: 0.9 })
  let z = cam.camera().zoom!
  cam.update(DT)
  if (cam.camera().zoom === z) fail("set({ rotation }) must not cancel a glide")
  cam.wheel(sx, sy, 100)
  cam.set({ x: 310 })
  z = cam.camera().zoom!
  cam.update(DT)
  cam.update(DT)
  if (cam.camera().zoom !== z) fail("set({ x }) cancels a glide in flight")
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
  if (!near(cam.camera().zoom!, 0.8)) fail(`the deferred fit runs once the viewport is known, got zoom ${cam.camera().zoom}`)
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
  let { cam } = make({ world: { width: 1000, height: 500 }, zoom: 2, x: 500, y: 250, deadZone: { width: 0.5, height: 0.5 } })
  // Zone half-width 200 px; the target at screen x 800 overshoots by 200 px
  // = 100 world px, so the camera stops at 600 with the target on the edge.
  cam.follow(700, 250)
  settle(cam)
  let c = cam.camera()
  if (!near(c.x!, 600, 1e-3) || !near(c.y!, 250, 1e-3)) fail(`dead-zone follow parks the target on the zone edge (camera 600,250), got ${c.x},${c.y}`)
  cam.follow(650, 250)
  let moved = settle(cam)
  if (moved !== 0 || !near(cam.camera().x!, 600, 1e-3)) fail(`a target inside the dead zone does not move the camera, moved ${moved} ticks to ${cam.camera().x}`)
  cam.unfollow()
  cam.follow(100, 250)
  settle(cam)
  if (!near(cam.camera().x!, 200, 1e-3)) fail(`follow honors the world clamp (200), got ${cam.camera().x}`)
}

// ---- Inertia: a flick keeps gliding and decays to rest; slow or disabled releases do not ----
{
  let drag = (cam: Camera2dMotion, perTick: number, ticks: number) => {
    for (let i = 0; i < ticks; i++) {
      cam.panBy(perTick, 0)
      cam.update(DT)
    }
    cam.panBy(perTick, 0)
    cam.release()
  }
  // The release's own frame still flushes the last pan; rest means nothing
  // after that.
  let rests = (cam: Camera2dMotion) => {
    cam.update(DT)
    return settle(cam) === 0
  }
  let { cam } = make({ minZoom: 0.01, maxZoom: 100, x: 0, y: 0, zoom: 1 })
  drag(cam, 20, 10)
  let atRelease = cam.camera().x!
  let ticks = settle(cam)
  let travelled = atRelease - cam.camera().x!
  // 20 px per 1/60 s = 1200 px/s; at 3 e-foldings/s the fling covers ~400 px.
  if (ticks <= 1 || ticks > SETTLE_TICKS) fail(`a flick flings and then rests, ticks=${ticks}`)
  if (!(travelled > 300 && travelled < 450)) fail(`fling distance ~400 px, got ${travelled}`)
  drag(cam, 0.5, 10)
  if (!rests(cam)) fail("a slow release (30 px/s) does not fling")
  let still = make({ minZoom: 0.01, maxZoom: 100, inertia: false })
  drag(still.cam, 20, 10)
  if (!rests(still.cam)) fail("inertia: false never flings")
  // A press landing on a fling stops it.
  drag(cam, 20, 10)
  cam.update(DT)
  cam.interrupt()
  if (cam.update(DT)) fail("interrupt() stops a fling")
  // Following swallows the release.
  cam.follow(0, 0)
  settle(cam)
  drag(cam, 20, 10)
  settle(cam)
  if (!near(cam.camera().x!, 0, 1e-3)) fail(`a release while following eases back instead of flinging, got x ${cam.camera().x}`)
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
  throws("deadZone 2", () => make({ deadZone: { width: 2, height: 0 } }))
  throws("zoom 0", () => make({ zoom: 0 }))
  throws("set NaN", () => make().cam.set({ x: NaN }))
  throws("zoomAt factor 0", () => make().cam.zoomAt(0, 0, 0))
  throws("fit without world or rect", () => make().cam.fit())
}

console.log(failures === 0 ? "CAMERA2D-OK" : `CAMERA2D-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} camera2d check(s) failed (seed ${seed})`)
