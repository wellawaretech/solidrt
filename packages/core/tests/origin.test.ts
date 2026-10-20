// The render origin's snap policy (origin.ts): the cell is the power of
// two at or below scale times factor, the origin moves only when the eye
// leaves the band of one cell around its own cell, lands on the grid, and
// never moves for a cell of zero. Pure, headless on flux: `sol test
// packages/core`.

import { test } from "flux:test"
import { originCell, snapOrigin } from "../src/origin.ts"

function fail(msg: string): void {
  throw new Error(msg)
}

test("originCell is the power of two at or below scale times factor, 0 for a scale that is not positive and finite", () => {
  if (originCell(0.1, 64) !== 4) fail("0.1 * 64 = 6.4 -> 4, got " + originCell(0.1, 64))
  if (originCell(1, 64) !== 64) fail("1 * 64 -> 64")
  if (originCell(1000, 64) !== 32768) fail("1000 * 64 = 64000 -> 32768, got " + originCell(1000, 64))
  if (originCell(1 / 2, 65536) !== 32768) fail("a 2d view at zoom 2 -> 32768 px")
  if (originCell(0, 64) !== 0) fail("zero scale -> 0")
  if (originCell(-1, 64) !== 0) fail("negative scale -> 0")
  if (originCell(Number.POSITIVE_INFINITY, 64) !== 0) fail("infinite scale -> 0")
  if (originCell(Number.NaN, 64) !== 0) fail("NaN scale -> 0")
})

test("the origin snaps to the eye's cell and holds while the eye stays within a cell of it", () => {
  let origin = [0, 0, 0]
  // A first eye far out: every axis snaps to its grid cell.
  if (!snapOrigin(origin, [1_000_001, 2_000_002.5, -3_000_003], 4)) fail("first snap moves")
  if (origin[0] !== 1_000_000 || origin[1] !== 2_000_000 || origin[2] !== -3_000_004) fail("snapped to the grid: " + origin)
  // Inside [-cell, 2 cell) on every axis: no move, whatever the fraction.
  if (snapOrigin(origin, [1_000_007.9, 1_999_996.1, -3_000_000.5], 4)) fail("within the band: holds")
  if (origin[0] !== 1_000_000) fail("held")
  // One axis leaves the band: that axis alone re-snaps.
  if (!snapOrigin(origin, [1_000_008, 2_000_001, -3_000_001], 4)) fail("leaving the band moves")
  if (origin[0] !== 1_000_008 || origin[1] !== 2_000_000 || origin[2] !== -3_000_004) fail("one axis re-snapped: " + origin)
  // Going back below the band re-snaps too (hysteresis both ways).
  if (!snapOrigin(origin, [1_000_003.9, 2_000_001, -3_000_001], 4)) fail("below the band moves")
  if (origin[0] !== 1_000_000) fail("re-snapped down: " + origin[0])
})

test("a cell of zero never moves the origin, and a 2d origin has two axes", () => {
  let origin = [5, 6, 7]
  if (snapOrigin(origin, [1e9, 1e9, 1e9], 0)) fail("cell 0 holds")
  if (origin[0] !== 5) fail("untouched")
  let flat = [0, 0]
  if (snapOrigin(flat, [70000, -1], 65536)) fail("within two cells: holds")
  if (!snapOrigin(flat, [200000, -70000], 65536)) fail("2d snap moves")
  if (flat[0] !== 196608 || flat[1] !== -131072) fail("2d grid: " + flat)
})
