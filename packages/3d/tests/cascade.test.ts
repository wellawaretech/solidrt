// The cascade fit's pure pieces (math.ts), checked against the geometry
// they claim: `bun test packages/3d/tests`.
import { describe, expect, test } from "bun:test"
import { cascadeSplit, frustumSliceSphere, lookAt, mat4 } from "../src/math.ts"
import { snapToGrid } from "../src/math.ts"
import type { FrustumSpec, Mat4, Vec3 } from "../src/math.ts"

let dist = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2])

// A tilted camera: eye off-axis, looking down and across.
let eye: Vec3 = [3, 4, 5]
let view: Mat4 = lookAt(mat4(), eye, [0, 1, -2], [0, 1, 0])
let right: Vec3 = [view[0], view[4], view[8]]
let up: Vec3 = [view[1], view[5], view[9]]
let forward: Vec3 = [-view[2], -view[6], -view[10]]
let at = (z: number, x: number, y: number): Vec3 => [
  eye[0] + forward[0] * z + right[0] * x + up[0] * y,
  eye[1] + forward[1] * z + right[1] * x + up[1] * y,
  eye[2] + forward[2] * z + right[2] * x + up[2] * y,
]

describe("cascadeSplit", () => {
  test("the last slice ends at far and the bounds climb", () => {
    let prev = 0.5
    for (let c = 0; c < 4; c++) {
      let z = cascadeSplit(0.5, 200, c, 4, 0.5)
      expect(z).toBeGreaterThan(prev)
      prev = z
    }
    expect(prev).toBe(200)
  })
  test("lambda 0 is uniform, 1 is logarithmic", () => {
    expect(cascadeSplit(1, 100, 0, 2, 0)).toBeCloseTo(50.5)
    expect(cascadeSplit(1, 100, 0, 2, 1)).toBeCloseTo(10)
    expect(cascadeSplit(1, 100, 0, 2, 0.5)).toBeCloseTo(30.25)
  })
  test("a near of 0 slices uniformly", () => {
    expect(cascadeSplit(0, 90, 0, 3, 0.5)).toBeCloseTo(30)
  })
})

describe("frustumSliceSphere", () => {
  let corners = (cam: FrustumSpec, aspect: number, zn: number, zf: number): Vec3[] => {
    let out: Vec3[] = []
    for (let z of [zn, zf]) {
      let hh = cam.ortho === null ? z * Math.tan((cam.fov * Math.PI) / 360) : 0
      let hw = hh * aspect
      let xs = cam.ortho === null ? [-hw, hw] : [cam.ortho.left, cam.ortho.right]
      let ys = cam.ortho === null ? [-hh, hh] : [cam.ortho.bottom, cam.ortho.top]
      for (let x of xs) for (let y of ys) out.push(at(z, x, y))
    }
    return out
  }
  test("contains every corner of a perspective slice and is not loose", () => {
    let cam: FrustumSpec = { view, eye, fov: 50, ortho: null }
    let center: Vec3 = [0, 0, 0]
    for (let [zn, zf] of [
      [0.5, 12],
      [12, 40],
      [40, 200],
    ] as const) {
      let r = frustumSliceSphere(center, cam, 1.5, zn, zf)
      for (let p of corners(cam, 1.5, zn, zf)) expect(dist(center, p)).toBeLessThanOrEqual(r + 1e-9)
      // Tight on the axis: the far corners touch the sphere.
      let far = corners(cam, 1.5, zn, zf).slice(4)
      expect(Math.max(...far.map(p => dist(center, p)))).toBeCloseTo(r, 6)
    }
  })
  test("contains every corner of an orthographic slice", () => {
    let cam: FrustumSpec = { view, eye, fov: 50, ortho: { left: -4, right: 6, top: 3, bottom: -5 } }
    let center: Vec3 = [0, 0, 0]
    let r = frustumSliceSphere(center, cam, 1, 2, 30)
    for (let p of corners(cam, 1, 2, 30)) expect(dist(center, p)).toBeCloseTo(r, 6)
  })
})

describe("snapToGrid", () => {
  test("lands on the grid of the frame, moves less than a step, keeps the third axis", () => {
    let basis = lookAt(mat4(), [0, 0, 0], [-0.4, -1, -0.3], [0, 1, 0])
    let p: Vec3 = [7.3, -2.1, 4.8]
    let out: Vec3 = [0, 0, 0]
    snapToGrid(out, p, basis, 0.25)
    let x = basis[0] * out[0] + basis[4] * out[1] + basis[8] * out[2]
    let y = basis[1] * out[0] + basis[5] * out[1] + basis[9] * out[2]
    let z = basis[2] * out[0] + basis[6] * out[1] + basis[10] * out[2]
    let pz = basis[2] * p[0] + basis[6] * p[1] + basis[10] * p[2]
    expect(Math.abs(x / 0.25 - Math.round(x / 0.25))).toBeLessThan(1e-9)
    expect(Math.abs(y / 0.25 - Math.round(y / 0.25))).toBeLessThan(1e-9)
    expect(z).toBeCloseTo(pz, 9)
    expect(dist(out, p)).toBeLessThanOrEqual(0.25 * Math.SQRT1_2 + 1e-9)
  })
  test("is idempotent and may alias its input", () => {
    let basis = lookAt(mat4(), [0, 0, 0], [0, -1, 0.01], [0, 1, 0])
    let p: Vec3 = [1.2, 3.4, -5.6]
    snapToGrid(p, p, basis, 0.5)
    let again: Vec3 = [0, 0, 0]
    snapToGrid(again, p, basis, 0.5)
    expect(again[0]).toBeCloseTo(p[0], 9)
    expect(again[1]).toBeCloseTo(p[1], 9)
    expect(again[2]).toBeCloseTo(p[2], 9)
  })
})
