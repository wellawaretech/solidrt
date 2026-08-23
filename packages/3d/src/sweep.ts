// Swept solids: the generators that run a Profile (profile.ts) along a
// path. extrude() sweeps along a straight line (z), lathe() around a
// circle (about y), sweep() along an arbitrary 3D polyline with mitred
// joints, and tube() is sweep() with a circular profile - the wire, rope
// and pipe primitive. Output is the shared vertex layout of geometry.ts
// with real texture UVs; winding is CCW seen from outside like every
// generator, so cull: "back" works. Indices pick Uint16Array or
// Uint32Array by vertex count - filleted profiles times many path points
// make dense outputs routine here.

import { STANDARD_FLOATS, packGeometry } from "./geometry.ts"
import type { Geometry, GeometryOptions } from "./geometry.ts"
import { add, cross, dot, normalize, scale, sub } from "./math.ts"
import type { Vec3 } from "./math.ts"
import { earClip, normalizeProfile, profileBounds, profileRing } from "./profile.ts"
import type { Profile, ProfileBounds, RingEntry } from "./profile.ts"

// Two CCW-outward triangles per cell between two profile rings already in
// the vertex buffer, baseA the ring nearer the sweep's start, entries
// inner. The zero-width cell between a sharp profile point's two entries
// is skipped.
function ringBand(baseA: number, baseB: number, entries: RingEntry[], out: number[]): void {
  for (let k = 0; k < entries.length - 1; k++) {
    let e = entries[k]!
    let f = entries[k + 1]!
    if (e.x === f.x && e.y === f.y) continue
    let a = baseA + k
    let b = a + 1
    let c = baseB + k + 1
    let d = baseB + k
    out.push(c, b, d, b, a, d)
  }
}

// A full swept grid of (outer + 1) consecutive rings: for extrude, outer
// runs down the z slices; for lathe, around the revolution. The split and
// winding reduce to box and cylinder respectively (verified against those
// generators).
function sweepIndices(outer: number, entries: RingEntry[], out: number[]): void {
  let stride = entries.length
  for (let o = 0; o < outer; o++) ringBand(o * stride, (o + 1) * stride, entries, out)
}

// A flat triangulated cap: place() maps a profile-space point into world,
// `n` is the outward normal, and the profile bounds map to the unit
// square. `mirrorU` flips u so a far-end cap's texture reads unmirrored
// from outside; `flip` reverses the CCW-for-a-+z-viewer triangulation for
// a cap whose outside is the profile's -z side.
function emitCap(
  verts: number[],
  indices: number[],
  px: number[],
  py: number[],
  tris: number[],
  b: ProfileBounds,
  place: (x: number, y: number) => Vec3,
  n: Vec3,
  mirrorU: boolean,
  flip: boolean,
): void {
  let base = verts.length / STANDARD_FLOATS
  for (let i = 0; i < px.length; i++) {
    let p = place(px[i]!, py[i]!)
    let u = mirrorU ? (b.maxX - px[i]!) / b.w : (px[i]! - b.minX) / b.w
    verts.push(p[0], p[1], p[2], n[0], n[1], n[2], u, (b.maxY - py[i]!) / b.h)
  }
  for (let i = 0; i < tris.length; i += 3) {
    if (flip) indices.push(base + tris[i]!, base + tris[i + 2]!, base + tris[i + 1]!)
    else indices.push(base + tris[i]!, base + tris[i + 1]!, base + tris[i + 2]!)
  }
}

/**
 * The profile swept along z, centered on the origin: z runs from depth/2
 * down to -depth/2, caps at both ends. `bevel` rounds both rims with a
 * quarter-circle roll of that radius (clamped to just under half the
 * depth), inset from the outline by miter offset so the bevel stays
 * inside the silhouette. Side UVs: u = normalized distance around the
 * outline (seam at the first point), v = 0 at the +z rim to 1 at the -z
 * rim, bevel arcs included. Caps map the profile's bounding box to the
 * unit square like plane(); the -z cap mirrors u so its texture reads
 * unmirrored from outside.
 */
export type ExtrudeOptions = GeometryOptions & { depth?: number; bevel?: number; bevelSegments?: number }

export function extrude(profile: Profile, options: ExtrudeOptions = {}): Geometry {
  let { depth = 1, bevel = 0, bevelSegments = 4 } = options
  let pts = normalizeProfile(profile)
  let { entries, miterX, miterY } = profileRing(pts)
  let h = depth / 2
  let b = Math.min(bevel, depth * 0.49)

  // Slices from the +z rim down: inset from the outline, z, and the
  // (radial, z) direction the entry normal tilts into. The rim slice's
  // normal equals the cap's, so a bevel blends into its cap crease-free.
  let slices: { inset: number; z: number; nr: number; nz: number }[] = []
  if (b > 1e-9) {
    let bsegs = Math.max(1, Math.round(bevelSegments))
    for (let i = 0; i <= bsegs; i++) {
      let a = (i / bsegs) * (Math.PI / 2)
      slices.push({ inset: b * (1 - Math.sin(a)), z: h - b * (1 - Math.cos(a)), nr: Math.sin(a), nz: Math.cos(a) })
    }
    for (let i = bsegs; i >= 0; i--) {
      let a = (i / bsegs) * (Math.PI / 2)
      slices.push({ inset: b * (1 - Math.sin(a)), z: -h + b * (1 - Math.cos(a)), nr: Math.sin(a), nz: -Math.cos(a) })
    }
  } else {
    slices.push({ inset: 0, z: h, nr: 1, nz: 0 }, { inset: 0, z: -h, nr: 1, nz: 0 })
  }

  // v follows the path length down the side, bevel arcs included.
  let v: number[] = [0]
  let path = 0
  for (let i = 1; i < slices.length; i++) {
    let p = slices[i - 1]!
    let s = slices[i]!
    path += Math.hypot(s.z - p.z, s.inset - p.inset)
    v.push(path)
  }
  for (let i = 0; i < v.length; i++) v[i] = v[i]! / (path || 1)

  let verts: number[] = []
  for (let si = 0; si < slices.length; si++) {
    let s = slices[si]!
    for (let e of entries) {
      // (e.n * nr, nz) is unit already: |e.n| = 1 and nr^2 + nz^2 = 1.
      verts.push(
        e.x - miterX[e.point]! * s.inset,
        e.y - miterY[e.point]! * s.inset,
        s.z,
        e.nx * s.nr,
        e.ny * s.nr,
        s.nz,
        e.t,
        v[si]!,
      )
    }
  }
  let indices: number[] = []
  sweepIndices(slices.length - 1, entries, indices)

  // Caps, inset to meet the bevel rim; UVs span the base profile's box.
  let bounds = profileBounds(pts)
  let inset = slices[0]!.inset
  let cx: number[] = []
  let cy: number[] = []
  for (let i = 0; i < pts.length; i++) {
    cx.push(pts[i]!.x - miterX[i]! * inset)
    cy.push(pts[i]!.y - miterY[i]! * inset)
  }
  let tris = earClip(cx, cy)
  emitCap(verts, indices, cx, cy, tris, bounds, (x, y) => [x, y, h], [0, 0, 1], false, false)
  emitCap(verts, indices, cx, cy, tris, bounds, (x, y) => [x, y, -h], [0, 0, -1], true, true)

  return packGeometry(verts, indices, options)
}

/**
 * A solid of revolution: the CLOSED profile - x is the radius (>= 0), y
 * the height - revolved about the y axis through `angle` radians starting
 * at `start`. The closed profile (a cross-section with thickness, or run
 * to the axis) keeps the output watertight and cull-correct where an open
 * polyline shell would show its missing back faces; partial sweeps get
 * flat triangulated end caps. UVs: u 0..1 around the sweep (seam column
 * duplicated like torus), v = normalized distance around the profile;
 * caps map the profile's bounding box, the end cap mirrored in u.
 */
export type LatheOptions = GeometryOptions & { segments?: number; angle?: number; start?: number }

export function lathe(profile: Profile, options: LatheOptions = {}): Geometry {
  let { segments = 32, angle = Math.PI * 2, start = 0 } = options
  if (!(angle > 0) || angle > Math.PI * 2 + 1e-9) throw new Error("Lathe angle must be in (0, 2*PI]")
  let pts = normalizeProfile(profile)
  let { entries } = profileRing(pts)
  let full = angle > Math.PI * 2 - 1e-9

  let verts: number[] = []
  for (let c = 0; c <= segments; c++) {
    let u = c / segments
    let phi = start + angle * u
    // The same radial direction as cylinder(), so winding transfers.
    let dx = -Math.cos(phi)
    let dz = Math.sin(phi)
    for (let e of entries) {
      verts.push(e.x * dx, e.y, e.x * dz, e.nx * dx, e.ny, e.nx * dz, u, e.t)
    }
  }
  let indices: number[] = []
  sweepIndices(segments, entries, indices)

  if (!full) {
    let bounds = profileBounds(pts)
    let px = pts.map((p) => p.x)
    let py = pts.map((p) => p.y)
    let tris = earClip(px, py)
    for (let end = 0; end < 2; end++) {
      let phi = end === 0 ? start : start + angle
      let dx = -Math.cos(phi)
      let dz = Math.sin(phi)
      // Outward along minus/plus the sweep direction; the CCW profile
      // triangulation faces the start normal as-is and flips for the end.
      let nx = end === 0 ? -Math.sin(phi) : Math.sin(phi)
      let nz = end === 0 ? -Math.cos(phi) : Math.cos(phi)
      emitCap(verts, indices, px, py, tris, bounds, (x, y) => [x * dx, y, x * dz], [nx, 0, nz], end === 1, end === 1)
    }
  }

  return packGeometry(verts, indices, options)
}

/** A sweep-path point: `p` in world space, `smooth` to share one ring
 * with averaged normals at this joint instead of creasing (tag the points
 * of a sampled curve; the default is sharp). Tags on the two endpoints
 * are ignored - ends get caps, not joints. */
export type PathPoint = { p: Vec3; smooth?: boolean }

/** An open 3D polyline to sweep along: bare [x, y, z] points are sharp
 * (creased) joints. Consecutive duplicates are dropped; the first and
 * last distinct points are the capped ends. Closed loops are not
 * supported yet - overlap the ends by a segment to fake one. */
export type SweepPath = (Vec3 | PathPoint)[]

type NPath = { p: Vec3; smooth: boolean }[]

function normalizePath(path: SweepPath): NPath {
  let pts: NPath = []
  for (let point of path) {
    let p: Vec3
    let smooth: boolean
    if (Array.isArray(point)) {
      p = [point[0], point[1], point[2]]
      smooth = false
    } else {
      p = [point.p[0], point.p[1], point.p[2]]
      smooth = point.smooth === true
    }
    let prev = pts[pts.length - 1]
    if (prev !== undefined &&
      Math.abs(p[0] - prev.p[0]) < 1e-9 && Math.abs(p[1] - prev.p[1]) < 1e-9 && Math.abs(p[2] - prev.p[2]) < 1e-9) continue
    pts.push({ p, smooth })
  }
  if (pts.length < 2) throw new Error("Sweep path needs at least 2 distinct points")
  return pts
}

/** Per-segment cross-section frames of a path (pathFrames' result). */
export type PathFrames = {
  /** The deduplicated path points. */
  points: Vec3[]
  /** Unit direction of travel, one per segment (points.length - 1). */
  tangents: Vec3[]
  /** The cross-section axes profile x and y map onto, one pair per
   * segment, minimally rotated from segment to segment (parallel
   * transport); yAxis starts as close to world up as the first segment
   * allows. */
  xAxes: Vec3[]
  yAxes: Vec3[]
  /** Cumulative arc length at each point; the last entry is the total. */
  lengths: number[]
}

// v rotated about a unit axis; cosA/sinA are the angle's cosine and sine.
function rotate(v: Vec3, axis: Vec3, cosA: number, sinA: number): Vec3 {
  let c = cross(axis, v)
  let k = dot(axis, v) * (1 - cosA)
  return [
    v[0] * cosA + c[0] * sinA + axis[0] * k,
    v[1] * cosA + c[1] * sinA + axis[1] * k,
    v[2] * cosA + c[2] * sinA + axis[2] * k,
  ]
}

function buildFrames(pts: NPath): PathFrames {
  let points = pts.map((q) => q.p)
  let tangents: Vec3[] = []
  let lengths: number[] = [0]
  let len = 0
  for (let i = 0; i < points.length - 1; i++) {
    let d = sub(points[i + 1]!, points[i]!)
    let l = Math.hypot(d[0], d[1], d[2])
    tangents.push(scale(d, 1 / l))
    len += l
    lengths.push(len)
  }
  // Looking along the travel direction the profile reads CCW with yAxis
  // up, so xAxis x yAxis = -tangent - the same handedness extrude's
  // slices have, which keeps the shared band winding CCW-outward.
  let t0 = tangents[0]!
  let ref: Vec3 = Math.abs(t0[1]) < 0.99 ? [0, 1, 0] : [0, 0, 1]
  let y0 = normalize(sub(ref, scale(t0, dot(ref, t0))))
  let xAxes: Vec3[] = [cross(t0, y0)]
  let yAxes: Vec3[] = [y0]
  for (let i = 1; i < tangents.length; i++) {
    let a = tangents[i - 1]!
    let b = tangents[i]!
    let axis = cross(a, b)
    let sinA = Math.hypot(axis[0], axis[1], axis[2])
    let x = xAxes[i - 1]!
    if (sinA > 1e-9) {
      x = rotate(x, scale(axis, 1 / sinA), Math.max(-1, Math.min(1, dot(a, b))), sinA)
    }
    // Drift guard: keep x exactly in the new cross-section plane. A full
    // reversal (sinA ~ 0, dot < 0) keeps x too - it is perpendicular to
    // both tangents.
    x = normalize(sub(x, scale(b, dot(x, b))))
    xAxes.push(x)
    yAxes.push(cross(x, b))
  }
  return { points, tangents, xAxes, yAxes, lengths }
}

/**
 * The per-segment frames sweep() places its rings with, exported for
 * custom work along a path (placing objects at path points, custom swept
 * surfaces): unit tangents, cross-section axes (parallel transported, so
 * the frame never spins between segments), cumulative arc lengths.
 * Frames are per SEGMENT; a joint's shared ring lives on the bisector
 * plane of its two segments.
 */
export function pathFrames(path: SweepPath): PathFrames {
  return buildFrames(normalizePath(path))
}

/**
 * The profile swept along an open 3D polyline - the strap, cable and
 * rail generator. Joints are mitred: the cross-section sits on the
 * bisector plane of its two segments, so bends never gape or overlap.
 * Shading follows the path points - bare points crease (two normal sets
 * share the mitre ring; right for a strap folding over an edge),
 * smooth-tagged points average into one continuous surface (tag a
 * sampled curve's points). Flat caps close both ends. The profile's y
 * axis starts as close to world up as the first segment allows (a
 * vertical start falls back to +z) and parallel-transports along the
 * path without spinning. UVs: u = normalized distance around the profile
 * (seam at the first point), v = normalized distance along the path;
 * caps map the profile's bounding box like extrude's, the end cap
 * mirrored in u. A near-reversal joint clamps its mitre stretch (4x,
 * like the profile's own miter clamp) instead of flinging vertices.
 */
export function sweep(profile: Profile, path: SweepPath, options: GeometryOptions = {}): Geometry {
  let pts = normalizeProfile(profile)
  let { entries } = profileRing(pts)
  let p = normalizePath(path)
  let { tangents, xAxes, yAxes, lengths } = buildFrames(p)
  let n = p.length
  let total = lengths[n - 1]! || 1

  let verts: number[] = []
  let indices: number[] = []
  let emitRing = (pos: Vec3[], normals: Vec3[], v: number): number => {
    let base = verts.length / STANDARD_FLOATS
    for (let k = 0; k < entries.length; k++) {
      let q = pos[k]!
      let m = normals[k]!
      verts.push(q[0], q[1], q[2], m[0], m[1], m[2], entries[k]!.t, v)
    }
    return base
  }
  // The ring's world positions in segment s's cross-section plane at
  // `center`, and the profile normals mapped through segment s's frame.
  let planeRing = (center: Vec3, s: number): Vec3[] => {
    let x = xAxes[s]!
    let y = yAxes[s]!
    return entries.map((e): Vec3 => [
      center[0] + x[0] * e.x + y[0] * e.y,
      center[1] + x[1] * e.x + y[1] * e.y,
      center[2] + x[2] * e.x + y[2] * e.y,
    ])
  }
  let frameNormals = (s: number): Vec3[] => {
    let x = xAxes[s]!
    let y = yAxes[s]!
    return entries.map((e): Vec3 => [
      x[0] * e.nx + y[0] * e.ny,
      x[1] * e.nx + y[1] * e.ny,
      x[2] * e.nx + y[2] * e.ny,
    ])
  }
  // The joint ring on the bisector plane: the incoming cross-section
  // projected along its own tangent onto the plane - which is exactly
  // where the outgoing cross-section projects too (that is the miter
  // joint fact parallel transport buys), so ONE mitred ring serves both
  // bands. The projection stretch 1 / (t . m) is clamped to 4x.
  let mitreRing = (i: number): Vec3[] => {
    let a = tangents[i - 1]!
    let m = add(a, tangents[i]!)
    let ml = Math.hypot(m[0], m[1], m[2])
    let bis = ml > 1e-9 ? scale(m, 1 / ml) : a
    let denom = Math.max(dot(a, bis), 0.25)
    return planeRing(p[i]!.p, i - 1).map((q) => {
      let off = sub(q, p[i]!.p)
      return sub(q, scale(a, dot(off, bis) / denom))
    })
  }

  let prev = emitRing(planeRing(p[0]!.p, 0), frameNormals(0), 0)
  for (let i = 1; i < n - 1; i++) {
    let v = lengths[i]! / total
    let pos = mitreRing(i)
    if (p[i]!.smooth) {
      let nin = frameNormals(i - 1)
      let nout = frameNormals(i)
      let base = emitRing(pos, nin.map((q, k) => normalize(add(q, nout[k]!))), v)
      ringBand(prev, base, entries, indices)
      prev = base
    } else {
      ringBand(prev, emitRing(pos, frameNormals(i - 1), v), entries, indices)
      prev = emitRing(pos, frameNormals(i), v)
    }
  }
  ringBand(prev, emitRing(planeRing(p[n - 1]!.p, n - 2), frameNormals(n - 2), 1), entries, indices)

  let bounds = profileBounds(pts)
  let px = pts.map((q) => q.x)
  let py = pts.map((q) => q.y)
  let tris = earClip(px, py)
  let capPlace = (center: Vec3, s: number) => (x: number, y: number): Vec3 => [
    center[0] + xAxes[s]![0] * x + yAxes[s]![0] * y,
    center[1] + xAxes[s]![1] * x + yAxes[s]![1] * y,
    center[2] + xAxes[s]![2] * x + yAxes[s]![2] * y,
  ]
  emitCap(verts, indices, px, py, tris, bounds, capPlace(p[0]!.p, 0), scale(tangents[0]!, -1), false, false)
  emitCap(verts, indices, px, py, tris, bounds, capPlace(p[n - 1]!.p, n - 2), tangents[n - 2]!, true, true)

  return packGeometry(verts, indices, options)
}

/**
 * A round-profile sweep - the wire, rope and pipe shorthand:
 * radialSegments smooth points around a circle of `radius` swept along
 * `path`. Joint shading still follows the path points (bare = creased
 * bend, smooth = continuous), and both ends get flat caps. UVs: u around
 * the tube, v along the path.
 */
export type TubeOptions = GeometryOptions & { radius?: number; radialSegments?: number }

export function tube(path: SweepPath, options: TubeOptions = {}): Geometry {
  let { radius = 0.5, radialSegments = 12, ...rest } = options
  let profile: Profile = []
  for (let i = 0; i < radialSegments; i++) {
    let a = (i / radialSegments) * Math.PI * 2
    profile.push({ p: [Math.cos(a) * radius, Math.sin(a) * radius], smooth: true })
  }
  return sweep(profile, path, rest)
}
