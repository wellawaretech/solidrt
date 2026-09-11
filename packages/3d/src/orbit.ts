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
// The pose moves on its own in one way only: toward a goal pose, eased
// by update(dt) (motion.ts). Two things write the goal. A delta that
// arrives outside a gesture - a wheel notch, a key step - is an impulse,
// not a finger, and with `damping` on (the default) it retargets the
// goal instead of jumping the pose, notches compounding on the pending
// value so a fast scroll is one long push; a delta bracketed by a
// gesture (a drag, a pinch, two fingers) applies at once, the content
// staying under the fingers, which is how the control tells a finger
// from an impulse (the 2d camera's rule). And a commanded move -
// glideTo(pose), fit(bounds, { glide }) - eases there the same way. Any
// input drops a commanded glide (a finger landing holds the view); a
// pose write through set() lands at once and drops any motion, so a
// parked pose is a still frame. A zoom's goal keeps its anchor: the
// target is derived from the eased distance each frame, so the point
// under the cursor stays pinned through the glide, not just at its end.
//
// Pose is plain mutable state; a nudge or a verb pushes it to the target
// at once (the next paint carries the new camera, no frame loop needed
// for a drag), and update(dt) integrates the rates, the auto-orbit and
// the motion from the app's own onFrame - the control registers no frame
// loop of its own. Only `orbiting` and `active` are reactive: slow UI
// state a HUD reads, and the frame-loop gate. update() returns whether
// the pose changed since the previous update (pushes included), so
// per-frame dependents (reprojecting HUD overlays via scene.project) can
// follow the camera without recomputing every frame.
//
// Every pose write funnels through one clamp: the elevation and distance
// ranges, then the app's `clampPose` hook, which sees the whole pose and
// so can express a clamp that depends on it (an eye held above a floor
// tightens the elevation floor as the zoom pulls out). A goal is clamped
// when it is set and the eased pose again every frame, so a motion heads
// for a legal pose and never shows an illegal one on the way.
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
import { easeStep, GLIDE_EASE, GLIDE_EPSILON } from "./motion.ts"

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
 * with their setCamera, camera() (the fov maps pan travel to world; the
 * projection decides what fit() frames) and size() (the aspect fit()
 * frames against). */
export type OrbitTarget = {
  setCamera(update: CameraUpdate): void
  camera(): Pick<CameraState, "fov" | "ortho">
  size(): { width: number; height: number }
}

/** A full pose: what pose() returns and clampPose sees. */
export type OrbitPoseState = { azimuth: number; elevation: number; distance: number; target: Vec3 }

/** A partial pose: what set() and glideTo() take and clampPose returns. */
export type OrbitPose = Partial<OrbitPoseState>

/** The pose fields (target, azimuth, elevation, distance) are initial
 * values, copied at creation and changed through set() afterwards. Every
 * other field is live: read from this object where it applies, so a
 * change takes effect on the next input or update. */
export type OrbitCameraOptions = OrbitPose & {
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
  /** How long an unbracketed delta (a wheel notch, a key step) takes to
   * ease in, as a multiple of the built-in settle time: 1 (the default)
   * is the built-in ease, 2 coasts twice as long, 0 applies such deltas
   * at once. Gesture-bracketed deltas and the rates are never damped. */
  damping?: number
  /** Constrain the pose: called with the whole pose after every input,
   * verb, motion step and set() (never setPivot, which leaves the picture
   * as it is), after the range clamps; return the fields to change, or
   * nothing to accept it. The typical uses: keep the target within a few
   * radii of the subject so a pan cannot strand the camera, or hold the
   * eye above a floor - an elevation floor that depends on the distance,
   * which a fixed `minElevation` cannot say. */
  clampPose?: (pose: OrbitPoseState) => OrbitPose | void
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

export type OrbitAxes = { rotate: "vec2"; zoom: "axis"; pan: "vec2" }

export type OrbitCamera = {
  /** Camera position for the current pose (a fresh array per call). */
  eye(): Vec3
  /** Pose snapshot - the shape debug commands return and set() takes. */
  pose(): OrbitPoseState
  /** Merge a pose in (clamps apply) and push it: a snap, dropping any
   * glide or damped motion in flight when it writes a pose field. Also
   * the auto-orbit switch: set({ orbiting: false }), which leaves a
   * motion running. */
  set(pose: OrbitPose & { orbiting?: boolean }): void
  /** Ease to a pose (the fields given, the rest as they are; clamps
   * apply) inside update(dt). Dropped by any input and by a set() that
   * writes a pose field; a new glideTo retargets. */
  glideTo(pose: OrbitPose): void
  /** Frame a box, `[minX, minY, minZ, maxX, maxY, maxZ]` (geometryBounds'
   * shape, a model's `bounds`): the target moves to its centre and the
   * distance to where its bounding sphere fills the view at the target's
   * fov and aspect (the tighter of the two), azimuth and elevation as
   * they are, clamps applied. A snap, so a park-then-snapshot repeats;
   * `{ glide: true }` eases there through glideTo. Under an orthographic
   * camera only the target moves: the extents frame, not the distance.
   * Reads the target's size when called. */
  fit(bounds: ArrayLike<number>, opts?: { glide?: boolean }): void
  /** Whether the auto-orbit is running. Reactive (signal-backed), so HUD
   * text can read it. */
  orbiting(): boolean
  /** Whether update(dt) has work: the auto-orbit on with a non-zero
   * `orbitSpeed`, a glide or damped motion in flight, or any axis rate
   * non-zero. The frame-loop gate, reactive - the same predicate every
   * camera control exposes. */
  active(): boolean
  /** Integrate the axis rates, the auto-orbit and the motion over dt
   * seconds and push any pose change. Call from onFrame; returns whether
   * the pose changed since the previous update (nudges and verbs
   * included). */
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
   * camera's right and up, through `clampPose`, and push. */
  panBy(right: number, up: number): void
  /** Re-seat the pivot at a world point without moving the picture: the
   * point is projected onto the view axis and becomes the target at that
   * depth. Points at or behind the eye are ignored. */
  setPivot(point: Vec3): void
}

// The goal pose a motion eases toward (see the header). A damped zoom
// keeps its anchor and the pose it was taken from, and derives the target
// from the eased distance each frame so the anchor stays pinned; a glide
// or a damped pan eases the target directly (anchor null).
type Motion = OrbitPoseState & {
  anchor: Vec3 | null
  refTarget: Vec3
  refDistance: number
  /** E-foldings per second. */
  rate: number
  /** A commanded glide is dropped by any input; damped input compounds. */
  commanded: boolean
  /** The fraction of the initial gap still open. */
  remaining: number
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
let copy = (v: Vec3): Vec3 => [v[0], v[1], v[2]]

function finite(what: string, v: number): void {
  if (!Number.isFinite(v)) throw new Error(`createOrbitCamera: ${what} must be a finite number, got ${v}`)
}

function checkPose(verb: string, pose: OrbitPose): void {
  if (pose.azimuth !== undefined) finite(`${verb} azimuth`, pose.azimuth)
  if (pose.elevation !== undefined) finite(`${verb} elevation`, pose.elevation)
  if (pose.distance !== undefined) finite(`${verb} distance`, pose.distance)
  if (pose.target) for (let i = 0; i < 3; i++) finite(`${verb} target[${i}]`, pose.target[i]!)
}

/**
 * Create an orbit camera driving `camera`'s position and target, where
 * `camera` is a Scene or one of its Views (fov, near, and far stay yours
 * via its setCamera). The initial pose applies immediately. In a component
 * tree, prefer the `<OrbitCamera>` component: it drives the enclosing
 * scene or view through context and takes an input map as a prop.
 */
export function createOrbitCamera(camera: OrbitTarget, options: OrbitCameraOptions = {}): OrbitCamera {
  if (!camera || typeof camera.setCamera !== "function" || typeof camera.camera !== "function" || typeof camera.size !== "function") {
    throw new Error("createOrbitCamera: the target needs setCamera(), camera() and size() (a Scene or a View)")
  }
  checkPose("initial", options)
  // The live pose: one object, mutated in place, so the mutations below
  // apply to it and to a motion's goal alike.
  let pose: OrbitPoseState = {
    azimuth: options.azimuth ?? 0,
    elevation: options.elevation ?? 0,
    distance: options.distance ?? 5,
    target: options.target ? copy(options.target) : [0, 0, 0],
  }
  // Everything below the pose is read from `options` where it applies.
  let minDistance = () => options.minDistance ?? MIN_DISTANCE
  let maxDistance = () => options.maxDistance ?? Infinity
  let minElevation = () => options.minElevation ?? -ELEVATION_LIMIT
  let maxElevation = () => options.maxElevation ?? ELEVATION_LIMIT
  let orbitSpeed = () => options.orbitSpeed ?? 0
  let rotateSpeed = () => options.rotateSpeed ?? 1
  let zoomSpeed = () => options.zoomSpeed ?? 1
  let panSpeed = () => options.panSpeed ?? 1
  let damping = () => options.damping ?? 1

  // The auto-orbit switch: starts on when the initial rate is positive.
  // Plain state is the truth (update() reads it in the same block set()
  // wrote it); the signal notifies HUDs and the active() gate.
  let running = orbitSpeed() > 0
  let [orbiting, setOrbiting] = createSignal(running, { ownedWrite: true })
  // Whether the pose changed since the last update() (see the header).
  let changed = false
  // The motion in flight, and its half of active(): a signal every entry
  // that starts or drops a motion refreshes (ownedWrite: entries run from
  // component bodies and handlers alike).
  let motion: Motion | null = null
  let [motionActive, setMotionActive] = createSignal(false, { ownedWrite: true })
  let notify = () => {
    let now = motion !== null
    if (now !== untrack(motionActive)) setMotionActive(now)
  }
  let interrupt = () => {
    motion = null
  }

  // Distance bounds for a write from `prev`: they widen to it, so a pose
  // already outside them (a setPivot re-seat may land anywhere) moves back
  // toward range instead of snap-jumping into it. null = the bounds
  // themselves (a set()).
  let distanceBounds = (prev: number | null): [number, number] =>
    prev === null ? [minDistance(), maxDistance()] : [Math.min(minDistance(), prev), Math.max(maxDistance(), prev)]
  // The one clamp every write goes through: the ranges, then the hook.
  let clampPose = (p: OrbitPoseState, prevDistance: number | null) => {
    p.elevation = clampNum(p.elevation, minElevation(), maxElevation())
    let [lo, hi] = distanceBounds(prevDistance)
    p.distance = clampNum(p.distance, lo, hi)
    let hook = options.clampPose
    if (!hook) return
    let r = hook({ azimuth: p.azimuth, elevation: p.elevation, distance: p.distance, target: copy(p.target) })
    if (!r) return
    checkPose("clampPose", r)
    if (r.azimuth !== undefined) p.azimuth = r.azimuth
    if (r.elevation !== undefined) p.elevation = r.elevation
    if (r.distance !== undefined) p.distance = r.distance
    if (r.target) p.target = copy(r.target)
  }
  let eyeOf = (p: OrbitPoseState): Vec3 => {
    let ce = Math.cos(p.elevation)
    return [
      p.target[0] + p.distance * ce * Math.sin(p.azimuth),
      p.target[1] + p.distance * Math.sin(p.elevation),
      p.target[2] + p.distance * ce * Math.cos(p.azimuth),
    ]
  }
  let eye = () => eyeOf(pose)
  let view = () => ({ eye: eye(), target: copy(pose.target) })
  let push = () => {
    changed = true
    camera.setCamera({ position: eye(), target: pose.target })
  }
  // World units per element height at the target's depth: the frustum's
  // height there, from the target camera's vertical fov.
  let worldPerHeight = () => 2 * Math.tan((camera.camera().fov * Math.PI) / 360) * pose.distance

  // Slide eye and target together along the camera's right/up. The
  // basis for this pose: right = (ca, 0, -sa), up = (-sa*se, ce, -ca*se),
  // written out from eye() = target + distance * (ce*sa, se, ce*ca).
  let slide = (p: OrbitPoseState, right: number, up: number) => {
    let sa = Math.sin(p.azimuth)
    let ca = Math.cos(p.azimuth)
    let se = Math.sin(p.elevation)
    let ce = Math.cos(p.elevation)
    p.target = [p.target[0] + right * ca - up * sa * se, p.target[1] + up * ce, p.target[2] - right * sa - up * ca * se]
  }
  // The target a zoom about `anchor` leaves when the distance goes from
  // `refDistance` (target `refTarget`) to `distance`: scaling eye and
  // target about the anchor by the distance ratio keeps it projecting to
  // the same pixel. The ratio is the one that actually applied after
  // clamping - once distance pins at a clamp the target stops moving too,
  // instead of sliding the view sideways under a dead zoom.
  let anchored = (anchor: Vec3, refTarget: Vec3, refDistance: number, distance: number): Vec3 => {
    let s = distance / refDistance
    return [anchor[0] + (refTarget[0] - anchor[0]) * s, anchor[1] + (refTarget[1] - anchor[1]) * s, anchor[2] + (refTarget[2] - anchor[2]) * s]
  }
  // Zoom by `ratio` (new distance over old, before clamping) about a world
  // anchor (null zooms toward the target), at once.
  let zoomAbout = (p: OrbitPoseState, ratio: number, anchor: Vec3 | null | undefined) => {
    let prev = p.distance
    let [lo, hi] = distanceBounds(prev)
    p.distance = clampNum(p.distance * ratio, lo, hi)
    if (anchor) p.target = anchored(anchor, p.target, prev, p.distance)
  }
  let setPivot = (point: Vec3) => {
    let e = eye()
    let fx = (pose.target[0] - e[0]) / pose.distance
    let fy = (pose.target[1] - e[1]) / pose.distance
    let fz = (pose.target[2] - e[2]) / pose.distance
    let depth = (point[0] - e[0]) * fx + (point[1] - e[1]) * fy + (point[2] - e[2]) * fz
    if (!(depth > 0)) return
    // Deliberately unclamped: the picture is unchanged, this only decides
    // what the next gesture pivots about. The distance bounds widen to the
    // pose's distance, so a pivot outside [min, max] cannot make the next
    // zoom snap-jump either.
    pose.distance = depth
    pose.target = [e[0] + fx * depth, e[1] + fy * depth, e[2] + fz * depth]
  }
  let rotate = (p: OrbitPoseState, dAzimuth: number, dElevation: number) => {
    p.azimuth += dAzimuth
    p.elevation += dElevation
  }
  // A write to the live pose: mutate, then clamp from the distance before
  // the write. `write` is the entry's form, pushing at once; update(dt)
  // mutates and pushes once at its end.
  let mutateLive = (mutate: (p: OrbitPoseState) => void) => {
    let prev = pose.distance
    mutate(pose)
    clampPose(pose, prev)
  }
  let write = (mutate: (p: OrbitPoseState) => void) => {
    mutateLive(mutate)
    push()
  }

  // ---- Motion: the goal pose and its ease ----
  let startMotion = (goal: OrbitPoseState, rate: number, commanded: boolean): Motion => {
    motion = { ...goal, target: copy(goal.target), anchor: null, refTarget: copy(pose.target), refDistance: pose.distance, rate, commanded, remaining: 1 }
    return motion
  }
  // The damped motion an unbracketed delta compounds on: the one in
  // flight, or a fresh goal at the live pose (a commanded glide is
  // dropped: the input wins).
  let dampedMotion = (): Motion => {
    let m = motion !== null && !motion.commanded ? motion : startMotion(pose, GLIDE_EASE / damping(), false)
    m.remaining = 1
    return m
  }
  // A damped write: the mutation lands on the goal, clamped from the
  // goal's distance so notches compound within the widened bounds.
  let writeDamped = (mutate: (m: Motion) => void) => {
    let m = dampedMotion()
    let prev = m.distance
    mutate(m)
    clampPose(m, prev)
  }
  let step = (dt: number) => {
    let m = motion!
    let k = easeStep(m.rate, dt)
    m.remaining *= 1 - k
    let prev = pose.distance
    if (m.remaining < GLIDE_EPSILON) {
      pose.azimuth = m.azimuth
      pose.elevation = m.elevation
      pose.distance = m.distance
      pose.target = copy(m.target)
      motion = null
    } else {
      pose.azimuth += (m.azimuth - pose.azimuth) * k
      pose.elevation += (m.elevation - pose.elevation) * k
      // Distance eases in log space so a long glide reads evenly: the
      // same ratio per second going in as coming out.
      pose.distance *= Math.pow(m.distance / pose.distance, k)
      if (m.anchor) pose.target = anchored(m.anchor, m.refTarget, m.refDistance, pose.distance)
      else pose.target = [pose.target[0] + (m.target[0] - pose.target[0]) * k, pose.target[1] + (m.target[1] - pose.target[1]) * k, pose.target[2] + (m.target[2] - pose.target[2]) * k]
    }
    clampPose(pose, prev)
  }
  let glideTo = (p: OrbitPose) => {
    checkPose("glideTo", p)
    let goal: OrbitPoseState = {
      azimuth: p.azimuth ?? pose.azimuth,
      elevation: p.elevation ?? pose.elevation,
      distance: p.distance ?? pose.distance,
      target: p.target ? copy(p.target) : copy(pose.target),
    }
    clampPose(goal, null)
    startMotion(goal, GLIDE_EASE, true)
    notify()
  }

  // A pinch holds one anchor for its whole gesture (see the header); the
  // wheel, unbracketed, anchors per notch.
  let pinchAnchor: Vec3 | null = null
  let pinchSeen = false
  let anchorAt = (focal: Vec2 | undefined): Vec3 | null => (focal && options.zoomAnchor ? options.zoomAnchor(focal, view()) : null)
  // A wheel notch with damping on: the goal's distance compounds, and the
  // goal's target is re-derived about this notch's anchor from the live
  // pose (the 2d camera's anchor glide), so the eased target stays pinned.
  let dampedZoom = (ratio: number, anchor: Vec3 | null) => {
    writeDamped(m => {
      let prev = m.distance
      let [lo, hi] = distanceBounds(prev)
      m.distance = clampNum(m.distance * ratio, lo, hi)
      m.anchor = anchor
      m.refTarget = copy(pose.target)
      m.refDistance = pose.distance
      m.target = anchor ? anchored(anchor, pose.target, pose.distance, m.distance) : copy(pose.target)
    })
  }

  let axes = createAxes<OrbitAxes>(
    { rotate: "vec2", zoom: "axis", pan: "vec2" },
    {
      onBegin: name => {
        // A finger landing holds a glide where it is.
        interrupt()
        if (name === "rotate") {
          let anchor = options.rotateAnchor?.(view())
          if (anchor) setPivot(anchor)
        }
        if (name === "zoom") {
          pinchSeen = false
          pinchAnchor = null
        }
        notify()
      },
      onNudge: (name, delta, focal) => {
        let bracketed = axes.inGesture(name)
        let eased = !bracketed && damping() > 0
        if (name === "rotate") {
          let d = delta as Vec2
          let rel = DRAG_TURNS * 2 * Math.PI * rotateSpeed()
          let mutate = (p: OrbitPoseState) => rotate(p, -d[0] * rel, d[1] * rel)
          if (eased) writeDamped(mutate)
          else {
            interrupt()
            write(mutate)
          }
        } else if (name === "zoom") {
          let ratio = Math.pow(2, -(delta as number) * zoomSpeed())
          if (bracketed) {
            if (!pinchSeen) {
              pinchSeen = true
              pinchAnchor = anchorAt(focal)
            }
            let anchor = pinchAnchor
            write(p => zoomAbout(p, ratio, anchor))
          } else {
            let anchor = anchorAt(focal)
            if (eased) dampedZoom(ratio, anchor)
            else {
              interrupt()
              write(p => zoomAbout(p, ratio, anchor))
            }
          }
        } else {
          let d = delta as Vec2
          let w = worldPerHeight() * panSpeed()
          let mutate = (p: OrbitPoseState) => slide(p, -d[0] * w, d[1] * w)
          if (eased) {
            writeDamped(m => {
              mutate(m)
              m.anchor = null
            })
          } else {
            interrupt()
            write(mutate)
          }
        }
        notify()
      },
    },
  )
  let interacting = () => axes.inGesture("rotate") || axes.inGesture("zoom") || axes.inGesture("pan")
  let active = createMemo(() => (orbiting() && orbitSpeed() !== 0) || motionActive() || axes.active())

  clampPose(pose, null)
  push()
  changed = false

  return {
    eye,
    pose: () => ({ azimuth: pose.azimuth, elevation: pose.elevation, distance: pose.distance, target: copy(pose.target) }),
    orbiting,
    active,
    axes,
    set(p) {
      checkPose("set", p)
      if (p.azimuth !== undefined || p.elevation !== undefined || p.distance !== undefined || p.target) interrupt()
      if (p.azimuth !== undefined) pose.azimuth = p.azimuth
      if (p.elevation !== undefined) pose.elevation = p.elevation
      if (p.distance !== undefined) pose.distance = p.distance
      if (p.target) pose.target = copy(p.target)
      clampPose(pose, null)
      if (p.orbiting !== undefined) {
        running = p.orbiting
        setOrbiting(running)
      }
      push()
      notify()
    },
    glideTo,
    fit(bounds, opts) {
      if (bounds.length !== 6) throw new Error(`createOrbitCamera: fit bounds must be [minX, minY, minZ, maxX, maxY, maxZ], got ${bounds.length} values`)
      for (let i = 0; i < 6; i++) finite(`fit bounds[${i}]`, bounds[i]!)
      let center: Vec3 = [(bounds[0]! + bounds[3]!) / 2, (bounds[1]! + bounds[4]!) / 2, (bounds[2]! + bounds[5]!) / 2]
      let radius = Math.hypot(bounds[3]! - bounds[0]!, bounds[4]! - bounds[1]!, bounds[5]! - bounds[2]!) / 2
      let distance = pose.distance
      let cam = camera.camera()
      if (!cam.ortho && radius > 0) {
        // The sphere is tangent to the frustum's side planes at
        // radius / sin(half-angle); the tighter of the vertical fov and
        // the horizontal one it implies at the target's aspect wins.
        let size = camera.size()
        let vertical = (cam.fov * Math.PI) / 360
        let aspect = size.height > 0 ? size.width / size.height : 1
        let horizontal = Math.atan(Math.tan(vertical) * aspect)
        distance = radius / Math.sin(Math.min(vertical, horizontal))
      }
      let goal: OrbitPose = { target: center, distance }
      if (opts?.glide) {
        glideTo(goal)
        return
      }
      interrupt()
      pose.target = center
      pose.distance = distance
      clampPose(pose, null)
      push()
      notify()
    },
    update(dt) {
      finite("update dt", dt)
      let moved = false
      let rate = orbitSpeed()
      if (rate !== 0 && !interacting() && running) {
        // The goal turns with the pose, so a glide on an orbiting camera
        // lands where the orbit has carried its goal.
        pose.azimuth += dt * rate
        if (motion !== null) motion.azimuth += dt * rate
        moved = true
      }
      let [rx, ry] = untrack(() => axes.rate("rotate"))
      if (rx !== 0 || ry !== 0) {
        let s = ROTATE_RATE * 2 * Math.PI * rotateSpeed() * dt
        interrupt()
        mutateLive(p => rotate(p, -rx * s, ry * s))
        moved = true
      }
      let z = untrack(() => axes.rate("zoom"))
      if (z !== 0) {
        interrupt()
        mutateLive(p => zoomAbout(p, Math.pow(2, -z * ZOOM_RATE * zoomSpeed() * dt), null))
        moved = true
      }
      let [px, py] = untrack(() => axes.rate("pan"))
      if (px !== 0 || py !== 0) {
        let w = worldPerHeight() * PAN_RATE * panSpeed() * dt
        interrupt()
        mutateLive(p => slide(p, -px * w, py * w))
        moved = true
      }
      if (motion !== null && dt > 0) {
        step(dt)
        moved = true
      }
      if (moved) push()
      notify()
      let result = changed
      changed = false
      return result
    },
    rotateBy(dAzimuth, dElevation) {
      finite("rotateBy azimuth", dAzimuth)
      finite("rotateBy elevation", dElevation)
      interrupt()
      write(p => rotate(p, dAzimuth, dElevation))
      notify()
    },
    zoomBy(factor, anchor) {
      if (!(Number.isFinite(factor) && factor > 0)) throw new Error(`createOrbitCamera: zoomBy factor must be positive, got ${factor}`)
      interrupt()
      write(p => zoomAbout(p, 1 / factor, anchor))
      notify()
    },
    panBy(right, up) {
      finite("panBy right", right)
      finite("panBy up", up)
      interrupt()
      write(p => slide(p, right, up))
      notify()
    },
    setPivot(point) {
      setPivot(point)
      push()
    },
  }
}
