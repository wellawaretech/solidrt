// An orbit camera for a scene: azimuth/elevation/distance around a target
// - the standard interactive-viewer camera, extracted so apps stop
// rebuilding it. The control is pure (ARCHITECTURE.md: controls through
// an abstraction, never direct event handling): it consumes four axes,
// `rotate` (vec2), `zoom` (axis), `pan` (vec2) and `focus` (axis), in
// device-free units, and exposes pose verbs for scripted moves. What
// drives the axes is the app's business - an input map with a pointer
// feed, a pad, a debug command - and the `orbitBindings` preset in
// input.ts is the standard wiring (drag rotates, pinch and wheel zoom,
// two fingers pan, a double tap focuses).
//
// The control runs the pipeline of okf/design/camera-controls.md: a
// SOURCE (an input nudge, a verb, or the follow) writes the pose; the
// FRAMING eases the followed point back into its zones; the LANES (a
// screen offset, shakes) are summed on top of the pose; the CONSTRAINTS
// (the clamps, `clampPose`, occlusion) apply; and the PUSH writes the
// final camera. pose() is the pose alone: a lane never enters it.
//
// Rotation is element-relative: one element height of drag travel (the
// feed's unit) sweeps DRAG_TURNS full turns on either axis, Three's
// OrbitControls convention, so the same drag feels the same on a phone
// and a 4k window. A rate on `rotate` (a stick) turns at ROTATE_RATE
// turns per second at full deflection. `zoom` is in octaves: a delta of
// 1 halves the distance, a rate of 1 halves it per second - a DOLLY,
// bounded by the target, unless `push` lets a step past `minDistance`
// carry eye and target forward together by the overflow (camera-controls'
// infinityDolly), so a pinch moves through a model at a speed that stays
// continuous across the floor. `pan` slides the target so the scene
// tracks the fingers 1:1 at the target's depth, the touch convention
// everywhere (three.js DOLLY_PAN, Sketchfab, touch CAD), mapped through
// the target camera's fov - which is why the driven target must report
// its camera (a Scene or a View does); `panPlane: "ground"` pans in the
// plane orthogonal to world up instead (Three's screenSpacePanning off,
// MapControls' choice).
//
// Zoom aims at the target by default. With `zoomAnchor` the gesture's
// focal point (a fraction of the element) maps to a world point, and the
// zoom scales the pose about that point instead - the spot under the
// fingers stays under the fingers, with the target sliding toward it.
// Focal-to-world needs the projection, which the `<OrbitCamera>`
// component builds from its scene (a pick, else the target-depth plane)
// and a function-face caller supplies itself. A pinch keeps ONE anchor
// for its whole gesture, taken at its first delta: the two fingers'
// events interleave, so the measured span oscillates around its true
// value even while the fingers rest, and per-delta re-anchoring turns
// that noise into a visible crawl; about a fixed point the pairs cancel
// exactly. The wheel, unbracketed, anchors per notch: its notches are
// discrete, there is no noise to cancel, and each notch re-aiming at the
// cursor is the point. `focus` uses the same mapping: a nudge (a double
// tap) glides the target to the point under it.
//
// Anchored zoom leaves the target wherever the zoom carried it - possibly
// a point in empty air near the eye, and a drag orbiting THAT swings the
// scene wildly around nothing. `rotateAnchor` is the countermeasure: when
// a rotate gesture begins, the app names the world point rotation should
// pivot about (what the camera is actually looking at), and setPivot
// re-seats the target there, projected onto the view axis first, so only
// the target's depth moves - the eye and the picture do not change.
// setOrbitPoint is the exact form: the target moves to the point itself,
// off the view axis, and the offset lane takes up the difference so the
// picture still does not change (camera-controls' setOrbitPoint with a
// focal offset) - a drag then orbits that point, which is how a viewer
// pushed through a model keeps turning about the model's centre.
//
// The pose moves on its own toward a goal pose, eased by update(dt). Two
// things write the goal. A delta that arrives outside a gesture - a wheel
// notch, a key step - is an impulse, not a finger, and with `damping` on
// (the default) it retargets the goal instead of jumping the pose,
// notches compounding on the pending value so a fast scroll is one long
// push; a delta bracketed by a gesture (a drag, a pinch, two fingers)
// applies at once, the content staying under the fingers, which is how
// the control tells a finger from an impulse (the 2d camera's rule); the
// gesture's end brings the release velocity, and with damping on the view
// keeps turning or sliding from that speed, decaying (Three's
// enableDamping glide). And a commanded move - glideTo(pose), fit(bounds,
// { glide }) - eases there the same way. Any input drops a commanded
// glide (a finger landing holds the view); a pose write through set()
// lands at once and drops any motion, so a parked pose is a still frame.
// A zoom's goal keeps its anchor: the target is derived from the eased
// distance each frame, so the point under the cursor stays pinned through
// the glide, not just at its end.
//
// The follow (Cinemachine's Orbital Follow): follow(point) makes the
// target chase a world point through the framing of `follow` - a dead
// zone, hard limits and damping per axis in view space (right, up,
// forward), lookahead along the point's velocity - while the orbit's own
// input keeps working around it: a rotate orbits the followed point, a
// zoom dollies toward it. A pan moves the target and the follow eases it
// back; unfollow(), or a verb that writes the target, ends it.
//
// Occlusion is a constraint on the FINAL camera, not the pose: the
// `occluder` hook reports the free distance from the target toward the
// eye (the `<OrbitCamera>` component fills it from a scene raycast), and
// when it is shorter than the pose's distance the eye is pulled in to it
// at once, then eased back out as the obstacle clears (Cinemachine's
// deoccluder damping; Godot's SpringArm3D snaps both ways). The pose's
// distance is untouched, so a zoom out from behind a wall still goes
// where the pose says once the wall is gone.
//
// Pose is plain mutable state; a nudge or a verb pushes it to the target
// at once (the next paint carries the new camera, no frame loop needed
// for a drag), and update(dt) integrates the rates, the auto-orbit, the
// motion, the follow, the lanes and the occlusion return from the app's
// own onFrame - the control registers no frame loop of its own. Only
// `orbiting` and `active` are reactive: slow UI state a HUD reads, and
// the frame-loop gate. update() returns whether the camera changed since
// the previous update (pushes included), so per-frame dependents
// (reprojecting HUD overlays via scene.project) can follow the camera
// without recomputing every frame.
//
// Every pose write funnels through one clamp: the elevation and distance
// ranges, then the app's `clampPose` hook, which sees the whole pose and
// so can express a clamp that depends on it (an eye held above a floor
// tightens the elevation floor as the zoom pulls out). A goal is clamped
// when it is set and the eased pose again every frame, so a motion heads
// for a legal pose and never shows an illegal one on the way.
//
// Options are read where they apply, not copied out: the clamps, the
// rates, the follow's zones and the hooks are re-read from the options
// object on every input or update, so a caller may change a field (or
// hand in an object of getters, which is what `<OrbitCamera>` does with
// its props) and the next gesture or frame sees it. Only the initial pose
// is copied at creation; later pose changes go through set() and the
// verbs.

import { createSignal, untrack } from "@solidjs/signals"
import { createAxes } from "@solidrt/core/input"
import type { Axes, Vec2 } from "@solidrt/core/input"
import { checkFollowOptions, checkOffset, checkShake, createActivity, createLanes, createLookahead, easeStep, followRate, frame, GLIDE_EASE, GLIDE_EPSILON } from "@solidrt/core/camera-control"
import type { FollowOptions } from "@solidrt/core/camera-control"
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
// A follow or an occlusion return lands once the remaining travel is
// under this many screen pixels (the 2d camera's threshold), so a
// resting follow writes nothing.
const LAND_PX = 0.5
// A heading recentre lands once the remaining turn is under this many
// radians (a hundredth of a degree: under a pixel at any distance).
const HEADING_EPSILON = 1e-4
// Default seconds without rotate input before the azimuth recentres on
// the followed heading (Cinemachine's recentering wait).
const RECENTER_WAIT = 1

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
  /** A zoom step that would cross `minDistance` (or `maxDistance`) moves
   * eye and target together by the overflow instead - along the view
   * axis, or the anchor's ray when the step is anchored - so the camera
   * passes through a model at a speed continuous across the floor
   * (default false: the dolly stops at the floor). */
  push?: boolean
  /** Where a pan slides: "screen" (default) along the view's right and
   * up; "ground" along right and the horizontal forward, the plane
   * orthogonal to world up (a map, a table top). */
  panPlane?: "screen" | "ground"
  /** How follow(point) chases its point: damping per view axis (x right,
   * y up, z forward), the dead zone and hard limits as fractions of the
   * view, lookahead; and `heading`, how the azimuth recentres on a
   * followed heading (follow's second argument): `damping` as the
   * others (default 1), after `wait` seconds without rotate input
   * (default 1, Cinemachine's recentering) - a drag looks around, and
   * the camera settles back behind the walker. Read where it applies. */
  follow?: FollowOptions & { heading?: { damping?: number; wait?: number } }
  /** The offset lane: where the target sits in the view, in view HEIGHTS
   * from its centre (the feed's unit; x right, y down: [0.25, 0] shows
   * the target a quarter of the view's height right of centre). Added on
   * top of the pose at push; setOrbitPoint's own offset compounds with
   * it. */
  offset?: Vec2
  /** Constrain the pose: called with the whole pose after every input,
   * verb, motion step and set() (never setPivot, which leaves the picture
   * as it is), after the range clamps; return the fields to change, or
   * nothing to accept it. The typical uses: keep the target within a few
   * radii of the subject so a pan cannot strand the camera, or hold the
   * eye above a floor - an elevation floor that depends on the distance,
   * which a fixed `minElevation` cannot say. */
  clampPose?: (pose: OrbitPoseState) => OrbitPose | void
  /** Map a zoom or focus gesture's focal point - a fraction of the driven
   * element, [0..1, 0..1] - to the world point it means, with the pose
   * the gesture is about to apply to. Called once per pinch gesture (the
   * anchor holds for the whole gesture), once per wheel notch and once
   * per focus nudge. Return null to zoom toward the target as usual (also
   * the default) or to ignore the focus. */
  zoomAnchor?: (focal: Vec2, view: { eye: Vec3; target: Vec3 }) => Vec3 | null
  /** The world point rotation should pivot about - called when a rotate
   * gesture begins. It is projected onto the view axis and becomes the new
   * target, preserving the picture exactly (only the pivot's depth moves),
   * so a drag after an anchored zoom orbits the scene under the camera
   * instead of wherever the zoom left the target. Points at or behind the
   * eye are ignored, as is null (both keep the current pivot). */
  rotateAnchor?: (view: { eye: Vec3; target: Vec3 }) => Vec3 | null
  /** The occlusion constraint: the free distance from `target` toward
   * `eye` (world units) when something stands between them, else null.
   * A free distance under the pose's pulls the final eye in to it at
   * once; the return eases. Evaluated at every push and every update. */
  occluder?: (target: Vec3, eye: Vec3) => number | null
}

export type OrbitAxes = { rotate: "vec2"; zoom: "axis"; pan: "vec2"; focus: "axis" }

export type OrbitCamera = {
  /** Camera position for the current pose (a fresh array per call): the
   * pose's eye, before the lanes and occlusion; camera() has the final
   * one. */
  eye(): Vec3
  /** The final camera as last pushed: the pose plus the lanes, the eye
   * pulled in by occlusion (fresh arrays per call). */
  camera(): { position: Vec3; target: Vec3 }
  /** Pose snapshot - the shape debug commands return and set() takes. */
  pose(): OrbitPoseState
  /** Merge a pose in (clamps apply) and push it: a snap, dropping any
   * glide or damped motion in flight when it writes a pose field, and the
   * follow and setOrbitPoint's offset when it writes the target. Also the
   * auto-orbit switch: set({ orbiting: false }), which leaves a motion
   * running. */
  set(pose: OrbitPose & { orbiting?: boolean }): void
  /** Ease to a pose (the fields given, the rest as they are; clamps
   * apply) inside update(dt). Dropped by any input and by a set() that
   * writes a pose field; a new glideTo retargets. A target ends the
   * follow and setOrbitPoint's offset. */
  glideTo(pose: OrbitPose): void
  /** Frame a box, `[minX, minY, minZ, maxX, maxY, maxZ]` (geometryBounds'
   * shape, a model's `bounds`): the target moves to its centre and the
   * distance to where its bounding sphere fills the view at the target's
   * fov and aspect (the tighter of the two), azimuth and elevation as
   * they are, clamps applied. A snap, so a park-then-snapshot repeats;
   * `{ glide: true }` eases there through glideTo. Under an orthographic
   * camera only the target moves: the extents frame, not the distance.
   * Reads the target's size when called. Ends the follow. */
  fit(bounds: ArrayLike<number>, opts?: { glide?: boolean }): void
  /** Chase a world point (the framing stage, options.follow): the target
   * eases toward it per view axis, through the dead zone and hard
   * limits, with lookahead. `heading`, when given, is the followed
   * thing's yaw in the first-person convention (0 faces -z, positive
   * turns left; a walker's `pose().yaw`): the azimuth recentres on it -
   * the camera settles behind the walker - after `follow.heading.wait`
   * seconds without rotate input, by the shortest turn. Call it every
   * frame for a moving point; the control settles once the point rests
   * inside the zones and the heading is reached. */
  follow(point: Vec3, heading?: number): void
  unfollow(): void
  /** Whether the auto-orbit is running. Reactive (signal-backed), so HUD
   * text can read it. */
  orbiting(): boolean
  /** Whether update(dt) has work: the auto-orbit on with a non-zero
   * `orbitSpeed`, a glide or damped motion in flight, a follow not yet
   * settled, a shake running, an occlusion return easing, or any axis
   * rate non-zero. The frame-loop gate, reactive - the same predicate
   * every camera control exposes. */
  active(): boolean
  /** Integrate the axis rates, the auto-orbit, the motion, the follow,
   * the lanes and the occlusion return over dt seconds and push any
   * camera change. Call from onFrame; returns whether the camera changed
   * since the previous update (nudges and verbs included). */
  update(dt: number): boolean
  /** The input abstraction: `rotate` (vec2, element heights of drag /
   * turns per second), `zoom` (axis, octaves, positive in), `pan` (vec2,
   * element heights) and `focus` (axis: a nudge with a focal glides the
   * target to the point under it through `zoomAnchor`; without a focal,
   * to the point under the view centre). An input map drives it by name
   * (InputMap.drive); an app may add sources and nudge directly. */
  axes: Axes<OrbitAxes>
  /** Turn by radians (azimuth positive counterclockwise seen from above,
   * elevation positive upward; clamps apply) and push. */
  rotateBy(azimuth: number, elevation: number): void
  /** Scale the distance by 1/factor (factor > 1 zooms in, as the 2d
   * camera's zoomAt) about `anchor`, a world point that stays where it
   * projects (default: toward the target), and push. With `push` on, the
   * overflow past the floor moves through. */
  zoomBy(factor: number, anchor?: Vec3 | null): void
  /** Slide the target (and the eye with it) by world units along the
   * camera's right and up (or the ground plane, `panPlane`), through
   * `clampPose`, and push. */
  panBy(right: number, up: number): void
  /** Re-seat the pivot at a world point without moving the picture: the
   * point is projected onto the view axis and becomes the target at that
   * depth. Points at or behind the eye are ignored. */
  setPivot(point: Vec3): void
  /** Re-seat the pivot at a world point EXACTLY, without moving the
   * picture: the target becomes the point, the distance its depth, and
   * the offset lane takes up the point's offset from the view axis, so
   * the camera's position and orientation do not change and the next
   * drag orbits that point (camera-controls' setOrbitPoint). Points at or
   * behind the eye are ignored. A later verb that writes the target
   * clears the offset this added. */
  setOrbitPoint(point: Vec3): void
  /** A shake on the lanes: `strength` is the peak displacement in view
   * heights, `duration` seconds, an optional frequency (cycles per
   * second) and direction (a unit [x, y], y down). Shakes sum and decay;
   * the pose is untouched. */
  shake(strength: number, duration: number, opts?: { frequency?: number; direction?: Vec2 }): void
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

function checkVec3(what: string, v: Vec3): void {
  if (!Array.isArray(v) || v.length !== 3) throw new Error(`createOrbitCamera: ${what} must be [x, y, z], got ${JSON.stringify(v)}`)
  for (let i = 0; i < 3; i++) finite(`${what}[${i}]`, v[i]!)
}

function checkPose(verb: string, pose: OrbitPose): void {
  if (pose.azimuth !== undefined) finite(`${verb} azimuth`, pose.azimuth)
  if (pose.elevation !== undefined) finite(`${verb} elevation`, pose.elevation)
  if (pose.distance !== undefined) finite(`${verb} distance`, pose.distance)
  if (pose.target) checkVec3(`${verb} target`, pose.target)
}

/**
 * Create an orbit camera driving `camera`'s position and target, where
 * `camera` is a Scene or one of its Views (fov, near, and far stay yours
 * via its setCamera). The initial pose applies immediately. In a component
 * tree, prefer the `<OrbitCamera>` component: it drives the enclosing
 * scene or view through context, builds the anchor and occlusion hooks
 * from the scene, and takes an input map as a prop.
 */
export function createOrbitCamera(camera: OrbitTarget, options: OrbitCameraOptions = {}): OrbitCamera {
  if (!camera || typeof camera.setCamera !== "function" || typeof camera.camera !== "function" || typeof camera.size !== "function") {
    throw new Error("createOrbitCamera: the target needs setCamera(), camera() and size() (a Scene or a View)")
  }
  checkPose("initial", options)
  checkFollowOptions("createOrbitCamera", options.follow)
  checkOffset("createOrbitCamera", options.offset)
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
  let pushing = () => options.push ?? false

  // The auto-orbit switch: starts on when the initial rate is positive.
  // Plain state is the truth (update() reads it in the same block set()
  // wrote it); the signal notifies HUDs and the active() gate.
  let running = orbitSpeed() > 0
  let [orbiting, setOrbiting] = createSignal(running, { ownedWrite: true })
  // Whether the camera changed since the last update() (see the header).
  let changed = false
  // The motion in flight, the follow, the lanes and the occlusion return:
  // together the motion half of active() (core's createActivity).
  let motion: Motion | null = null
  let followAt: Vec3 | null = null
  let followHeading: number | null = null
  let followSettled = false
  // Seconds since the last rotate input, for the heading recentre.
  let rotateIdle = Infinity
  let lookahead = createLookahead()
  let lanes = createLanes()
  // setOrbitPoint's share of the offset lane, on top of options.offset.
  let pivotOffset: Vec2 = [0, 0]
  // The occlusion displacement: the final eye's distance while pulled in
  // (null when the pose's distance stands), easing back to null.
  let occluded: number | null = null
  let occlusionReturning = false
  let busy = () => motion !== null || (followAt !== null && !followSettled) || lanes.active() || occlusionReturning
  // The frame-loop gate (core's createActivity), built once the axes exist.
  let activity!: ReturnType<typeof createActivity>
  let notify = () => activity.notify()
  let interrupt = () => {
    motion = null
  }
  // A verb that writes the target owns it: the follow and the orbit
  // point's offset both go.
  let retarget = () => {
    followAt = null
    followHeading = null
    pivotOffset[0] = 0
    pivotOffset[1] = 0
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
  // The view basis for a pose: the unit vector from the target to the
  // eye, and the camera's right and up, written out from
  // eye() = target + distance * (ce*sa, se, ce*ca).
  let basis = (p: OrbitPoseState) => {
    let sa = Math.sin(p.azimuth)
    let ca = Math.cos(p.azimuth)
    let se = Math.sin(p.elevation)
    let ce = Math.cos(p.elevation)
    return {
      out: [ce * sa, se, ce * ca] as Vec3,
      right: [ca, 0, -sa] as Vec3,
      up: [-sa * se, ce, -ca * se] as Vec3,
      // The horizontal forward, for a ground-plane pan.
      ahead: [-sa, 0, -ca] as Vec3,
    }
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
  // World units per element height at the target's depth: the frustum's
  // height there, from the target camera's vertical fov.
  let worldPerHeightAt = (distance: number) => 2 * Math.tan((camera.camera().fov * Math.PI) / 360) * distance
  let worldPerHeight = () => worldPerHeightAt(pose.distance)
  let aspect = () => {
    let size = camera.size()
    return size.height > 0 ? size.width / size.height : 1
  }

  // ---- The lanes and the push: the final camera ----
  // The lane shift in world units: an offset of (ox, oy) view heights
  // shows the target ox right of centre and oy below it, so the look
  // point moves the other way along right and along up.
  let laneShift = (total: Vec2, p: OrbitPoseState): Vec3 => {
    let { right, up } = basis(p)
    let w = worldPerHeightAt(p.distance)
    let sx = -total[0] * w
    let sy = total[1] * w
    return [right[0] * sx + up[0] * sy, right[1] * sx + up[1] * sy, right[2] * sx + up[2] * sy]
  }
  let laneTotal = (): Vec2 => {
    let base = options.offset
    let t = lanes.total(base)
    t[0] += pivotOffset[0]
    t[1] += pivotOffset[1]
    return t
  }
  let last: { position: Vec3; target: Vec3 } = { position: eye(), target: copy(pose.target) }
  // The occlusion constraint at push: a free distance under the pose's
  // pulls the eye in at once; a longer one starts the eased return.
  let occlude = (target: Vec3, eyePos: Vec3) => {
    let hook = options.occluder
    let free = hook ? hook(copy(target), copy(eyePos)) : null
    if (free !== null && Number.isFinite(free) && free < pose.distance) {
      let d = Math.max(free, 0)
      if (occluded === null || d < occluded) occluded = d
      occlusionReturning = false
    } else if (occluded !== null) occlusionReturning = true
  }
  let push = () => {
    changed = true
    let shift = laneShift(laneTotal(), pose)
    let target: Vec3 = [pose.target[0] + shift[0], pose.target[1] + shift[1], pose.target[2] + shift[2]]
    let e = eye()
    let position: Vec3 = [e[0] + shift[0], e[1] + shift[1], e[2] + shift[2]]
    occlude(target, position)
    if (occluded !== null) {
      let { out } = basis(pose)
      position = [target[0] + out[0] * occluded, target[1] + out[1] * occluded, target[2] + out[2] * occluded]
    }
    last = { position, target }
    camera.setCamera({ position, target })
  }

  // Slide eye and target together along the camera's right and up (or
  // the ground plane's forward).
  let slide = (p: OrbitPoseState, right: number, up: number) => {
    let b = basis(p)
    let u = options.panPlane === "ground" ? b.ahead : b.up
    p.target = [p.target[0] + right * b.right[0] + up * u[0], p.target[1] + right * b.right[1] + up * u[1], p.target[2] + right * b.right[2] + up * u[2]]
  }
  // The target a zoom about `anchor` leaves when the distance goes from
  // `refDistance` (target `refTarget`) to `distance`: scaling eye and
  // target about the anchor by the distance ratio keeps it projecting to
  // the same pixel. The ratio is the one that actually applied after
  // clamping - once distance pins at a clamp the target stops moving too,
  // instead of sliding the view sideways under a dead zoom; with `push`
  // the overflow past the clamp moves through instead (below).
  let anchored = (anchor: Vec3, refTarget: Vec3, refDistance: number, distance: number): Vec3 => {
    let s = distance / refDistance
    return [anchor[0] + (refTarget[0] - anchor[0]) * s, anchor[1] + (refTarget[1] - anchor[1]) * s, anchor[2] + (refTarget[2] - anchor[2]) * s]
  }
  // The push: move eye and target together by `overflow` world units
  // (positive forward) along the ray from the eye through the anchor, or
  // the view axis. Along the anchor ray the anchor keeps its pixel.
  let pushThrough = (p: OrbitPoseState, overflow: number, anchor: Vec3 | null | undefined) => {
    if (overflow === 0) return
    let e = eyeOf(p)
    let dx: number
    let dy: number
    let dz: number
    if (anchor) {
      dx = anchor[0] - e[0]
      dy = anchor[1] - e[1]
      dz = anchor[2] - e[2]
    } else {
      dx = p.target[0] - e[0]
      dy = p.target[1] - e[1]
      dz = p.target[2] - e[2]
    }
    let len = Math.hypot(dx, dy, dz)
    if (!(len > 0)) return
    let s = overflow / len
    p.target = [p.target[0] + dx * s, p.target[1] + dy * s, p.target[2] + dz * s]
  }
  // The overflow of a wanted distance past the bounds for a write from
  // `prev`: positive when the step wanted to go closer than the floor.
  let overflowOf = (prev: number, wanted: number): number => {
    if (!pushing()) return 0
    let [lo, hi] = distanceBounds(prev)
    return wanted < lo ? lo - wanted : wanted > hi ? hi - wanted : 0
  }
  // Zoom by `ratio` (new distance over old, before clamping) about a world
  // anchor (null zooms toward the target), at once.
  let zoomAbout = (p: OrbitPoseState, ratio: number, anchor: Vec3 | null | undefined) => {
    let prev = p.distance
    let [lo, hi] = distanceBounds(prev)
    let wanted = prev * ratio
    p.distance = clampNum(wanted, lo, hi)
    if (anchor) p.target = anchored(anchor, p.target, prev, p.distance)
    pushThrough(p, overflowOf(prev, wanted), anchor)
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
  // The exact re-seat: the target becomes the point, the distance its
  // depth along the view axis, and the offset lane absorbs the point's
  // sideways offset from the FINAL eye, so position and orientation hold.
  let setOrbitPoint = (point: Vec3) => {
    let { out, right, up } = basis(pose)
    // The final eye without the shakes: where the picture is anchored.
    let baseX = options.offset?.[0] ?? 0
    let baseY = options.offset?.[1] ?? 0
    let shift = laneShift([baseX + pivotOffset[0], baseY + pivotOffset[1]], pose)
    let e = eye()
    let dx = e[0] + shift[0] - point[0]
    let dy = e[1] + shift[1] - point[1]
    let dz = e[2] + shift[2] - point[2]
    let depth = dx * out[0] + dy * out[1] + dz * out[2]
    if (!(depth > 0)) return
    // The sideways remainder lies in the view plane; the lane shift that
    // reproduces it at the new depth is its right and up components.
    let sx = dx - out[0] * depth
    let sy = dy - out[1] * depth
    let sz = dz - out[2] * depth
    let w = worldPerHeightAt(depth)
    let ox = -(sx * right[0] + sy * right[1] + sz * right[2]) / w
    let oy = (sx * up[0] + sy * up[1] + sz * up[2]) / w
    pose.distance = depth
    pose.target = copy(point)
    pivotOffset[0] = ox - baseX
    pivotOffset[1] = oy - baseY
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
    followSettled = false
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
    if (p.target) retarget()
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
  // A pushed overflow moves the goal's target instead, eased as a pose.
  let dampedZoom = (ratio: number, anchor: Vec3 | null) => {
    writeDamped(m => {
      let prev = m.distance
      let [lo, hi] = distanceBounds(prev)
      let wanted = prev * ratio
      m.distance = clampNum(wanted, lo, hi)
      m.anchor = anchor
      m.refTarget = copy(pose.target)
      m.refDistance = pose.distance
      m.target = anchor ? anchored(anchor, pose.target, pose.distance, m.distance) : copy(pose.target)
      let overflow = overflowOf(prev, wanted)
      if (overflow !== 0) {
        pushThrough(m, overflow, anchor)
        m.anchor = null
      }
    })
  }

  // ---- The follow: the framing stage ----
  let predicted: number[] = [0, 0, 0]
  let stepFollow = (dt: number) => {
    let point = lookahead.predict(followAt!, options.follow?.lookahead, dt, predicted)
    let { out, right, up } = basis(pose)
    let dx = point[0]! - pose.target[0]
    let dy = point[1]! - pose.target[1]
    let dz = point[2]! - pose.target[2]
    let r = dx * right[0] + dy * right[1] + dz * right[2]
    let u = dx * up[0] + dy * up[1] + dz * up[2]
    // Forward is toward the target from the eye: minus `out`.
    let f = -(dx * out[0] + dy * out[1] + dz * out[2])
    let w = worldPerHeight()
    let ww = w * aspect()
    let size = camera.size()
    let epsilon = size.height > 0 ? LAND_PX / size.height : LAND_PX
    // The point's offset from the view centre in view fractions, y down.
    let framed = frame([r / ww, -u / w], options.follow, dt, epsilon)
    let kz = easeStep(followRate(options.follow, "z"), dt)
    let depthRest = Math.abs(f) * (w > 0 ? 1 / w : 1) < epsilon
    let mz = depthRest ? f : f * kz
    let mx = framed.x * ww
    let mu = -framed.y * w
    if (framed.x !== 0 || framed.y !== 0 || mz !== 0) {
      mutateLive(p => {
        p.target = [
          p.target[0] + right[0] * mx + up[0] * mu - out[0] * mz,
          p.target[1] + right[1] * mx + up[1] * mu - out[1] * mz,
          p.target[2] + right[2] * mx + up[2] * mu - out[2] * mz,
        ]
      })
    }
    let moved = framed.x !== 0 || framed.y !== 0 || mz !== 0
    // The heading recentre: the azimuth eases onto the followed heading
    // by the shortest turn once the rotate input has rested for `wait`.
    let headingRest = true
    let h = options.follow?.heading
    if (followHeading !== null && !interacting() && rotateIdle >= (h?.wait ?? RECENTER_WAIT)) {
      let turn = Math.atan2(Math.sin(followHeading - pose.azimuth), Math.cos(followHeading - pose.azimuth))
      if (Math.abs(turn) >= HEADING_EPSILON) {
        let d = h?.damping ?? 1
        let k = d <= 0 ? 1 : easeStep(followRate({ damping: d }, "x"), dt)
        let step = Math.abs(turn) * (1 - k) < HEADING_EPSILON ? turn : turn * k
        mutateLive(p => rotate(p, step, 0))
        headingRest = Math.abs(turn - step) < HEADING_EPSILON
        moved = true
      }
    } else if (followHeading !== null) headingRest = false
    followSettled = framed.settled && depthRest && headingRest
    return moved
  }

  let axes = createAxes<OrbitAxes>(
    { rotate: "vec2", zoom: "axis", pan: "vec2", focus: "axis" },
    {
      onBegin: name => {
        // A finger landing holds a glide where it is.
        interrupt()
        if (name === "rotate") {
          rotateIdle = 0
          let anchor = options.rotateAnchor?.(view())
          if (anchor) setPivot(anchor)
        }
        if (name === "zoom") {
          pinchSeen = false
          pinchAnchor = null
        }
        notify()
      },
      onEnd: (name, velocity) => {
        // A lift at speed keeps turning or sliding (Three's enableDamping
        // glide): the damped goal is set the fling's whole travel ahead,
        // velocity over the ease rate, so the motion starts at exactly
        // the release speed and decays. Nothing with damping off, and
        // nothing for a rested finger (no velocity).
        if (velocity === undefined || damping() <= 0 || name === "zoom" || name === "focus") return
        let v = velocity as Vec2
        let travel = 1 / (GLIDE_EASE / damping())
        if (name === "rotate") {
          let rel = DRAG_TURNS * 2 * Math.PI * rotateSpeed()
          writeDamped(m => rotate(m, -v[0] * travel * rel, v[1] * travel * rel))
        } else {
          let w = worldPerHeight() * panSpeed()
          writeDamped(m => {
            slide(m, -v[0] * travel * w, v[1] * travel * w)
            m.anchor = null
          })
        }
        notify()
      },
      onNudge: (name, delta, focal) => {
        let bracketed = axes.inGesture(name)
        let eased = !bracketed && damping() > 0
        if (name === "rotate") {
          rotateIdle = 0
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
        } else if (name === "focus") {
          if ((delta as number) === 0) return
          let point = anchorAt(focal ?? [0.5, 0.5])
          if (point) glideTo({ target: point })
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
  activity = createActivity(busy, () => axes.active())
  let active = () => (orbiting() && orbitSpeed() !== 0) || activity.active()

  clampPose(pose, null)
  push()
  changed = false

  return {
    eye,
    camera: () => ({ position: copy(last.position), target: copy(last.target) }),
    pose: () => ({ azimuth: pose.azimuth, elevation: pose.elevation, distance: pose.distance, target: copy(pose.target) }),
    orbiting,
    active,
    axes,
    set(p) {
      checkPose("set", p)
      // The re-validate entry for live options too (set({}) after an
      // options change), so they are checked here as at creation.
      checkFollowOptions("createOrbitCamera", options.follow)
      checkOffset("createOrbitCamera", options.offset)
      if (p.azimuth !== undefined || p.elevation !== undefined || p.distance !== undefined || p.target) interrupt()
      if (p.target) retarget()
      if (p.azimuth !== undefined) pose.azimuth = p.azimuth
      if (p.elevation !== undefined) pose.elevation = p.elevation
      if (p.distance !== undefined) pose.distance = p.distance
      if (p.target) pose.target = copy(p.target)
      clampPose(pose, null)
      followSettled = false
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
        let vertical = (cam.fov * Math.PI) / 360
        let horizontal = Math.atan(Math.tan(vertical) * aspect())
        distance = radius / Math.sin(Math.min(vertical, horizontal))
      }
      let goal: OrbitPose = { target: center, distance }
      if (opts?.glide) {
        glideTo(goal)
        return
      }
      interrupt()
      retarget()
      pose.target = center
      pose.distance = distance
      clampPose(pose, null)
      push()
      notify()
    },
    follow(point, heading) {
      checkVec3("follow point", point)
      if (heading !== undefined) finite("follow heading", heading)
      // A commanded glide yields to the follow; a damped notch keeps
      // running (a per-frame follow must not cancel the zoom under it).
      if (motion !== null && motion.commanded) motion = null
      if (followAt === null) {
        followAt = copy(point)
        lookahead.reset()
      } else {
        followAt[0] = point[0]
        followAt[1] = point[1]
        followAt[2] = point[2]
      }
      followHeading = heading ?? null
      followSettled = false
      notify()
    },
    unfollow() {
      followAt = null
      followHeading = null
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
        rotateIdle = 0
        mutateLive(p => rotate(p, -rx * s, ry * s))
        moved = true
      } else rotateIdle += dt
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
      if (followAt !== null && !followSettled && dt > 0) {
        if (stepFollow(dt)) moved = true
      }
      if (lanes.active() && dt > 0) {
        lanes.step(dt)
        moved = true
      }
      if (occlusionReturning && occluded !== null && dt > 0) {
        // The eased return: toward the pose's distance, landing under a
        // pixel of travel at the target's depth.
        let gap = pose.distance - occluded
        let w = worldPerHeight()
        let size = camera.size()
        let epsilon = (size.height > 0 ? LAND_PX / size.height : LAND_PX) * w
        if (gap <= epsilon) {
          occluded = null
          occlusionReturning = false
        } else occluded += gap * easeStep(GLIDE_EASE, dt)
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
      if (anchor) checkVec3("zoomBy anchor", anchor)
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
      checkVec3("setPivot point", point)
      setPivot(point)
      push()
    },
    setOrbitPoint(point) {
      checkVec3("setOrbitPoint point", point)
      setOrbitPoint(point)
      push()
    },
    shake(strength, duration, opts) {
      checkShake("createOrbitCamera", strength, duration, opts)
      lanes.shake(strength, duration, opts)
      notify()
    },
  }
}
