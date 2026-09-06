// The 2d camera's motion, pure: a pose (the world point at a viewport
// pivot, zoom, rotation), the contain clamp against world bounds, anchored
// zoom, the eased glides (wheel notch, glideTo, fit), follow with damping
// and a dead zone, and drag inertia. Everything createCamera2d does except
// the input glue, which lives in camera2d.ts: this module imports no GUI
// or core module, so checks/camera2d-check.ts runs it headless on flux.
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

import { projectCamera, unprojectCamera } from "./camera.ts"
import type { CameraUpdate } from "./camera.ts"

// Zoom exponent per wheel-delta unit (wheel up zooms in); the same rate as
// @solidrt/3d's orbit camera, so the two feel alike.
const WHEEL_ZOOM = 0.0015
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

/** What a 2d camera drives: anything with the layers' `setCamera`. */
export type Camera2dTarget = { setCamera(update: CameraUpdate): void }

export type Camera2dPose = {
  x?: number
  y?: number
  zoom?: number
  rotation?: number
}

export type Rect2d = { x: number; y: number; w: number; h: number }

export type Camera2dMotionOptions = Camera2dPose & {
  /** Viewport size in layer pixels, read whenever the camera needs it (a
   * resize re-clamps at the next update). Return zeros while unknown -
   * clamping and fitting wait for a real size. */
  viewport: () => { w: number; h: number }
  /** World bounds (origin 0,0) the view is kept inside: an axis whose view
   * is wider than the world centers. Absent = unbounded. */
  world?: { w: number; h: number }
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
  deadZone?: { w: number; h: number }
  /** Multiplier over the built-in wheel and pinch sensitivity. */
  zoomSpeed?: number
  /** Multiplier over the built-in follow damping. */
  followSpeed?: number
  /** Whether a drag release keeps the view gliding (default true). */
  inertia?: boolean
}

export type Camera2dMotion = {
  /** The pose as the layers receive it (a fresh object per call): the
   * argument for projectCamera/unprojectCamera. */
  camera(): CameraUpdate
  /** Merge a pose in (clamps apply) and reach the layers at the next
   * update(). A snap: an x/y/zoom write cancels a glide or fling in
   * flight; rotation alone leaves them running. */
  set(pose: Camera2dPose): void
  /** Pan the content by a screen delta (a drag: positive dx slides the
   * world rightward). Feeds the inertia estimate; release() flings. */
  panBy(dx: number, dy: number): void
  /** End of a drag: keep gliding with the drag's velocity (inertia). */
  release(): void
  /** Zoom by `factor` about a screen point - the world point under it
   * stays under it. */
  zoomAt(sx: number, sy: number, factor: number): void
  /** A wheel notch at a screen point: retargets an eased, anchored zoom. */
  wheel(sx: number, sy: number, deltaY: number): void
  /** Ease the pose until world (x, y) sits at the pivot, at `zoom` (default
   * the current zoom). Cancels a follow. */
  glideTo(x: number, y: number, zoom?: number): void
  /** Show a world rect (default the world) whole, centered in the
   * viewport, snapping or gliding. Waits for the viewport if unknown. */
  fit(rect?: Rect2d, opts?: { glide?: boolean }): void
  /** Keep world (x, y) at the pivot through the dead zone and damping;
   * call again whenever the target moves. Cancels a glide. */
  follow(x: number, y: number): void
  unfollow(): void
  /** Stop a glide or fling in flight (a press landing). */
  interrupt(): void
  /** The world-space bounding box of what the viewport shows. */
  viewRect(): Rect2d
  /** Advance glides, follow and inertia, then push the pose to the
   * layers if it changed. Call from onFrame with the frame's dt in
   * seconds; returns whether the pose changed. */
  update(dt: number): boolean
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

export function createCameraMotion(target: Camera2dTarget | Camera2dTarget[], options: Camera2dMotionOptions): Camera2dMotion {
  let targets = Array.isArray(target) ? target : [target]
  if (typeof options.viewport !== "function") throw new Error("createCamera2d: viewport must be a function returning { w, h }")
  let world = options.world ?? null
  if (world) {
    positive("world.w", world.w)
    positive("world.h", world.h)
  }
  if (options.minZoom !== undefined) positive("minZoom", options.minZoom)
  if (options.maxZoom !== undefined) positive("maxZoom", options.maxZoom)
  if (options.minZoom !== undefined && options.maxZoom !== undefined && options.minZoom > options.maxZoom) {
    throw new Error(`createCamera2d: minZoom ${options.minZoom} exceeds maxZoom ${options.maxZoom}`)
  }
  checkPose("", options)
  let pivotFx = options.pivot?.x ?? 0.5
  let pivotFy = options.pivot?.y ?? 0.5
  finite("pivot.x", pivotFx)
  finite("pivot.y", pivotFy)
  let deadW = options.deadZone?.w ?? 0
  let deadH = options.deadZone?.h ?? 0
  checkFraction("deadZone.w", deadW)
  checkFraction("deadZone.h", deadH)
  let wheelZoom = WHEEL_ZOOM * (options.zoomSpeed ?? 1)
  let zoomExponent = options.zoomSpeed ?? 1
  let followEase = FOLLOW_EASE * (options.followSpeed ?? 1)
  let inertia = options.inertia ?? true
  let maxZoom = options.maxZoom ?? Infinity

  let x = options.x ?? (world ? world.w / 2 : 0)
  let y = options.y ?? (world ? world.h / 2 : 0)
  let zoom = options.zoom ?? 1
  let rotation = options.rotation ?? 0

  // The viewport as last read; zeros mean unknown (nothing clamps or fits
  // until a real size arrives).
  let vw = 0
  let vh = 0
  let known = () => vw > 0 && vh > 0
  let readViewport = () => {
    let v = options.viewport()
    if (!(Number.isFinite(v.w) && Number.isFinite(v.h) && v.w >= 0 && v.h >= 0)) {
      throw new Error(`createCamera2d: viewport() must return non-negative finite w/h, got ${v.w}x${v.h}`)
    }
    vw = v.w
    vh = v.h
  }
  let px = () => vw * pivotFx
  let py = () => vh * pivotFy

  let minZoom = () => {
    let fit = world && known() ? Math.min(vw / world.w, vh / world.h) : 0
    return Math.min(options.minZoom ?? fit, maxZoom)
  }

  // The contain clamp on an unrotated view rect (Godot's rule: limits
  // ignore rotation): returns the pose's x/y, moved inside the world.
  let contain = (cx: number, cy: number, z: number): [number, number] => {
    if (!world || !known()) return [cx, cy]
    let viewW = vw / z
    let viewH = vh / z
    let left = cx - px() / z
    let top = cy - py() / z
    left = viewW >= world.w ? (world.w - viewW) / 2 : clampNum(left, 0, world.w - viewW)
    top = viewH >= world.h ? (world.h - viewH) / 2 : clampNum(top, 0, world.h - viewH)
    return [left + px() / z, top + py() / z]
  }
  let clamp = () => {
    zoom = clampNum(zoom, minZoom(), maxZoom)
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
  let pendingFit: { rect: Rect2d | null; glide: boolean } | null = options.zoom === undefined && world ? { rect: null, glide: false } : null

  let touch = () => {
    dirty = true
    followSettled = false
  }
  let interrupt = () => {
    glide = null
    fling = null
  }
  let apply = () => {
    let update = camera()
    for (let t of targets) t.setCamera(update)
  }

  let zoomTo = (sx: number, sy: number, next: number) => {
    let [wx, wy] = unprojectCamera(camera(), sx, sy)
    zoom = clampNum(next, minZoom(), maxZoom)
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
    let z = clampNum(tz ?? zoom, minZoom(), maxZoom)
    let [cx, cy] = contain(tx, ty, z)
    glide = { kind: "pose", x: cx, y: cy, zoom: z }
  }
  let fitNow = (rect: Rect2d | null, ease: boolean) => {
    let r = rect ?? (world ? { x: 0, y: 0, w: world.w, h: world.h } : null)
    if (!r) throw new Error("createCamera2d: fit() needs a rect when the camera has no world")
    let z = clampNum(Math.min(vw / r.w, vh / r.h), minZoom(), maxZoom)
    let [nx, ny] = anchorPose(r.x + r.w / 2, r.y + r.h / 2, vw / 2, vh / 2, z)
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

  readViewport()
  clamp()
  if (pendingFit && known()) {
    fitNow(pendingFit.rect, pendingFit.glide)
    pendingFit = null
  }
  dirty = false
  apply()

  return {
    camera,
    set(pose) {
      checkPose("set", pose)
      readViewport()
      if (pose.x !== undefined || pose.y !== undefined || pose.zoom !== undefined) interrupt()
      if (pose.x !== undefined) x = pose.x
      if (pose.y !== undefined) y = pose.y
      if (pose.zoom !== undefined) zoom = pose.zoom
      if (pose.rotation !== undefined) rotation = pose.rotation
      clamp()
      touch()
    },
    panBy(dx, dy) {
      readViewport()
      interrupt()
      shift(dx, dy)
      clamp()
      dragDx += dx
      dragDy += dy
      touch()
    },
    release() {
      // Fold the delta still pending from the release's own frame in with
      // the last frame time, then start from rest either way.
      let rvx = vx + (dragDx / lastDt - vx) * VELOCITY_SMOOTH
      let rvy = vy + (dragDy / lastDt - vy) * VELOCITY_SMOOTH
      dragDx = 0
      dragDy = 0
      vx = 0
      vy = 0
      if (!inertia || followAt !== null) return
      if (Math.hypot(rvx, rvy) < FLING_MIN_SPEED) return
      fling = { vx: rvx, vy: rvy }
    },
    zoomAt(sx, sy, factor) {
      positive("zoomAt factor", factor)
      readViewport()
      interrupt()
      zoomTo(sx, sy, zoom * Math.pow(factor, zoomExponent))
    },
    wheel(sx, sy, deltaY) {
      readViewport()
      fling = null
      if (glide !== null && glide.kind === "pose") glide = null
      // Notches compound on the pending target, so a fast scroll is one
      // long push; the anchor is the world point under the pointer now.
      let from = glide !== null ? glide.target : zoom
      let target = clampNum(from * Math.exp(-deltaY * wheelZoom), minZoom(), maxZoom)
      let [wx, wy] = unprojectCamera(camera(), sx, sy)
      glide = { kind: "anchor", target, sx, sy, wx, wy }
    },
    glideTo,
    fit(rect, opts) {
      readViewport()
      if (!known()) {
        pendingFit = { rect: rect ?? null, glide: opts?.glide ?? false }
        return
      }
      fitNow(rect ?? null, opts?.glide ?? false)
    },
    follow(tx, ty) {
      finite("follow x", tx)
      finite("follow y", ty)
      glide = null
      fling = null
      if (followAt === null) followAt = { x: tx, y: ty }
      else {
        followAt.x = tx
        followAt.y = ty
      }
      followSettled = false
    },
    unfollow() {
      followAt = null
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
      return { x: minX, y: minY, w: maxX - minX, h: maxY - minY }
    },
    update(dt) {
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
        let hw = (deadW * vw) / 2
        let hh = (deadH * vh) / 2
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
          let k = 1 - Math.exp(-followEase * dt)
          shift(-ox * k, -oy * k)
          clamp()
          dirty = true
        }
      }
      if (!dirty) return false
      dirty = false
      apply()
      return true
    },
  }
}
