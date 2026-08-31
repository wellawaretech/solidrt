// Differential check for the camera mapping (camera.ts): projectCamera
// against an oracle that spells the mapping the way <TileLayer> does with
// element transforms (origin at the camera point, rotate + scale there,
// translate the camera point onto the pivot), so the exported function and
// the component's view props cannot drift; unprojectCamera as the exact
// round-trip inverse; plus the documented conventions as hand-written
// cases. Pure-module input only (camera.ts imports no GUI), so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/camera-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end; the flux binary exits 0
// regardless, so read the output, not the exit code.

import { argv } from "flux:process"
import { checkCamera, projectCamera, unprojectCamera } from "../src/camera.ts"
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

// Absolute tolerance for comparing screen/world coordinates: inputs span
// a few thousand pixels, so float64 noise stays far below this.
const EPS = 1e-6

function close(a: [number, number], b: [number, number]): boolean {
  return Math.abs(a[0] - b[0]) <= EPS && Math.abs(a[1] - b[1]) <= EPS
}

// Oracle: the <view> props exactly as <TileLayer> in components.tsx sets
// them, applied with element-transform semantics (rotate + scale about the
// origin point, then the x/y translation). If either spelling changes, the
// sweep below catches the drift.
function viewOracle(camera: CameraUpdate, worldX: number, worldY: number): [number, number] {
  let originX = camera.x ?? 0
  let originY = camera.y ?? 0
  let rotate = camera.rotation ?? 0
  let scale = camera.zoom ?? 1
  let x = (camera.pivotX ?? 0) - originX
  let y = (camera.pivotY ?? 0) - originY
  let dx = worldX - originX
  let dy = worldY - originY
  let c = Math.cos(rotate)
  let sn = Math.sin(rotate)
  return [originX + (dx * c - dy * sn) * scale + x, originY + (dx * sn + dy * c) * scale + y]
}

// Hand-written cases: the documented conventions.
// Default camera is the identity mapping.
if (!close(projectCamera({}, 12, -3), [12, -3])) fail("default camera is not identity")
// Pivot (0, 0): the sprite-camera formula (world - cam) * zoom.
if (!close(projectCamera({ x: 10, y: 20, zoom: 2 }, 15, 26), [10, 12])) fail("pivot (0,0) is not (world - cam) * zoom")
// The camera's world point lands on the pivot at any rotation and zoom.
if (!close(projectCamera({ x: 7, y: 9, zoom: 3.7, rotation: 1.234, pivotX: 40, pivotY: 50 }, 7, 9), [40, 50])) {
  fail("camera world point does not land on the pivot")
}
// A quarter turn clockwise (y-down) carries screen-right onto screen-down.
if (!close(projectCamera({ rotation: Math.PI / 2 }, 1, 0), [0, 1])) fail("quarter turn is not clockwise y-down")
// The heading convention: rotation = -h - pi/2 renders a unit step along
// heading h (direction (cos h, sin h)) straight up from the pivot.
{
  let h = range(-3, 3)
  let cam: CameraUpdate = { x: 100, y: 200, zoom: 1.5, rotation: -h - Math.PI / 2, pivotX: 30, pivotY: 60 }
  let up = projectCamera(cam, 100 + Math.cos(h), 200 + Math.sin(h))
  if (!close(up, [30, 60 - 1.5])) fail(`heading convention: got (${up[0]}, ${up[1]}), want (30, ${60 - 1.5})`)
}

// checkCamera: every present field finite, zoom positive; empty and
// partial updates pass.
let throws = (update: CameraUpdate) => {
  try {
    checkCamera(update)
    return false
  } catch {
    return true
  }
}
if (throws({})) fail("checkCamera rejects an empty update")
if (throws({ x: -5, rotation: 0.5 })) fail("checkCamera rejects a valid partial update")
if (!throws({ rotation: NaN })) fail("checkCamera accepts NaN rotation")
if (!throws({ pivotX: Infinity })) fail("checkCamera accepts Infinity pivotX")
if (!throws({ zoom: 0 })) fail("checkCamera accepts zoom 0")
if (!throws({ zoom: -1 })) fail("checkCamera accepts negative zoom")
if (!throws({ zoom: NaN })) fail("checkCamera accepts NaN zoom")

// Randomized sweep: projectCamera vs the view-prop oracle, and the
// unproject round trip.
const SWEEPS = 20000
let checked = 0
for (let i = 0; i < SWEEPS; i++) {
  let camera: CameraUpdate = {
    x: range(-2000, 2000),
    y: range(-2000, 2000),
    zoom: range(0.1, 8),
    rotation: range(-7, 7),
    pivotX: range(-1000, 1000),
    pivotY: range(-1000, 1000),
  }
  // Absent keys read as defaults; drop some to sweep that path too.
  if (rand() < 0.2) delete camera.rotation
  if (rand() < 0.2) delete camera.zoom
  if (rand() < 0.2) {
    delete camera.pivotX
    delete camera.pivotY
  }
  let wx = range(-3000, 3000)
  let wy = range(-3000, 3000)
  let got = projectCamera(camera, wx, wy)
  let want = viewOracle(camera, wx, wy)
  if (!close(got, want)) {
    fail(`project mismatch at (${wx}, ${wy}) camera ${JSON.stringify(camera)}: got (${got[0]}, ${got[1]}), oracle (${want[0]}, ${want[1]})`)
  }
  let back = unprojectCamera(camera, got[0], got[1])
  if (!close(back, [wx, wy])) {
    fail(`round trip drifted at (${wx}, ${wy}) camera ${JSON.stringify(camera)}: back (${back[0]}, ${back[1]})`)
  }
  checked++
}

if (failures === 0) console.log(`PASS: ${checked} random cameras match the view-prop oracle and round-trip, conventions hold`)
else throw new Error(`${failures} FAILURES (seed ${seed})`)
