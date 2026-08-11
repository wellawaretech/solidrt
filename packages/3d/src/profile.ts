// Profile kit: the 2D outline vocabulary the solid generators build on. A
// Profile is a closed simple polygon in the XY plane - bare [x, y] tuples
// are sharp (creased) corners, tagged { p, smooth } points shade round -
// and winding is normalized to CCW on use, so either authoring direction
// works. fillet() and roundRect() produce smooth-tagged arc corners,
// shape() fills a profile as a flat +z face in the shared vertex layout,
// and triangulate() (the ear-clipping core behind every cap) is exported
// for custom flat work. The swept-solid generators consuming this
// vocabulary - extrude, lathe, sweep, tube - live in sweep.ts.

import { packIndices } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { Vec2 } from "./math.ts"

/** A profile point: `p` in profile space, `smooth` to share an averaged
 * normal with its neighbours instead of creasing (arc points want this;
 * the default is sharp). */
export type ProfilePoint = { p: Vec2; smooth?: boolean }

/** A closed 2D outline: bare [x, y] points are sharp corners. Winding may
 * go either way; consumers normalize to CCW. */
export type Profile = (Vec2 | ProfilePoint)[]

/** A normalized profile point (normalizeProfile's output). */
export type Pt = { x: number; y: number; smooth: boolean }

/** One side-strip vertex of the profile ring: position, outward 2D normal,
 * owning point index (for miter lookup) and normalized perimeter
 * parameter. Sharp points appear twice (once per edge normal); the list
 * ends with a copy of the first entry at t = 1, the UV seam. */
export type RingEntry = { x: number; y: number; nx: number; ny: number; point: number; t: number }

function signedArea(px: number[], py: number[]): number {
  let a = 0
  for (let i = 0; i < px.length; i++) {
    let j = (i + 1) % px.length
    a += px[i]! * py[j]! - px[j]! * py[i]!
  }
  return a / 2
}

/** Tag/tuple points to one shape, consecutive duplicates (including a
 * closing repeat of the first point) dropped, winding forced CCW - the
 * shared entry point of every profile consumer. */
export function normalizeProfile(profile: Profile): Pt[] {
  let pts: Pt[] = []
  for (let point of profile) {
    let x: number, y: number, smooth: boolean
    if (Array.isArray(point)) {
      x = point[0]; y = point[1]; smooth = false
    } else {
      x = point.p[0]; y = point.p[1]; smooth = point.smooth === true
    }
    let prev = pts[pts.length - 1]
    if (prev !== undefined && Math.abs(x - prev.x) < 1e-9 && Math.abs(y - prev.y) < 1e-9) continue
    pts.push({ x, y, smooth })
  }
  let first = pts[0]
  let last = pts[pts.length - 1]
  if (first !== undefined && last !== undefined && pts.length > 1 &&
    Math.abs(first.x - last.x) < 1e-9 && Math.abs(first.y - last.y) < 1e-9) pts.pop()
  if (pts.length < 3) throw new Error("Profile needs at least 3 distinct points")
  if (signedArea(pts.map((p) => p.x), pts.map((p) => p.y)) < 0) pts.reverse()
  return pts
}

/** Per-point miter offsets and the vertex entries a swept strip needs.
 * Offsetting a point by -miter * d insets the polygon by exactly d on both
 * adjacent edges; miter length is clamped so a spiky corner cannot fling
 * its inset point far into the interior. */
export function profileRing(pts: Pt[]) {
  let n = pts.length
  let enx: number[] = []
  let eny: number[] = []
  let elen: number[] = []
  for (let i = 0; i < n; i++) {
    let a = pts[i]!
    let b = pts[(i + 1) % n]!
    let dx = b.x - a.x
    let dy = b.y - a.y
    let l = Math.hypot(dx, dy) || 1
    // Outward normal of a CCW polygon's edge.
    enx.push(dy / l)
    eny.push(-dx / l)
    elen.push(l)
  }
  let perimeter = 0
  for (let l of elen) perimeter += l
  let entries: RingEntry[] = []
  let miterX: number[] = []
  let miterY: number[] = []
  let dist = 0
  for (let i = 0; i < n; i++) {
    let p = pts[i]!
    let px = enx[(i - 1 + n) % n]!
    let py = eny[(i - 1 + n) % n]!
    let nx = enx[i]!
    let ny = eny[i]!
    let k = 1 + px * nx + py * ny
    let mx: number
    let my: number
    if (k > 1e-3) {
      mx = (px + nx) / k
      my = (py + ny) / k
    } else {
      mx = px
      my = py
    }
    let ml = Math.hypot(mx, my)
    if (ml > 4) {
      mx = (mx * 4) / ml
      my = (my * 4) / ml
    }
    miterX.push(mx)
    miterY.push(my)
    let t = dist / perimeter
    if (p.smooth) {
      let sx = px + nx
      let sy = py + ny
      let sl = Math.hypot(sx, sy) || 1
      entries.push({ x: p.x, y: p.y, nx: sx / sl, ny: sy / sl, point: i, t })
    } else {
      entries.push({ x: p.x, y: p.y, nx: px, ny: py, point: i, t })
      entries.push({ x: p.x, y: p.y, nx, ny, point: i, t })
    }
    dist += elen[i]!
  }
  let e0 = entries[0]!
  entries.push({ x: e0.x, y: e0.y, nx: e0.nx, ny: e0.ny, point: 0, t: 1 })
  return { entries, miterX, miterY }
}

export type ProfileBounds = { minX: number; maxX: number; minY: number; maxY: number; w: number; h: number }

/** The profile's bounding box with degenerate spans widened to 1 - the
 * denominator every cap UV map shares. */
export function profileBounds(pts: { x: number; y: number }[]): ProfileBounds {
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity
  for (let p of pts) {
    minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x)
    minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y)
  }
  return { minX, maxX, minY, maxY, w: maxX - minX || 1, h: maxY - minY || 1 }
}

// Inside a CCW triangle, edges included: a vertex sitting exactly on an
// ear's chord must BLOCK the ear (a reflex corner on the chord means the
// ear overlaps area outside the polygon). The ear's own neighbours are
// excluded from the test, and a convex fillet arc's points lie strictly
// outside its chord, so legitimate ears still get through.
function pointInTriangle(
  x: number, y: number,
  ax: number, ay: number, bx: number, by: number, cx: number, cy: number,
): boolean {
  let s1 = (bx - ax) * (y - ay) - (by - ay) * (x - ax)
  let s2 = (cx - bx) * (y - by) - (cy - by) * (x - bx)
  let s3 = (ax - cx) * (y - cy) - (ay - cy) * (x - cx)
  return s1 >= 0 && s2 >= 0 && s3 >= 0
}

/** Ear clipping for a simple polygon, either winding; returns index
 * triples into the input, wound CCW for a +z viewer. Whatever remains
 * un-clippable becomes a fan, so a cap is never silently dropped.
 * triangulate() is the public face; the generators call this directly. */
export function earClip(px: number[], py: number[]): number[] {
  let idx: number[] = []
  for (let i = 0; i < px.length; i++) idx.push(i)
  if (signedArea(px, py) < 0) idx.reverse()
  let out: number[] = []
  while (idx.length > 3) {
    let clipped = false
    for (let i = 0; i < idx.length; i++) {
      let a = idx[(i - 1 + idx.length) % idx.length]!
      let b = idx[i]!
      let c = idx[(i + 1) % idx.length]!
      let cross = (px[b]! - px[a]!) * (py[c]! - py[a]!) - (py[b]! - py[a]!) * (px[c]! - px[a]!)
      if (cross <= 1e-12) continue
      let ok = true
      for (let j of idx) {
        if (j === a || j === b || j === c) continue
        if (pointInTriangle(px[j]!, py[j]!, px[a]!, py[a]!, px[b]!, py[b]!, px[c]!, py[c]!)) {
          ok = false
          break
        }
      }
      if (!ok) continue
      out.push(a, b, c)
      idx.splice(i, 1)
      clipped = true
      break
    }
    if (!clipped) break
  }
  for (let i = 1; i < idx.length - 1; i++) out.push(idx[0]!, idx[i]!, idx[i + 1]!)
  return out
}

/**
 * Ear-clip a simple polygon (either winding, no holes): flat index triples
 * into `points`, wound CCW for a +z viewer. Un-clippable leftovers fall
 * back to a fan rather than being dropped. This is the core behind every
 * cap here, exported for custom flat geometry.
 */
export function triangulate(points: Profile): number[] {
  let px: number[] = []
  let py: number[] = []
  for (let point of points) {
    if (Array.isArray(point)) {
      px.push(point[0])
      py.push(point[1])
    } else {
      px.push(point.p[0])
      py.push(point.p[1])
    }
  }
  return earClip(px, py)
}

/**
 * Round the corners of a polygon with tangent arcs: `radius` is one radius
 * for every corner or one per point (0 keeps that corner sharp), clamped
 * so neighbouring fillets never overlap. Arc points come out
 * smooth-tagged, so filleted profiles shade round; straight-through points
 * collapse to a single smooth point.
 */
export function fillet(points: Vec2[], radius: number | number[], segments = 4): ProfilePoint[] {
  if (Array.isArray(radius) && radius.length !== points.length) {
    throw new Error("fillet: radius array length must match points length")
  }
  let segs = Math.max(1, Math.round(segments))
  let out: ProfilePoint[] = []
  let n = points.length
  for (let i = 0; i < n; i++) {
    let r = typeof radius === "number" ? radius : radius[i]!
    let p = points[i]!
    let a = points[(i - 1 + n) % n]!
    let b = points[(i + 1) % n]!
    let d0x = a[0] - p[0], d0y = a[1] - p[1]
    let d1x = b[0] - p[0], d1y = b[1] - p[1]
    let l0 = Math.hypot(d0x, d0y), l1 = Math.hypot(d1x, d1y)
    if (r <= 0 || l0 < 1e-9 || l1 < 1e-9) {
      out.push({ p: [p[0], p[1]] })
      continue
    }
    d0x /= l0; d0y /= l0
    d1x /= l1; d1y /= l1
    let cosA = Math.max(-1, Math.min(1, d0x * d1x + d0y * d1y))
    let ang = Math.acos(cosA)
    if (ang > Math.PI - 1e-3) {
      out.push({ p: [p[0], p[1]], smooth: true })
      continue
    }
    let half = ang / 2
    // Tangent distance along both edges, kept off the neighbours' halves
    // so adjacent fillets never overlap; the radius follows the clamp.
    let t = Math.min(r / Math.tan(half), l0 / 2, l1 / 2)
    let rr = t * Math.tan(half)
    let t0x = p[0] + d0x * t, t0y = p[1] + d0y * t
    let t1x = p[0] + d1x * t, t1y = p[1] + d1y * t
    let bx = d0x + d1x, by = d0y + d1y
    let bl = Math.hypot(bx, by) || 1
    let dist = rr / Math.sin(half)
    let cx = p[0] + (bx / bl) * dist, cy = p[1] + (by / bl) * dist
    let a0 = Math.atan2(t0y - cy, t0x - cx)
    let a1 = Math.atan2(t1y - cy, t1x - cx)
    let da = a1 - a0
    while (da > Math.PI) da -= Math.PI * 2
    while (da < -Math.PI) da += Math.PI * 2
    for (let s = 0; s <= segs; s++) {
      let th = a0 + (da * s) / segs
      out.push({ p: [cx + Math.cos(th) * rr, cy + Math.sin(th) * rr], smooth: true })
    }
  }
  return out
}

/**
 * A width x height rectangle centered on the origin with corners rounded
 * by `radius` - one for all, or per corner as [bottom-left, bottom-right,
 * top-right, top-left] - the classic extrude() input. Radii clamp to what
 * the sides can fit (radius >= height / 2 makes a pill).
 */
export function roundRect(
  width = 1,
  height = 1,
  radius: number | number[] = 0.1,
  segments = 4,
): ProfilePoint[] {
  let x = width / 2
  let y = height / 2
  let corners: Vec2[] = [[-x, -y], [x, -y], [x, y], [-x, y]]
  return fillet(corners, radius, segments)
}

/**
 * The profile filled as a flat face in the XY plane facing +z - the
 * general case of circle()/ring() for arbitrary outlines. UVs map the
 * profile's bounding box to the unit square like plane(); rotate flat the
 * same way: `rotation={[-Math.PI / 2, 0, 0]}`.
 */
export function shape(profile: Profile, label?: string): Geometry {
  let pts = normalizeProfile(profile)
  let { minX, maxY, w, h } = profileBounds(pts)
  let px = pts.map((p) => p.x)
  let py = pts.map((p) => p.y)
  let verts: number[] = []
  for (let p of pts) {
    verts.push(p.x, p.y, 0, 0, 0, 1, (p.x - minX) / w, (maxY - p.y) / h)
  }
  return {
    vertices: new Float32Array(verts),
    indices: packIndices(earClip(px, py), pts.length),
    label,
  }
}
