// Check rig for the geometry-as-data ops (src/geometry.ts): transformGeometry
// against hand-computed points and normals (non-uniform scale included),
// mergeGeometries offsets, uint32 widening and the mixed-layout rejection,
// and the exported bounds/ray helpers. Pure-module inputs only, so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/geometry-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end; the flux binary exits 0
// regardless, so read the output, not the exit code.

import { box, geometryBounds, mergeGeometries, plane, transformGeometry, withColors, FLOATS_PER_VERTEX } from "../src/geometry.ts"
import type { Geometry } from "../src/geometry.ts"
import { rayBoxDistance } from "../src/bvh.ts"

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}
let near = (a: number, b: number, eps = 1e-5): boolean => Math.abs(a - b) <= eps
let expectVec = (label: string, got: ArrayLike<number>, want: ArrayLike<number>): void => {
  for (let i = 0; i < want.length; i++) {
    if (!near(got[i]!, want[i]!)) {
      fail(`${label}: got [${Array.from(got as number[]).map((v) => v.toFixed(4))}] want [${Array.from(want as number[]).map((v) => v.toFixed(4))}]`)
      return
    }
  }
}
let throws = (label: string, fn: () => unknown): void => {
  try {
    fn()
    fail(`${label}: did not throw`)
  } catch {
    // expected
  }
}

// One-triangle geometry with a known position and normal.
let tri = (): Geometry => ({
  vertices: new Float32Array([
    1, 0, 0, 0, 0, 1, 0, 0,
    0, 1, 0, 0, 0, 1, 1, 0,
    0, 0, 0, 0, 0, 1, 0, 1,
  ]),
  indices: new Uint16Array([0, 1, 2]),
  label: "tri",
})

// Translation moves positions, leaves normals and uvs alone.
{
  let g = transformGeometry(tri(), { position: [10, 20, 30] })
  expectVec("translate pos", g.vertices.subarray(0, 3), [11, 20, 30])
  expectVec("translate normal", g.vertices.subarray(3, 6), [0, 0, 1])
  expectVec("translate uv", g.vertices.subarray(14, 16), [1, 0])
  if (g.label !== "tri-transformed") fail("label default: " + g.label)
  if (g.indices.length !== 3) fail("indices carried")
}

// 90 degrees about y: +x -> -z, the +z normal -> +x.
{
  let g = transformGeometry(tri(), { rotation: [0, Math.PI / 2, 0] })
  expectVec("rotate pos", g.vertices.subarray(0, 3), [0, 0, -1])
  expectVec("rotate normal", g.vertices.subarray(3, 6), [1, 0, 0])
}

// Quaternion form agrees with the euler form.
{
  let s = Math.sin(0.4), c = Math.cos(0.4)
  let b = transformGeometry(tri(), { quaternion: [0, s, 0, c] })
  let e = transformGeometry(tri(), { rotation: [0, 0.8, 0] })
  expectVec("quat vs euler", b.vertices.subarray(0, 6), e.vertices.subarray(0, 6))
}

// Non-uniform scale: a tilted normal must go through the inverse transpose.
// Normal (1,1,0)/sqrt2 on a surface scaled by (2,1,1): the plane x+y=c
// becomes x/2+y=c, whose normal is (0.5,1,0) normalized, NOT (2,1,0).
{
  let g: Geometry = {
    vertices: new Float32Array([0, 0, 0, Math.SQRT1_2, Math.SQRT1_2, 0, 0, 0]),
    indices: new Uint16Array([0, 0, 0]),
  }
  let t = transformGeometry(g, { scale: [2, 1, 1] })
  let l = Math.hypot(0.5, 1)
  expectVec("non-uniform normal", t.vertices.subarray(3, 6), [0.5 / l, 1 / l, 0])
  let u = transformGeometry(g, { scale: 3 })
  expectVec("uniform scale normal", u.vertices.subarray(3, 6), [Math.SQRT1_2, Math.SQRT1_2, 0])
}

// Colored layout: stride 12, color slots copy through untouched.
{
  let c = withColors(tri(), [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1])
  let g = transformGeometry(c, { position: [1, 0, 0] })
  if (g.layout !== "colored") fail("colored layout kept")
  expectVec("colored pos", g.vertices.subarray(12, 15), [1, 1, 0])
  expectVec("colored color", g.vertices.subarray(20, 24), [0, 1, 0, 1])
}

throws("rotation and quaternion", () => transformGeometry(tri(), { rotation: [0, 0, 0], quaternion: [0, 0, 0, 1] }))

// Merge: offsets and counts.
{
  let a = box(1, 1, 1)
  let b = transformGeometry(box(1, 1, 1), { position: [3, 0, 0] })
  let m = mergeGeometries([a, b], "pair")
  let va = a.vertices.length / FLOATS_PER_VERTEX
  if (m.vertices.length !== a.vertices.length + b.vertices.length) fail("merge vertex count")
  if (m.indices.length !== a.indices.length + b.indices.length) fail("merge index count")
  if (m.indices[a.indices.length]! !== b.indices[0]! + va) fail("merge index offset")
  if (!(m.indices instanceof Uint16Array)) fail("merge stays uint16")
  if (m.label !== "pair") fail("merge label")
  expectVec("merge bounds", geometryBounds(m), [-0.5, -0.5, -0.5, 3.5, 0.5, 0.5])
}

// Merge past 64k vertices widens the index array.
{
  let parts: Geometry[] = []
  for (let i = 0; i < 70000 / 4 + 1; i++) parts.push(plane(1, 1))
  let m = mergeGeometries(parts)
  if (!(m.indices instanceof Uint32Array)) fail("merge widens to uint32")
  let last = parts.length - 1
  let lastPart = parts[last]!
  if (m.indices[m.indices.length - 1]! !== lastPart.indices[lastPart.indices.length - 1]! + last * 4) fail("uint32 offset")
}

throws("merge mixed layouts", () => mergeGeometries([tri(), withColors(tri(), () => [1, 1, 1, 1])]))
throws("merge empty", () => mergeGeometries([]))

// Public ray helper: hit from outside, inside, miss.
{
  if (!near(rayBoxDistance(-2, 0, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1), 1)) fail("ray enters at 1")
  if (rayBoxDistance(0, 0, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1) !== 0) fail("ray inside is 0")
  if (rayBoxDistance(-2, 5, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1) !== -1) fail("ray misses")
}

if (failures > 0) throw new Error(failures + " geometry check(s) failed")
console.log("PASS: geometry ops")
