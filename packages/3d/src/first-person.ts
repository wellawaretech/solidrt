// A first-person camera for a scene: a position plus yaw/pitch, with mouse
// look under pointer lock, drag-to-look without it (the touch path), and
// walk (or fly) from keys and gamepad sticks - Unity's FirstPersonController
// shape, look and move in one control, rather than Three's split into
// PointerLockControls (look only) and a hand-written key loop.
//
// Look input has three sources, all summed into the same yaw/pitch:
// - pointer-move deltas (movementX/movementY) while the pointer is locked,
//   the desktop mouse-look path; the control never engages the lock itself
//   (click-to-lock and Escape-to-release are window-level decisions that
//   stay app code, see lockPointer in @solidrt/core);
// - a one-finger drag through core's transform recognizer while NOT locked,
//   so a touch viewport looks around with the arena arbitration the orbit
//   camera has, and viewport-relative like it;
// - the right stick, read from core's gamepads() at update time.
// Move input: WASD/arrows (physical codes AND logical keys, so a layout
// or a synthetic event without a code both work), Q/E for down/up in fly
// mode, and the left stick. Walking flattens the forward vector onto the
// ground plane at fixed height; flying moves along the view direction.
//
// Pose is plain mutable state advanced by update(dt) from the app's own
// onFrame, as in orbit.ts: only `active` is reactive - whether anything is
// held or deflected, what a frame loop gates on; it derives from the held
// count and core's gamepads() so a stick moved while the loop is off wakes
// it - while the pose moves at frame rate and bypasses reactivity. Held
// keys cannot be polled (there is no key-state accessor in core), so the
// control tracks them from the down/up pair; an `onBlur` handler drops
// them all, because the up never arrives once focus has left.
//
// Collision is deliberately absent: a camera control cannot know the
// level. Clamp or reject positions through `clampPosition`.
//
// Options are read where they apply, not copied out: `fly`, `moveSpeed`,
// `lookSpeed`, the pitch clamps, `viewport` and `clampPosition` are
// re-read from the options object on every input or update, so a caller
// may change a field (or hand in an object of getters, which is what
// `<FirstPersonCamera>` does with its props) and the next move sees it -
// walk and fly are one control, not two mounts. Only the initial pose is
// copied at creation; later pose changes go through set().

import { createMemo, createSignal } from "@solidjs/signals"
import { createTransform, gamepads, pointerLocked } from "@solidrt/core"
import type { KeyEvent, PointerEvent } from "@solidrt/core"
import type { CameraUpdate } from "./camera.ts"
import type { Vec3 } from "./math.ts"

// Baseline sensitivities at lookSpeed 1. Mouse look is radians per moved
// logical pixel under lock (Three's PointerLockControls figure); with a
// `viewport` the drag is viewport-relative, one viewport height sweeping
// DRAG_TURNS turns, and without one it falls back to the mouse figure.
// The stick turns at LOOK_STICK radians/second at full deflection, with
// a dead zone below which a resting stick reads as zero.
const LOOK_MOUSE = 0.002
const DRAG_TURNS = 0.5
const LOOK_STICK = 2.5
const STICK_DEADZONE = 0.15
// Default walking speed, world units per second.
const MOVE_SPEED = 3
// Pitch clamps stop short of the poles so the look direction never
// degenerates against world up.
const PITCH_LIMIT = Math.PI / 2 - 0.01

/** What a first-person camera drives: a Scene, or one of its Views. */
export type FirstPersonTarget = { setCamera(update: CameraUpdate): void }

/** The pose fields (position, yaw, pitch) are initial values, copied at
 * creation and changed through set() afterwards. Every other field is
 * live: read from this object where it applies, so a change takes effect
 * on the next input or update. */
export type FirstPersonCameraOptions = {
  /** Initial eye position (default [0, 1.6, 0]: standing height at the
   * origin). */
  position?: Vec3
  /** Initial look, radians. Yaw 0 faces -z (the camera default), positive
   * turns left; pitch 0 is level, positive looks up. */
  yaw?: number
  pitch?: number
  /** Pitch clamps, radians; the defaults stop just short of the poles. */
  minPitch?: number
  maxPitch?: number
  /** Movement speed in world units per second (default 3). */
  moveSpeed?: number
  /** Multiplier over the built-in mouse, drag and stick look rates. */
  lookSpeed?: number
  /** Walk (default) keeps the height fixed and moves along the ground
   * projection of the view; fly moves along the view itself and arms Q/E
   * for down/up (bound either way, inert while walking). Toggling it on a
   * running control keeps the pose and the held keys. */
  fly?: boolean
  /** The viewport a drag lives in: the input element's own laid-out
   * height (the frame the recognizer's deltas arrive in); makes the drag
   * viewport-relative. Null while unknown. */
  viewport?: () => { height: number } | null
  /** Constrain where a move may put the eye: called with the eye the
   * move asks for and the eye it starts from (both fresh arrays), returns
   * the position to use - a level's bounds, a floor height, a collision
   * controller's `moveAndSlide` over the difference. Look does not
   * consult it. */
  clampPosition?: (next: Vec3, current: Vec3) => Vec3
}

export type FirstPersonPose = {
  position?: Vec3
  yaw?: number
  pitch?: number
}

export type FirstPersonCamera = {
  /** Eye position (a fresh array per call). */
  eye(): Vec3
  /** Unit look direction for the current pose (a fresh array per call). */
  forward(): Vec3
  /** Pose snapshot - the shape debug commands return and set() takes. */
  pose(): { position: Vec3; yaw: number; pitch: number }
  /** Merge a pose in (clamps apply); reaches the scene at the next
   * update(). */
  set(pose: FirstPersonPose): void
  /** Whether a movement key is held or a stick deflected - what a frame
   * loop should run on. Reactive (signal-backed). */
  active(): boolean
  /** Integrate held keys and sticks over dt seconds and push any pose
   * change to the driven camera; returns whether the pose changed. */
  update(dt: number): boolean
  /** Spread onto the element that receives input. Keys route through the
   * focused node, so that element must hold focus (or be the window). */
  handlers: {
    onPointerDown(e: PointerEvent): void
    onPointerMove(e: PointerEvent): void
    onPointerUp(e: PointerEvent): void
    onKeyDown(e: KeyEvent): void
    onKeyUp(e: KeyEvent): void
    onBlur(): void
  }
}

type Move = "forward" | "back" | "left" | "right" | "up" | "down"

// Physical codes first (layout-independent), then the logical keys a
// synthetic event or an odd layout reports.
const KEY_MOVES: Record<string, Move> = {
  KeyW: "forward", ArrowUp: "forward", w: "forward", W: "forward",
  KeyS: "back", ArrowDown: "back", s: "back", S: "back",
  KeyA: "left", ArrowLeft: "left", a: "left", A: "left",
  KeyD: "right", ArrowRight: "right", d: "right", D: "right",
  KeyE: "up", e: "up", E: "up",
  KeyQ: "down", q: "down", Q: "down",
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
let deadzone = (v: number) => (Math.abs(v) < STICK_DEADZONE ? 0 : v)

/**
 * Create a first-person camera driving `camera`'s position and target,
 * where `camera` is a Scene or one of its Views (fov, near, and far stay
 * yours via its setCamera). The initial pose applies immediately. In a
 * component tree, prefer the `<FirstPersonCamera>` component: it wires
 * scene, input, focus, viewport and the frame loop through the Scene
 * context.
 */
export function createFirstPersonCamera(camera: FirstPersonTarget, options: FirstPersonCameraOptions = {}): FirstPersonCamera {
  let position: Vec3 = options.position ? [options.position[0], options.position[1], options.position[2]] : [0, 1.6, 0]
  let yaw = options.yaw ?? 0
  let pitch = options.pitch ?? 0
  // Everything below the pose is read from `options` where it applies.
  let lookSpeed = () => options.lookSpeed ?? 1
  let clampPitch = () => {
    pitch = clampNum(pitch, options.minPitch ?? -PITCH_LIMIT, options.maxPitch ?? PITCH_LIMIT)
  }

  let held = new Set<Move>()
  let [heldCount, setHeldCount] = createSignal(0)
  let dirty = false

  let forward = (): Vec3 => {
    let cp = Math.cos(pitch)
    return [-Math.sin(yaw) * cp, Math.sin(pitch), -Math.cos(yaw) * cp]
  }
  let apply = () => {
    let f = forward()
    camera.setCamera({ position: [position[0], position[1], position[2]], target: [position[0] + f[0], position[1] + f[1], position[2] + f[2]] })
  }
  let look = (dx: number, dy: number) => {
    yaw -= dx
    pitch -= dy
    clampPitch()
    dirty = true
  }
  let moveBy = (dx: number, dy: number, dz: number) => {
    let next: Vec3 = [position[0] + dx, position[1] + dy, position[2] + dz]
    position = options.clampPosition ? options.clampPosition(next, [position[0], position[1], position[2]]) : next
    dirty = true
  }

  // The stick axes of every connected pad, summed: a game with one player
  // sees its one pad, and a pad at rest contributes nothing.
  let sticks = () => {
    let s = { moveX: 0, moveY: 0, lookX: 0, lookY: 0 }
    for (let pad of gamepads()) {
      if (!pad) continue
      s.moveX += deadzone(pad.axes.leftX ?? 0)
      s.moveY += deadzone(pad.axes.leftY ?? 0)
      s.lookX += deadzone(pad.axes.rightX ?? 0)
      s.lookY += deadzone(pad.axes.rightY ?? 0)
    }
    return s
  }
  let active = createMemo(() => {
    let s = sticks()
    return heldCount() > 0 || s.moveX !== 0 || s.moveY !== 0 || s.lookX !== 0 || s.lookY !== 0
  })

  // The unlocked look: a one-finger drag, arena-arbitrated. Two fingers
  // have no first-person meaning and keep looking.
  let transform = createTransform({
    onTransformMove: t => {
      let vp = options.viewport?.() ?? null
      let rate = (vp !== null ? (DRAG_TURNS * 2 * Math.PI) / vp.height : LOOK_MOUSE) * lookSpeed()
      look(t.dx * rate, t.dy * rate)
      apply()
      dirty = false
    },
  })

  clampPitch()
  apply()

  return {
    eye: () => [position[0], position[1], position[2]],
    forward,
    pose: () => ({ position: [position[0], position[1], position[2]], yaw, pitch }),
    active,
    set(pose) {
      if (pose.position) position = [pose.position[0], pose.position[1], pose.position[2]]
      if (pose.yaw !== undefined) yaw = pose.yaw
      if (pose.pitch !== undefined) pitch = pose.pitch
      clampPitch()
      dirty = true
    },
    update(dt) {
      let s = sticks()
      if (s.lookX !== 0 || s.lookY !== 0) {
        let rate = LOOK_STICK * lookSpeed() * dt
        look(s.lookX * rate, s.lookY * rate)
      }
      // Key axes: -1..1 per axis, the stick added on top (stick up is -y,
      // the web convention, so forward is -leftY), then clamped so a key
      // and a stick together do not exceed full speed.
      let ahead = clampNum((held.has("forward") ? 1 : 0) - (held.has("back") ? 1 : 0) - s.moveY, -1, 1)
      let side = clampNum((held.has("right") ? 1 : 0) - (held.has("left") ? 1 : 0) + s.moveX, -1, 1)
      // One read per update, so a mode toggle lands between steps, never
      // between the rise and the heading of the same step.
      let fly = options.fly ?? false
      let rise = fly ? (held.has("up") ? 1 : 0) - (held.has("down") ? 1 : 0) : 0
      if (ahead !== 0 || side !== 0 || rise !== 0) {
        let step = (options.moveSpeed ?? MOVE_SPEED) * dt
        let f = forward()
        // Walking projects the view onto the ground plane, so looking down
        // does not slow the walk; right is always horizontal.
        let fx = fly ? f[0] : -Math.sin(yaw)
        let fy = fly ? f[1] : 0
        let fz = fly ? f[2] : -Math.cos(yaw)
        let rx = Math.cos(yaw)
        let rz = -Math.sin(yaw)
        moveBy((fx * ahead + rx * side) * step, (fy * ahead + rise) * step, (fz * ahead + rz * side) * step)
      }
      if (!dirty) return false
      dirty = false
      apply()
      return true
    },
    handlers: {
      onPointerDown(e) {
        if (!pointerLocked()) transform.handlers.onPointerDown(e)
      },
      onPointerMove(e) {
        if (pointerLocked()) {
          let rate = LOOK_MOUSE * lookSpeed()
          look(e.movementX * rate, e.movementY * rate)
          apply()
          dirty = false
        } else {
          transform.handlers.onPointerMove(e)
        }
      },
      onPointerUp(e) {
        transform.handlers.onPointerUp(e)
      },
      onKeyDown(e) {
        let move = KEY_MOVES[e.code] ?? KEY_MOVES[e.key]
        if (!move) return
        held.add(move)
        setHeldCount(held.size)
      },
      onKeyUp(e) {
        let move = KEY_MOVES[e.code] ?? KEY_MOVES[e.key]
        if (!move) return
        held.delete(move)
        setHeldCount(held.size)
      },
      onBlur() {
        held.clear()
        setHeldCount(0)
      },
    },
  }
}
