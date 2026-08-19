// Check rig for the scene's draw-list ordering (src/order.ts): the cases
// the transparency probe verified on a client, plus a randomized invariant
// sweep against a linear oracle. Pure-module inputs only (order.ts, math.ts
// import no GUI), so it runs headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/order-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end, so the run exits nonzero.

import { argv } from "flux:process"
import { orderEntries } from "../src/order.ts"
import type { Orderable } from "../src/order.ts"
import { lookAt, mat4 } from "../src/math.ts"
import type { Vec3 } from "../src/math.ts"

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
let same = (a: number[], b: number[]): boolean => a.length === b.length && a.every((v, i) => v === b[i])
let expect = (label: string, got: number[], want: number[]): void => {
  if (!same(got, want)) fail(`${label}: got [${got}] want [${want}]`)
}

let item = (entry: number, transparent: boolean, center: Vec3, renderOrder = 0): Orderable<number> => ({
  _entry: entry,
  _transparent: transparent,
  renderOrder,
  _center: center,
})

// Camera at +z looking at the origin: larger world z is nearer.
let view = mat4()
lookAt(view, [0, 0, 5], [0, 0, 0], [0, 1, 0])

// --- The probe's cases ---

// A: add order already back-to-front stays.
expect("A add order right", orderEntries([item(1, true, [-0.3, 0, 0]), item(2, true, [0.3, 0, 0.5])], view), [1, 2])
// B: add order wrong is reversed.
expect("B add order wrong", orderEntries([item(1, true, [-0.3, 0, 0.5]), item(2, true, [0.3, 0, 0])], view), [2, 1])
// Off-origin geometry sorts by center: red's center at z=2 beats blue at 1.
expect("center key", orderEntries([item(1, true, [-0.3, 0, 2]), item(2, true, [0.3, 0, 1])], view), [2, 1])
// Opaques first, in add order, whatever their depth; background before all.
expect(
  "groups + background",
  orderEntries([item(1, true, [0, 0, 0]), item(2, false, [0, 0, 3]), item(3, false, [0, 0, -3]), item(4, true, [0, 0, 1])], view, 9),
  [9, 2, 3, 1, 4],
)
// renderOrder sorts within a group and beats depth for transparents.
expect(
  "renderOrder",
  orderEntries([item(1, false, [0, 0, 0], 1), item(2, false, [0, 0, 0], 0), item(3, true, [0, 0, 2], 1), item(4, true, [0, 0, 0], 0)], view),
  [2, 1, 4, 3],
)
// Detached meshes (no entry) are skipped.
expect("skips detached", orderEntries([item(1, false, [0, 0, 0]), { ...item(2, false, [0, 0, 0]), _entry: null }], view), [1])
// A single transparent mesh needs no depth: still last.
expect("single transparent", orderEntries([item(1, true, [0, 0, 9]), item(2, false, [0, 0, 0])], view), [2, 1])

// --- Randomized invariants against a linear oracle ---

let viewZ = (c: Vec3): number => view[2] * c[0] + view[6] * c[1] + view[10] * c[2] + view[14]
let sweeps = 0
for (let round = 0; round < 500; round++) {
  lookAt(view, [range(-10, 10), range(-10, 10), range(-10, 10)], [range(-1, 1), range(-1, 1), range(-1, 1)], [0, 1, 0])
  let n = 1 + Math.floor(rand() * 12)
  let items: Orderable<number>[] = []
  for (let i = 0; i < n; i++) {
    items.push(item(i + 1, rand() < 0.5, [range(-5, 5), range(-5, 5), range(-5, 5)], Math.floor(rand() * 3)))
  }
  let order = orderEntries(items, view)
  let byEntry = new Map(items.map(m => [m._entry!, m]))
  if (order.length !== items.length || new Set(order).size !== items.length) {
    fail(`round ${round}: not a permutation`)
    continue
  }
  let seenTransparent = false
  for (let i = 0; i < order.length; i++) {
    let m = byEntry.get(order[i]!)!
    if (m._transparent) seenTransparent = true
    else if (seenTransparent) fail(`round ${round}: opaque after transparent`)
    if (i === 0) continue
    let p = byEntry.get(order[i - 1]!)!
    if (p._transparent !== m._transparent) continue
    if (p.renderOrder > m.renderOrder) fail(`round ${round}: renderOrder not ascending`)
    if (p.renderOrder < m.renderOrder) continue
    if (m._transparent) {
      if (viewZ(p._center) > viewZ(m._center) + 1e-9) fail(`round ${round}: transparent not back-to-front`)
    } else if (p._entry! > m._entry!) {
      fail(`round ${round}: opaque add order not kept`)
    }
  }
  sweeps++
}

if (failures === 0) {
  console.log(`PASS: 7 cases, ${sweeps} randomized sweeps`)
} else {
  throw new Error(`${failures} FAILURES (seed ${seed})`)
}
