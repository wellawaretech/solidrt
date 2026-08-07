// Profile kit: 3D geometry from 2D outlines. A Profile is a closed simple
// polygon in the XY plane - bare [x, y] tuples are sharp (creased) corners,
// tagged { p, smooth } points shade round - and winding is normalized to
// CCW on use, so either authoring direction works. extrude() sweeps a
// profile along z with an optional quarter-round bevel, lathe() revolves
// one about the y axis, shape() fills one as a flat face; fillet() and
// roundRect() produce smooth-tagged arc corners, and triangulate() (the
// ear-clipping core behind every cap) is exported for custom flat work.
//
// Output is the shared vertex layout of geometry.ts with real texture UVs.
// Winding is CCW seen from outside like every generator, so cull: "back"
// works: sharp profile points emit one vertex per adjacent edge, smooth
// points one vertex with the averaged normal. Indices pick Uint16Array or
// Uint32Array by vertex count - filleted profiles times bevel slices make
// dense outputs routine here.

import { FLOATS_PER_VERTEX } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { Vec2 } from "./math.ts"

/** A profile point: `p` in profile space, `smooth` to share an averaged
 * normal with its neighbours instead of creasing (arc points want this;
 * the default is sharp). */
export type ProfilePoint = { p: Vec2; smooth?: boolean }

/** A closed 2D outline: bare [x, y] points are sharp corners. Winding may
 * go either way; consumers normalize to CCW. */
export type Profile = (Vec2 | ProfilePoint)[]

type Pt = { x: number; y: number; smooth: boolean }

// One side-strip vertex of the profile ring: position, outward 2D normal,
// owning point index (for miter lookup) and normalized perimeter
// parameter. Sharp points appear twice (once per edge normal); the list
// ends with a copy of the first entry at t = 1, the UV seam.
type RingEntry = { x: number; y: number; nx: number; ny: number; point: number; t: number }

function signedArea(px: number[], py: number[]): number {
  let a = 0
  for (let i = 0; i < px.length; i++) {
    let j = (i + 1) % px.length
    a += px[i]! * py[j]! - px[j]! * py[i]!
  }
  return a / 2
}

// Tag/tuple points to one shape, consecutive duplicates (including a
// closing repeat of the first point) dropped, winding forced CCW.
function normalizeProfile(profile: Profile): Pt[] {
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

// Per-point miter offsets and the vertex entries a swept strip needs.
// Offsetting a point by -miter * d insets the polygon by exactly d on both
// adjacent edges; miter length is clamped so a spiky corner cannot fling
// its inset point far into the interior.
function profileRing(pts: Pt[]) {
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

// Two CCW-outward triangles per cell of a swept (outer + 1) x entries
// vertex grid, entries inner: for extrude, outer runs down the z slices;
// for lathe, around the revolution. The split and winding reduce to box
// and cylinder respectively (verified against those generators). The
// zero-width cell between a sharp point's two entries is skipped.
function sweepIndices(outer: number, entries: RingEntry[], out: number[]): void {
  let stride = entries.length
  for (let o = 0; o < outer; o++) {
    for (let k = 0; k < stride - 1; k++) {
      let e = entries[k]!
      let f = entries[k + 1]!
      if (e.x === f.x && e.y === f.y) continue
      let a = o * stride + k
      let b = a + 1
      let c = a + stride + 1
      let d = a + stride
      out.push(c, b, d, b, a, d)
    }
  }
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

// Ear clipping for a simple polygon, either winding; returns index triples
// into the input, wound CCW for a +z viewer. Whatever remains un-clippable
// becomes a fan, so a cap is never silently dropped.
function earClip(px: number[], py: number[]): number[] {
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

function packIndices(indices: number[], vertexCount: number): Uint16Array | Uint32Array {
  return vertexCount > 65535 ? new Uint32Array(indices) : new Uint16Array(indices)
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
export function extrude(
  profile: Profile,
  depth = 1,
  bevel = 0,
  bevelSegments = 4,
  label?: string,
): Geometry {
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
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity
  for (let p of pts) {
    minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x)
    minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y)
  }
  let bw = maxX - minX || 1
  let bh = maxY - minY || 1
  let inset = slices[0]!.inset
  let cx: number[] = []
  let cy: number[] = []
  for (let i = 0; i < pts.length; i++) {
    cx.push(pts[i]!.x - miterX[i]! * inset)
    cy.push(pts[i]!.y - miterY[i]! * inset)
  }
  let tris = earClip(cx, cy)
  let base = verts.length / FLOATS_PER_VERTEX
  for (let i = 0; i < cx.length; i++) {
    verts.push(cx[i]!, cy[i]!, h, 0, 0, 1, (cx[i]! - minX) / bw, (maxY - cy[i]!) / bh)
  }
  for (let i = 0; i < tris.length; i += 3) {
    indices.push(base + tris[i]!, base + tris[i + 1]!, base + tris[i + 2]!)
  }
  base = verts.length / FLOATS_PER_VERTEX
  for (let i = 0; i < cx.length; i++) {
    verts.push(cx[i]!, cy[i]!, -h, 0, 0, -1, (maxX - cx[i]!) / bw, (maxY - cy[i]!) / bh)
  }
  for (let i = 0; i < tris.length; i += 3) {
    indices.push(base + tris[i]!, base + tris[i + 2]!, base + tris[i + 1]!)
  }

  return {
    vertices: new Float32Array(verts),
    indices: packIndices(indices, verts.length / FLOATS_PER_VERTEX),
    label,
  }
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
export function lathe(
  profile: Profile,
  segments = 32,
  angle = Math.PI * 2,
  start = 0,
  label?: string,
): Geometry {
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
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity
    for (let p of pts) {
      minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x)
      minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y)
    }
    let bw = maxX - minX || 1
    let bh = maxY - minY || 1
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
      let base = verts.length / FLOATS_PER_VERTEX
      for (let p of pts) {
        let u = end === 0 ? (p.x - minX) / bw : (maxX - p.x) / bw
        verts.push(p.x * dx, p.y, p.x * dz, nx, 0, nz, u, (maxY - p.y) / bh)
      }
      for (let i = 0; i < tris.length; i += 3) {
        if (end === 0) indices.push(base + tris[i]!, base + tris[i + 1]!, base + tris[i + 2]!)
        else indices.push(base + tris[i]!, base + tris[i + 2]!, base + tris[i + 1]!)
      }
    }
  }

  return {
    vertices: new Float32Array(verts),
    indices: packIndices(indices, verts.length / FLOATS_PER_VERTEX),
    label,
  }
}

/**
 * The profile filled as a flat face in the XY plane facing +z - the
 * general case of circle()/ring() for arbitrary outlines. UVs map the
 * profile's bounding box to the unit square like plane(); rotate flat the
 * same way: `rotation={[-Math.PI / 2, 0, 0]}`.
 */
export function shape(profile: Profile, label?: string): Geometry {
  let pts = normalizeProfile(profile)
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity
  for (let p of pts) {
    minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x)
    minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y)
  }
  let bw = maxX - minX || 1
  let bh = maxY - minY || 1
  let px = pts.map((p) => p.x)
  let py = pts.map((p) => p.y)
  let verts: number[] = []
  for (let p of pts) {
    verts.push(p.x, p.y, 0, 0, 0, 1, (p.x - minX) / bw, (maxY - p.y) / bh)
  }
  return {
    vertices: new Float32Array(verts),
    indices: packIndices(earClip(px, py), pts.length),
    label,
  }
}
