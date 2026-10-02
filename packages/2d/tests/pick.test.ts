// Differential check for pointInSprite (the picking narrowphase): the
// rotated-rect containment test against a brute-force oracle that
// transforms the rect's corners forward and half-plane-tests the point,
// plus hand-written edge cases. Pure-module input only (pick.ts imports no
// GUI), so it runs headless on flux: `sol test packages/2d`. The random
// inputs come from Math.random, which `sol test` seeds: the same on every
// run, and `--seed <n>` tries others.

import { test } from "flux:test"
import { pointInSprite } from "../src/pick.ts"

function range(lo: number, hi: number): number {
  return lo + Math.random() * (hi - lo)
}

function fail(msg: string): void {
  throw new Error(msg)
}

// Oracle: transform the four corners forward, then test the point against
// each edge's half-plane (convex, consistent winding).
function oracle(px: number, py: number, cx: number, cy: number, w: number, h: number, rotation: number): boolean {
  let c = Math.cos(rotation)
  let sn = Math.sin(rotation)
  let corners: [number, number][] = []
  for (let [ux, uy] of [
    [-0.5, -0.5],
    [0.5, -0.5],
    [0.5, 0.5],
    [-0.5, 0.5],
  ] as const) {
    let ox = ux * w
    let oy = uy * h
    corners.push([cx + ox * c - oy * sn, cy + ox * sn + oy * c])
  }
  for (let i = 0; i < 4; i++) {
    let [ax, ay] = corners[i]!
    let [bx, by] = corners[(i + 1) % 4]!
    let cross = (bx - ax) * (py - ay) - (by - ay) * (px - ax)
    if (cross < 0) return false
  }
  return true
}

test("pointInSprite: axis-aligned boundary, zero size, full turn", () => {
  // Hand-written edge cases first: axis-aligned boundary, zero size, full turn.
  if (!pointInSprite(10, 10, 10, 10, 4, 4, 0)) fail("center not contained")
  if (!pointInSprite(12, 10, 10, 10, 4, 4, 0)) fail("right edge not contained (inclusive)")
  if (pointInSprite(12.001, 10, 10, 10, 4, 4, 0)) fail("just past right edge contained")
  if (pointInSprite(10, 10, 20, 20, 0, 0, 0)) fail("zero-size sprite contains a distant point")
  if (!pointInSprite(10, 10, 10, 10, 0, 0, 0)) fail("zero-size sprite excludes its own center")
  // A 90-degree turn swaps the extents.
  if (!pointInSprite(10, 13, 10, 10, 8, 2, Math.PI / 2)) fail("90deg: rotated long axis not contained")
  if (pointInSprite(13, 10, 10, 10, 8, 2, Math.PI / 2)) fail("90deg: original long axis still contained")
})

test("pointInSprite matches the corner-transform oracle, over random rects", () => {
  // Randomized differential sweep. Points near the boundary are the
  // interesting ones, so half the samples hug the rect's extent.
  const SWEEPS = 20000
  for (let i = 0; i < SWEEPS; i++) {
    let cx = range(-100, 100)
    let cy = range(-100, 100)
    let w = range(0.1, 60)
    let h = range(0.1, 60)
    let rot = range(-7, 7)
    let reach = (w + h) * (Math.random() < 0.5 ? 0.75 : 2)
    let px = cx + range(-reach, reach)
    let py = cy + range(-reach, reach)
    let got = pointInSprite(px, py, cx, cy, w, h, rot)
    let want = oracle(px, py, cx, cy, w, h, rot)
    if (got !== want) {
      // The test and the oracle may round differently within a float-epsilon
      // band of the boundary; a nudge toward the center decides whether the
      // disagreement is real (measure-zero boundary noise is not a failure).
      let eps = 1e-6
      let nx = px + (cx - px) * eps
      let ny = py + (cy - py) * eps
      if (pointInSprite(nx, ny, cx, cy, w, h, rot) !== oracle(nx, ny, cx, cy, w, h, rot)) {
        fail(`mismatch at p=(${px}, ${py}) rect=(${cx}, ${cy}, ${w}x${h}, rot ${rot}): got ${got}, oracle ${want}`)
      }
    }
  }
})
