// The 2d camera: Godot's Camera2D and Three's MapControls in one control
// over the layers' shared CameraUpdate - a pose (the world point at a
// viewport pivot, zoom, rotation), the contain clamp against world
// bounds, anchored zoom, eased glides (a wheel notch, glideTo, fit),
// follow with damping and a dead zone, drag inertia, rotation about the
// pivot - extracted so apps stop rebuilding the same hundred lines.
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
// half): the pose is "world point at the pivot" with the pivot defaulting
// to the viewport center (Godot's drag-center anchor, Cinemachine's screen
// position, a map's center); bounds CONTAIN the view and center an axis
// whose view is wider than the world (Cinemachine's confiner skeleton,
// tldraw's contain origin - Godot's left-wins is the worse behavior);
// limits ignore rotation, clamping the unrotated view rect (Godot's rule);
// every ease is exponential and frame-rate independent, zoom eased in log
// space so a long glide reads evenly.
//
// Input units: a `pan` delta is finger travel in viewport heights (the
// pointer feed's unit), applied as content travel - the world follows the
// finger; a `pan` rate slides at PAN_RATE viewport heights per second. A
// `zoom` delta is octaves about the gesture's focal point (a fraction of
// the viewport): bracketed by a gesture (a pinch) it applies exactly,
// unbracketed (a wheel notch) it retargets an eased glide, which is how
// the control tells a finger from an impulse. A `roll` delta is turns. A
// pan gesture's begin stops any glide (a finger landing on a gliding view
// holds it), its end flings with the drag's velocity.
//
// Pose is plain mutable state; a nudge or a verb pushes it at once (the
// next paint carries the new camera, no frame loop needed for a drag),
// and update(dt) advances glides, follow, inertia and the axis rates from
// the app's own onFrame. `active()` is the reactive frame-loop gate (a
// glide, fling, fit or follow in flight, or a rate driving); a resting
// camera costs nothing. update() pushes the pose to the layers only when
// it changed (one setCamera per driven layer: a shared-params write,
// however many sprites exist) and reports that, so per-frame dependents
// (labels re-projected via projectCamera) can follow the camera without
// recomputing every frame.

import { createMemo, createSignal, untrack } from "@solidjs/signals"
import { createAxes } from "@solidrt/core/input"
import type { Axes, Vec2 } from "@solidrt/core/input"
import { projectCamera, unprojectCamera } from "./camera.ts"
import type { CameraUpdate } from "./camera.ts"

// Glide rate toward a pending zoom or pose, e-foldings per second: high
// enough that a wheel notch reads as one push, low enough to look smooth.
const GLIDE_EASE = 9
// A glide lands when the remaining zoom gap is under this (relative) ...
const GLIDE_EPSILON = 0.002
// ... and the remaining travel is under this many screen pixels. Also the
// follow's settle threshold and, times the fling decay, the speed a fling
// stops at (the distance it would still travel).
const LAND_PX = 0.5
// Follow damping, e-foldings per second (Godot's default
// position_smoothing_speed).
const FOLLOW_EASE = 5
// Inertia after a drag release: velocity decay in e-foldings per second. A
// 1500 px/s flick travels 500 px; iOS scrolling decelerates at ~2, maps
// are snappier.
const FLING_DECAY = 3
// Release speeds under this (px/s) do not fling: a finger that stopped
// before lifting leaves the view where it is.
const FLING_MIN_SPEED = 50
// Per-update EMA weight of the drag velocity estimate: heavy on the newest
// frame so a direction change registers within a frame or two.
const VELOCITY_SMOOTH = 0.5
// Frame time assumed for the drag delta still pending at a release when the
// release arrives in the same frame as the last move.
const FALLBACK_DT = 1 / 60
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
   * is wider than the world centers. Absent = unbounded. */
  world?: { width: number; height: number }
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
  /** Follow dead zone as fractions of the viewport, centered on the pivot:
   * the target roams inside it without moving the camera. Default 0,0. */
  deadZone?: { width: number; height: number }
  /** Multipliers over the built-in pan, zoom (pinch, wheel and rate) and
   * roll sensitivities. */
  panSpeed?: number
  zoomSpeed?: number
  rollSpeed?: number
  /** Multiplier over the built-in follow damping. */
  followSpeed?: number
  /** Whether a drag release keeps the view gliding (default true). */
  inertia?: boolean
}

export type Camera2dAxes = { pan: "vec2"; zoom: "axis"; roll: "axis" }

export type Camera2d = {
  /** The pose as the layers receive it (a fresh object per call): the
   * argument for projectCamera/unprojectCamera. */
  camera(): CameraUpdate
  /** Pose snapshot - the control's own four values, the shape set() takes
   * (camera() adds the pivot the layers are told). */
  pose(): Required<Camera2dPose>
  /** Merge a pose in (clamps apply) and push it. A snap: an x/y/zoom
   * write cancels a glide or fling in flight; rotation alone leaves them
   * running. */
  set(pose: Camera2dPose): void
  /** Pan the content by a screen delta (a drag: positive dx slides the
   * world rightward) and push. Feeds the inertia estimate; release()
   * flings. */
  panBy(dx: number, dy: number): void
  /** End of a drag: keep gliding with the drag's velocity (inertia). */
  release(): void
  /** Zoom by `factor` about a screen point - the world point under it
   * stays under it - and push. `glide` eases there instead (a wheel
   * notch): notches compound on the pending target, so a fast scroll is
   * one long push. */
  zoomAt(sx: number, sy: number, factor: number, opts?: { glide?: boolean }): void
  /** Turn by radians about the pivot and push. */
  rollBy(angle: number): void
  /** Ease the pose until world (x, y) sits at the pivot, at `zoom` (default
   * the current zoom). Cancels a follow. */
  glideTo(x: number, y: number, zoom?: number): void
  /** Show a world rect (default the world) whole, centered in the
   * viewport, snapping or gliding. Waits for the viewport if unknown. */
  fit(rect?: Rect2d, opts?: { glide?: boolean }): void
  /** Keep world (x, y) at the pivot through the dead zone and damping;
   * call again whenever the target moves (every frame for a moving one).
   * Cancels a pose glide (glideTo, fit) and a fling; a wheel zoom keeps
   * gliding - zoom and follow are separate axes. */
  follow(x: number, y: number): void
  unfollow(): void
  /** Stop a glide or fling in flight (a press landing). */
  interrupt(): void
  /** The world-space bounding box of what the viewport shows. */
  viewRect(): Rect2d
  /** Advance glides, follow, inertia and the axis rates, then push the
   * pose to the layers if it changed. Call from onFrame with the frame's
   * dt in seconds; returns whether the pose changed since the previous
   * update (nudges and verbs included). */
  update(dt: number): boolean
  /**
   * Reactive: whether update() still has work - a glide, fling or
   * deferred fit in flight, a follow engaged, a write not yet pushed, or
   * an axis rate driving. The frame-loop gate: run update(dt) on frames
   * while true, and nothing while false (a resting camera costs no frames).
   */
  active(): boolean
  /** The input abstraction: `pan` (vec2, viewport heights of travel, the
   * world follows), `zoom` (axis, octaves, positive in) and `roll` (axis,
   * turns). An input map drives it by name (InputMap.drive); an app may
   * add sources and nudge directly. */
  axes: Axes<Camera2dAxes>
}

type Glide = { kind: "anchor"; target: number; sx: number; sy: number; wx: number; wy: number } | { kind: "pose"; x: number; y: number; zoom: number }

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

function checkFraction(what: string, v: number): void {
  if (!(Number.isFinite(v) && v >= 0 && v <= 1)) throw new Error(`createCamera2d: ${what} must be within 0..1, got ${v}`)
}

/** The non-pose options, validated at creation and again at every set()
 * (the live-option re-clamp entry). */
function checkOptions(options: Camera2dOptions): void {
  if (options.world) {
    positive("world.width", options.world.width)
    positive("world.height", options.world.height)
  }
  if (options.minZoom !== undefined) positive("minZoom", options.minZoom)
  if (options.maxZoom !== undefined) positive("maxZoom", options.maxZoom)
  if (options.minZoom !== undefined && options.maxZoom !== undefined && options.minZoom > options.maxZoom) {
    throw new Error(`createCamera2d: minZoom ${options.minZoom} exceeds maxZoom ${options.maxZoom}`)
  }
  finite("pivot.x", options.pivot?.x)
  finite("pivot.y", options.pivot?.y)
  checkFraction("deadZone.width", options.deadZone?.width ?? 0)
  checkFraction("deadZone.height", options.deadZone?.height ?? 0)
}

/**
 * Create a 2d camera driving `target`'s camera, where `target` is a sprite
 * or record layer's view (or several: a view and its overlay layer's
 * share one camera), or anything else with the layers' `setCamera`, such
 * as a signal setter feeding `<TileLayer camera>`. The initial pose
 * applies immediately: the world fitted when `world` is given and no
 * `zoom`. The pose options are initial values; every other option is read
 * where it applies, so a caller holding the options object (or a props
 * object handing out getters) changes bounds, pivot, dead zone and rates
 * live - `set({})` re-clamps and pushes the pose under new bounds. In a
 * component tree, prefer `<Camera2d>`: it drives the nearest view through
 * context and takes an input map as a prop.
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
  let deadW = () => options.deadZone?.width ?? 0
  let deadH = () => options.deadZone?.height ?? 0
  let panSpeed = () => options.panSpeed ?? 1
  let zoomExponent = () => options.zoomSpeed ?? 1
  let rollSpeed = () => options.rollSpeed ?? 1
  let followEase = () => FOLLOW_EASE * (options.followSpeed ?? 1)
  let inertia = () => options.inertia ?? true

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

  // The contain clamp on an unrotated view rect (Godot's rule: limits
  // ignore rotation): returns the pose's x/y, moved inside the world.
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
  let clamp = () => {
    zoom = clampNum(zoom, minZoom(), maxZoom())
    ;[x, y] = contain(x, y, zoom)
  }

  let camera = (): CameraUpdate => ({ x, y, zoom, rotation, pivotX: px(), pivotY: py() })
  // Pose x/y that put world (wx, wy) under screen (sx, sy) at zoom z:
  // the inverse of projectCamera solved for the camera point.
  let anchorPose = (wx: number, wy: number, sx: number, sy: number, z: number): [number, number] => {
    let c = Math.cos(rotation)
    let s = Math.sin(rotation)
    let dx = sx - px()
    let dy = sy - py()
    return [wx - (dx * c + dy * s) / z, wy - (dy * c - dx * s) / z]
  }
  // Slide the content by a screen delta: the camera point moves the
  // opposite way, the delta un-rotated into world axes.
  let shift = (dx: number, dy: number) => {
    let c = Math.cos(rotation)
    let s = Math.sin(rotation)
    x -= (dx * c + dy * s) / zoom
    y -= (dy * c - dx * s) / zoom
  }

  let dirty = false
  // Whether the pose changed since the last update() (pushes included).
  let changed = false
  let glide: Glide | null = null
  let fling: { vx: number; vy: number } | null = null
  let followAt: { x: number; y: number } | null = null
  // A follow that reached its target stops writing until something moves.
  let followSettled = false
  // Drag deltas since the last update, and the smoothed velocity (px/s).
  let dragDx = 0
  let dragDy = 0
  let vx = 0
  let vy = 0
  let lastDt = FALLBACK_DT
  // A fit that waits for the viewport (the default pose when a world is
  // given, or an explicit fit() before the size is known).
  let pendingFit: { rect: Rect2d | null; glide: boolean } | null = options.zoom === undefined && w0 ? { rect: null, glide: false } : null

  let touch = () => {
    dirty = true
    followSettled = false
  }
  // A pan gesture in flight keeps the camera active: update(dt) then folds
  // the drag's deltas into the velocity estimate every frame, so the fling
  // at release carries the finger's speed and not the whole drag's travel
  // over one assumed frame.
  let dragging = false
  let busy = () => dirty || dragging || glide !== null || fling !== null || followAt !== null || pendingFit !== null
  // The motion's half of active(), a signal every public entry refreshes
  // (ownedWrite: entries run from component bodies and handlers alike).
  let [motionActive, setMotionActive] = createSignal(false, { ownedWrite: true })
  let notify = () => {
    let now = busy()
    if (now !== untrack(motionActive)) setMotionActive(now)
  }
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
    let [wx, wy] = unprojectCamera(camera(), sx, sy)
    zoom = clampNum(next, minZoom(), maxZoom())
    ;[x, y] = anchorPose(wx, wy, sx, sy, zoom)
    clamp()
    touch()
  }
  let glideTo = (tx: number, ty: number, tz?: number) => {
    finite("glideTo x", tx)
    finite("glideTo y", ty)
    if (tz !== undefined) positive("glideTo zoom", tz)
    readViewport()
    fling = null
    followAt = null
    let z = clampNum(tz ?? zoom, minZoom(), maxZoom())
    let [cx, cy] = contain(tx, ty, z)
    glide = { kind: "pose", x: cx, y: cy, zoom: z }
  }
  let fitNow = (rect: Rect2d | null, ease: boolean) => {
    let w = world()
    let r = rect ?? (w ? { x: 0, y: 0, width: w.width, height: w.height } : null)
    if (!r) throw new Error("createCamera2d: fit() needs a rect when the camera has no world")
    let z = clampNum(Math.min(vw / r.width, vh / r.height), minZoom(), maxZoom())
    let [nx, ny] = anchorPose(r.x + r.width / 2, r.y + r.height / 2, vw / 2, vh / 2, z)
    if (ease) {
      glideTo(nx, ny, z)
      return
    }
    interrupt()
    x = nx
    y = ny
    zoom = z
    clamp()
    touch()
  }
  let panBy = (dx: number, dy: number) => {
    readViewport()
    interrupt()
    shift(dx, dy)
    clamp()
    dragDx += dx
    dragDy += dy
    touch()
  }
  let release = () => {
    // Fold the delta still pending from the release's own frame in with
    // the last frame time, then start from rest either way. The fling
    // gates on the SMOOTHED velocity too: a finger that stopped before
    // lifting has let it decay over the still frames, and the stray delta
    // the release frame may still carry (the resampler's last correction)
    // must not fling the view on its own.
    let smoothed = Math.hypot(vx, vy)
    let rvx = vx + (dragDx / lastDt - vx) * VELOCITY_SMOOTH
    let rvy = vy + (dragDy / lastDt - vy) * VELOCITY_SMOOTH
    dragDx = 0
    dragDy = 0
    vx = 0
    vy = 0
    if (!inertia() || followAt !== null) return
    if (smoothed < FLING_MIN_SPEED || Math.hypot(rvx, rvy) < FLING_MIN_SPEED) return
    fling = { vx: rvx, vy: rvy }
  }
  let zoomAt = (sx: number, sy: number, factor: number, ease: boolean) => {
    positive("zoomAt factor", factor)
    readViewport()
    if (!ease) {
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
    let [wx, wy] = unprojectCamera(camera(), sx, sy)
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

  let axes = createAxes<Camera2dAxes>(
    { pan: "vec2", zoom: "axis", roll: "axis" },
    {
      onBegin: name => {
        if (name !== "pan") return
        dragging = true
        interrupt()
      },
      onEnd: name => {
        if (name !== "pan") return
        dragging = false
        release()
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
  let active = createMemo(() => motionActive() || axes.active())

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
      if (pose.x !== undefined || pose.y !== undefined || pose.zoom !== undefined) interrupt()
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
    release() {
      release()
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
    glideTo(tx, ty, tz) {
      glideTo(tx, ty, tz)
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
      if (followAt === null) followAt = { x: tx, y: ty }
      else {
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
        vx += (dragDx / dt - vx) * VELOCITY_SMOOTH
        vy += (dragDy / dt - vy) * VELOCITY_SMOOTH
        dragDx = 0
        dragDy = 0
        lastDt = dt
      }
      if (fling !== null && dt > 0) {
        shift(fling.vx * dt, fling.vy * dt)
        let decay = Math.exp(-FLING_DECAY * dt)
        fling.vx *= decay
        fling.vy *= decay
        if (Math.hypot(fling.vx, fling.vy) < LAND_PX * FLING_DECAY) fling = null
        clamp()
        touch()
      }
      if (glide !== null && dt > 0) {
        let k = 1 - Math.exp(-GLIDE_EASE * dt)
        if (glide.kind === "anchor") {
          let gap = Math.log(glide.target / zoom)
          zoom = Math.abs(gap) < GLIDE_EPSILON ? glide.target : zoom * Math.exp(gap * k)
          ;[x, y] = anchorPose(glide.wx, glide.wy, glide.sx, glide.sy, zoom)
          if (zoom === glide.target) glide = null
        } else {
          let gap = Math.log(glide.zoom / zoom)
          let landed = Math.abs(gap) < GLIDE_EPSILON && Math.hypot(glide.x - x, glide.y - y) * zoom < LAND_PX
          if (landed) {
            x = glide.x
            y = glide.y
            zoom = glide.zoom
            glide = null
          } else {
            zoom *= Math.exp(gap * k)
            x += (glide.x - x) * k
            y += (glide.y - y) * k
          }
        }
        clamp()
        touch()
      }
      if (followAt !== null && !followSettled && dt > 0 && known()) {
        // How far the target sits outside the dead zone, in screen pixels;
        // the camera eases by that overshoot so the target rides the zone
        // edge, or snaps the last sub-pixel and settles.
        let [sx, sy] = projectCamera(camera(), followAt.x, followAt.y)
        let hw = (deadW() * vw) / 2
        let hh = (deadH() * vh) / 2
        let ox = sx < px() - hw ? sx - (px() - hw) : sx > px() + hw ? sx - (px() + hw) : 0
        let oy = sy < py() - hh ? sy - (py() - hh) : sy > py() + hh ? sy - (py() + hh) : 0
        if (Math.hypot(ox, oy) < LAND_PX) {
          if (ox !== 0 || oy !== 0) {
            shift(-ox, -oy)
            clamp()
            dirty = true
          }
          followSettled = true
        } else {
          let k = 1 - Math.exp(-followEase() * dt)
          shift(-ox * k, -oy * k)
          clamp()
          dirty = true
        }
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
