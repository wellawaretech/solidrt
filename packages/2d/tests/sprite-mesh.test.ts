// Tests for the frame mesh math (sprite-mesh.ts) and its reach into the
// slicers (frames.ts): every generated mesh, pushed out the way the hull
// vertex stage pushes it (glsl.ts spriteCornerSource, its lattice term
// mirrored here in JS), contains every covered texel of its frame, padded
// half a texel and clipped to the frame - across random shapes, sheets
// and frame rects, fractional ones included. Plus the budget, the frame
// bound, the packing round trip, the coverage thresholds and the slicers'
// fill. Pure-module input only, so it runs headless on flux: `sol test
// packages/2d`. The random inputs come from Math.random, which `sol test`
// seeds.

import { test } from "flux:test"
import type { TextureId } from "@solidrt/core/gpu"
import { fullFrame, grid, namedFrames, writeMesh } from "../src/frames.ts"
import { coverageMask, hullMesh, MESH_FLOATS, MESH_LATTICE, MESH_VERTICES, unpackPoints } from "../src/sprite-mesh.ts"

// Slack on a containment test, in frame pixels: float error only.
const CONTAIN_EPSILON = 1e-6
// How much more area than the disc it hugs a disc's mesh may take: the
// 8-gon's 5.5%, the half-texel padding and the lattice, at radius 24.
const DISC_AREA_RATIO = 1.25
const RANDOM_SHEETS = 40

function fail(msg: string): never {
  throw new Error(msg)
}

function int(lo: number, hi: number): number {
  return lo + Math.floor(Math.random() * (hi - lo + 1))
}

type Point = [number, number]

/** RGBA8 pixels of a `width` x `height` sheet, alpha from `inside`. */
function sheet(width: number, height: number, inside: (x: number, y: number) => boolean, alpha = 255): Uint8Array {
  let data = new Uint8Array(width * height * 4)
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) if (inside(x, y)) data[(y * width + x) * 4 + 3] = alpha
  return data
}

/** The ring's 8 points from a packed field, in frame fractions. */
function ringOf(packed: ArrayLike<number>): Point[] {
  let ring: Point[] = []
  let byte = (i: number) => Math.floor(packed[Math.floor(i / 3)]! / 256 ** (i % 3)) % 256
  for (let i = 0; i < MESH_VERTICES; i++) ring.push([byte(i * 2) / MESH_LATTICE, byte(i * 2 + 1) / MESH_LATTICE])
  return ring
}

/** The vertex stage's push, in JS: each ring point (scaled to `w` x `h`
 * frame pixels) moved along its miter by `push`, its neighbours the
 * nearest distinct points either way round. */
function pushed(ring: Point[], w: number, h: number, push: number): Point[] {
  let n = ring.length
  let px = ring.map(([x, y]): Point => [(x - 0.5) * w, (y - 0.5) * h])
  let same = (a: Point, b: Point) => a[0] === b[0] && a[1] === b[1]
  return px.map((p, k) => {
    let a = p
    let b = p
    for (let j = 1; j < n; j++) {
      if (!same(ring[(k + n - j) % n]!, ring[k]!)) {
        a = px[(k + n - j) % n]!
        break
      }
    }
    for (let j = 1; j < n; j++) {
      if (!same(ring[(k + j) % n]!, ring[k]!)) {
        b = px[(k + j) % n]!
        break
      }
    }
    if (same(a, p)) return p
    let unit = (v: Point): Point => {
      let l = Math.hypot(v[0], v[1])
      return [v[0] / l, v[1] / l]
    }
    let e1 = unit([p[0] - a[0], p[1] - a[1]])
    let e2 = unit([b[0] - p[0], b[1] - p[1]])
    let n1: Point = [e1[1], -e1[0]]
    let n2: Point = [e2[1], -e2[0]]
    let d = 1 + n1[0] * n2[0] + n1[1] * n2[1]
    return [p[0] + (push * (n1[0] + n2[0])) / d, p[1] + (push * (n1[1] + n2[1])) / d]
  })
}

function inside(polygon: Point[], p: Point): boolean {
  let n = polygon.length
  for (let i = 0; i < n; i++) {
    let a = polygon[i]!
    let b = polygon[(i + 1) % n]!
    if (a[0] === b[0] && a[1] === b[1]) continue
    let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    let len = Math.hypot(b[0] - a[0], b[1] - a[1])
    if (cross / len < -CONTAIN_EPSILON) return false
  }
  return true
}

function area(polygon: Point[]): number {
  let sum = 0
  for (let i = 0; i < polygon.length; i++) {
    let a = polygon[i]!
    let b = polygon[(i + 1) % polygon.length]!
    sum += a[0] * b[1] - b[0] * a[1]
  }
  return Math.abs(sum) / 2
}

/** Every covered texel of the rect, padded half a texel and clipped to the
 * frame, inside the mesh pushed by its lattice term alone. */
function checkContains(what: string, data: Uint8Array, bits: Uint32Array, width: number, height: number, x: number, y: number, w: number, h: number, alphaMin: number): void {
  let mesh = hullMesh(bits, width, height, x, y, w, h)
  let any = false
  for (let ty = Math.max(0, Math.floor(y)); ty < Math.min(height, Math.ceil(y + h)); ty++) {
    for (let tx = Math.max(0, Math.floor(x)); tx < Math.min(width, Math.ceil(x + w)); tx++) {
      if (data[(ty * width + tx) * 4 + 3]! >= alphaMin) any = true
    }
  }
  if (!any) {
    if (mesh !== undefined) fail(`${what}: an empty frame got a mesh`)
    return
  }
  if (mesh === undefined) fail(`${what}: a frame with covered texels got no mesh`)
  if (mesh.packed.length !== MESH_FLOATS) fail(`${what}: packed field is ${mesh.packed.length} floats`)
  let points = mesh.points
  if (points.length / 2 > MESH_VERTICES || points.length / 2 < 3) fail(`${what}: ${points.length / 2} mesh points`)
  for (let v of points) if (!(v >= 0 && v <= 1)) fail(`${what}: mesh point coordinate ${v} outside the frame`)
  let decoded = unpackPoints(mesh.packed)
  if (decoded.length !== points.length || decoded.some((v, i) => v !== points[i])) fail(`${what}: points are not the packed field decoded`)
  let polygon = pushed(ringOf(mesh.packed), w, h, (0.5 * Math.hypot(w, h)) / MESH_LATTICE)
  for (let ty = Math.max(0, Math.floor(y)); ty < Math.min(height, Math.ceil(y + h)); ty++) {
    for (let tx = Math.max(0, Math.floor(x)); tx < Math.min(width, Math.ceil(x + w)); tx++) {
      if (data[(ty * width + tx) * 4 + 3]! < alphaMin) continue
      for (let [cx, cy] of [
        [tx - 0.5, ty - 0.5],
        [tx + 1.5, ty - 0.5],
        [tx - 0.5, ty + 1.5],
        [tx + 1.5, ty + 1.5],
      ] as Point[]) {
        let p: Point = [Math.min(x + w, Math.max(x, cx)) - x - w / 2, Math.min(y + h, Math.max(y, cy)) - y - h / 2]
        if (!inside(polygon, p)) fail(`${what}: covered texel (${tx}, ${ty}) corner (${cx}, ${cy}) is outside the mesh`)
      }
    }
  }
}

test("every mesh contains every covered texel of its frame, on random shapes and frame rects", () => {
  for (let n = 0; n < RANDOM_SHEETS; n++) {
    let width = int(8, 96)
    let height = int(8, 96)
    // A few blobs (ellipses) and a sprinkle of lone texels: shapes with
    // long tips, thin lines and holes alike.
    let blobs = Array.from({ length: int(1, 3) }, () => ({ cx: Math.random() * width, cy: Math.random() * height, rx: 0.3 + Math.random() * width * 0.4, ry: 0.3 + Math.random() * height * 0.4 }))
    let lone = new Set(Array.from({ length: int(0, 4) }, () => int(0, width * height - 1)))
    let data = sheet(width, height, (x, y) => lone.has(y * width + x) || blobs.some(b => ((x + 0.5 - b.cx) / b.rx) ** 2 + ((y + 0.5 - b.cy) / b.ry) ** 2 <= 1))
    let bits = coverageMask(data, width, height, false)
    checkContains(`sheet ${n} whole`, data, bits, width, height, 0, 0, width, height, 1)
    for (let f = 0; f < 6; f++) {
      let fractional = f % 2 === 1
      let w = fractional ? 1 + Math.random() * (width - 1) : int(1, width)
      let h = fractional ? 1 + Math.random() * (height - 1) : int(1, height)
      let x = fractional ? Math.random() * (width - w) : int(0, width - w)
      let y = fractional ? Math.random() * (height - h) : int(0, height - h)
      checkContains(`sheet ${n} frame [${x}, ${y}, ${w}, ${h}]`, data, bits, width, height, x, y, w, h, 1)
    }
  }
})

test("a disc's mesh is an 8-gon close to the disc; a full frame's is the frame", () => {
  let size = 64
  let r = 24
  let data = sheet(size, size, (x, y) => (x + 0.5 - size / 2) ** 2 + (y + 0.5 - size / 2) ** 2 <= r * r)
  let mesh = hullMesh(coverageMask(data, size, size, false), size, size, 0, 0, size, size)!
  let points: Point[] = []
  for (let i = 0; i < mesh.points.length; i += 2) points.push([mesh.points[i]! * size, mesh.points[i + 1]! * size])
  if (points.length !== MESH_VERTICES) fail(`disc: ${points.length} points, want the full budget`)
  let ratio = area(points) / (Math.PI * r * r)
  if (!(ratio > 1 && ratio < DISC_AREA_RATIO)) fail(`disc: mesh area is ${ratio.toFixed(3)}x the disc`)
  let full = hullMesh(coverageMask(sheet(16, 16, () => true), 16, 16, false), 16, 16, 0, 0, 16, 16)!
  let want = [0, 0, 1, 0, 1, 1, 0, 1]
  if (full.points.length !== want.length || want.some((v, i) => full.points[i] !== v)) fail(`full frame: mesh is [${[...full.points]}]`)
})

test("a shape thinner than a lattice cell still packs a polygon with area", () => {
  // One texel column in a 1024 px wide frame: narrower than 1/255 of it.
  let width = 1024
  let height = 8
  let data = sheet(width, height, x => x === 500)
  let bits = coverageMask(data, width, height, false)
  checkContains("thin column", data, bits, width, height, 0, 0, width, height, 1)
  let mesh = hullMesh(bits, width, height, 0, 0, width, height)!
  let points: Point[] = []
  for (let i = 0; i < mesh.points.length; i += 2) points.push([mesh.points[i]!, mesh.points[i + 1]!])
  if (!(area(points) > 0)) fail("thin column: the mesh has no area")
})

test("coverage: alpha above zero for colour, the field's edge for a distance field", () => {
  let data = new Uint8Array([0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 127, 0, 0, 0, 128])
  let colour = coverageMask(data, 4, 1, false)
  let field = coverageMask(data, 4, 1, true)
  if (colour[0] !== 0b1110) fail(`colour coverage is ${colour[0]!.toString(2)}`)
  if (field[0] !== 0b1000) fail(`field coverage is ${field[0]!.toString(2)}`)
})

test("the slicers fill the mesh over a mesh atlas, and writeMesh writes it or zeros", () => {
  let width = 32
  let height = 16
  // Two cells: a small square in the left one, nothing in the right.
  let data = sheet(width, height, (x, y) => x >= 4 && x < 8 && y >= 4 && y < 8)
  let texture = 7 as unknown as TextureId
  let plain = { texture, width, height }
  let meshed = { ...plain, mesh: { kind: "hull" as const, coverage: coverageMask(data, width, height, false) } }
  let [left, right] = grid(meshed, 2, 1)
  if (left!.mesh === undefined) fail("grid: the covered cell has no mesh")
  if (right!.mesh !== undefined) fail("grid: the empty cell has a mesh")
  if (grid(plain, 2, 1)[0]!.mesh !== undefined) fail("grid: a plain atlas's frame has a mesh")
  if (namedFrames(meshed, { a: [2, 2, 10, 10] }).a.mesh === undefined) fail("namedFrames: no mesh")
  if (fullFrame(meshed).mesh === undefined) fail("fullFrame: no mesh")
  let record = new Float32Array(MESH_FLOATS + 2).fill(9)
  writeMesh(record, 1, left!)
  if (record[0] !== 9 || record[MESH_FLOATS + 1] !== 9) fail("writeMesh wrote outside its field")
  if (record.subarray(1, 1 + MESH_FLOATS).some((v, i) => v !== left!.mesh!.packed[i])) fail("writeMesh: field is not the packed mesh")
  writeMesh(record, 1, right!)
  if (record.subarray(1, 1 + MESH_FLOATS).some(v => v !== 0)) fail("writeMesh: a frame without a mesh did not zero the field")
})
