// Blend spaces over a mixer: a 1D or 2D parameter (speed; speed and
// direction) picks weights over a set of clips that play together on one
// layer and share a phase (a sync group, so a walk and a run blended
// half and half land their feet together). Unity's blend trees, Godot's
// AnimationNodeBlendSpace1D/2D, Unreal's blend spaces, as the weight
// policy they are: O(changes) - a `set(position)` is a handful of weight
// writes, and nothing runs per frame. 1D interpolates between the two
// neighbouring samples; 2D triangulates the samples (Delaunay, Godot's
// way) and takes the barycentric weights of the containing triangle, a
// position outside the hull projecting onto the nearest hull edge.

import type { Mixer, MixerPlayOptions } from "./mixer.ts"
import type { Vec2 } from "./math.ts"

/** One clip at a position of the space. */
export type ClipBlendSample = { clip: string; position: number | Vec2 }

export type ClipBlendSpaceOptions = {
  samples: ClipBlendSample[]
  /** The layer the clips play on (the base when absent). */
  layer?: string
  /** How the clips share their phase (default "time"; "markers" aligns
   * the markers common to every clip). */
  sync?: "time" | "markers"
  /** The initial position (default: the first sample's). */
  position?: number | Vec2
  /** Weight ramps on each set(), ms (default 0: a snap - a controller
   * that writes every frame wants none). */
  fadeMs?: number
  /** Options every clip plays with (speed, inPlace, start/end). The
   * weight, layer, exclusivity and blend are the space's. */
  play?: Omit<MixerPlayOptions, "weight" | "layer" | "exclusive" | "blend" | "fadeMs" | "fadeOutMs">
}

export type ClipBlendSpace = {
  /** Move the parameter: the clips' weights follow (ramped by fadeMs). */
  set(position: number | Vec2): void
  /** The current weights, per sample. */
  weights(): number[]
  /** Stop the clips and dissolve the group. */
  dispose(): void
}

/** The group a space makes on its mixer: one per space. */
let spaceCount = 0

/** A weight below this is written as 0 (and the sample stops costing a
 * sample). */
const WEIGHT_EPSILON = 1e-4
/** Barycentric coordinates this far below 0 still count as inside: a
 * position on a shared edge belongs to one of its triangles. */
const INSIDE_EPSILON = 1e-6
/** A triangle whose doubled area is below this is degenerate. */
const DEGENERATE_AREA = 1e-12
/** The super-triangle of the triangulation sits this many extents away
 * from the samples. */
const SUPER_TRIANGLE_REACH = 20
/** The smallest extent the super-triangle is sized from (collinear or
 * coincident samples). */
const MIN_EXTENT = 1e-6

export function createBlendSpace(mixer: Mixer, opts: ClipBlendSpaceOptions): ClipBlendSpace {
  let samples = opts.samples
  if (samples.length === 0) throw new Error("createBlendSpace: no samples")
  let dims = typeof samples[0]!.position === "number" ? 1 : 2
  for (let s of samples) {
    let d = typeof s.position === "number" ? 1 : 2
    if (d !== dims) throw new Error("createBlendSpace: every sample must be " + dims + "D like the first")
  }
  let points: Vec2[] = samples.map((s) => (typeof s.position === "number" ? [s.position, 0] : [s.position[0], s.position[1]]))
  let triangles = dims === 2 ? delaunay(points) : []
  if (dims === 2 && triangles.length === 0 && samples.length >= 3) {
    throw new Error("createBlendSpace: the samples are collinear; use a 1D space")
  }
  let group = "blend-space-" + spaceCount++
  mixer.group(group, samples.map((s) => s.clip), { sync: opts.sync ?? "time" })
  let weights = samples.map(() => 0)
  let evaluate = (position: Vec2): number[] => (dims === 1 ? weights1d(points, position[0]) : weights2d(points, triangles, position))
  let apply = (next: number[], fadeMs: number): void => {
    samples.forEach((s, i) => {
      let w = next[i]! < WEIGHT_EPSILON ? 0 : next[i]!
      if (w !== weights[i]) mixer.setWeight(s.clip, w, { fadeMs })
      weights[i] = w
    })
  }
  let initial = opts.position ?? samples[0]!.position
  let at: Vec2 = typeof initial === "number" ? [initial, 0] : [initial[0], initial[1]]
  let first = evaluate(at)
  samples.forEach((s, i) => {
    mixer.play(s.clip, { ...(opts.play ?? {}), layer: opts.layer, weight: first[i]!, exclusive: false })
    weights[i] = first[i]!
  })
  return {
    set(position) {
      let p: Vec2 = typeof position === "number" ? [position, 0] : [position[0], position[1]]
      apply(evaluate(p), opts.fadeMs ?? 0)
    },
    weights() {
      return weights.slice()
    },
    dispose() {
      for (let s of samples) {
        if (mixer.playing(opts.layer).includes(s.clip)) mixer.setWeight(s.clip, 0)
      }
      mixer.ungroup(group)
      mixer.stop({ layer: opts.layer })
    },
  }
}

/** Linear between the two neighbouring samples on the line; clamped at
 * the ends. */
function weights1d(points: Vec2[], x: number): number[] {
  let order = points.map((p, i) => ({ x: p[0], i })).sort((a, b) => a.x - b.x)
  let out = points.map(() => 0)
  if (x <= order[0]!.x) {
    out[order[0]!.i] = 1
    return out
  }
  let last = order[order.length - 1]!
  if (x >= last.x) {
    out[last.i] = 1
    return out
  }
  for (let k = 0; k + 1 < order.length; k++) {
    let a = order[k]!
    let b = order[k + 1]!
    if (x >= a.x && x <= b.x) {
      let span = b.x - a.x
      let t = span > 0 ? (x - a.x) / span : 0
      out[a.i] = (out[a.i] ?? 0) + 1 - t
      out[b.i] = (out[b.i] ?? 0) + t
      return out
    }
  }
  return out
}

/** Barycentric weights in the containing triangle; outside the hull, on
 * the nearest point of the nearest hull edge. One or two samples blend
 * by distance along their segment. */
function weights2d(points: Vec2[], triangles: [number, number, number][], p: Vec2): number[] {
  let out = points.map(() => 0)
  if (points.length === 1) {
    out[0] = 1
    return out
  }
  if (points.length === 2 || triangles.length === 0) {
    let a = points[0]!
    let b = points[1]!
    let t = projectOnSegment(a, b, p)
    out[0] = 1 - t
    out[1] = t
    return out
  }
  for (let [i, j, k] of triangles) {
    let bc = barycentric(points[i]!, points[j]!, points[k]!, p)
    if (bc !== null && bc[0] >= -INSIDE_EPSILON && bc[1] >= -INSIDE_EPSILON && bc[2] >= -INSIDE_EPSILON) {
      out[i] = Math.max(0, bc[0])
      out[j] = Math.max(0, bc[1])
      out[k] = Math.max(0, bc[2])
      return out
    }
  }
  // Outside: the nearest point on any triangle edge (the hull's edges
  // are among them and are the nearest for an outside point).
  let best: { d: number; i: number; j: number; t: number } | null = null
  for (let [i, j, k] of triangles) {
    for (let [a, b] of [
      [i, j],
      [j, k],
      [k, i],
    ] as [number, number][]) {
      let t = projectOnSegment(points[a]!, points[b]!, p)
      let q: Vec2 = [points[a]![0] + (points[b]![0] - points[a]![0]) * t, points[a]![1] + (points[b]![1] - points[a]![1]) * t]
      let d = Math.hypot(q[0] - p[0], q[1] - p[1])
      if (best === null || d < best.d) best = { d, i: a, j: b, t }
    }
  }
  if (best !== null) {
    out[best.i] = 1 - best.t
    out[best.j] = best.t
  }
  return out
}

/** The fraction along a..b of the point of the segment nearest p. */
function projectOnSegment(a: Vec2, b: Vec2, p: Vec2): number {
  let dx = b[0] - a[0]
  let dy = b[1] - a[1]
  let len2 = dx * dx + dy * dy
  if (len2 === 0) return 0
  let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2
  return Math.min(1, Math.max(0, t))
}

function barycentric(a: Vec2, b: Vec2, c: Vec2, p: Vec2): [number, number, number] | null {
  let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
  if (Math.abs(det) < DEGENERATE_AREA) return null
  let u = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / det
  let v = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / det
  return [u, v, 1 - u - v]
}

/** Bowyer-Watson Delaunay triangulation of a handful of points (a blend
 * space has a dozen at most). Collinear input yields no triangles. */
function delaunay(points: Vec2[]): [number, number, number][] {
  if (points.length < 3) return []
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (let p of points) {
    minX = Math.min(minX, p[0])
    minY = Math.min(minY, p[1])
    maxX = Math.max(maxX, p[0])
    maxY = Math.max(maxY, p[1])
  }
  let span = Math.max(maxX - minX, maxY - minY, MIN_EXTENT)
  let mid: Vec2 = [(minX + maxX) / 2, (minY + maxY) / 2]
  // A super-triangle well outside the points.
  let big = span * SUPER_TRIANGLE_REACH
  let all: Vec2[] = [...points, [mid[0] - big, mid[1] - big], [mid[0], mid[1] + big], [mid[0] + big, mid[1] - big]]
  let n = points.length
  let tris: [number, number, number][] = [[n, n + 1, n + 2]]
  for (let i = 0; i < n; i++) {
    let p = all[i]!
    let bad: [number, number, number][] = []
    for (let t of tris) if (inCircumcircle(all[t[0]]!, all[t[1]]!, all[t[2]]!, p)) bad.push(t)
    let edges: [number, number][] = []
    for (let t of bad) {
      for (let e of [
        [t[0], t[1]],
        [t[1], t[2]],
        [t[2], t[0]],
      ] as [number, number][]) {
        let shared = bad.some((o) => o !== t && hasEdge(o, e))
        if (!shared) edges.push(e)
      }
    }
    tris = tris.filter((t) => !bad.includes(t))
    for (let e of edges) tris.push([e[0], e[1], i])
  }
  return tris.filter((t) => t[0] < n && t[1] < n && t[2] < n).filter((t) => barycentric(points[t[0]]!, points[t[1]]!, points[t[2]]!, points[t[0]]!) !== null)
}

function hasEdge(t: [number, number, number], e: [number, number]): boolean {
  let edges = [
    [t[0], t[1]],
    [t[1], t[2]],
    [t[2], t[0]],
  ]
  return edges.some(([a, b]) => (a === e[0] && b === e[1]) || (a === e[1] && b === e[0]))
}

function inCircumcircle(a: Vec2, b: Vec2, c: Vec2, p: Vec2): boolean {
  let ax = a[0] - p[0], ay = a[1] - p[1]
  let bx = b[0] - p[0], by = b[1] - p[1]
  let cx = c[0] - p[0], cy = c[1] - p[1]
  let det = (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay) + (cx * cx + cy * cy) * (ax * by - bx * ay)
  // The orientation of a, b, c decides the sign.
  let orient = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
  return orient > 0 ? det > 0 : det < 0
}
