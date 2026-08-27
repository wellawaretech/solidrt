// An orbit camera for a scene: azimuth/elevation/distance around a target,
// with drag-to-rotate, pinch- and wheel-to-zoom, optional auto-orbit, and
// clamps - the standard interactive-viewer camera, extracted so apps stop
// rebuilding it.
//
// Input goes through core's merged transform recognizer (createTransform), so
// the drag participates in the gesture arena: on a viewport embedded in a
// scrollable layout, an orbit drag and the ancestor scroller's pan arbitrate
// instead of both acting, and rotation only starts once the drag crosses the
// recognizer's slop. One finger (or a mouse drag) rotates; two-finger pinch
// zooms (pair rotation is ignored); wheel zooms directly - no recognizer
// needed for a discrete wheel step. What two-finger translation means
// depends on `viewport`: with it, two fingers pan - the scene slides under
// the fingers 1:1 at the target's depth, the touch convention everywhere
// (three.js DOLLY_PAN, Sketchfab, touch CAD) - which also keeps an
// imperfect pinch from smearing rotation into the zoom. Without it there
// is no pixel-to-world mapping, so focal translation falls back to
// rotating regardless of finger count.
//
// Zoom aims at the target by default. With `zoomAnchor` the app maps the
// pinch focal / wheel cursor to a world point, and the zoom scales the pose
// about that point instead - the spot under the fingers stays under the
// fingers, with the target sliding toward it. Only the app can own that
// mapping: screen-to-ray needs the projection and the element's placement
// (fov, aspect, viewBox scaling), which live in the app's camera and layout,
// not here.
//
// Anchored zoom leaves the target wherever the zoom carried it - possibly a
// point in empty air near the eye, and a drag orbiting THAT swings the scene
// wildly around nothing. `rotateAnchor` is the countermeasure: at gesture
// start the app names the world point rotation should pivot about (what the
// camera is actually looking at), and the pivot is re-seated there. The
// point is projected onto the view axis first, so re-seating moves only the
// target's depth - the eye and the picture do not change, only what a drag
// swings around.
//
// Pose is plain mutable state advanced by update(dt) from the app's own
// onFrame; the control registers no frame loop of its own, so whether
// anything animates stays the app's decision (a paused orbit with no other
// animation costs nothing). Only `orbiting` is a signal - slow UI state
// that HUDs read - while the pose moves at frame rate and bypasses
// reactivity: the package's structure-vs-motion split. update() pushes the
// pose to the scene camera only when it actually changed (one setCamera:
// the scene's own shared write carries uViewProj and uCamPos), and reports
// that, so per-frame dependents (reprojecting HUD overlays via
// scene.project) can follow the camera without recomputing every frame.

import { createSignal } from "@solidjs/signals"
import { createTransform } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"
import type { CameraUpdate } from "./scene.ts"
import type { Vec3 } from "./math.ts"

// Baseline sensitivities at rotateSpeed/zoomSpeed 1, in radians per dragged
// pixel and zoom exponent per wheel-delta unit.
const DRAG_AZIMUTH = 0.008
const DRAG_ELEVATION = 0.006
const WHEEL_ZOOM = 0.0015

/** What an orbit camera drives: a Scene, or one of its Views. */
export type OrbitTarget = { setCamera(update: CameraUpdate): void }

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
   * `orbiting()` and no drag/pinch is in progress; toggle with
   * set({ orbiting }). */
  orbitSpeed?: number
  /** Multipliers over the built-in drag and zoom (wheel + pinch) sensitivities. */
  rotateSpeed?: number
  zoomSpeed?: number
  /** Multiplier over the two-finger pan (1 = the scene tracks the fingers
   * exactly at the target's depth). */
  panSpeed?: number
  /** The viewport the pointer coordinates live in: height in the same
   * logical units as clientX/clientY, and the camera's vertical fov in
   * degrees. Providing it enables two-finger pan; without it two-finger
   * translation rotates, as it always did. */
  viewport?: () => { height: number; fov: number }
  /** Constrain where a pan may put the target - return the target to use.
   * The typical use: keep the pivot within a few radii of the subject so
   * panning cannot strand the camera. Zoom and rotation do not consult it. */
  clampTarget?: (target: Vec3) => Vec3
  /** Map a screen point (window coordinates, the clientX/clientY frame) to
   * the world point a zoom there should keep pinned. Called once per pinch
   * gesture (at its first span change - the anchor then holds for the whole
   * gesture, see the jitter note at `pinchAnchor`) and once per wheel event,
   * with the pose the zoom is about to apply to. Return null to zoom toward
   * the target as usual (also the default). */
  zoomAnchor?: (x: number, y: number, view: { eye: Vec3; target: Vec3 }) => Vec3 | null
  /** The world point rotation should pivot about - called when a drag or
   * pinch starts. It is projected onto the view axis and becomes the new
   * target, preserving the picture exactly (only the pivot's depth moves),
   * so a drag after an anchored zoom orbits the scene under the camera
   * instead of wherever the zoom left the target. Points at or behind the
   * eye are ignored, as is null (both keep the current pivot). */
  rotateAnchor?: (view: { eye: Vec3; target: Vec3 }) => Vec3 | null
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
  /** Advance the auto-orbit and push any pose change to the driven camera.
   * Call from onFrame with the frame's dt in seconds; returns whether the
   * pose changed. */
  update(dt: number): boolean
  /** Spread onto the element that receives input:
   * `<window {...orbit.handlers} />`. */
  handlers: {
    onPointerDown(e: PointerEvent): void
    onPointerMove(e: PointerEvent): void
    onPointerUp(e: PointerEvent): void
    /** Position is optional so a bare `{ deltaY }` still zooms; without it
     * a zoomAnchor cannot apply and the zoom falls back to the target. */
    onWheel(e: { deltaY: number; clientX?: number; clientY?: number }): void
  }
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/**
 * Create an orbit camera driving `camera`'s position and target, where
 * `camera` is a Scene or one of its Views (fov, near, and far stay yours via
 * its setCamera). The initial pose applies immediately. In a component tree,
 * reach the scene via `<Scene ref>` or useScene() (a view via `<View ref>`)
 * and hand the handlers to whichever element owns input.
 */
export function createOrbitCamera(camera: OrbitTarget, options: OrbitCameraOptions = {}): OrbitCamera {
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
  let interacting = false
  let dirty = false

  let clampPose = () => {
    elevation = clampNum(elevation, minElevation, maxElevation)
    distance = clampNum(distance, minDistance, maxDistance)
  }
  // Slide eye and target together along the camera's right/up so the scene
  // tracks the fingers: dragged pixels map to world units through the
  // frustum height at the target's depth. Screen +y is down, so the up-axis
  // term is added (fingers down -> camera up -> scene follows down).
  let pan = (dx: number, dy: number) => {
    let vp = options.viewport!()
    let wpp = ((2 * Math.tan((vp.fov * Math.PI) / 360) * distance) / vp.height) * (options.panSpeed ?? 1)
    let sa = Math.sin(azimuth)
    let ca = Math.cos(azimuth)
    let se = Math.sin(elevation)
    let ce = Math.cos(elevation)
    // right = (ca, 0, -sa), up = (-sa*se, ce, -ca*se) for this pose - the
    // camera basis written out for eye() = target + distance * (ce*sa, se, ce*ca).
    let next: Vec3 = [
      target[0] - dx * wpp * ca - dy * wpp * sa * se,
      target[1] + dy * wpp * ce,
      target[2] + dx * wpp * sa - dy * wpp * ca * se,
    ]
    target = options.clampTarget ? options.clampTarget(next) : next
  }
  let eye = (): Vec3 => {
    let ce = Math.cos(elevation)
    return [
      target[0] + distance * ce * Math.sin(azimuth),
      target[1] + distance * Math.sin(elevation),
      target[2] + distance * ce * Math.cos(azimuth),
    ]
  }
  let apply = () => camera.setCamera({ position: eye(), target })

  // Zoom by `ratio` (new distance over old, before clamping) about a world
  // anchor (null zooms toward the target). Scaling eye and target about the
  // anchor by the distance ratio keeps it projecting to the same pixel, so
  // the shift below uses the ratio that actually applied after clamping -
  // once distance pins at a clamp the target stops moving too, instead of
  // sliding the view sideways under a dead zoom.
  let zoomAbout = (ratio: number, anchor: Vec3 | null | undefined) => {
    let prev = distance
    // Bounds widen to the current distance so a pose already outside them
    // (a rotateAnchor re-seat may land anywhere) zooms back toward range
    // instead of snap-jumping into it.
    distance = clampNum(distance * ratio, Math.min(minDistance, prev), Math.max(maxDistance, prev))
    if (!anchor) return
    let s = distance / prev
    target = [
      anchor[0] + (target[0] - anchor[0]) * s,
      anchor[1] + (target[1] - anchor[1]) * s,
      anchor[2] + (target[2] - anchor[2]) * s,
    ]
  }
  let anchorAt = (x?: number, y?: number) =>
    x !== undefined && y !== undefined && options.zoomAnchor
      ? options.zoomAnchor(x, y, { eye: eye(), target: [target[0], target[1], target[2]] })
      : null

  // The pinch keeps ONE anchor for its whole gesture, taken at the first
  // span change. Per-event re-derivation looks equivalent but jitters in
  // practice: the two fingers' events interleave, so the measured span
  // oscillates around its true value even while the fingers rest, and every
  // event zooms slightly in or back out. About a fixed point those pairs
  // cancel exactly (the ratios telescope); about a fresh anchor each time -
  // a new raycast against a pose the previous event just moved, or the
  // app's hit/fallback choice flipping between events - each pair leaves a
  // residual target slide and the model visibly crawls under resting
  // fingers. The wheel stays per-event: its notches are discrete, there is
  // no noise to cancel, and each notch re-aiming at the cursor is the point.
  let pinchAnchor: Vec3 | null = null
  let pinchSeen = false

  clampPose()
  apply()

  // One finger rotates. With two down, the span change zooms and the focal
  // translation pans (or, with no viewport to map pixels through, keeps
  // rotating). Pair rotation has no orbit meaning and is ignored.
  let transform = createTransform({
    onTransformStart: () => {
      interacting = true
      let anchor = options.rotateAnchor?.({ eye: eye(), target: [target[0], target[1], target[2]] })
      if (anchor) {
        let e = eye()
        let fx = (target[0] - e[0]) / distance
        let fy = (target[1] - e[1]) / distance
        let fz = (target[2] - e[2]) / distance
        let depth = (anchor[0] - e[0]) * fx + (anchor[1] - e[1]) * fy + (anchor[2] - e[2]) * fz
        if (depth > 0) {
          // Deliberately unclamped: the pose is unchanged, this only decides
          // what the gesture pivots about. The zoom clamps below widen to
          // the current distance, so a pivot outside [min, max] cannot make
          // the next zoom snap-jump either.
          distance = depth
          target = [e[0] + fx * depth, e[1] + fy * depth, e[2] + fz * depth]
        }
      }
    },
    onTransformMove: (t) => {
      if (t.pointers >= 2 && options.viewport) {
        pan(t.dx, t.dy)
      } else {
        azimuth -= t.dx * dragAzimuth
        elevation = clampNum(elevation + t.dy * dragElevation, minElevation, maxElevation)
      }
      if (t.scale !== 1) {
        // Fingers spreading (scale > 1) zooms in: the distance shrinks by the
        // span ratio, exponent-weighted by the zoomSpeed multiplier, about
        // the gesture's anchor when the app provides one.
        if (!pinchSeen) {
          pinchSeen = true
          pinchAnchor = anchorAt(t.x, t.y)
        }
        zoomAbout(1 / Math.pow(t.scale, options.zoomSpeed ?? 1), pinchAnchor)
      }
      dirty = true
    },
    onTransformEnd: () => {
      interacting = false
      pinchSeen = false
      pinchAnchor = null
    },
  })

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
      if (orbitSpeed !== 0 && !interacting && orbiting()) {
        azimuth += dt * orbitSpeed
        dirty = true
      }
      if (!dirty) return false
      dirty = false
      apply()
      return true
    },
    handlers: {
      ...transform.handlers,
      onWheel(e) {
        zoomAbout(Math.exp(e.deltaY * wheelZoom), anchorAt(e.clientX, e.clientY))
        dirty = true
      },
    },
  }
}
