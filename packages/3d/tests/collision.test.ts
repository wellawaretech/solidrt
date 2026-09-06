// moveAndSlide's loop logic against an analytic scene: one-sided planes
// answering sweep()/overlap() for spheres exactly, so what is asserted is
// the controller (skin, slide, floor/wall/ceiling, snap, depenetration),
// not the core narrowphase (alloy's spatial_collide tests cover that).
// `bun test packages/3d/tests`.
import { describe, expect, test } from "bun:test"
import { moveAndSlide } from "../src/collision.ts"
import type { MoveScene } from "../src/collision.ts"
import type { Mesh } from "../src/mesh.ts"
import type { Impact, Overlap, QueryOptions, Vec3, Volume } from "../src/index.ts"

// A plane n . x = d whose outside is the +n side.
type Plane = { normal: Vec3; d: number; mesh: Mesh }

let plane = (normal: Vec3, d: number): Plane => ({ normal, d, mesh: {} as Mesh })
let dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2]

function sphereOf(volume: Volume): { center: Vec3; radius: number } {
  if (!("center" in volume) || !("radius" in volume)) throw new Error("the analytic scene takes spheres only")
  return volume
}

/** The planes as a scene; `seen` records the options every query got. */
function sceneOf(planes: Plane[], seen: QueryOptions[] = []): MoveScene {
  return {
    overlap(volume, opts) {
      if (opts) seen.push(opts)
      let { center, radius } = sphereOf(volume)
      let out: Overlap[] = []
      for (let p of planes) {
        let gap = dot(p.normal, center) - p.d - radius
        if (gap <= 0) out.push({ mesh: p.mesh, point: center, normal: p.normal, depth: -gap })
      }
      return out
    },
    sweep(volume, motion, opts) {
      if (opts) seen.push(opts)
      let { center, radius } = sphereOf(volume)
      let out: Impact[] = []
      for (let p of planes) {
        let gap = dot(p.normal, center) - p.d - radius
        let closing = -dot(p.normal, motion)
        if (closing <= 0) continue
        let time = gap <= 0 ? 0 : gap / closing
        if (time <= 1) out.push({ mesh: p.mesh, time, point: center, normal: p.normal })
      }
      return out.sort((a, b) => a.time - b.time)
    },
  }
}

const SKIN = 0.01
let floor = plane([0, 1, 0], 0)
let ball = (x: number, y: number, z: number): Volume => ({ center: [x, y, z], radius: 0.5 })

function expectVec(v: Vec3, want: Vec3, digits = 4) {
  expect(v[0]).toBeCloseTo(want[0], digits)
  expect(v[1]).toBeCloseTo(want[1], digits)
  expect(v[2]).toBeCloseTo(want[2], digits)
}

describe("moveAndSlide", () => {
  test("moves freely with nothing in the way", () => {
    let r = moveAndSlide(sceneOf([]), ball(0, 5, 0), [1, -2, 3])
    expectVec(r.motion, [1, -2, 3])
    expect(r.floor).toBeNull()
    expect(r.wall).toBe(false)
    expect(r.hits).toHaveLength(0)
  })

  test("lands on a floor a skin short and drops the fall", () => {
    let r = moveAndSlide(sceneOf([floor]), ball(0, 2, 0), [0, -3, 0])
    expectVec(r.motion, [0, -(1.5 - SKIN), 0])
    expectVec(r.floor!, [0, 1, 0])
    expect(r.wall).toBe(false)
  })

  test("walks along a floor while gravity pulls", () => {
    let r = moveAndSlide(sceneOf([floor]), ball(0, 0.5 + SKIN, 0), [1, -0.2, 0])
    expectVec(r.motion, [1, 0, 0])
    expectVec(r.floor!, [0, 1, 0])
  })

  test("slides along a wall and keeps the floor", () => {
    let wall = plane([-1, 0, 0], -5)
    let r = moveAndSlide(sceneOf([floor, wall]), ball(0, 0.5 + SKIN, 0), [10, 0, 5])
    expectVec(r.motion, [4.5 - SKIN, 0, 5], 3)
    expect(r.wall).toBe(true)
    expect(r.floor).not.toBeNull()
  })

  test("stops in a corner", () => {
    let walls = [plane([-1, 0, 0], -5), plane([0, 0, -1], -5)]
    let r = moveAndSlide(sceneOf([floor, ...walls]), ball(4.5 - SKIN, 0.5 + SKIN, 4.5 - SKIN), [1, 0, 1])
    expect(Math.hypot(r.motion[0], r.motion[2])).toBeLessThan(1e-6)
    expect(r.wall).toBe(true)
  })

  test("bumps a ceiling without snapping to a floor", () => {
    let ceiling = plane([0, -1, 0], -3)
    let r = moveAndSlide(sceneOf([floor, ceiling]), ball(0, 1, 0), [0, 5, 0])
    expectVec(r.motion, [0, 1.5 - SKIN, 0])
    expect(r.ceiling).toBe(true)
    expect(r.floor).toBeNull()
  })

  test("slides down a slope too steep to stand on", () => {
    let steep = plane([Math.sin(Math.PI / 3), Math.cos(Math.PI / 3), 0], 0)
    let r = moveAndSlide(sceneOf([steep]), ball(2, 2, 0), [0, -6, 0])
    expect(r.floor).toBeNull()
    expect(r.wall).toBe(true)
    expect(r.motion[0]).toBeGreaterThan(0.5)
    expect(r.motion[1]).toBeLessThan(-4)
  })

  test("pushes out of a surface it starts inside", () => {
    let r = moveAndSlide(sceneOf([floor]), ball(0, 0.3, 0), [0, 0, 0])
    expectVec(r.motion, [0, 0.2 + SKIN, 0])
    expect(r.floor).not.toBeNull()
  })

  test("snaps down onto a floor within reach, never upward", () => {
    let r = moveAndSlide(sceneOf([floor]), ball(0, 0.55, 0), [1, 0, 0])
    expectVec(r.motion, [1, -(0.05 - SKIN), 0])
    expect(r.floor).not.toBeNull()
    let off = moveAndSlide(sceneOf([floor]), ball(0, 0.55, 0), [1, 0, 0], { floorSnap: 0 })
    expectVec(off.motion, [1, 0, 0])
    expect(off.floor).toBeNull()
    let rising = moveAndSlide(sceneOf([floor]), ball(0, 0.55, 0), [1, 0.1, 0])
    expectVec(rising.motion, [1, 0.1, 0])
    expect(rising.floor).toBeNull()
    let far = moveAndSlide(sceneOf([floor]), ball(0, 1, 0), [1, 0, 0])
    expect(far.floor).toBeNull()
  })

  test("passes the layer and mesh filters to every query", () => {
    let seen: QueryOptions[] = []
    let meshes = [floor.mesh]
    moveAndSlide(sceneOf([floor], seen), ball(0, 2, 0), [0, -3, 0], { layers: 2, meshes })
    expect(seen.length).toBeGreaterThan(0)
    for (let q of seen) {
      expect(q.layers).toBe(2)
      expect(q.meshes).toBe(meshes)
    }
  })
})
