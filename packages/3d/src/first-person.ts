// A first-person camera for a scene: a position plus yaw/pitch, look and
// move in one control - Unity's FirstPersonController shape, rather than
// Three's split into PointerLockControls (look only) and a hand-written
// key loop. The control is pure (ARCHITECTURE.md: controls through an
// abstraction, never direct event handling): it consumes three axes,
// `look` (vec2), `move` (vec2) and `rise` (axis), in device-free units,
// and exposes pose verbs for scripted moves. Mouse look under pointer
// lock, drag-to-look on touch, WASD, sticks: all of that is the app's
// wiring through an input map, and `firstPersonBindings` in input.ts is
// the standard set.
//
// `look` deltas are element heights of travel (a drag across the element
// sweeps DRAG_TURNS turns) or the pointer feed's mouse-motion
// equivalent; a rate (a stick) turns at LOOK_RATE turns per second at
// full deflection. `move` is [right, forward] in the screen convention
// (forward is -y, as a stick pushed up reads): a rate walks at
// `moveSpeed`, a delta is a step in world units. `rise` moves along
// world up at the same speed, in fly mode only. Walking flattens the
// forward vector onto the ground plane at fixed height; flying moves
// along the view direction.
//
// The pose moves on its own in one way only: toward a goal pose set by
// glideTo, eased by update(dt) (motion.ts, the orbit camera's motion).
// Any input drops the glide - a walker who touches the controls has the
// camera back - and a pose write through set() lands at once and drops
// it too. There is no damping here: this control's unbracketed delta is
// mouse motion under pointer lock, and easing THAT is the thing a
// first-person player will not forgive.
//
// Pose is plain mutable state; a nudge or a verb pushes it at once (a
// mouse move under lock needs no frame loop), and update(dt) integrates
// the rates and the glide from the app's own onFrame. Only `active` is
// reactive: the frame-loop gate, true while any rate reads non-zero (a
// held key, a deflected stick) or a glide is in flight, so a still scene
// renders nothing new. update() returns whether the pose changed since
// the previous update.
//
// Collision is deliberately absent: a camera control cannot know the
// level. Clamp or reject positions through `clampPosition`, which a
// glide consults every frame as a walk does every step.
//
// Options are read where they apply, not copied out: `fly`, `moveSpeed`,
// `lookSpeed`, the pitch clamps and `clampPosition` are re-read from the
// options object on every input or update, so a caller may change a field
// (or hand in an object of getters, which is what `<FirstPersonCamera>`
// does with its props) and the next move sees it - walk and fly are one
// control, not two mounts. Only the initial pose is copied at creation;
// later pose changes go through set() and the verbs.

import { createMemo, createSignal, untrack } from "@solidjs/signals"
import { createAxes } from "@solidrt/core/input"
import type { Axes, Vec2 } from "@solidrt/core/input"
import type { CameraUpdate } from "./camera.ts"
import type { Vec3 } from "./math.ts"
import { easeStep, GLIDE_EASE, GLIDE_EPSILON } from "./motion.ts"

// One element height of drag sweeps this many turns of look (half a turn:
// a drag across the screen turns the walker around).
const DRAG_TURNS = 0.5
// Look rate at full stick deflection, turns per second.
const LOOK_RATE = 0.4
// Default walking speed, world units per second.
const MOVE_SPEED = 3
// Default `boostSpeed`: the multiplier on moveSpeed while `boost` is held.
const BOOST_SPEED = 2
// Pitch clamps stop short of the poles so the look direction never
// degenerates against world up.
const PITCH_LIMIT = Math.PI / 2 - 0.01

/** What a first-person camera drives: a Scene, or one of its Views. */
export type FirstPersonTarget = { setCamera(update: CameraUpdate): void }

/** A full pose: what pose() returns. */
export type FirstPersonPoseState = { position: Vec3; yaw: number; pitch: number }

/** A partial pose: what set() and glideTo() take. */
export type FirstPersonPose = Partial<FirstPersonPoseState>

/** The pose fields (position, yaw, pitch) are initial values, copied at
 * creation and changed through set() afterwards. Every other field is
 * live: read from this object where it applies, so a change takes effect
 * on the next input or update. */
export type FirstPersonCameraOptions = FirstPersonPose & {
  /** Pitch clamps, radians; the defaults stop just short of the poles. */
  minPitch?: number
  maxPitch?: number
  /** Movement speed in world units per second (default 3). */
  moveSpeed?: number
  /** Multiplier on `moveSpeed` while the `boost` axis reads non-zero (a
   * held Shift, a pressed stick; default 2): the sprint, walking and
   * flying alike. Scales rates only; a `move` delta is a step in world
   * units regardless. */
  boostSpeed?: number
  /** Multiplier over the built-in look sensitivities (drags, mouse motion
   * and rates alike). */
  lookSpeed?: number
  /** Walk (default) keeps the height fixed and moves along the ground
   * projection of the view; fly moves along the view itself and lets
   * `rise` move along world up. Toggling it on a running control keeps
   * the pose. */
  fly?: boolean
  /** Constrain where a move may put the eye: called with the eye the
   * move asks for and the eye it starts from (both fresh arrays), returns
   * the position to use - a level's bounds, a floor height, a collision
   * controller's `moveAndSlide` over the difference. A glide consults it
   * every frame. Look does not. */
  clampPosition?: (next: Vec3, current: Vec3) => Vec3
}

export type FirstPersonAxes = { look: "vec2"; move: "vec2"; rise: "axis"; boost: "axis" }

export type FirstPersonCamera = {
  /** Eye position (a fresh array per call). */
  eye(): Vec3
  /** Unit look direction for the current pose (a fresh array per call). */
  forward(): Vec3
  /** Pose snapshot - the shape debug commands return and set() takes. */
  pose(): FirstPersonPoseState
  /** Merge a pose in (clamps apply) and push it: a snap, dropping a glide
   * in flight when it writes a pose field. */
  set(pose: FirstPersonPose): void
  /** Ease to a pose (the fields given, the rest as they are; the pitch
   * clamp applies, `clampPosition` every frame) inside update(dt).
   * Dropped by any input and by a set() that writes a pose field; a new
   * glideTo retargets. Yaw eases to the number given, not the shortest
   * turn: pass the turn you mean. */
  glideTo(pose: FirstPersonPose): void
  /** Whether update(dt) has work: any axis rate non-zero, or a glide in
   * flight. The frame-loop gate, reactive. */
  active(): boolean
  /** Integrate the axis rates and the glide over dt seconds and push any
   * pose change; returns whether the pose changed since the previous
   * update (nudges and verbs included). */
  update(dt: number): boolean
  /** The input abstraction: `look` (vec2, element heights of drag / turns
   * per second), `move` (vec2 [right, forward], forward = -y; world units
   * per delta, `moveSpeed` per second), `rise` (axis, fly only) and
   * `boost` (axis: non-zero multiplies the move and rise rates by
   * `boostSpeed`; a button binding reads 1 while held). */
  axes: Axes<FirstPersonAxes>
  /** Turn by radians (yaw positive left, pitch positive up; clamps
   * apply) and push. */
  lookBy(yaw: number, pitch: number): void
  /** Step by world units in the walker's frame: right, forward (ground
   * projection when walking, the view direction when flying) and up
   * (world up, fly mode only), through `clampPosition`, and push. */
  moveBy(right: number, forward: number, up?: number): void
}

// The goal pose a glide eases toward (see the header).
type Motion = FirstPersonPoseState & {
  /** The fraction of the initial gap still open. */
  remaining: number
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
let copy = (v: Vec3): Vec3 => [v[0], v[1], v[2]]

function finite(what: string, v: number): void {
  if (!Number.isFinite(v)) throw new Error(`createFirstPersonCamera: ${what} must be a finite number, got ${v}`)
}

function checkPose(verb: string, pose: FirstPersonPose): void {
  if (pose.yaw !== undefined) finite(`${verb} yaw`, pose.yaw)
  if (pose.pitch !== undefined) finite(`${verb} pitch`, pose.pitch)
  if (pose.position) for (let i = 0; i < 3; i++) finite(`${verb} position[${i}]`, pose.position[i]!)
}

/**
 * Create a first-person camera driving `camera`'s position and target,
 * where `camera` is a Scene or one of its Views (fov, near, and far stay
 * yours via its setCamera). The initial pose applies immediately. In a
 * component tree, prefer the `<FirstPersonCamera>` component: it drives
 * the enclosing scene or view through context and takes an input map as
 * a prop.
 */
export function createFirstPersonCamera(camera: FirstPersonTarget, options: FirstPersonCameraOptions = {}): FirstPersonCamera {
  if (!camera || typeof camera.setCamera !== "function") throw new Error("createFirstPersonCamera: the target needs setCamera() (a Scene or a View)")
  checkPose("initial", options)
  let position: Vec3 = options.position ? copy(options.position) : [0, 1.6, 0]
  let yaw = options.yaw ?? 0
  let pitch = options.pitch ?? 0
  // Everything below the pose is read from `options` where it applies.
  let lookSpeed = () => options.lookSpeed ?? 1
  let moveSpeed = () => options.moveSpeed ?? MOVE_SPEED
  let boostSpeed = () => options.boostSpeed ?? BOOST_SPEED
  let clampedPitch = (v: number) => clampNum(v, options.minPitch ?? -PITCH_LIMIT, options.maxPitch ?? PITCH_LIMIT)
  let clampPitch = () => {
    pitch = clampedPitch(pitch)
  }
  let clampedPosition = (next: Vec3): Vec3 => (options.clampPosition ? options.clampPosition(next, copy(position)) : next)
  let changed = false
  // The glide in flight, and its half of active(): a signal every entry
  // that starts or drops it refreshes (ownedWrite: entries run from
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

  let forward = (): Vec3 => {
    let cp = Math.cos(pitch)
    return [-Math.sin(yaw) * cp, Math.sin(pitch), -Math.cos(yaw) * cp]
  }
  let push = () => {
    changed = true
    let f = forward()
    camera.setCamera({ position: copy(position), target: [position[0] + f[0], position[1] + f[1], position[2] + f[2]] })
  }
  let look = (dYaw: number, dPitch: number) => {
    yaw += dYaw
    pitch += dPitch
    clampPitch()
  }
  // A step in the walker's frame. Walking projects the view onto the
  // ground plane, so looking down does not slow the walk; right is always
  // horizontal; up applies in fly mode only.
  let step = (right: number, ahead: number, up: number) => {
    let fly = options.fly ?? false
    let f = forward()
    let fx = fly ? f[0] : -Math.sin(yaw)
    let fy = fly ? f[1] : 0
    let fz = fly ? f[2] : -Math.cos(yaw)
    let rx = Math.cos(yaw)
    let rz = -Math.sin(yaw)
    let rise = fly ? up : 0
    position = clampedPosition([position[0] + fx * ahead + rx * right, position[1] + fy * ahead + rise, position[2] + fz * ahead + rz * right])
  }
  // One frame of the glide: the ease closes the gap by the same fraction
  // on every component, the last fraction snapping to the goal.
  let glideStep = (dt: number) => {
    let m = motion!
    let k = easeStep(GLIDE_EASE, dt)
    m.remaining *= 1 - k
    if (m.remaining < GLIDE_EPSILON) {
      yaw = m.yaw
      pitch = m.pitch
      position = clampedPosition(copy(m.position))
      motion = null
    } else {
      yaw += (m.yaw - yaw) * k
      pitch += (m.pitch - pitch) * k
      position = clampedPosition([position[0] + (m.position[0] - position[0]) * k, position[1] + (m.position[1] - position[1]) * k, position[2] + (m.position[2] - position[2]) * k])
    }
    clampPitch()
  }

  let axes = createAxes<FirstPersonAxes>(
    { look: "vec2", move: "vec2", rise: "axis", boost: "axis" },
    {
      onBegin: () => {
        interrupt()
        notify()
      },
      onNudge: (name, delta) => {
        interrupt()
        if (name === "look") {
          let d = delta as Vec2
          let rel = DRAG_TURNS * 2 * Math.PI * lookSpeed()
          look(-d[0] * rel, -d[1] * rel)
        } else if (name === "move") {
          let d = delta as Vec2
          step(d[0], -d[1], 0)
        } else {
          step(0, 0, delta as number)
        }
        push()
        notify()
      },
    },
  )
  let active = createMemo(() => motionActive() || axes.active())

  clampPitch()
  push()
  changed = false

  return {
    eye: () => copy(position),
    forward,
    pose: () => ({ position: copy(position), yaw, pitch }),
    active,
    axes,
    set(pose) {
      checkPose("set", pose)
      if (pose.position || pose.yaw !== undefined || pose.pitch !== undefined) interrupt()
      if (pose.position) position = copy(pose.position)
      if (pose.yaw !== undefined) yaw = pose.yaw
      if (pose.pitch !== undefined) pitch = pose.pitch
      clampPitch()
      push()
      notify()
    },
    glideTo(pose) {
      checkPose("glideTo", pose)
      motion = {
        position: pose.position ? copy(pose.position) : copy(position),
        yaw: pose.yaw ?? yaw,
        pitch: clampedPitch(pose.pitch ?? pitch),
        remaining: 1,
      }
      notify()
    },
    update(dt) {
      finite("update dt", dt)
      let moved = false
      let [lx, ly] = untrack(() => axes.rate("look"))
      if (lx !== 0 || ly !== 0) {
        let rate = LOOK_RATE * 2 * Math.PI * lookSpeed() * dt
        interrupt()
        look(-lx * rate, -ly * rate)
        moved = true
      }
      let [mx, my] = untrack(() => axes.rate("move"))
      let rise = untrack(() => axes.rate("rise"))
      if (mx !== 0 || my !== 0 || rise !== 0) {
        // A held boost (any non-zero read) scales the rates, not a nudge.
        let boost = untrack(() => axes.rate("boost")) !== 0 ? boostSpeed() : 1
        let speed = moveSpeed() * boost * dt
        interrupt()
        step(mx * speed, -my * speed, rise * speed)
        moved = true
      }
      if (motion !== null && dt > 0) {
        glideStep(dt)
        moved = true
      }
      if (moved) push()
      notify()
      let result = changed
      changed = false
      return result
    },
    lookBy(dYaw, dPitch) {
      finite("lookBy yaw", dYaw)
      finite("lookBy pitch", dPitch)
      interrupt()
      look(dYaw, dPitch)
      push()
      notify()
    },
    moveBy(right, ahead, up = 0) {
      finite("moveBy right", right)
      finite("moveBy forward", ahead)
      finite("moveBy up", up)
      interrupt()
      step(right, ahead, up)
      push()
      notify()
    },
  }
}
