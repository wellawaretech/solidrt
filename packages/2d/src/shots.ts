// Shots and blends over the 2d camera (okf/design/camera-controls.md):
// core's blender specialized to the layers' CameraState - a control
// drives a shot's recording target instead of the view, the blender
// owns the view's setCamera, and a switch blends x/y/rotation/pivot
// linearly and the zoom in log space (a zoom blend in linear space
// races in and crawls out).

import { createShotBlend } from "@solidrt/core/camera-control"
import type { ShotBlend, ShotBlendOptions, ShotTarget as RecordingTarget } from "@solidrt/core/camera-control"
import type { CameraState } from "./camera.ts"
import type { Camera2dTarget } from "./camera2d.ts"

// The camera every shot starts from: the layers' own defaults (world 0,0
// at the top-left, pixel for pixel).
const INITIAL: CameraState = { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 }

let lerp = (a: number, b: number, t: number) => a + (b - a) * t

/** Interpolate two 2d cameras: everything linear, the zoom in log space. */
export let mixCamera2d = (a: CameraState, b: CameraState, t: number): CameraState => ({
  x: lerp(a.x, b.x, t),
  y: lerp(a.y, b.y, t),
  zoom: a.zoom * Math.pow(b.zoom / a.zoom, t),
  rotation: lerp(a.rotation, b.rotation, t),
  pivotX: lerp(a.pivotX, b.pivotX, t),
  pivotY: lerp(a.pivotY, b.pivotY, t),
})

export type ShotsHandle = ShotBlend<CameraState>
export type ShotTarget = RecordingTarget<CameraState> & Camera2dTarget

/**
 * Shots over a view (or several: one output over a scene's layers):
 * `shot(name, { priority? })` returns the target a `createCamera2d` (or
 * a `<Camera2d>` through a custom viewport) drives; `activate(name, {
 * blend? })` makes it live when its priority wins, blending from the
 * current output; `update(dt)` from a frame loop while `active()`.
 */
export function createShots(target: Camera2dTarget | Camera2dTarget[], options: ShotBlendOptions = {}): ShotsHandle {
  let targets = Array.isArray(target) ? target : [target]
  for (let t of targets) if (!t || typeof t.setCamera !== "function") throw new Error("createShots: every target needs setCamera() (a view)")
  return createShotBlend<CameraState>(
    camera => {
      for (let t of targets) t.setCamera(camera)
    },
    mixCamera2d,
    INITIAL,
    options,
  )
}
