// Tests for the 2d shots (shots.ts, over core's createShots): the camera
// mix (the zoom in log space, the rest linear), a shot's control driving
// the view through the blender, a control-less shot holding the view's
// camera, and the list form (one output over several layers' views).
// Pure-module input only, so it runs headless on flux: `sol test
// packages/2d`. The 3d counterpart is packages/3d/tests/shots.test.ts.

import { test } from "flux:test"
import { createCamera2d } from "../src/camera2d.ts"
import { createShots, mixCamera } from "../src/shots.ts"
import type { CameraUpdate } from "../src/camera.ts"

function fail(msg: string): void {
  throw new Error(msg)
}

// Absolute tolerance for coordinates.
const EPS = 1e-6
let near = (a: number, b: number, eps = EPS) => Math.abs(a - b) <= eps
// The frame time a blend is stepped at.
const DT = 1 / 60

test("shots: the zoom blends in log space; a shot's control drives the view through the blender", () => {
  let a = { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 }
  let b = { x: 100, y: 50, zoom: 4, rotation: 1, pivotX: 10, pivotY: 20 }
  let m = mixCamera(a, b, 0.5)
  if (!near(m.zoom, 2) || !near(m.x, 50) || !near(m.rotation, 0.5) || !near(m.pivotX, 5)) fail(`mixCamera: zoom in log space, the rest linear, got ${JSON.stringify(m)}`)
  let last = null as CameraUpdate | null
  let shots = createShots({ setCamera: u => (last = u), camera: () => a, size: () => ({ width: 800, height: 600 }) }, { blend: 0.25 })
  let wide = createCamera2d(shots.shot("wide"), { zoom: 1, x: 400, y: 300 })
  let close = createCamera2d(shots.shot("close", { priority: 1 }), { zoom: 4, x: 100, y: 100 })
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
})

test("shots: a shot no control drives holds the view's camera at creation (the 3d owner rule)", () => {
  let view = { x: 120, y: 80, zoom: 2, rotation: 0.5, pivotX: 400, pivotY: 300 }
  let last = null as CameraUpdate | null
  let shots = createShots({ setCamera: u => (last = u), camera: () => view, size: () => ({ width: 800, height: 600 }) })
  if (shots.camera().zoom !== 2) fail(`before any shot is live the blender reads the view's camera, got ${JSON.stringify(shots.camera())}`)
  shots.shot("hold")
  shots.activate("hold")
  if (last === null || (last as CameraUpdate).zoom !== 2 || !near((last as CameraUpdate).x!, 120)) fail(`a control-less shot holds the view, got ${JSON.stringify(last)}`)
  let threw = false
  try {
    createShots([])
  } catch {
    threw = true
  }
  if (!threw) fail("an empty target list throws")
})

test("shots: a list of views all receive the output; size is the first view's", () => {
  let view = { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 }
  let seen: string[] = []
  let target = (name: string, width: number) => ({ setCamera: (u: CameraUpdate) => seen.push(name + " " + u.zoom), camera: () => view, size: () => ({ width, height: 600 }) })
  let shots = createShots([target("ground", 800), target("sky", 400)])
  let shot = shots.shot("main")
  if (shot.size().width !== 800) fail(`a shot's size is the first view's, got ${shot.size().width}`)
  shots.activate("main")
  if (seen.join("; ") !== "ground 1; sky 1") fail(`every view receives the output, got ${seen.join("; ")}`)
})
