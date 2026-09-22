// Shots and blends over the 3d cameras (okf/design/camera-controls.md):
// core's blender specialized to the scene's CameraState - an orbit or
// first-person control drives a shot's recording target instead of the
// scene, the blender owns the scene's setCamera, and a switch blends
// position, target, up, fov, near and far linearly; two orthographic
// extents blend too, a perspective-to-ortho switch cuts at the midpoint
// (there is no in-between projection).

import { createShotBlend } from "@solidrt/core/camera-control"
import type { ShotBlend, ShotBlendOptions, ShotTarget as RecordingTarget } from "@solidrt/core/camera-control"
import type { CameraState, CameraUpdate } from "./camera.ts"
import type { Vec3 } from "./math.ts"

let lerp = (a: number, b: number, t: number) => a + (b - a) * t
let lerp3 = (a: Vec3, b: Vec3, t: number): Vec3 => [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]

/** Interpolate two scene cameras (see the header). */
export let mixCamera = (a: CameraState, b: CameraState, t: number): CameraState => ({
  position: lerp3(a.position, b.position, t),
  target: lerp3(a.target, b.target, t),
  up: lerp3(a.up, b.up, t),
  fov: lerp(a.fov, b.fov, t),
  near: lerp(a.near, b.near, t),
  far: lerp(a.far, b.far, t),
  ortho:
    a.ortho !== null && b.ortho !== null
      ? { left: lerp(a.ortho.left, b.ortho.left, t), right: lerp(a.ortho.right, b.ortho.right, t), top: lerp(a.ortho.top, b.ortho.top, t), bottom: lerp(a.ortho.bottom, b.ortho.bottom, t) }
      : t < 0.5
        ? a.ortho
        : b.ortho,
})

/** What shots drive: a Scene, or one of its Views. */
export type ShotsOwner = { setCamera(update: CameraUpdate): void; camera(): CameraState; size(): { width: number; height: number } }

/** A shot's target: the recording half plus the owner's size, so an
 * orbit or first-person control drives it exactly like the scene. */
export type ShotTarget = RecordingTarget<CameraState> & { size(): { width: number; height: number } }

export type ShotsHandle = Omit<ShotBlend<CameraState>, "shot"> & { shot(name: string, opts?: { priority?: number }): ShotTarget }

/**
 * Shots over a scene or a view: `shot(name, { priority? })` returns the
 * target a `createOrbitCamera` or `createFirstPersonCamera` drives (its
 * `camera()` is the shot's own, its `size()` the owner's); `activate(
 * name, { blend? })` makes it live when its priority wins, blending
 * from the current output; `update(dt)` from a frame loop while
 * `active()`. Every shot starts from the owner's camera at creation.
 */
export function createShots(owner: ShotsOwner, options: ShotBlendOptions = {}): ShotsHandle {
  if (!owner || typeof owner.setCamera !== "function" || typeof owner.camera !== "function" || typeof owner.size !== "function") {
    throw new Error("createShots: the owner needs setCamera(), camera() and size() (a Scene or a View)")
  }
  let initial = owner.camera()
  let blend = createShotBlend<CameraState>(camera => owner.setCamera(camera), mixCamera, initial, options)
  return {
    ...blend,
    shot(name, opts) {
      let rec = blend.shot(name, opts)
      return { setCamera: rec.setCamera, camera: rec.camera, size: () => owner.size() }
    },
  }
}
