// Differential check rig for the picking stack: the BVH against a linear
// oracle, invertAffine against compose, and the slab test's edge cases.
// Pure-module inputs only (bvh.ts, math.ts import no GUI), so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/pick-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end; the flux binary exits 0
// regardless, so read the output, not the exit code.

import { argv } from "flux:process"
import { createBvh, rayBoxDistance } from "../src/bvh.ts"
import { compose, invertAffine, mat4, multiply, quatFromEuler, transformPoint, transformVector } from "../src/math.ts"
import type { Mat4, Quat, Vec3, Vec4 } from "../src/math.ts"

let seed = Number(argv[0] ?? Math.floor(Math.random() * 1e9))
console.log("seed", seed)
let state = seed || 1
// mulberry32
let rand = (): number => {
  state = (state + 0x6d2b79f5) | 0
  let t = Math.imul(state ^ (state >>> 15), 1 | state)
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296
}
let range = (lo: number, hi: number): number => lo + rand() * (hi - lo)

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}

// --- rayBoxDistance edge cases ---

// Origin inside the box: distance 0.
if (rayBoxDistance(0, 0, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1) !== 0) fail("inside box should hit at 0")
// Box behind the ray: miss.
if (rayBoxDistance(5, 0, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1) !== -1) fail("box behind ray should miss")
// Axis-parallel ray, zero direction component inside the slab: hit.
if (rayBoxDistance(0, 0, -5, 0, 0, 1, -1, -1, -1, 1, 1, 1) < 0) fail("axis-parallel ray should hit")
// Zero component outside the slab: miss even though other axes cross.
if (rayBoxDistance(0, 2, -5, 0, 0, 1, -1, -1, -1, 1, 1, 1) !== -1) fail("parallel outside slab should miss")
// Flat (zero-extent) box: a plane still hits.
if (rayBoxDistance(0, 5, 0, 0, -1, 0, -1, 0, -1, 1, 0, 1) !== 5) fail("flat box should hit at 5")

// --- invertAffine: M * M^-1 == identity over random TRS matrices ---

let q: Quat = [0, 0, 0, 1]
let m = mat4()
let inv = mat4()
let prod = mat4()
for (let i = 0; i < 2000; i++) {
  let euler: Vec3 = [range(-Math.PI, Math.PI), range(-Math.PI, Math.PI), range(-Math.PI, Math.PI)]
  quatFromEuler(q, euler)
  let pos: Vec3 = [range(-50, 50), range(-50, 50), range(-50, 50)]
  // Non-uniform, sign-flipping, and near-degenerate scales included.
  let scale: Vec3 = [range(0.01, 10) * (rand() < 0.2 ? -1 : 1), range(0.01, 10), range(0.01, 10)]
  compose(m, pos, q, scale)
  invertAffine(inv, m)
  multiply(prod, m, inv)
  for (let k = 0; k < 16; k++) {
    let want = k % 5 === 0 ? 1 : 0
    if (Math.abs(prod[k]! - want) > 1e-9 * Math.max(1, Math.abs(m[12]!) + Math.abs(m[13]!) + Math.abs(m[14]!))) {
      fail(`invertAffine round-trip off at [${k}]: ${prod[k]} (iteration ${i})`)
      i = 2000
      break
    }
  }
}

// --- ray parameter preserved under affine transform into local space ---

for (let i = 0; i < 500; i++) {
  quatFromEuler(q, [range(-3, 3), range(-3, 3), range(-3, 3)])
  compose(m, [range(-5, 5), range(-5, 5), range(-5, 5)], q, [range(0.1, 4), range(0.1, 4), range(0.1, 4)])
  invertAffine(inv, m)
  let o: Vec3 = [range(-10, 10), range(-10, 10), range(-10, 10)]
  let d: Vec3 = [range(-1, 1), range(-1, 1), range(-1, 1)]
  if (Math.hypot(d[0], d[1], d[2]) < 1e-3) continue
  let t = range(0.1, 20)
  // World point at parameter t, sent to local space directly...
  let world: Vec3 = [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t]
  let lp: Vec4 = [0, 0, 0, 0]
  transformPoint(lp, inv, world)
  // ...must equal local origin + t * local direction.
  let lo: Vec4 = [0, 0, 0, 0]
  transformPoint(lo, inv, o)
  let ld: Vec3 = [0, 0, 0]
  transformVector(ld, inv, d)
  for (let k = 0; k < 3; k++) {
    if (Math.abs(lp[k]! - (lo[k]! + ld[k]! * t)) > 1e-8 * Math.max(1, Math.abs(lp[k]!))) {
      fail(`affine map broke the ray parameter (iteration ${i}, axis ${k})`)
      i = 500
      break
    }
  }
}

// --- BVH vs linear oracle, through inserts, moves, and removals ---

type Box = { id: number; leaf: number; min: Vec3; max: Vec3; alive: boolean }

let bvh = createBvh<Box>()
let boxes: Box[] = []
let makeBox = (id: number): Box => {
  let cx = range(-100, 100)
  let cy = range(-100, 100)
  let cz = range(-100, 100)
  // Mixed sizes, including flat boxes (a plane's world AABB).
  let ex = rand() < 0.1 ? 0 : range(0.1, 8)
  let ey = rand() < 0.1 ? 0 : range(0.1, 8)
  let ez = rand() < 0.1 ? 0 : range(0.1, 8)
  return { id, leaf: -1, min: [cx - ex, cy - ey, cz - ez], max: [cx + ex, cy + ey, cz + ez], alive: true }
}

for (let i = 0; i < 300; i++) {
  let box = makeBox(i)
  box.leaf = bvh.insert(box, box.min[0], box.min[1], box.min[2], box.max[0], box.max[1], box.max[2])
  boxes.push(box)
}

let queryCount = 0
let compare = (label: string): void => {
  for (let i = 0; i < 200; i++) {
    let ox = range(-150, 150)
    let oy = range(-150, 150)
    let oz = range(-150, 150)
    let dx = range(-1, 1)
    let dy = range(-1, 1)
    let dz = range(-1, 1)
    if (rand() < 0.2) dx = 0
    if (rand() < 0.2) dy = 0
    if (Math.hypot(dx, dy, dz) < 1e-3) continue
    queryCount++
    // Oracle: narrowphase every live box linearly.
    let expected = new Set<number>()
    for (let box of boxes) {
      if (!box.alive) continue
      let t = rayBoxDistance(ox, oy, oz, dx, dy, dz, box.min[0], box.min[1], box.min[2], box.max[0], box.max[1], box.max[2])
      if (t >= 0) expected.add(box.id)
    }
    // BVH broadphase (fat boxes: a superset) + the same narrowphase.
    let got = new Set<number>()
    let visits = 0
    bvh.raycast(ox, oy, oz, dx, dy, dz, box => {
      visits++
      let t = rayBoxDistance(ox, oy, oz, dx, dy, dz, box.min[0], box.min[1], box.min[2], box.max[0], box.max[1], box.max[2])
      if (t >= 0) got.add(box.id)
    })
    for (let id of expected) {
      if (!got.has(id)) fail(`${label}: BVH missed box ${id} (query ${i})`)
    }
    for (let id of got) {
      if (!expected.has(id)) fail(`${label}: BVH hit phantom box ${id} (query ${i})`)
    }
  }
}

compare("static")

// Move a third of the boxes (some a little, some far), matching how the
// scene refits leaves per sync.
for (let box of boxes) {
  if (rand() < 0.33) {
    let far = rand() < 0.5
    let shift = (): number => (far ? range(-80, 80) : range(-0.05, 0.05))
    let sx = shift(), sy = shift(), sz = shift()
    box.min = [box.min[0] + sx, box.min[1] + sy, box.min[2] + sz]
    box.max = [box.max[0] + sx, box.max[1] + sy, box.max[2] + sz]
    bvh.update(box.leaf, box.min[0], box.min[1], box.min[2], box.max[0], box.max[1], box.max[2])
  }
}
compare("after moves")

// Remove a quarter, then insert replacements (slot reuse paths).
for (let box of boxes) {
  if (rand() < 0.25) {
    bvh.remove(box.leaf)
    box.alive = false
  }
}
compare("after removals")
for (let i = 0; i < 100; i++) {
  let box = makeBox(1000 + i)
  box.leaf = bvh.insert(box, box.min[0], box.min[1], box.min[2], box.max[0], box.max[1], box.max[2])
  boxes.push(box)
}
compare("after reinserts")

if (failures === 0) {
  console.log(`PASS: edge cases, 2000 inverses, 500 ray-parameter trips, ${queryCount} differential queries`)
} else {
  throw new Error(`${failures} FAILURES (seed ${seed})`)
}
