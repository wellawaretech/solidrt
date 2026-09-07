// An orbit camera for a scene: azimuth/elevation/distance around a target
// - the standard interactive-viewer camera, extracted so apps stop
// rebuilding it. The control is pure (ARCHITECTURE.md: controls through
// an abstraction, never direct event handling): it consumes three axes,
// `rotate` (vec2), `zoom` (axis) and `pan` (vec2), in device-free units,
// and exposes pose verbs for scripted moves. What drives the axes is the
// app's business - an input map with a pointer feed, a pad, a debug
// command - and the `orbitBindings` preset in input.ts is the standard
// wiring (drag rotates, pinch and wheel zoom, two fingers pan).
//
// Rotation is element-relative: one element height of drag travel (the
// feed's unit) sweeps DRAG_TURNS full turns on either axis, Three's
// OrbitControls convention, so the same drag feels the same on a phone
// and a 4k window. A rate on `rotate` (a stick) turns at ROTATE_RATE
// turns per second at full deflection. `zoom` is in octaves: a delta of
// 1 halves the distance, a rate of 1 halves it per second. `pan` slides
// the target so the scene tracks the fingers 1:1 at the target's depth,
// the touch convention everywhere (three.js DOLLY_PAN, Sketchfab, touch
// CAD), mapped through the target camera's fov - which is why the driven
// target must report its camera (a Scene or a View does).
//
// Zoom aims at the target by default. With `zoomAnchor` the app maps the
// gesture's focal point (a fraction of the element) to a world point,
// and the zoom scales the pose about that point instead - the spot under
// the fingers stays under the fingers, with the target sliding toward it.
// Only the app can own that mapping: focal-to-ray needs the projection,
// which lives in the app's scene (scene.unproject over the scene's size).
// A pinch keeps ONE anchor for its whole gesture, taken at its first
// delta: the two fingers' events interleave, so the measured span
// oscillates around its true value even while the fingers rest, and
// per-delta re-anchoring turns that noise into a visible crawl; about a
// fixed point the pairs cancel exactly. The wheel, unbracketed, anchors
// per notch: its notches are discrete, there is no noise to cancel, and
// each notch re-aiming at the cursor is the point.
//
// Anchored zoom leaves the target wherever the zoom carried it - possibly
// a point in empty air near the eye, and a drag orbiting THAT swings the
// scene wildly around nothing. `rotateAnchor` is the countermeasure: when
// a rotate gesture begins, the app names the world point rotation should
// pivot about (what the camera is actually looking at), and setPivot
// re-seats the target there, projected onto the view axis first, so only
// the target's depth moves - the eye and the picture do not change.
//
// Pose is plain mutable state; a nudge or a verb pushes it to the target
// at once (the next paint carries the new camera, no frame loop needed
// for a drag), and update(dt) integrates the rates and the auto-orbit
// from the app's own onFrame - the control registers no frame loop of its
// own. Only `orbiting` and `active` are reactive: slow UI state a HUD
// reads, and the frame-loop gate. update() returns whether the pose
// changed since the previous update (pushes included), so per-frame
// dependents (reprojecting HUD overlays via scene.project) can follow the
// camera without recomputing every frame.
//
// Options are read where they apply, not copied out: the clamps, the
// rates and the anchor callbacks are re-read from the options object on
// every input or update, so a caller may change a field (or hand in an
// object of getters, which is what `<OrbitCamera>` does with its props)
// and the next gesture or frame sees it. Only the initial pose is copied
// at creation; later pose changes go through set() and the verbs.

import { createMemo, createSignal, untrack } from "@solidjs/signals"
import { createAxes } from "@solidrt/core/input"
import type { Axes, Vec2 } from "@solidrt/core/input"
import type { CameraState, CameraUpdate } from "./camera.ts"
import type { Vec3 } from "./math.ts"

// One element height of drag sweeps this many full turns on either axis
// (Three's OrbitControls convention).
const DRAG_TURNS = 1
// Rate axes at full deflection, per second: turns of rotation, octaves of
// zoom (1 = the distance halves each second), element heights of pan.
const ROTATE_RATE = 0.5
const ZOOM_RATE = 1
const PAN_RATE = 1
// Default clamps: the distance floor keeps the eye off the target; the
// elevation limits (radians) stop just short of the poles.
const MIN_DISTANCE = 0.01
const ELEVATION_LIMIT = 1.55

/** What an orbit camera drives: a Scene, or one of its Views - anything
 * with their setCamera and camera() (the fov maps pan travel to world). */
export type OrbitTarget = { setCamera(update: CameraUpdate): void; camera(): Pick<CameraState, "fov"> }

/** The pose fields (target, azimuth, elevation, distance) are initial
 * values, copied at creation and changed through set() afterwards. Every
 * other field is live: read from this object where it applies, so a
 * change takes effect on the next input or update. */
export type OrbitCameraOptions = {
  /** Initial point the camera orbits and looks at (default origin). */
  target?: Vec3
  /** Initial pose, radians and world units. */
  azimuth?: number
  elevation?: number
  distance?: number
  /** Distance clamps, world units. */
  minDistance?: number
  maxDistance?: number
  /** Elevation clamps, radians; the defaults stop just short of the poles. */
  minElevation?: number
  maxElevation?: number
  /** Auto-orbit rate in radians/second (default 0: none). Runs while
   * `orbiting()` and no gesture is in progress; `orbiting` starts on when
   * the initial rate is positive and toggles with set({ orbiting }). A new
   * rate applies from the next update. */
  orbitSpeed?: number
  /** Multipliers over the built-in rotate, zoom and pan sensitivities
   * (drags and rates alike). */
  rotateSpeed?: number
  zoomSpeed?: number
  panSpeed?: number
  /** Constrain where a pan may put the target - return the target to use.
   * The typical use: keep the pivot within a few radii of the subject so
   * panning cannot strand the camera. Zoom and rotation do not consult it. */
  clampTarget?: (target: Vec3) => Vec3
  /** Map a zoom gesture's focal point - a fraction of the driven element,
   * [0..1, 0..1] - to the world point the zoom should keep pinned, with
   * the pose the zoom is about to apply to. Called once per pinch gesture
   * (the anchor holds for the whole gesture) and once per wheel notch.
   * Return null to zoom toward the target as usual (also the default). */
  zoomAnchor?: (focal: Vec2, view: { eye: Vec3; target: Vec3 }) => Vec3 | null
  /** The world point rotation should pivot about - called when a rotate
   * gesture begins. It is projected onto the view axis and becomes the new
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

export type OrbitAxes = { rotate: "vec2"; zoom: "axis"; pan: "vec2" }

export type OrbitCamera = {
  /** Camera position for the current pose (a fresh array per call). */
  eye(): Vec3
  /** Pose snapshot - the shape debug commands return and set() takes. */
  pose(): { azimuth: number; elevation: number; distance: number; target: Vec3 }
  /** Merge a pose in (clamps apply) and push it. Also the auto-orbit
   * switch: set({ orbiting: false }). */
  set(pose: OrbitPose): void
  /** Whether the auto-orbit is running. Reactive (signal-backed), so HUD
   * text can read it. */
  orbiting(): boolean
  /** Whether update(dt) has work: the auto-orbit on with a non-zero
   * `orbitSpeed`, or any axis rate non-zero. The frame-loop gate,
   * reactive - the same predicate every camera control exposes. */
  active(): boolean
  /** Integrate the axis rates and the auto-orbit over dt seconds and push
   * any pose change. Call from onFrame; returns whether the pose changed
   * since the previous update (nudges and verbs included). */
  update(dt: number): boolean
  /** The input abstraction: `rotate` (vec2, element heights of drag /
   * turns per second), `zoom` (axis, octaves, positive in) and `pan`
   * (vec2, element heights). An input map drives it by name
   * (InputMap.drive); an app may add sources and nudge directly. */
  axes: Axes<OrbitAxes>
  /** Turn by radians (azimuth positive counterclockwise seen from above,
   * elevation positive upward; clamps apply) and push. */
  rotateBy(azimuth: number, elevation: number): void
  /** Scale the distance by 1/factor (factor > 1 zooms in, as the 2d
   * camera's zoomAt) about `anchor`, a world point that stays where it
   * projects (default: toward the target), and push. */
  zoomBy(factor: number, anchor?: Vec3 | null): void
  /** Slide the target (and the eye with it) by world units along the
   * camera's right and up, through `clampTarget`, and push. */
  panBy(right: number, up: number): void
  /** Re-seat the pivot at a world point without moving the picture: the
   * point is projected onto the view axis and becomes the target at that
   * depth. Points at or behind the eye are ignored. */
  setPivot(point: Vec3): void
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

function finite(what: string, v: number): void {
  if (!Number.isFinite(v)) throw new Error(`createOrbitCamera: ${what} must be a finite number, got ${v}`)
}

/**
 * Create an orbit camera driving `camera`'s position and target, where
 * `camera` is a Scene or one of its Views (fov, near, and far stay yours
 * via its setCamera). The initial pose applies immediately. In a component
 * tree, prefer the `<OrbitCamera>` component: it drives the enclosing
 * scene or view through context and takes an input map as a prop.
 */
export function createOrbitCamera(camera: OrbitTarget, options: OrbitCameraOptions = {}): OrbitCamera {
  if (!camera || typeof camera.setCamera !== "function" || typeof camera.camera !== "function") {
    throw new Error("createOrbitCamera: the target needs setCamera() and camera() (a Scene or a View)")
  }
  let target: Vec3 = options.target ? [options.target[0], options.target[1], options.target[2]] : [0, 0, 0]
  let azimuth = options.azimuth ?? 0
  let elevation = options.elevation ?? 0
  let distance = options.distance ?? 5
  // Everything below the pose is read from `options` where it applies.
  let minDistance = () => options.minDistance ?? MIN_DISTANCE
  let maxDistance = () => options.maxDistance ?? Infinity
  let minElevation = () => options.minElevation ?? -ELEVATION_LIMIT
  let maxElevation = () => options.maxElevation ?? ELEVATION_LIMIT
  let orbitSpeed = () => options.orbitSpeed ?? 0
  let rotateSpeed = () => options.rotateSpeed ?? 1
  let zoomSpeed = () => options.zoomSpeed ?? 1
  let panSpeed = () => options.panSpeed ?? 1

  // The auto-orbit switch: starts on when the initial rate is positive.
  // Plain state is the truth (update() reads it in the same block set()
  // wrote it); the signal notifies HUDs and the active() gate.
  let running = orbitSpeed() > 0
  let [orbiting, setOrbiting] = createSignal(running, { ownedWrite: true })
  // Whether the pose changed since the last update() (see the header).
  let changed = false

  let clampPose = () => {
    elevation = clampNum(elevation, minElevation(), maxElevation())
    distance = clampNum(distance, minDistance(), maxDistance())
  }
  let eye = (): Vec3 => {
    let ce = Math.cos(elevation)
    return [
      target[0] + distance * ce * Math.sin(azimuth),
      target[1] + distance * Math.sin(elevation),
      target[2] + distance * ce * Math.cos(azimuth),
    ]
  }
  let view = () => ({ eye: eye(), target: [target[0], target[1], target[2]] as Vec3 })
  let push = () => {
    changed = true
    camera.setCamera({ position: eye(), target })
  }
  // World units per element height at the target's depth: the frustum's
  // height there, from the target camera's vertical fov.
  let worldPerHeight = () => 2 * Math.tan((camera.camera().fov * Math.PI) / 360) * distance

  // Slide eye and target together along the camera's right/up. The
  // basis for this pose: right = (ca, 0, -sa), up = (-sa*se, ce, -ca*se),
  // written out from eye() = target + distance * (ce*sa, se, ce*ca).
  let slide = (right: number, up: number) => {
    let sa = Math.sin(azimuth)
    let ca = Math.cos(azimuth)
    let se = Math.sin(elevation)
    let ce = Math.cos(elevation)
    let next: Vec3 = [target[0] + right * ca - up * sa * se, target[1] + up * ce, target[2] - right * sa - up * ca * se]
    target = options.clampTarget ? options.clampTarget(next) : next
  }
  // Zoom by `ratio` (new distance over old, before clamping) about a world
  // anchor (null zooms toward the target). Scaling eye and target about the
  // anchor by the distance ratio keeps it projecting to the same pixel, so
  // the shift below uses the ratio that actually applied after clamping -
  // once distance pins at a clamp the target stops moving too, instead of
  // sliding the view sideways under a dead zoom.
  let zoomAbout = (ratio: number, anchor: Vec3 | null | undefined) => {
    let prev = distance
    // Bounds widen to the current distance so a pose already outside them
    // (a setPivot re-seat may land anywhere) zooms back toward range
    // instead of snap-jumping into it.
    distance = clampNum(distance * ratio, Math.min(minDistance(), prev), Math.max(maxDistance(), prev))
    if (!anchor) return
    let s = distance / prev
    target = [anchor[0] + (target[0] - anchor[0]) * s, anchor[1] + (target[1] - anchor[1]) * s, anchor[2] + (target[2] - anchor[2]) * s]
  }
  let setPivot = (point: Vec3) => {
    let e = eye()
    let fx = (target[0] - e[0]) / distance
    let fy = (target[1] - e[1]) / distance
    let fz = (target[2] - e[2]) / distance
    let depth = (point[0] - e[0]) * fx + (point[1] - e[1]) * fy + (point[2] - e[2]) * fz
    if (!(depth > 0)) return
    // Deliberately unclamped: the picture is unchanged, this only decides
    // what the next gesture pivots about. The zoom clamps widen to the
    // current distance, so a pivot outside [min, max] cannot make the next
    // zoom snap-jump either.
    distance = depth
    target = [e[0] + fx * depth, e[1] + fy * depth, e[2] + fz * depth]
  }
  let rotate = (dAzimuth: number, dElevation: number) => {
    azimuth += dAzimuth
    elevation = clampNum(elevation + dElevation, minElevation(), maxElevation())
  }

  // A pinch holds one anchor for its whole gesture (see the header); the
  // wheel, unbracketed, anchors per notch.
  let pinchAnchor: Vec3 | null = null
  let pinchSeen = false
  let anchorAt = (focal: Vec2 | undefined): Vec3 | null => (focal && options.zoomAnchor ? options.zoomAnchor(focal, view()) : null)

  let axes = createAxes<OrbitAxes>(
    { rotate: "vec2", zoom: "axis", pan: "vec2" },
    {
      onBegin: name => {
        if (name === "rotate") {
          let anchor = options.rotateAnchor?.(view())
          if (anchor) setPivot(anchor)
        }
        if (name === "zoom") {
          pinchSeen = false
          pinchAnchor = null
        }
      },
      onNudge: (name, delta, focal) => {
        if (name === "rotate") {
          let d = delta as Vec2
          let rel = DRAG_TURNS * 2 * Math.PI * rotateSpeed()
          rotate(-d[0] * rel, d[1] * rel)
        } else if (name === "zoom") {
          let anchor: Vec3 | null
          if (axes.inGesture("zoom")) {
            if (!pinchSeen) {
              pinchSeen = true
              pinchAnchor = anchorAt(focal)
            }
            anchor = pinchAnchor
          } else {
            anchor = anchorAt(focal)
          }
          zoomAbout(Math.pow(2, -(delta as number) * zoomSpeed()), anchor)
        } else {
          let d = delta as Vec2
          let w = worldPerHeight() * panSpeed()
          slide(-d[0] * w, d[1] * w)
        }
        push()
      },
    },
  )
  let interacting = () => axes.inGesture("rotate") || axes.inGesture("zoom") || axes.inGesture("pan")
  let active = createMemo(() => (orbiting() && orbitSpeed() !== 0) || axes.active())

  clampPose()
  push()
  changed = false

  return {
    eye,
    pose: () => ({ azimuth, elevation, distance, target: [target[0], target[1], target[2]] }),
    orbiting,
    active,
    axes,
    set(pose) {
      if (pose.azimuth !== undefined) azimuth = pose.azimuth
      if (pose.elevation !== undefined) elevation = pose.elevation
      if (pose.distance !== undefined) distance = pose.distance
      if (pose.target) target = [pose.target[0], pose.target[1], pose.target[2]]
      clampPose()
      if (pose.orbiting !== undefined) {
        running = pose.orbiting
        setOrbiting(running)
      }
      push()
    },
    update(dt) {
      finite("update dt", dt)
      let moved = false
      let rate = orbitSpeed()
      if (rate !== 0 && !interacting() && running) {
        azimuth += dt * rate
        moved = true
      }
      let [rx, ry] = untrack(() => axes.rate("rotate"))
      if (rx !== 0 || ry !== 0) {
        let step = ROTATE_RATE * 2 * Math.PI * rotateSpeed() * dt
        rotate(-rx * step, ry * step)
        moved = true
      }
      let z = untrack(() => axes.rate("zoom"))
      if (z !== 0) {
        zoomAbout(Math.pow(2, -z * ZOOM_RATE * zoomSpeed() * dt), null)
        moved = true
      }
      let [px, py] = untrack(() => axes.rate("pan"))
      if (px !== 0 || py !== 0) {
        let w = worldPerHeight() * PAN_RATE * panSpeed() * dt
        slide(-px * w, py * w)
        moved = true
      }
      if (moved) push()
      let result = changed
      changed = false
      return result
    },
    rotateBy(dAzimuth, dElevation) {
      finite("rotateBy azimuth", dAzimuth)
      finite("rotateBy elevation", dElevation)
      rotate(dAzimuth, dElevation)
      push()
    },
    zoomBy(factor, anchor) {
      if (!(Number.isFinite(factor) && factor > 0)) throw new Error(`createOrbitCamera: zoomBy factor must be positive, got ${factor}`)
      zoomAbout(1 / factor, anchor)
      push()
    },
    panBy(right, up) {
      finite("panBy right", right)
      finite("panBy up", up)
      slide(right, up)
      push()
    },
    setPivot(point) {
      setPivot(point)
      push()
    },
  }
}
