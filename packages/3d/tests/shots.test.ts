// Tests for the 3d shots (shots.ts, over core's createShots): the camera
// mix (linear, a perspective-to-ortho switch at the midpoint, two ortho
// extents blended) and an orbit control driving a shot through the
// blender. Pure-module input only, so it runs headless on flux: `sol test
// packages/3d`. The 2d counterpart is packages/2d/tests/shots.test.ts.

import { test } from "flux:test"
import { createOrbitCamera } from "../src/orbit.ts"
import { createShots, mixCamera } from "../src/shots.ts"
import type { CameraState } from "../src/camera.ts"
import type { Vec3 } from "../src/math.ts"

function fail(msg: string): void {
  throw new Error(msg)
}
let near = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) <= eps
let nearV = (a: Vec3, b: Vec3, eps = 1e-9) => near(a[0], b[0], eps) && near(a[1], b[1], eps) && near(a[2], b[2], eps)
// The frame time a blend is stepped at.
const DT = 1 / 60

test("shots: the mix, and an orbit control driving a shot through the blender", () => {
  let a: CameraState = { position: [0, 0, 4], target: [0, 0, 0], up: [0, 1, 0], fov: 60, near: 0.1, far: 100, ortho: null }
  let b: CameraState = { position: [4, 0, 0], target: [1, 1, 1], up: [0, 1, 0], fov: 30, near: 0.1, far: 200, ortho: { left: -1, right: 1, top: 1, bottom: -1 } }
  let m = mixCamera(a, b, 0.25)
  if (!nearV(m.position, [1, 0, 3]) || !near(m.fov, 52.5) || m.ortho !== null) fail(`mixCamera: linear, perspective until the midpoint, got ${JSON.stringify(m)}`)
  if (mixCamera(a, b, 0.75).ortho !== b.ortho) fail("mixCamera switches to the ortho projection past the midpoint")
  let both = mixCamera(b, { ...b, ortho: { left: -3, right: 3, top: 3, bottom: -3 } }, 0.5)
  if (!near(both.ortho!.right, 2)) fail(`two ortho extents blend, got ${JSON.stringify(both.ortho)}`)
  // An orbit control drives a shot: its pushes reach the scene only when
  // the shot is live, through the blender; size() is the scene's.
  let last = null as CameraState | null
  let scene = { setCamera: (u: Partial<CameraState>) => (last = { ...a, ...u }), camera: () => a, size: () => ({ width: 800, height: 600 }) }
  let shots = createShots(scene, { blend: 0.25 })
  let wide = createOrbitCamera(shots.shot("wide"), { distance: 10, target: [0, 0, 0] })
  let close = createOrbitCamera(shots.shot("close", { priority: 1 }), { distance: 2, target: [1, 0, 0] })
  if (last !== null) fail("a shot's control pushes into the recorder until live")
  shots.activate("wide")
  if (last === null || !nearV((last as CameraState).position, [0, 0, 10])) fail("the live shot's camera reaches the scene")
  wide.panBy(1, 0)
  if (!near((last as CameraState).target[0], 1)) fail("a live shot's control drives the scene through the blender")
  shots.activate("close")
  for (let i = 0; i < 30; i++) shots.update(DT)
  if (!nearV((last as CameraState).position, [1, 0, 2], 1e-9)) fail(`the blend lands on the close shot, got ${(last as CameraState).position}`)
  void close
})
