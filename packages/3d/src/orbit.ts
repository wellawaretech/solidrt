// An orbit camera for a scene: azimuth/elevation/distance around a target,
// with drag-to-rotate, wheel-to-zoom, optional auto-orbit, and clamps - the
// standard interactive-viewer camera, extracted so apps stop rebuilding it.
//
// Pose is plain mutable state advanced by update(dt) from the app's own
// onFrame; the control registers no frame loop of its own, so whether
// anything animates stays the app's decision (a paused orbit with no other
// animation costs nothing). Only `orbiting` is a signal - slow UI state
// that HUDs read - while the pose moves at frame rate and bypasses
// reactivity: the package's structure-vs-motion split. update() pushes the
// pose to the scene camera only when it actually changed, and reports
// that, so per-frame dependents (a uCamPos uniform) can follow the camera
// without writing every frame.

import { createSignal } from "@solidjs/signals"
import type { Scene } from "./scene.ts"
import type { Vec3 } from "./math.ts"

// Baseline sensitivities at rotateSpeed/zoomSpeed 1, in radians per dragged
// pixel and zoom exponent per wheel-delta unit.
const DRAG_AZIMUTH = 0.008
const DRAG_ELEVATION = 0.006
const WHEEL_ZOOM = 0.0015

export type OrbitCameraOptions = {
  /** The point the camera orbits and looks at (default origin). */
  target?: Vec3
  /** Initial pose, radians and world units. */
  azimuth?: number
  elevation?: number
  distance?: number
  minDistance?: number
  maxDistance?: number
  /** Elevation clamps, radians; the defaults stop just short of the poles. */
  minElevation?: number
  maxElevation?: number
  /** Auto-orbit rate in radians/second (default 0: none). Runs while
   * `orbiting()` and not dragging; toggle with set({ orbiting }). */
  orbitSpeed?: number
  /** Multipliers over the built-in drag/wheel sensitivities. */
  rotateSpeed?: number
  zoomSpeed?: number
}

export type OrbitPose = {
  azimuth?: number
  elevation?: number
  distance?: number
  target?: Vec3
  orbiting?: boolean
}

export type OrbitCamera = {
  /** Camera position for the current pose (a fresh array per call). */
  eye(): Vec3
  /** Pose snapshot - the shape debug commands return and set() takes. */
  pose(): { azimuth: number; elevation: number; distance: number }
  /** Merge a pose in (clamps apply); reaches the scene at the next
   * update(). Also the auto-orbit switch: set({ orbiting: false }). */
  set(pose: OrbitPose): void
  /** Whether the auto-orbit is running. Reactive (signal-backed), so HUD
   * text can read it. */
  orbiting(): boolean
  /** Advance the auto-orbit and push any pose change to the scene camera.
   * Call from onFrame with the frame's dt in seconds; returns whether the
   * pose changed. */
  update(dt: number): boolean
  /** Spread onto the element that receives input:
   * `<window {...orbit.handlers} />`. */
  handlers: {
    onPointerDown(e: { clientX: number; clientY: number }): void
    onPointerMove(e: { clientX: number; clientY: number }): void
    onPointerUp(): void
    onWheel(e: { deltaY: number }): void
  }
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/**
 * Create an orbit camera driving `scene`'s camera position and target (fov,
 * near, and far stay yours via scene.setCamera). The initial pose applies
 * immediately. In a component tree, reach the scene via `<Scene ref>` or
 * useScene() and hand the handlers to whichever element owns input.
 */
export function createOrbitCamera(scene: Scene, options: OrbitCameraOptions = {}): OrbitCamera {
  let target: Vec3 = options.target ? [options.target[0], options.target[1], options.target[2]] : [0, 0, 0]
  let azimuth = options.azimuth ?? 0
  let elevation = options.elevation ?? 0
  let distance = options.distance ?? 5
  let minDistance = options.minDistance ?? 0.01
  let maxDistance = options.maxDistance ?? Infinity
  let minElevation = options.minElevation ?? -1.55
  let maxElevation = options.maxElevation ?? 1.55
  let orbitSpeed = options.orbitSpeed ?? 0
  let dragAzimuth = DRAG_AZIMUTH * (options.rotateSpeed ?? 1)
  let dragElevation = DRAG_ELEVATION * (options.rotateSpeed ?? 1)
  let wheelZoom = WHEEL_ZOOM * (options.zoomSpeed ?? 1)

  let [orbiting, setOrbiting] = createSignal(orbitSpeed > 0)
  let drag: { x: number; y: number } | null = null
  let dirty = false

  let clampPose = () => {
    elevation = clampNum(elevation, minElevation, maxElevation)
    distance = clampNum(distance, minDistance, maxDistance)
  }
  let eye = (): Vec3 => {
    let ce = Math.cos(elevation)
    return [
      target[0] + distance * ce * Math.sin(azimuth),
      target[1] + distance * Math.sin(elevation),
      target[2] + distance * ce * Math.cos(azimuth),
    ]
  }
  let apply = () => scene.setCamera({ position: eye(), target })

  clampPose()
  apply()

  return {
    eye,
    pose: () => ({ azimuth, elevation, distance }),
    orbiting,
    set(pose) {
      if (pose.azimuth !== undefined) azimuth = pose.azimuth
      if (pose.elevation !== undefined) elevation = pose.elevation
      if (pose.distance !== undefined) distance = pose.distance
      if (pose.target) target = [pose.target[0], pose.target[1], pose.target[2]]
      clampPose()
      if (pose.orbiting !== undefined) setOrbiting(pose.orbiting)
      dirty = true
    },
    update(dt) {
      if (orbitSpeed !== 0 && drag === null && orbiting()) {
        azimuth += dt * orbitSpeed
        dirty = true
      }
      if (!dirty) return false
      dirty = false
      apply()
      return true
    },
    handlers: {
      onPointerDown(e) {
        drag = { x: e.clientX, y: e.clientY }
      },
      onPointerMove(e) {
        if (!drag) return
        azimuth -= (e.clientX - drag.x) * dragAzimuth
        elevation = clampNum(elevation + (e.clientY - drag.y) * dragElevation, minElevation, maxElevation)
        drag = { x: e.clientX, y: e.clientY }
        dirty = true
      },
      onPointerUp() {
        drag = null
      },
      onWheel(e) {
        distance = clampNum(distance * Math.exp(e.deltaY * wheelZoom), minDistance, maxDistance)
        dirty = true
      },
    },
  }
}
