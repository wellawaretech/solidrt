// The camera record: one per scene target and one per view (the shadow
// views included), the same state and the same one-shared-write contract
// everywhere. Split out of scene.ts so the shadow system can place its
// map cameras through the exact code the scene uses for its own.

import { invert, lookAt as lookAtMatrix, mat4, multiply, orthographic, perspective } from "./math.ts"
import type { Mat4, Vec3 } from "./math.ts"
import type { ShaderParams } from "@solidrt/core/gpu"

/** An orthographic projection's view-space extents, in world units (the
 * same box at every depth). */
export type OrthoExtent = { left: number; right: number; top: number; bottom: number }

/** A camera snapshot (Scene/View `camera()`): CameraUpdate's fields, all
 * present. Arrays are copies of the internal state. */
export type CameraState = {
  fov: number
  near: number
  far: number
  position: Vec3
  target: Vec3
  up: Vec3
  ortho: OrthoExtent | null
}

export type CameraUpdate = {
  /** Vertical field of view in DEGREES (default 60). */
  fov?: number
  near?: number
  far?: number
  position?: Vec3
  target?: Vec3
  up?: Vec3
  /** An orthographic projection with these extents (`fov` is then
   * ignored); null returns to perspective. Three's OrthographicCamera as
   * a camera option: a top-down map, an isometric view, a shadow-map
   * light. */
  ortho?: OrthoExtent | null
}

// `dirty` = the matrices need recomputing (a setCamera or a resize),
// `pending` = the GPU write is owed to the next sync. The recompute is
// split from the sync so project()/viewProj() see a fresh matrix right
// after setCamera, before the microtask runs.
export type Camera = {
  fov: number
  near: number
  far: number
  eye: Vec3
  target: Vec3
  up: Vec3
  ortho: OrthoExtent | null
  dirty: boolean
  pending: boolean
  proj: Mat4
  view: Mat4
  viewProj: Mat4
  invViewProj: Mat4
}

export function makeCamera(): Camera {
  return {
    fov: 60,
    near: 0.1,
    far: 100,
    eye: [0, 0, 3],
    target: [0, 0, 0],
    up: [0, 1, 0],
    ortho: null,
    dirty: true,
    pending: false,
    proj: mat4(),
    view: mat4(),
    viewProj: mat4(),
    invViewProj: mat4(),
  }
}

export function cameraState(cam: Camera): CameraState {
  return {
    fov: cam.fov,
    near: cam.near,
    far: cam.far,
    position: [cam.eye[0], cam.eye[1], cam.eye[2]],
    target: [cam.target[0], cam.target[1], cam.target[2]],
    up: [cam.up[0], cam.up[1], cam.up[2]],
    ortho: cam.ortho === null ? null : { left: cam.ortho.left, right: cam.ortho.right, top: cam.ortho.top, bottom: cam.ortho.bottom },
  }
}

export function updateCamera(cam: Camera, update: CameraUpdate): void {
  if (update.fov !== undefined) cam.fov = update.fov
  if (update.near !== undefined) cam.near = update.near
  if (update.far !== undefined) cam.far = update.far
  if (update.position) cam.eye = [update.position[0], update.position[1], update.position[2]]
  if (update.target) cam.target = [update.target[0], update.target[1], update.target[2]]
  if (update.up) cam.up = [update.up[0], update.up[1], update.up[2]]
  if (update.ortho !== undefined) {
    let o = update.ortho
    cam.ortho = o === null ? null : { left: o.left, right: o.right, top: o.top, bottom: o.bottom }
  }
  cam.dirty = true
}

export function ensureCamera(cam: Camera, width: number, height: number): void {
  if (!cam.dirty) return
  cam.dirty = false
  cam.pending = true
  let o = cam.ortho
  if (o === null) perspective(cam.proj, (cam.fov * Math.PI) / 180, width / height, cam.near, cam.far)
  else orthographic(cam.proj, o.left, o.right, o.top, o.bottom, cam.near, cam.far)
  lookAtMatrix(cam.view, cam.eye, cam.target, cam.up)
  multiply(cam.viewProj, cam.proj, cam.view)
  invert(cam.invViewProj, cam.viewProj)
}

// The camera is target state: one shared write, whatever the target holds.
// Entries are untouched - uModel is camera-independent, and uCamPos is
// stored even when no current material declares it. The camera basis rides
// along: the view matrix's first two rows are the camera's world-space
// right and up (no clip flip - that lives in the projection), so a
// billboard needs no reconstruction from uViewProj. The inverse rides too:
// uInvViewProj carries a clip position back to world, which is how the
// background slot (and any shader declaring it) gets a world-space ray
// per pixel without knowing the projection.
export function cameraParams(cam: Camera): ShaderParams {
  let v = cam.view
  return {
    uViewProj: cam.viewProj,
    uInvViewProj: cam.invViewProj,
    uCamPos: cam.eye,
    uCamRight: [v[0], v[4], v[8]],
    uCamUp: [v[1], v[5], v[9]],
  }
}
