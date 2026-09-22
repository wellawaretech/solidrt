// The 2d camera: Godot's Camera2D and Three's MapControls in one control
// over the layers' shared CameraUpdate - a pose (the world point at a
// viewport pivot, zoom, rotation), the contain clamp against world
// bounds, anchored zoom, eased glides (a wheel notch, glideTo, fit),
// follow through Cinemachine's framing (a dead zone, hard limits,
// damping per axis, lookahead), drag inertia, rotation about the pivot,
// and the lanes (a screen offset, shakes) - extracted so apps stop
// rebuilding the same hundred lines.
//
// The control runs the pipeline of okf/design/camera-controls.md: a
// SOURCE (an input nudge, a verb, or the follow) writes the pose; the
// FRAMING eases the followed point back into its zones; the LANES are
// summed on top of the pose; the CONSTRAINTS (the zoom range, the
// contain clamp) apply to pose plus the persistent offset and move the
// pose, and clip a shake at push without touching it; and the PUSH
// writes the final camera. pose() is the pose alone; camera() is the
// final camera as the layers received it (what projectCamera takes).
//
// The control is pure (ARCHITECTURE.md: controls through an abstraction,
// never direct event handling): it consumes three axes, `pan` (vec2),
// `zoom` (axis) and `roll` (axis), in device-free units, and exposes
// pose verbs for scripted moves. What drives the axes is the app's
// business - an input map over the view's pointer feed, a pad, a debug
// command - and `camera2dBindings` in input.ts is the standard wiring
// (drag pans, pinch and wheel zoom, twist rolls). This module imports no
// GUI or runtime module, so checks/camera2d-check.ts runs it headless.
//
// Conventions, decided against Godot's Camera2D, Unity's Cinemachine and
// Three's MapControls (plus the map/whiteboard libraries for the canvas
// half; okf/notes/2d-camera-conventions.md): the pose is "world point at
// the pivot" with the pivot defaulting to the viewport center (Godot's
// drag-center anchor, Cinemachine's screen position, a map's center);
// bounds CONTAIN the view and center an axis whose view is wider than
// the world (Cinemachine's confiner skeleton, tldraw's contain origin -
// Godot's left-wins is the worse behavior), at once for a direct write
// and eased when `world.damping` says so for a motion (Godot's
// limit_smoothed); limits ignore rotation, clamping the unrotated view
// rect (Godot's rule); every ease is exponential and frame-rate
// independent, zoom eased in log space so a long glide reads evenly.
//
// Input units: a `pan` delta is finger travel in viewport heights (the
// pointer feed's unit), applied as content travel - the world follows the
// finger; a `pan` rate slides at PAN_RATE viewport heights per second. A
// `zoom` delta is octaves about the gesture's focal point (a fraction of
// the viewport): bracketed by a gesture (a pinch) it applies exactly,
// unbracketed (a wheel notch) it retargets an eased glide, which is how
// the control tells a finger from an impulse. A `roll` delta is turns. A
// pan gesture's begin stops any glide (a finger landing on a gliding view
// holds it), its end flings with the release velocity the gesture
// measured (the pointer feed's, in viewport heights per second: one
// estimator for every recognizer, velocity.ts in core), decaying.
//
// Pose is plain mutable state; a nudge or a verb pushes it at once (the
// next paint carries the new camera, no frame loop needed for a drag),
// and update(dt) advances glides, follow, inertia, the lanes and the axis
// rates from the app's own onFrame. `active()` is the reactive frame-loop
// gate (a glide, fling, fit, follow, shake or damped bound in flight, or
// a rate driving); a resting camera costs nothing. update() pushes the
// pose to the layers only when it changed (one setCamera per driven
// layer: a shared-params write, however many sprites exist) and reports
// that, so per-frame dependents (labels re-projected via projectCamera)
// can follow the camera without recomputing every frame.

import { untrack } from "@solidjs/signals"
import { createAxes } from "@solidrt/core/input"
import type { Axes, Vec2 } from "@solidrt/core/input"
import { checkFollowOptions, checkOffset, checkShake, createActivity, createLanes, createLookahead, easeStep, frame, GLIDE_EASE, GLIDE_EPSILON } from "@solidrt/core/camera-control"
import type { FollowOptions } from "@solidrt/core/camera-control"
import { projectCamera, unprojectCamera } from "./camera.ts"
import type { CameraState, CameraUpdate } from "./camera.ts"

// A glide lands when the remaining travel is under this many screen
// pixels. Also the follow's settle threshold, the damped bound's, and,
// times the fling decay, the speed a fling stops at (the distance it
// would still travel).
const LAND_PX = 0.5
// Inertia after a drag release: velocity decay in e-foldings per second. A
// 1500 px/s flick travels 500 px; iOS scrolling decelerates at ~2, maps
// are snappier. The release speed itself comes from the gesture (a
// rested finger reads zero there), so nothing gates it here.
const FLING_DECAY = 3
// Rate axes at full deflection, per second: viewport heights of pan,
// octaves of zoom (1 = the zoom doubles each second), turns of roll.
const PAN_RATE = 1
const ZOOM_RATE = 1
const ROLL_RATE = 0.5

/** What a 2d camera drives: anything with the layers' `setCamera`. */
export type Camera2dTarget = { setCamera(update: CameraUpdate): void }

/** The control's parameters: what set() takes and pose() returns filled. */
export type Camera2dPose = {
  x?: number
  y?: number
  zoom?: number
  rotation?: number
}

export type Rect2d = { x: number; y: number; width: number; height: number }

export type Camera2dOptions = Camera2dPose & {
  /** Viewport size in layer pixels, read whenever the camera needs it (a
   * resize re-clamps at the next update). Return zeros while unknown -
   * clamping and fitting wait for a real size. */
  viewport: () => { width: number; height: number }
  /** World bounds (origin 0,0) the view is kept inside: an axis whose view
   * is wider than the world centers. Absent = unbounded. `damping` eases
   * a MOTION (a follow, a glide, a fling) into the bounds instead of
   * cutting it at the edge, as a multiple of the built-in settle time
   * (Godot's limit_smoothed); a direct write (a drag, set) always clamps
   * at once. Default 0: at once. */
  world?: { width: number; height: number; damping?: number }
  /** Zoom range. minZoom defaults to the fit zoom (whole world visible)
   * when `world` is given, else to no lower bound; maxZoom to no upper
   * bound. An explicit maxZoom below the fit zoom wins, the world then
   * floats centered. */
  minZoom?: number
  maxZoom?: number
  /** Where in the viewport the camera's world point sits, as fractions of
   * its size; default the center. glideTo and follow land their point
   * here and rotation turns about it. */
  pivot?: { x: number; y: number }
  /** How follow(x, y) chases its point (the framing stage): damping per
   * axis, the dead zone and hard limits as fractions of the viewport
   * centered on the pivot, lookahead. Read where it applies. */
  follow?: FollowOptions
  /** The offset lane: where the pose's world point is shown, in viewport
   * HEIGHTS from the pivot (x right, y down; [0, -0.25] shows it a
   * quarter of the height above the pivot - the look-up of a platformer).
   * Added on top of the pose at push, never in pose(); the bounds
   * contain pose plus offset by moving the pose. */
  offset?: Vec2
  /** Multipliers over the built-in pan, zoom (pinch, wheel and rate) and
   * roll sensitivities. */
  panSpeed?: number
  zoomSpeed?: number
  rollSpeed?: number
  /** How long an unbracketed zoom delta (a wheel notch) takes to ease in,
   * as a multiple of the built-in settle time: 1 (the default) is the
   * built-in ease, 2 coasts twice as long, 0 applies notches at once, as
   * a pinch does. The 3d controls' knob, same meaning. */
  damping?: number
  /** Whether a drag release keeps the view gliding (default true). */
  inertia?: boolean
}

export type Camera2dAxes = { pan: "vec2"; zoom: "axis"; roll: "axis" }

export type Camera2d = {
  /** The final camera as the layers received it (a fresh object per
   * call, every field set): the pose plus the lanes - the argument for
   * projectCamera/unprojectCamera. */
  camera(): CameraState
  /** Pose snapshot - the control's own four values, the shape set() takes
   * (the 3d controls' pose()); no lane in it. */
  pose(): Required<Camera2dPose>
  /** Merge a pose in (clamps apply) and push it. A snap: an x/y/zoom
   * write drops a glide or fling in flight and ends the follow; a
   * rotation write leaves them running. */
  set(pose: Camera2dPose): void
  /** Pan the content by a screen delta (a drag: positive dx slides the
   * world right, the camera point moves left), clamped, and push. */
  panBy(dx: number, dy: number): void
  /** End of a drag: keep gliding from its release velocity, screen pixels
   * per second of content travel, decaying (nothing while following, or
   * with `inertia` off). */
  release(velocity: Vec2): void
  /** Zoom by `factor` about a screen point - the world point under it
   * stays under it (factor > 1 zooms in), clamped to the zoom range and
   * the bounds, and push. `glide` eases there instead (a wheel notch),
   * retargeting the pending glide so notches compound. */
  zoomAt(sx: number, sy: number, factor: number, opts?: { glide?: boolean }): void
  /** Turn the world about the pivot by radians (clockwise, y-down) and push. */
  rollBy(angle: number): void
  /** Ease the pose to put world (x, y) at the pivot, at `zoom` (default
   * the current, or a pending wheel or double-tap zoom's target) and
   * `rotation` (default the current), landing exactly; a fling or a
   * follow yields. */
  glideTo(x: number, y: number, zoom?: number, rotation?: number): void
  /** Frame a world rect (default the whole world): centered in the view
   * at the zoom that fits it, clamped. A snap unless `glide`. Waits for
   * the viewport when its size is not known yet. */
  fit(rect?: Rect2d, opts?: { glide?: boolean }): void
  /** Chase a world point (the framing stage, options.follow): the pose
   * eases toward it per axis, through the dead zone and hard limits,
   * with lookahead. Call it every frame for a moving point; the control
   * settles once the point rests inside the zones. A pose glide yields;
   * an anchor glide (a wheel zoom) keeps running - zoom and follow are
   * separate axes. */
  follow(x: number, y: number): void
  unfollow(): void
  /** A shake on the lanes: `strength` is the peak displacement in
   * viewport heights, `duration` seconds, an optional frequency (cycles
   * per second) and direction (a unit [x, y], y down). Shakes sum and
   * decay; the pose is untouched, and at a world edge the shake is
   * clipped by the bounds (it never shows the outside). */
  shake(strength: number, duration: number, opts?: { frequency?: number; direction?: Vec2 }): void
  /** Stop a glide or fling in flight (a press landing). */
  interrupt(): void
  /** The world-space bounding box of what the viewport shows. */
  viewRect(): Rect2d
  /** Advance glides, follow, inertia, the lanes and the axis rates, then
   * push the camera to the layers if it changed. Call from onFrame with
   * the frame's dt in seconds; returns whether the camera changed since
   * the previous update (nudges and verbs included). */
  update(dt: number): boolean
  /**
   * Reactive: whether update() still has work - a glide, fling or
   * deferred fit in flight, a follow engaged and not settled, a shake
   * running, a damped bound still easing, a write not yet pushed, or an
   * axis rate driving. The frame-loop gate: run update(dt) on frames
   * while true, and nothing while false (a resting camera costs no frames).
   */
  active(): boolean
  /** The input abstraction: `pan` (vec2, viewport heights of travel, the
   * world follows), `zoom` (axis, octaves, positive in) and `roll` (axis,
   * turns). An input map drives it by name (InputMap.drive); an app may
   * add sources and nudge directly. */
  axes: Axes<Camera2dAxes>
}

type Glide = { kind: "anchor"; target: number; sx: number; sy: number; wx: number; wy: number } | { kind: "pose"; x: number; y: number; zoom: number; rotation: number }

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

function positive(what: string, v: number): void {
  if (!(Number.isFinite(v) && v > 0)) throw new Error(`createCamera2d: ${what} must be a positive number, got ${v}`)
}

function finite(what: string, v: number | undefined): void {
  if (v !== undefined && !Number.isFinite(v)) throw new Error(`createCamera2d: ${what} must be a finite number, got ${v}`)
}

function checkPose(verb: string, pose: Camera2dPose): void {
  finite(`${verb} x`, pose.x)
  finite(`${verb} y`, pose.y)
  finite(`${verb} rotation`, pose.rotation)
  if (pose.zoom !== undefined) positive(`${verb} zoom`, pose.zoom)
}

/** The non-pose options, validated at creation and again at every set()
 * (the live-option re-clamp entry). */
function checkOptions(options: Camera2dOptions): void {
  if (options.world) {
    positive("world.width", options.world.width)
    positive("world.height", options.world.height)
    if (options.world.damping !== undefined && !(Number.isFinite(options.world.damping) && options.world.damping >= 0)) {
      throw new Error(`createCamera2d: world.damping must be a non-negative number, got ${options.world.damping}`)
    }
  }
  if (options.minZoom !== undefined) positive("minZoom", options.minZoom)
  if (options.maxZoom !== undefined) positive("maxZoom", options.maxZoom)
  if (options.minZoom !== undefined && options.maxZoom !== undefined && options.minZoom > options.maxZoom) {
    throw new Error(`createCamera2d: minZoom ${options.minZoom} exceeds maxZoom ${options.maxZoom}`)
  }
  finite("pivot.x", options.pivot?.x)
  finite("pivot.y", options.pivot?.y)
  if (options.damping !== undefined && !(Number.isFinite(options.damping) && options.damping >= 0)) {
    throw new Error(`createCamera2d: damping must be a non-negative number, got ${options.damping}`)
  }
  checkFollowOptions("createCamera2d", options.follow)
  checkOffset("createCamera2d", options.offset)
}

/**
 * Create a 2d camera driving `target`'s camera, where `target` is a sprite
 * or record layer's view (or several: a view and its overlay layer's
 * share one camera), or anything else with the layers' `setCamera`, such
 * as a signal setter feeding `<TileLayer camera>`. The initial pose
 * applies immediately: the world fitted when `world` is given and no
 * `zoom`. The pose options are initial values; every other option is read
 * where it applies, so a caller holding the options object (or a props
 * object handing out getters) changes bounds, pivot, the follow's zones
 * and rates live - `set({})` re-clamps and pushes the pose under new
 * bounds. In a component tree, prefer `<Camera2d>`: it drives the nearest
 * view through context and takes an input map as a prop.
 */
export function createCamera2d(target: Camera2dTarget | Camera2dTarget[], options: Camera2dOptions): Camera2d {
  let targets = Array.isArray(target) ? target : [target]
  if (typeof options.viewport !== "function") throw new Error("createCamera2d: viewport must be a function returning { width, height }")
  checkOptions(options)
  checkPose("", options)
  // Everything below the pose is read from `options` where it applies
  // (the 3d orbit camera's rule): a prop object's getters stay live, so a
  // clamp, a pivot or a rate follows its prop, and set({}) re-clamps the
  // pose under new bounds.
  let world = () => options.world ?? null
  let maxZoom = () => options.maxZoom ?? Infinity
  let pivotFx = () => options.pivot?.x ?? 0.5
  let pivotFy = () => options.pivot?.y ?? 0.5
  let panSpeed = () => options.panSpeed ?? 1
  let zoomExponent = () => options.zoomSpeed ?? 1
  let rollSpeed = () => options.rollSpeed ?? 1
  let damping = () => options.damping ?? 1
  let inertia = () => options.inertia ?? true
  let boundsDamping = () => options.world?.damping ?? 0

  let w0 = world()
  let x = options.x ?? (w0 ? w0.width / 2 : 0)
  let y = options.y ?? (w0 ? w0.height / 2 : 0)
  let zoom = options.zoom ?? 1
  let rotation = options.rotation ?? 0

  // The viewport as last read; zeros mean unknown (nothing clamps or fits
  // until a real size arrives).
  let vw = 0
  let vh = 0
  let known = () => vw > 0 && vh > 0
  let readViewport = () => {
    let v = options.viewport()
    if (!(Number.isFinite(v.width) && Number.isFinite(v.height) && v.width >= 0 && v.height >= 0)) {
      throw new Error(`createCamera2d: viewport() must return non-negative finite width/height, got ${v.width}x${v.height}`)
    }
    vw = v.width
    vh = v.height
  }
  let px = () => vw * pivotFx()
  let py = () => vh * pivotFy()

  let minZoom = () => {
    let w = world()
    let fit = w && known() ? Math.min(vw / w.width, vh / w.height) : 0
    return Math.min(options.minZoom ?? fit, maxZoom())
  }

  // The pose as a camera (no lanes): what the framing and the anchors
  // measure against.
  let poseCamera = (): CameraState => ({ x, y, zoom, rotation, pivotX: px(), pivotY: py() })
  // Pose x/y that put world (wx, wy) under screen (sx, sy) at zoom z:
  // the inverse of projectCamera solved for the camera point.
  let anchorPose = (wx: number, wy: number, sx: number, sy: number, z: number): [number, number] => {
    let c = Math.cos(rotation)
    let s = Math.sin(rotation)
    let dx = sx - px()
    let dy = sy - py()
    return [wx - (dx * c + dy * s) / z, wy - (dy * c - dx * s) / z]
  }
  // ---- The lanes ----
  let lanes = createLanes()
  // The camera point for a pose point under a lane offset (viewport
  // heights): the pose's world point is shown at the pivot plus the
  // offset, so the point AT the pivot is the one that far back.
  let shifted = (cx: number, cy: number, z: number, ox: number, oy: number): [number, number] => {
    if (ox === 0 && oy === 0) return [cx, cy]
    let c = Math.cos(rotation)
    let s = Math.sin(rotation)
    let dx = ox * vh
    let dy = oy * vh
    return [cx - (dx * c + dy * s) / z, cy - (dy * c - dx * s) / z]
  }
  // The persistent lane only: what the bounds contain (moving the pose)
  // and what a zoom or glide lands against.
  let withOffset = (cx: number, cy: number, z: number): [number, number] => shifted(cx, cy, z, options.offset?.[0] ?? 0, options.offset?.[1] ?? 0)
  let steadyCamera = (): CameraState => {
    let [fx, fy] = withOffset(x, y, zoom)
    return { x: fx, y: fy, zoom, rotation, pivotX: px(), pivotY: py() }
  }
  // The final camera: the shakes on top, clipped by the bounds at push
  // (a shake never enters the pose and never shows the outside).
  let camera = (): CameraState => {
    let [ox, oy] = lanes.total(options.offset)
    let [fx, fy] = shifted(x, y, zoom, ox, oy)
    if (lanes.active()) [fx, fy] = contain(fx, fy, zoom)
    return { x: fx, y: fy, zoom, rotation, pivotX: px(), pivotY: py() }
  }

  // The contain clamp on an unrotated view rect (Godot's rule: limits
  // ignore rotation): returns the camera point, moved inside the world.
  let contain = (cx: number, cy: number, z: number): [number, number] => {
    let w = world()
    if (!w || !known()) return [cx, cy]
    let viewW = vw / z
    let viewH = vh / z
    let left = cx - px() / z
    let top = cy - py() / z
    left = viewW >= w.width ? (w.width - viewW) / 2 : clampNum(left, 0, w.width - viewW)
    top = viewH >= w.height ? (w.height - viewH) / 2 : clampNum(top, 0, w.height - viewH)
    return [left + px() / z, top + py() / z]
  }
  // The constraints on pose plus offset: the zoom range, then the contain
  // clamp of the camera point, which moves the pose by the same delta
  // (the offset stays). At once, or eased over dt when the bounds damp a
  // motion; leaves whether the pose is contained now in boundsSettled.
  let boundsSettled = true
  let clampTo = (dt: number | null) => {
    zoom = clampNum(zoom, minZoom(), maxZoom())
    let [fx, fy] = withOffset(x, y, zoom)
    let [cx, cy] = contain(fx, fy, zoom)
    let dx = cx - fx
    let dy = cy - fy
    if (dx === 0 && dy === 0) {
      boundsSettled = true
      return
    }
    let d = boundsDamping()
    if (dt === null || d <= 0 || Math.hypot(dx, dy) * zoom < LAND_PX) {
      x += dx
      y += dy
      boundsSettled = true
      return
    }
    let k = easeStep(GLIDE_EASE / d, dt)
    x += dx * k
    y += dy * k
    boundsSettled = false
  }
  let clamp = () => clampTo(null)
  // Slide the content by a screen delta: the camera point moves the
  // opposite way, the delta un-rotated into world axes.
  let shift = (dx: number, dy: number) => {
    let c = Math.cos(rotation)
    let s = Math.sin(rotation)
    x -= (dx * c + dy * s) / zoom
    y -= (dy * c - dx * s) / zoom
  }

  let dirty = false
  // Whether the camera changed since the last update() (pushes included).
  let changed = false
  let glide: Glide | null = null
  let fling: { vx: number; vy: number } | null = null
  let followAt: { x: number; y: number } | null = null
  // A follow that reached its target stops writing until something moves.
  let followSettled = false
  let lookahead = createLookahead()
  let predicted: number[] = [0, 0]
  // A fit that waits for the viewport (the default pose when a world is
  // given, or an explicit fit() before the size is known).
  let pendingFit: { rect: Rect2d | null; glide: boolean } | null = options.zoom === undefined && w0 ? { rect: null, glide: false } : null

  let touch = () => {
    dirty = true
    followSettled = false
  }
  let busy = () => dirty || glide !== null || fling !== null || (followAt !== null && !followSettled) || pendingFit !== null || lanes.active() || !boundsSettled
  // The frame-loop gate (core's createActivity), built once the axes
  // exist; notified at the end of every public entry.
  let activity!: ReturnType<typeof createActivity>
  let notify = () => activity.notify()
  let interrupt = () => {
    glide = null
    fling = null
    notify()
  }
  let apply = () => {
    let update = camera()
    for (let t of targets) t.setCamera(update)
    changed = true
  }
  // Push a pending write now: the synchronous half of update(dt).
  let flush = () => {
    if (!dirty) return
    dirty = false
    apply()
    notify()
  }

  let zoomTo = (sx: number, sy: number, next: number) => {
    let [wx, wy] = unprojectCamera(steadyCamera(), sx, sy)
    zoom = clampNum(next, minZoom(), maxZoom())
    let [nx, ny] = anchorPose(wx, wy, sx, sy, zoom)
    // anchorPose places the camera point; the pose sits the offset back
    // from it.
    let [lx, ly] = withOffset(0, 0, zoom)
    x = nx - lx
    y = ny - ly
    clamp()
    touch()
  }
  let glideTo = (tx: number, ty: number, tz?: number, tr?: number) => {
    finite("glideTo x", tx)
    finite("glideTo y", ty)
    finite("glideTo rotation", tr)
    if (tz !== undefined) positive("glideTo zoom", tz)
    readViewport()
    fling = null
    followAt = null
    // A pending anchor glide (a wheel notch, a double tap's octave) hands
    // its zoom over: the glide centres the point AND lands that zoom,
    // instead of the later verb cancelling the earlier gesture.
    let pending = glide !== null && glide.kind === "anchor" ? glide.target : zoom
    let z = clampNum(tz ?? pending, minZoom(), maxZoom())
    let [lx, ly] = withOffset(0, 0, z)
    let [cx, cy] = contain(tx + lx, ty + ly, z)
    glide = { kind: "pose", x: cx - lx, y: cy - ly, zoom: z, rotation: tr ?? rotation }
  }
  let fitNow = (rect: Rect2d | null, ease: boolean) => {
    let w = world()
    let r = rect ?? (w ? { x: 0, y: 0, width: w.width, height: w.height } : null)
    if (!r) throw new Error("createCamera2d: fit() needs a rect when the camera has no world")
    let z = clampNum(Math.min(vw / r.width, vh / r.height), minZoom(), maxZoom())
    let [nx, ny] = anchorPose(r.x + r.width / 2, r.y + r.height / 2, vw / 2, vh / 2, z)
    let [lx, ly] = withOffset(0, 0, z)
    if (ease) {
      glideTo(nx - lx, ny - ly, z)
      return
    }
    interrupt()
    followAt = null
    x = nx - lx
    y = ny - ly
    zoom = z
    clamp()
    touch()
  }
  let panBy = (dx: number, dy: number) => {
    readViewport()
    interrupt()
    shift(dx, dy)
    clamp()
    touch()
  }
  let release = (velocity: Vec2) => {
    if (!inertia() || followAt !== null) return
    if (velocity[0] === 0 && velocity[1] === 0) return
    fling = { vx: velocity[0], vy: velocity[1] }
  }
  let zoomAt = (sx: number, sy: number, factor: number, ease: boolean) => {
    positive("zoomAt factor", factor)
    readViewport()
    if (!ease || damping() <= 0) {
      interrupt()
      zoomTo(sx, sy, zoom * factor)
      return
    }
    fling = null
    if (glide !== null && glide.kind === "pose") glide = null
    // Notches compound on the pending target, so a fast scroll is one
    // long push; the anchor is the world point under the pointer now.
    let from = glide !== null ? glide.target : zoom
    let target = clampNum(from * factor, minZoom(), maxZoom())
    let [wx, wy] = unprojectCamera(steadyCamera(), sx, sy)
    glide = { kind: "anchor", target, sx, sy, wx, wy }
  }
  let rollBy = (angle: number) => {
    finite("rollBy angle", angle)
    rotation += angle
    clamp()
    touch()
  }
  // The focal of a zoom nudge in viewport pixels, the pivot when the
  // gesture has none.
  let focalPx = (focal: Vec2 | undefined): [number, number] => (focal ? [focal[0] * vw, focal[1] * vh] : [px(), py()])

  // ---- The follow: the framing stage ----
  let stepFollow = (dt: number): boolean => {
    let point = lookahead.predict([followAt!.x, followAt!.y], options.follow?.lookahead, dt, predicted)
    // The point's offset from the pivot in viewport fractions, measured
    // against the POSE camera: the lanes shift the picture afterwards.
    let [sx, sy] = projectCamera(poseCamera(), point[0]!, point[1]!)
    let framed = frame([(sx - px()) / vw, (sy - py()) / vh], options.follow, dt, LAND_PX / vh)
    let moved = framed.x !== 0 || framed.y !== 0
    if (moved) {
      shift(-framed.x * vw, -framed.y * vh)
      clampTo(dt)
      dirty = true
    }
    followSettled = framed.settled
    return moved
  }

  let axes = createAxes<Camera2dAxes>(
    { pan: "vec2", zoom: "axis", roll: "axis" },
    {
      onBegin: name => {
        if (name !== "pan") return
        interrupt()
      },
      onEnd: (name, velocity) => {
        if (name !== "pan") return
        // Viewport heights per second of finger travel to screen pixels
        // per second of content travel, the pan delta's own mapping.
        let v = (velocity as Vec2 | undefined) ?? [0, 0]
        release([v[0] * vh * panSpeed(), v[1] * vh * panSpeed()])
        notify()
      },
      onNudge: (name, delta, focal) => {
        readViewport()
        if (name === "pan") {
          let d = delta as Vec2
          panBy(d[0] * vh * panSpeed(), d[1] * vh * panSpeed())
        } else if (name === "zoom") {
          let [sx, sy] = focalPx(focal)
          zoomAt(sx, sy, Math.pow(2, (delta as number) * zoomExponent()), !axes.inGesture("zoom"))
        } else {
          rollBy((delta as number) * 2 * Math.PI * rollSpeed())
        }
        flush()
        notify()
      },
    },
  )
  activity = createActivity(busy, () => axes.active())
  let active = activity.active

  readViewport()
  clamp()
  if (pendingFit && known()) {
    fitNow(pendingFit.rect, pendingFit.glide)
    pendingFit = null
  }
  dirty = false
  apply()
  changed = false
  notify()

  return {
    camera,
    pose: () => ({ x, y, zoom, rotation }),
    active,
    axes,
    set(pose) {
      checkPose("set", pose)
      // The re-clamp entry for live options too (set({}) after a bounds
      // change), so the options are validated here as at creation.
      checkOptions(options)
      readViewport()
      if (pose.x !== undefined || pose.y !== undefined || pose.zoom !== undefined) {
        interrupt()
        followAt = null
      }
      if (pose.x !== undefined) x = pose.x
      if (pose.y !== undefined) y = pose.y
      if (pose.zoom !== undefined) zoom = pose.zoom
      if (pose.rotation !== undefined) rotation = pose.rotation
      clamp()
      touch()
      flush()
    },
    panBy(dx, dy) {
      finite("panBy dx", dx)
      finite("panBy dy", dy)
      panBy(dx, dy)
      flush()
    },
    release(velocity) {
      if (!Array.isArray(velocity) || velocity.length !== 2) throw new Error(`createCamera2d: release() takes [vx, vy], got ${JSON.stringify(velocity)}`)
      finite("release vx", velocity[0])
      finite("release vy", velocity[1])
      release(velocity)
      notify()
    },
    zoomAt(sx, sy, factor, opts) {
      finite("zoomAt sx", sx)
      finite("zoomAt sy", sy)
      zoomAt(sx, sy, factor, opts?.glide ?? false)
      flush()
      notify()
    },
    rollBy(angle) {
      rollBy(angle)
      flush()
    },
    glideTo(tx, ty, tz, tr) {
      glideTo(tx, ty, tz, tr)
      notify()
    },
    fit(rect, opts) {
      readViewport()
      if (!known()) {
        pendingFit = { rect: rect ?? null, glide: opts?.glide ?? false }
        notify()
        return
      }
      fitNow(rect ?? null, opts?.glide ?? false)
      flush()
      notify()
    },
    follow(tx, ty) {
      finite("follow x", tx)
      finite("follow y", ty)
      // A pose glide yields to the follow; an anchor glide (a wheel zoom)
      // keeps running, as a notch itself lets one - a per-frame follow of
      // a moving target must not cancel the zoom under it.
      if (glide !== null && glide.kind === "pose") glide = null
      fling = null
      if (followAt === null) {
        followAt = { x: tx, y: ty }
        lookahead.reset()
      } else {
        followAt.x = tx
        followAt.y = ty
      }
      followSettled = false
      notify()
    },
    unfollow() {
      followAt = null
      notify()
    },
    shake(strength, duration, opts) {
      checkShake("createCamera2d", strength, duration, opts)
      lanes.shake(strength, duration, opts)
      notify()
    },
    interrupt,
    viewRect() {
      readViewport()
      let cam = camera()
      let corners = [unprojectCamera(cam, 0, 0), unprojectCamera(cam, vw, 0), unprojectCamera(cam, 0, vh), unprojectCamera(cam, vw, vh)]
      let minX = Infinity
      let minY = Infinity
      let maxX = -Infinity
      let maxY = -Infinity
      for (let [cx, cy] of corners) {
        minX = Math.min(minX, cx)
        minY = Math.min(minY, cy)
        maxX = Math.max(maxX, cx)
        maxY = Math.max(maxY, cy)
      }
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY }
    },
    update(dt) {
      finite("update dt", dt)
      let pw = vw
      let ph = vh
      readViewport()
      if (vw !== pw || vh !== ph) {
        // The pivot moved with the viewport; the world point stays under
        // it. A resize can also change the fit zoom, hence the re-clamp.
        if (pendingFit !== null && known()) {
          fitNow(pendingFit.rect, pendingFit.glide)
          pendingFit = null
        }
        clamp()
        touch()
      }
      if (dt > 0) {
        // The axis rates: content travel in viewport heights per second,
        // zoom in octaves per second about the pivot, roll in turns.
        let [rx, ry] = untrack(() => axes.rate("pan"))
        if (rx !== 0 || ry !== 0) {
          interrupt()
          shift(rx * PAN_RATE * panSpeed() * vh * dt, ry * PAN_RATE * panSpeed() * vh * dt)
          clamp()
          touch()
        }
        let rz = untrack(() => axes.rate("zoom"))
        if (rz !== 0) {
          interrupt()
          zoomTo(px(), py(), zoom * Math.pow(2, rz * ZOOM_RATE * zoomExponent() * dt))
        }
        let rr = untrack(() => axes.rate("roll"))
        if (rr !== 0) {
          rotation += rr * ROLL_RATE * rollSpeed() * 2 * Math.PI * dt
          clamp()
          touch()
        }
      }
      if (fling !== null && dt > 0) {
        shift(fling.vx * dt, fling.vy * dt)
        let decay = Math.exp(-FLING_DECAY * dt)
        fling.vx *= decay
        fling.vy *= decay
        if (Math.hypot(fling.vx, fling.vy) < LAND_PX * FLING_DECAY) fling = null
        clampTo(dt)
        touch()
      }
      if (glide !== null && dt > 0) {
        // A wheel notch's glide coasts as `damping` says; a commanded
        // glide (glideTo, fit) at the built-in rate.
        let k = easeStep(glide.kind === "anchor" ? GLIDE_EASE / damping() : GLIDE_EASE, dt)
        if (glide.kind === "anchor") {
          let gap = Math.log(glide.target / zoom)
          zoom = Math.abs(gap) < GLIDE_EPSILON ? glide.target : zoom * Math.exp(gap * k)
          let [nx, ny] = anchorPose(glide.wx, glide.wy, glide.sx, glide.sy, zoom)
          let [lx, ly] = withOffset(0, 0, zoom)
          x = nx - lx
          y = ny - ly
          if (zoom === glide.target) glide = null
        } else {
          let gap = Math.log(glide.zoom / zoom)
          let turn = glide.rotation - rotation
          let landed = Math.abs(gap) < GLIDE_EPSILON && Math.hypot(glide.x - x, glide.y - y) * zoom < LAND_PX && Math.abs(turn) * Math.hypot(vw, vh) < LAND_PX
          if (landed) {
            x = glide.x
            y = glide.y
            zoom = glide.zoom
            rotation = glide.rotation
            glide = null
          } else {
            zoom *= Math.exp(gap * k)
            x += (glide.x - x) * k
            y += (glide.y - y) * k
            rotation += turn * k
          }
        }
        clampTo(dt)
        touch()
      }
      if (followAt !== null && !followSettled && dt > 0 && known()) {
        stepFollow(dt)
      }
      if (lanes.active() && dt > 0) {
        lanes.step(dt)
        dirty = true
      }
      if (!boundsSettled && dt > 0 && glide === null && fling === null && (followAt === null || followSettled)) {
        // A damped bound still easing after the motion that crossed it
        // has landed: finish the way in.
        clampTo(dt)
        dirty = true
      }
      if (dirty) {
        dirty = false
        apply()
      }
      notify()
      let result = changed
      changed = false
      return result
    },
  }
}
