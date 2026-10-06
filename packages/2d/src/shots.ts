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
export let mixCamera = (a: CameraState, b: CameraState, t: number): CameraState => ({
  x: lerp(a.x, b.x, t),
  y: lerp(a.y, b.y, t),
  zoom: a.zoom * Math.pow(b.zoom / a.zoom, t),
  rotation: lerp(a.rotation, b.rotation, t),
  pivotX: lerp(a.pivotX, b.pivotX, t),
  pivotY: lerp(a.pivotY, b.pivotY, t),
})

/** A shot's target: the recording half plus the first view's size, so a
 * createCamera2d drives it exactly like the view. */
export type ShotTarget = RecordingTarget<CameraState> & Camera2dTarget

export type ShotsHandle = Omit<ShotBlend<CameraState>, "shot"> & { shot(name: string, opts?: { priority?: number }): ShotTarget }

/**
 * Shots over a view (or several: one output over a scene's layers):
 * `shot(name, { priority? })` returns the target a `createCamera2d`
 * drives (its `camera()` is the shot's own, its `size()` the first
 * view's); `activate(name, { blend? })` makes it live when its priority
 * wins, blending from the current output; `update(dt)` from a frame
 * loop while `active()`.
 */
export function createShots(target: Camera2dTarget | Camera2dTarget[], options: ShotBlendOptions = {}): ShotsHandle {
  let targets = Array.isArray(target) ? target : [target]
  for (let t of targets) {
    if (!t || typeof t.setCamera !== "function" || typeof t.size !== "function") throw new Error("createShots: every target needs setCamera() and size() (a view)")
  }
  let first = targets[0]!
  let blend = createShotBlend<CameraState>(
    camera => {
      for (let t of targets) t.setCamera(camera)
    },
    mixCamera,
    INITIAL,
    options,
  )
  return {
    ...blend,
    shot(name, opts) {
      let rec = blend.shot(name, opts)
      return { setCamera: rec.setCamera, camera: rec.camera, size: () => first.size() }
    },
  }
}
