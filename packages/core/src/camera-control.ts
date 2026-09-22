// The pure half every camera control shares (okf/design/camera-controls.md):
// the ease, the framing of a followed point, and the lanes. Runtime-free,
// so the 2d and 3d controls import it and their headless checks run it
// on the bare flux binary.
//
// Ease: every self-driven motion (a glide, a damped wheel notch, a follow)
// closes the gap to its goal by the same exponential fraction per frame,
// 1 - e^(-rate * dt), never a fixed fraction, so it is frame-rate
// independent and reads the same in 2d and 3d.
//
// Framing (Cinemachine's Position Composer): a followed point is measured
// as an offset from the pivot in viewport fractions. Inside the dead zone
// the camera does not respond; in the band between the dead zone and the
// hard limits (the soft zone) it eases the point back toward the dead
// edge, damped per axis; beyond the hard limits the point is clamped
// inside at once, whatever the damping - the guarantee that a fast target
// never leaves the screen. Lookahead projects the point ahead along its
// velocity, the velocity estimated from successive points and smoothed,
// so a noisy input does not jitter the camera.
//
// Lanes: additive offsets summed on top of the pose at push time and
// never written into it, so pose() stays what the app set or the follow
// reached: a persistent screen offset (Babylon's targetScreenOffset,
// Godot's offset), and shakes - decaying oscillations that sum
// (Cinemachine Impulse, Phaser's shake). Both in viewport fractions; the
// control maps them to its own space at push.

import { createMemo, createSignal } from "@solidjs/signals"

export type Vec2 = [number, number]

/**
 * A control's frame-loop gate: `active()` is reactive, true while the
 * control's own motion (`busy()`: a glide, a follow, a shake, an eased
 * return) or any of its axis rates (`rates()`) has work. `notify()` is
 * called at the end of every entry that may start or stop a motion. The
 * plain flag is the truth and the signal mirrors it: a signal read
 * between flushes reports the value before the queued writes, so a
 * control comparing against its own signal drops the write back to
 * false once a true is queued (a check rig that never flushes mid-run
 * shows it; a live app, which flushes every frame, hides it).
 */
export function createActivity(busy: () => boolean, rates: () => boolean) {
  // ownedWrite: entries run from component bodies and handlers alike.
  let [motion, setMotion] = createSignal(false, { ownedWrite: true })
  let on = false
  let active = createMemo(() => motion() || rates())
  return {
    active,
    notify() {
      let now = busy()
      if (now !== on) {
        on = now
        setMotion(now)
      }
    },
  }
}

// E-foldings per second toward a goal: high enough that a wheel notch
// reads as one push, low enough to look smooth.
export const GLIDE_EASE = 9
// A motion lands - snaps to its goal and stops - once this fraction of
// its initial gap remains: under a pixel on any pose component a glide
// moves through at ordinary distances.
export const GLIDE_EPSILON = 0.001
// Follow damping, e-foldings per second (Godot's default
// position_smoothing_speed).
export const FOLLOW_EASE = 5
// A shake's default oscillation rate, cycles per second: a hand-held
// judder, the range Cinemachine's 6D shake presets sit in.
export const SHAKE_FREQUENCY = 12

/** The fraction of the remaining gap an ease at `rate` closes over dt. */
export let easeStep = (rate: number, dt: number): number => 1 - Math.exp(-rate * dt)

/** A zone's size as fractions of the viewport, centred on the pivot. */
export type Zone = { width: number; height: number }

/** How a follow chases its point (the framing stage). Every field is
 * read where it applies, so a live options object changes it on the
 * next update. */
export type FollowOptions = {
  /** How long the follow takes to close in, as a multiple of the built-in
   * settle time (1, the default): 2 trails twice as lazily, 0 snaps.
   * Per axis as `{ x, y }` (2d) or `{ x, y, z }` (3d: right, up,
   * forward) - a platformer's tight horizontal and lazy vertical. */
  damping?: number | { x?: number; y?: number; z?: number }
  /** The point roams inside it without moving the camera. Default 0. */
  deadZone?: Zone
  /** The point is clamped inside it at once; the band between the dead
   * zone and this is where the damping applies. Default: none (the whole
   * plane); Cinemachine's default for a game camera is 0.8 x 0.8. */
  hardLimits?: Zone
  /** Frame ahead of the point by `time` seconds of its velocity, the
   * velocity smoothed over `smoothing` seconds (default 0: raw). */
  lookahead?: { time: number; smoothing?: number }
}

/** A per-axis correction: how far the camera must move its view so the
 * point returns toward the pivot, in the offset's units, and whether the
 * framing has settled (nothing left to move that a frame would show). */
export type Framing = { x: number; y: number; settled: boolean }

let dampingOf = (d: FollowOptions["damping"], axis: "x" | "y" | "z"): number => {
  if (d === undefined) return 1
  if (typeof d === "number") return d
  return d[axis] ?? 1
}

/** The ease rate of one follow axis, e-foldings per second (Infinity for
 * damping 0: at once). */
export let followRate = (options: FollowOptions | undefined, axis: "x" | "y" | "z"): number => {
  let d = dampingOf(options?.damping, axis)
  return d <= 0 ? Infinity : FOLLOW_EASE / d
}

// One axis of the zones: the correction for an offset `o` from the pivot
// given the half-sizes of the dead zone and the hard limits, eased by k
// (the fraction of the soft overshoot closed this frame; 1 snaps).
let frameAxis = (o: number, dead: number, hard: number, k: number): number => {
  let a = Math.abs(o)
  if (a <= dead) return 0
  let sign = o < 0 ? -1 : 1
  let over = Math.min(a, hard) - dead
  let beyond = a > hard ? a - hard : 0
  return sign * (beyond + over * k)
}

/**
 * Frame a followed point: `offset` is the point's position relative to
 * the pivot in viewport fractions (x right, y down), the result the
 * amount the view must move so the point comes back inside the zones,
 * in the same units (subtract it from the point's screen position). The
 * hard part applies whole; the soft part closes by the axis's ease over
 * dt. `epsilon` (viewport fractions) is the remainder under which the
 * soft part snaps and the framing reports settled, so a resting follow
 * writes nothing.
 */
export function frame(offset: Vec2, options: FollowOptions | undefined, dt: number, epsilon: number): Framing {
  let dead = options?.deadZone
  let hard = options?.hardLimits
  let dw = (dead?.width ?? 0) / 2
  let dh = (dead?.height ?? 0) / 2
  let hw = hard ? Math.max(hard.width / 2, dw) : Infinity
  let hh = hard ? Math.max(hard.height / 2, dh) : Infinity
  let kx = easeStep(followRate(options, "x"), dt)
  let ky = easeStep(followRate(options, "y"), dt)
  let x = frameAxis(offset[0], dw, hw, kx)
  let y = frameAxis(offset[1], dh, hh, ky)
  // What the soft part would still leave after this frame: under epsilon
  // the whole overshoot applies and the framing rests.
  let restX = frameAxis(offset[0], dw, hw, 1)
  let restY = frameAxis(offset[1], dh, hh, 1)
  if (Math.hypot(restX - x, restY - y) < epsilon) return { x: restX, y: restY, settled: true }
  return { x, y, settled: false }
}

/** Zone fractions must be within 0..1 (a zone wider than the viewport
 * frames nothing); throws with the control's prefix. */
export function checkFollowOptions(prefix: string, options: FollowOptions | undefined): void {
  if (!options) return
  let fraction = (what: string, v: number | undefined) => {
    if (v !== undefined && !(Number.isFinite(v) && v >= 0 && v <= 1)) throw new Error(`${prefix}: ${what} must be within 0..1, got ${v}`)
  }
  let nonNegative = (what: string, v: number | undefined) => {
    if (v !== undefined && !(Number.isFinite(v) && v >= 0)) throw new Error(`${prefix}: ${what} must be a non-negative number, got ${v}`)
  }
  fraction("follow.deadZone.width", options.deadZone?.width)
  fraction("follow.deadZone.height", options.deadZone?.height)
  fraction("follow.hardLimits.width", options.hardLimits?.width)
  fraction("follow.hardLimits.height", options.hardLimits?.height)
  let d = options.damping
  if (typeof d === "number") nonNegative("follow.damping", d)
  else if (d) {
    nonNegative("follow.damping.x", d.x)
    nonNegative("follow.damping.y", d.y)
    nonNegative("follow.damping.z", d.z)
  }
  nonNegative("follow.lookahead.time", options.lookahead?.time)
  nonNegative("follow.lookahead.smoothing", options.lookahead?.smoothing)
}

/**
 * The lookahead filter over a followed point of any dimension: feed it
 * the point each frame and read the predicted point - the point plus
 * `time` seconds of its velocity, the velocity estimated from successive
 * points and smoothed over `smoothing` seconds (an exponential average).
 * reset() forgets the velocity (a teleport, a new follow).
 */
export function createLookahead() {
  let prev: number[] | null = null
  let velocity: number[] = []
  return {
    predict(point: ArrayLike<number>, options: FollowOptions["lookahead"], dt: number, out: number[]): number[] {
      let n = point.length
      if (prev === null || prev.length !== n) {
        prev = Array.from(point)
        velocity = new Array(n).fill(0)
      }
      if (dt > 0) {
        let smoothing = options?.smoothing ?? 0
        let k = smoothing > 0 ? easeStep(1 / smoothing, dt) : 1
        for (let i = 0; i < n; i++) {
          let v = (point[i]! - prev[i]!) / dt
          velocity[i] = velocity[i]! + (v - velocity[i]!) * k
          prev[i] = point[i]!
        }
      }
      let time = options?.time ?? 0
      for (let i = 0; i < n; i++) out[i] = point[i]! + velocity[i]! * time
      return out
    },
    reset() {
      prev = null
    },
  }
}

// A shake in flight: a damped oscillation along a direction, its amplitude
// falling linearly to zero over the duration.
type Shake = { strength: number; duration: number; frequency: number; dx: number; dy: number; phase: number; t: number }

/**
 * The lanes: a persistent `offset` (viewport fractions, x right, y down)
 * and shakes summed over it. `step(dt)` advances the shakes, `total()`
 * is what a control adds to its pose at push, `active()` whether a shake
 * is still running (the frame-loop gate's lane half).
 */
export function createLanes() {
  let shakes: Shake[] = []
  let total: Vec2 = [0, 0]
  return {
    /** Start a shake: `strength` is its peak in viewport fractions,
     * `duration` seconds, `frequency` cycles per second (default
     * SHAKE_FREQUENCY). `direction` fixes its axis (a unit vector, y
     * down); default random, so repeated hits read as a judder, not a
     * metronome. Shakes sum. */
    shake(strength: number, duration: number, opts?: { frequency?: number; direction?: Vec2 }) {
      let dir = opts?.direction
      let angle = Math.random() * 2 * Math.PI
      let dx = dir ? dir[0] : Math.cos(angle)
      let dy = dir ? dir[1] : Math.sin(angle)
      let len = Math.hypot(dx, dy) || 1
      shakes.push({ strength, duration, frequency: opts?.frequency ?? SHAKE_FREQUENCY, dx: dx / len, dy: dy / len, phase: dir ? Math.PI / 2 : Math.random() * 2 * Math.PI, t: 0 })
    },
    step(dt: number) {
      if (shakes.length === 0) return
      let kept: Shake[] = []
      for (let s of shakes) {
        s.t += dt
        if (s.t < s.duration) kept.push(s)
      }
      shakes = kept
    },
    /** The summed lane offset now: `offset` plus every running shake. */
    total(offset: Vec2 | undefined): Vec2 {
      total[0] = offset?.[0] ?? 0
      total[1] = offset?.[1] ?? 0
      for (let s of shakes) {
        let amplitude = s.strength * (1 - s.t / s.duration)
        let wave = Math.sin(2 * Math.PI * s.frequency * s.t + s.phase) * amplitude
        total[0] += s.dx * wave
        total[1] += s.dy * wave
      }
      return total
    },
    active: () => shakes.length > 0,
    clear() {
      shakes = []
    },
  }
}

export type Lanes = ReturnType<typeof createLanes>

/** A lane offset must be a finite pair; a shake's strength and duration
 * finite and positive. Throws with the control's prefix. */
export function checkShake(prefix: string, strength: number, duration: number, opts?: { frequency?: number; direction?: Vec2 }): void {
  if (!(Number.isFinite(strength) && strength >= 0)) throw new Error(`${prefix}: shake strength must be a non-negative number, got ${strength}`)
  if (!(Number.isFinite(duration) && duration > 0)) throw new Error(`${prefix}: shake duration must be a positive number of seconds, got ${duration}`)
  if (opts?.frequency !== undefined && !(Number.isFinite(opts.frequency) && opts.frequency > 0)) throw new Error(`${prefix}: shake frequency must be a positive number, got ${opts.frequency}`)
  if (opts?.direction !== undefined && !(Array.isArray(opts.direction) && opts.direction.length === 2 && Number.isFinite(opts.direction[0]) && Number.isFinite(opts.direction[1]))) {
    throw new Error(`${prefix}: shake direction must be [dx, dy], got ${JSON.stringify(opts.direction)}`)
  }
}

export function checkOffset(prefix: string, offset: Vec2 | undefined): void {
  if (offset === undefined) return
  if (!(Array.isArray(offset) && offset.length === 2 && Number.isFinite(offset[0]) && Number.isFinite(offset[1]))) throw new Error(`${prefix}: offset must be [x, y] viewport fractions, got ${JSON.stringify(offset)}`)
}

// ---- Shots and blends ----
//
// Cinemachine's architecture of a game camera, over the controls: several
// cameras (shots), each driven by its own control into a RECORDING target
// instead of the scene, a priority that says which is live, and a blend
// between the outgoing output and the new live shot over a fixed time.
// The blender owns the one push to the real target; a live shot's push
// goes straight through at rest, so a still camera costs no frames, and
// a blend runs on update(dt). A switch mid-blend starts from the output
// of that moment, so a quick back-and-forth never jumps.

// The default blend time, seconds: short enough for a game cut to read as
// deliberate, long enough to see (Cinemachine's default is 2 s, a
// cinematic pace; maps use a third of a second).
export const BLEND_SECONDS = 0.5

/** A shot's recording target: what its control drives. `camera()` is
 * the shot's latest full camera (the control's partial updates merged
 * over the initial one). */
export type ShotTarget<C> = { setCamera(update: Partial<C>): void; camera(): C }

export type ShotBlendOptions = {
  /** The default blend time in seconds when the live shot changes
   * (BLEND_SECONDS); 0 cuts. */
  blend?: number
}

/**
 * The generic blender: `push` writes the output camera to the real
 * target, `mix(a, b, t)` interpolates two full cameras (the package
 * decides the space: zoom in log space, ortho extents, ...), `initial`
 * is the camera every shot starts from. A shot is live when it is the
 * enabled one with the highest priority (the most recently enabled on a
 * tie); with none enabled nothing is pushed. The blend eases in and out
 * (smoothstep) over its time, and lands exactly.
 */
export function createShotBlend<C extends object>(push: (camera: C) => void, mix: (a: C, b: C, t: number) => C, initial: C, options: ShotBlendOptions = {}) {
  type Shot = { name: string; priority: number; latest: C; enabled: boolean; order: number }
  let shots = new Map<string, Shot>()
  let live: Shot | null = null
  // The output as last pushed, the start of a blend when frozen.
  let current: C = { ...initial }
  let from: C | null = null
  let progress = 0
  let duration = 0
  let order = 0
  let activity = createActivity(() => from !== null, () => false)
  let choose = (): Shot | null => {
    let best: Shot | null = null
    for (let s of shots.values()) {
      if (!s.enabled) continue
      if (best === null || s.priority > best.priority || (s.priority === best.priority && s.order > best.order)) best = s
    }
    return best
  }
  let smooth = (t: number) => t * t * (3 - 2 * t)
  let pushNow = () => {
    if (live === null) return
    current = from === null ? { ...live.latest } : mix(from, live.latest, smooth(progress))
    push(current)
  }
  let relive = (blend: number | undefined) => {
    let seconds = blend ?? options.blend ?? BLEND_SECONDS
    if (!(Number.isFinite(seconds) && seconds >= 0)) throw new Error(`createShotBlend: blend must be a non-negative number of seconds, got ${seconds}`)
    let next = choose()
    if (next === live) return
    if (live !== null && next !== null && seconds > 0) {
      from = { ...current }
      progress = 0
      duration = seconds
    } else from = null
    live = next
    pushNow()
    activity.notify()
  }
  let named = (name: string, what: string): Shot => {
    let s = shots.get(name)
    if (!s) throw new Error(`createShotBlend: ${what}("${name}"): no such shot (${[...shots.keys()].join(", ") || "none"})`)
    return s
  }
  return {
    /** Declare a shot; returns the target its control drives. */
    shot(name: string, opts?: { priority?: number }): ShotTarget<C> {
      if (typeof name !== "string" || name.length === 0) throw new Error("createShotBlend: a shot needs a name")
      if (shots.has(name)) throw new Error(`createShotBlend: shot "${name}" exists`)
      let s: Shot = { name, priority: opts?.priority ?? 0, latest: { ...initial }, enabled: false, order: 0 }
      shots.set(name, s)
      return {
        setCamera(update) {
          Object.assign(s.latest, update)
          if (s === live && from === null) pushNow()
        },
        camera: () => ({ ...s.latest }),
      }
    },
    /** Enable a shot: it goes live when its priority wins, blending from
     * the current output over `blend` seconds (the default otherwise). */
    activate(name: string, opts?: { blend?: number }) {
      let s = named(name, "activate")
      s.enabled = true
      s.order = ++order
      relive(opts?.blend)
    },
    /** Disable a shot: the next by priority goes live, blended. */
    deactivate(name: string, opts?: { blend?: number }) {
      named(name, "deactivate").enabled = false
      relive(opts?.blend)
    },
    /** Forget a shot (an unmounted component): disabled first, so the
     * next by priority goes live, blended; its name is free again. */
    remove(name: string, opts?: { blend?: number }) {
      named(name, "remove").enabled = false
      relive(opts?.blend)
      shots.delete(name)
    },
    /** The live shot's name, or null. */
    live: () => (live === null ? null : live.name),
    /** The output as last pushed (a fresh object). */
    camera: (): C => ({ ...current }),
    /** Reactive: whether a blend is in flight - the frame-loop gate. */
    active: activity.active,
    /** Advance a blend over dt seconds and push; returns whether the
     * output changed. A live shot's own pushes need no update. */
    update(dt: number): boolean {
      if (!Number.isFinite(dt)) throw new Error(`createShotBlend: update dt must be a finite number, got ${dt}`)
      if (from === null || dt <= 0) return false
      progress = Math.min(1, progress + dt / duration)
      pushNow()
      if (progress >= 1) from = null
      activity.notify()
      return true
    },
  }
}

export type ShotBlend<C extends object> = ReturnType<typeof createShotBlend<C>>
