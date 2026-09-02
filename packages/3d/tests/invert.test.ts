// The general inverse (math.ts invert) against the matrices it exists
// for: a view-projection, whose bottom row invertAffine cannot handle,
// and the world-space ray the background slot rebuilds from it.
// `bun test packages/3d/tests`.
import { describe, expect, test } from "bun:test"
import { invert, lookAt, mat4, multiply, orthographic, perspective, transformPoint } from "../src/math.ts"
import type { Mat4, Vec3, Vec4 } from "../src/math.ts"

let eye: Vec3 = [3, 4, 5]
let target: Vec3 = [0, 1, -2]
let view: Mat4 = lookAt(mat4(), eye, target, [0, 1, 0])
let forward: Vec3 = [-view[2], -view[6], -view[10]]

// A clip position carried back to world and dehomogenized.
function unproject(inv: Mat4, x: number, y: number, z: number): Vec3 {
  let p: Vec4 = [0, 0, 0, 0]
  transformPoint(p, inv, [x, y, z])
  return [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
}

function expectIdentity(m: Mat4): void {
  for (let i = 0; i < 16; i++) expect(m[i]).toBeCloseTo(i % 5 === 0 ? 1 : 0, 6)
}

describe("invert", () => {
  test("a perspective view-projection inverts to the identity both ways", () => {
    let proj = perspective(mat4(), Math.PI / 3, 16 / 9, 0.1, 100)
    let viewProj = multiply(mat4(), proj, view)
    let inv = invert(mat4(), viewProj)
    expectIdentity(multiply(mat4(), viewProj, inv))
    expectIdentity(multiply(mat4(), inv, viewProj))
  })

  test("an orthographic view-projection inverts, and out may alias m", () => {
    let proj = orthographic(mat4(), -4, 4, 3, -3, 0.5, 50)
    let viewProj = multiply(mat4(), proj, view)
    let inv = invert(mat4(), viewProj)
    expectIdentity(multiply(mat4(), viewProj, inv))
    let aliased: Mat4 = [...viewProj] as Mat4
    invert(aliased, aliased)
    for (let i = 0; i < 16; i++) expect(aliased[i]).toBeCloseTo(inv[i], 9)
  })

  test("the center pixel's ray is the camera's forward under both projections", () => {
    let projs = [perspective(mat4(), Math.PI / 3, 16 / 9, 0.1, 100), orthographic(mat4(), -4, 4, 3, -3, 0.5, 50)]
    for (let proj of projs) {
      let inv = invert(mat4(), multiply(mat4(), proj, view))
      let near = unproject(inv, 0, 0, -1)
      let far = unproject(inv, 0, 0, 1)
      let ray: Vec3 = [far[0] - near[0], far[1] - near[1], far[2] - near[2]]
      let len = Math.hypot(ray[0], ray[1], ray[2])
      for (let i = 0; i < 3; i++) expect(ray[i]! / len).toBeCloseTo(forward[i]!, 6)
    }
  })

  test("a perspective ray starts at the near plane in front of the eye", () => {
    let proj = perspective(mat4(), Math.PI / 3, 16 / 9, 0.1, 100)
    let inv = invert(mat4(), multiply(mat4(), proj, view))
    let near = unproject(inv, 0, 0, -1)
    let d: Vec3 = [near[0] - eye[0], near[1] - eye[1], near[2] - eye[2]]
    expect(d[0] * forward[0] + d[1] * forward[1] + d[2] * forward[2]).toBeCloseTo(0.1, 6)
  })

  test("a singular matrix yields finite numbers, not NaN", () => {
    let zero: Mat4 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    let inv = invert(mat4(), zero)
    for (let i = 0; i < 16; i++) expect(Number.isFinite(inv[i])).toBe(true)
  })
})
