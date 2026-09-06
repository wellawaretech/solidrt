// A kinematic character move over the scene's volume queries: Godot's
// CharacterBody3D.move_and_slide and Unity's CharacterController.Move as
// one pure function. Sweep the body along its motion, stop a skin short
// of the first contact, slide the rest along the contact plane, repeat a
// few times - with the two passes both engines wrap around that loop: a
// depenetration first (a body that starts inside something is pushed out
// before it moves) and a floor snap after (a body walking down a ramp is
// pulled back onto it instead of stepping into the air). Per body per
// frame that is the input read and this one call; the geometry runs in
// the spatial core.
//
// Pure on purpose: no node, no velocity state. It takes a volume where the
// body IS and the motion it WANTS, and returns the motion it gets plus
// what it touched - the first-person camera (which has no node) composes
// it in clampPosition, and a node-driven body applies the result with
// setTransform. Gravity is the caller's: fold it into `motion`; a walkable
// floor absorbs the vertical part (a body does not creep down a slope it
// can stand on), a steep one slides it.

import type { Vec3 } from "./math.ts"
import type { Impact, QueryOptions, Scene, Volume } from "./scene.ts"

/** Gap the body keeps from every surface, world units (Unity's skinWidth,
 * Godot's safe_margin): what keeps float error from sinking a resting body
 * into its floor. */
const SKIN = 0.01
/** Contacts within this angle of `up` are floor: 45 degrees, Godot's
 * floor_max_angle and Unity's slopeLimit. */
const FLOOR_MAX_ANGLE = Math.PI / 4
/** Most contacts one call slides along before it stops where it is
 * (Godot's max_slides). */
const MAX_SLIDES = 6
/** How far below the body a floor is pulled to while it is not rising
 * (Godot's floor_snap_length). */
const FLOOR_SNAP = 0.1
/** Passes of the depenetration loop, one contact set pushed out per pass
 * (Godot's recovery attempts). */
const MAX_RECOVERIES = 4
/** Motion below which a slide stops: nothing left worth sweeping. */
const MIN_MOVE = 1e-6
/** Floor of the approach cosine the skin is divided by to retreat along
 * the motion: a grazing contact retreats a few skins, not the whole step. */
const SKIN_MIN_COS = 0.1

const UP: Vec3 = [0, 1, 0]

/** What moveAndSlide needs of a scene. */
export type MoveScene = Pick<Scene, "sweep" | "overlap">

export type MoveOptions = QueryOptions & {
  /** The direction floors face (default [0, 1, 0]). */
  up?: Vec3
  /** Largest angle (radians) between a contact normal and `up` that still
   * counts as floor (default 45 degrees). Steeper contacts are walls, and
   * the body slides down them. */
  floorMaxAngle?: number
  /** Most contacts one call slides along (default 6). */
  maxSlides?: number
  /** Gap kept from every surface, world units (default 0.01). */
  skin?: number
  /** How far below the body a floor is pulled to when the motion does not
   * rise (default 0.1; 0 disables): what keeps a walker on a ramp going
   * down, and what decides `floor` at the end of the move - a body that
   * ends higher than this above its floor is airborne, whatever it
   * touched on the way. A rising motion (a jump) never snaps. */
  floorSnap?: number
}

export type MoveResult = {
  /** The displacement the body gets: add it to the body's position. */
  motion: Vec3
  /** The unit normal of the floor the body ends the move on - within
   * `floorSnap` below it, snapped onto - else null (airborne, or on a
   * slope too steep to stand on). With `floorSnap: 0` it is a floor met
   * during the move instead. */
  floor: Vec3 | null
  /** Whether a wall (a contact steeper than a floor and flatter than a
   * ceiling) or a ceiling was met. */
  wall: boolean
  ceiling: boolean
  /** Every contact met, in order, the floor snap's last. */
  hits: Impact[]
}

/**
 * Move a body `motion` through the scene's colliders, sliding along what
 * it hits (Godot's move_and_slide, Unity's CharacterController.Move): a
 * capsule for a character, a sphere for a ball, a box for a crate. The
 * body first pushes out of anything it starts inside, then sweeps and
 * slides up to `maxSlides` times, then, unless the motion rises, snaps
 * down onto a floor within `floorSnap` - the floor it reports is the one
 * it ends on. Colliders are whatever `opts`
 * selects (`layers`/`meshes`, as for sweep): pass the collision layer,
 * or the body's own mesh answers for its own walls.
 *
 * Gravity is the caller's: fold the fall into `motion` each frame, zero
 * it while `floor` is set (the snap keeps reporting the floor while the
 * body stands still), and a walkable floor absorbs the vertical part -
 * `motion` only ever moves the body along a floor it stands on, never
 * down it.
 */
export function moveAndSlide(scene: MoveScene, volume: Volume, motion: Vec3, opts: MoveOptions = {}): MoveResult {
  let up = opts.up ?? UP
  let floorCos = Math.cos(opts.floorMaxAngle ?? FLOOR_MAX_ANGLE)
  let maxSlides = opts.maxSlides ?? MAX_SLIDES
  let skin = opts.skin ?? SKIN
  let snap = opts.floorSnap ?? FLOOR_SNAP
  let query: QueryOptions = { layers: opts.layers, meshes: opts.meshes }
  let offset: Vec3 = [0, 0, 0]
  let hits: Impact[] = []
  let floor: Vec3 | null = null
  let wall = false
  let ceiling = false

  // Depenetration: every contact's push-out, each only as far as the
  // pushes so far have not already covered along its normal, so two
  // walls of a corner do not push twice.
  for (let pass = 0; pass < MAX_RECOVERIES; pass++) {
    let push: Vec3 = [0, 0, 0]
    let any = false
    for (let c of scene.overlap(shifted(volume, offset), query)) {
      if (c.depth <= 0) continue
      any = true
      let extra = c.depth + skin - dot(push, c.normal)
      if (extra > 0) push = add(push, scale(c.normal, extra))
    }
    if (!any) break
    offset = add(offset, push)
  }

  let remaining: Vec3 = [motion[0], motion[1], motion[2]]
  for (let slide = 0; slide < maxSlides; slide++) {
    let len = Math.hypot(remaining[0], remaining[1], remaining[2])
    if (len < MIN_MOVE) break
    let hit = scene.sweep(shifted(volume, offset), remaining, query)[0]
    if (hit === undefined) {
      offset = add(offset, remaining)
      break
    }
    hits.push(hit)
    let dir = scale(remaining, 1 / len)
    let n = hit.normal
    // Stop where the gap to the surface, measured along its normal, is
    // one skin; the sweep guarantees the approach is positive.
    let retreat = skin / Math.max(-dot(dir, n), SKIN_MIN_COS)
    let advance = Math.max(hit.time * len - retreat, 0)
    offset = add(offset, scale(dir, advance))
    remaining = scale(dir, len - advance)
    let rise = dot(n, up)
    if (rise >= floorCos) {
      // A floor takes the vertical part (the fall, a landing) and passes
      // the horizontal part along its plane.
      floor = n
      let level = sub(remaining, scale(up, dot(remaining, up)))
      remaining = sub(level, scale(n, dot(level, n)))
    } else {
      if (rise <= -floorCos) ceiling = true
      else wall = true
      remaining = sub(remaining, scale(n, dot(remaining, n)))
    }
  }

  // Where the body ends decides the floor: a contact met on the way
  // (a ramp's top before a big step past its crest) is not one to stand
  // on unless it is still within reach below.
  if (snap > 0 && dot(motion, up) <= 0) {
    floor = null
    let hit = scene.sweep(shifted(volume, offset), scale(up, -snap), query)[0]
    if (hit !== undefined && dot(hit.normal, up) >= floorCos) {
      hits.push(hit)
      let retreat = skin / Math.max(dot(hit.normal, up), SKIN_MIN_COS)
      offset = add(offset, scale(up, -Math.max(hit.time * snap - retreat, 0)))
      floor = hit.normal
    }
  }
  return { motion: offset, floor, wall, ceiling, hits }
}

/** The volume displaced by `d`. */
function shifted(volume: Volume, d: Vec3): Volume {
  if ("center" in volume) return { ...volume, center: add(volume.center, d) }
  return { ...volume, a: add(volume.a, d), b: add(volume.b, d) }
}

function add(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

function scale(v: Vec3, s: number): Vec3 {
  return [v[0] * s, v[1] * s, v[2] * s]
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
