// Tests for the tile-grid math (tiles-math.ts): a cell's chunk and record
// slot against a brute-force oracle over negative cells too, the chunk walk
// of a rect covering every cell of the rect exactly once and nothing
// outside it, each slice inside its own chunk, the cell/rect validation
// under bounds, the index form's flag codec (Tiled's three bits and the
// clear values) and the eight cell symmetries as the record encodes them
// against the composed oracle. Pure-module input only (tiles-math.ts
// imports nothing), so it runs headless on flux: `sol test packages/2d`.
// The random inputs come from Math.random, which `sol test` seeds: the
// same on every run, and `--seed <n>` tries others.

import { test, expect } from "flux:test"
import { cellSymmetry, checkCell, checkRect, chunkKey, eachChunkSlice, flagsOf, floorDiv, footprint, indexOf, isClearIndex, mod, slotOf, TILE_FLIP_X, TILE_FLIP_Y, TILE_TRANSPOSE } from "../src/tiles-math.ts"

function int(lo: number, hi: number): number {
  return lo + Math.floor(Math.random() * (hi - lo + 1))
}

function fail(msg: string): void {
  throw new Error(msg)
}

// Random grids per sweep, and rects walked per grid.
const GRIDS = 200
const RECTS = 20
// The record width the slot oracle assumes (any positive integer works;
// the layer's own is INSTANCE_FLOATS).
const FLOATS = 13
// The cell range the sweeps draw from, both sides of the origin.
const REACH = 100

test("floorDiv, mod and slotOf against the oracle over negative cells: the chunk contains the cell, slots are row-major inside it", () => {
  for (let g = 0; g < GRIDS; g++) {
    let chunkTiles = int(1, 16)
    for (let k = 0; k < RECTS; k++) {
      let col = int(-REACH, REACH)
      let row = int(-REACH, REACH)
      let cc = floorDiv(col, chunkTiles)
      let cr = floorDiv(row, chunkTiles)
      if (!(cc * chunkTiles <= col && col < (cc + 1) * chunkTiles && cr * chunkTiles <= row && row < (cr + 1) * chunkTiles)) {
        fail(`chunk ${cc}, ${cr} does not contain cell ${col}, ${row} (chunkTiles ${chunkTiles})`)
      }
      if (mod(col, chunkTiles) !== col - cc * chunkTiles) fail(`mod(${col}, ${chunkTiles}) is not the offset inside chunk ${cc}`)
      let slot = slotOf(col, row, chunkTiles, FLOATS)
      let want = ((row - cr * chunkTiles) * chunkTiles + (col - cc * chunkTiles)) * FLOATS
      if (slot !== want) fail(`slotOf(${col}, ${row}) = ${slot}, want ${want} (chunkTiles ${chunkTiles})`)
      if (slot < 0 || slot >= chunkTiles * chunkTiles * FLOATS) fail(`slotOf(${col}, ${row}) = ${slot} outside the chunk's records`)
    }
  }
})

test("chunkKey is one-to-one over chunk pairs and throws beyond the addressable world", () => {
  let seen = new Map<number, string>()
  for (let cc = -40; cc <= 40; cc++) {
    for (let cr = -40; cr <= 40; cr++) {
      let key = chunkKey(cc, cr)
      let prior = seen.get(key)
      if (prior !== undefined) fail(`chunks ${prior} and ${cc},${cr} share key ${key}`)
      seen.set(key, `${cc},${cr}`)
    }
  }
  expect(() => chunkKey(2 ** 25, 0)).toThrow()
  expect(() => chunkKey(0, -(2 ** 25))).toThrow()
})

test("eachChunkSlice: the slices cover the rect exactly once, stay inside their own chunk, and arrive chunk rows outer", () => {
  for (let g = 0; g < GRIDS; g++) {
    let chunkTiles = int(1, 16)
    for (let k = 0; k < RECTS; k++) {
      let w = int(1, 60)
      let h = int(1, 60)
      let col = int(-REACH, REACH)
      let row = int(-REACH, REACH)
      let seen = new Uint8Array(w * h)
      let lastRow = -Infinity
      let lastCol = -Infinity
      let slices = 0
      eachChunkSlice(col, row, w, h, chunkTiles, (cc, cr, colA, colB, rowA, rowB) => {
        slices++
        if (cr < lastRow || (cr === lastRow && cc <= lastCol)) fail(`slices arrive chunk rows outer: ${cc},${cr} after ${lastCol},${lastRow}`)
        lastRow = cr
        lastCol = cc
        if (colA >= colB || rowA >= rowB) fail(`empty slice ${colA}..${colB} x ${rowA}..${rowB} for chunk ${cc},${cr}`)
        for (let y = rowA; y < rowB; y++) {
          for (let x = colA; x < colB; x++) {
            if (floorDiv(x, chunkTiles) !== cc || floorDiv(y, chunkTiles) !== cr) fail(`cell ${x}, ${y} sliced into chunk ${cc},${cr}`)
            if (!(x >= col && x < col + w && y >= row && y < row + h)) fail(`cell ${x}, ${y} outside the rect ${col}, ${row} of ${w} x ${h}`)
            seen[(y - row) * w + (x - col)]! += 1
          }
        }
      })
      let want = (floorDiv(col + w - 1, chunkTiles) - floorDiv(col, chunkTiles) + 1) * (floorDiv(row + h - 1, chunkTiles) - floorDiv(row, chunkTiles) + 1)
      if (slices !== want) fail(`rect ${col}, ${row} of ${w} x ${h} touches ${want} chunks, walked ${slices}`)
      for (let i = 0; i < seen.length; i++) if (seen[i] !== 1) fail(`cell ${i} of the rect visited ${seen[i]} times (rect ${col}, ${row} of ${w} x ${h}, chunkTiles ${chunkTiles})`)
    }
  }
})

test("checkCell and checkRect: any integer cell without bounds, the bounds clamp with them, fractions and empty rects throw", () => {
  checkCell("t", -5, 7, null)
  checkRect("t", -3, -3, 6, 6, null)
  expect(() => checkCell("t", 1.5, 0, null)).toThrow()
  expect(() => checkRect("t", 0, 0, 0, 1, null)).toThrow()
  expect(() => checkRect("t", 0, 0, 2.5, 2, null)).toThrow()
  let bounds = { col: -4, row: 2, cols: 8, rows: 4 }
  checkCell("t", -4, 2, bounds)
  checkCell("t", 3, 5, bounds)
  expect(() => checkCell("t", 4, 2, bounds)).toThrow()
  expect(() => checkCell("t", -5, 2, bounds)).toThrow()
  expect(() => checkCell("t", 0, 6, bounds)).toThrow()
  checkRect("t", -4, 2, 8, 4, bounds)
  expect(() => checkRect("t", -4, 2, 9, 4, bounds)).toThrow()
  expect(() => checkRect("t", -4, 1, 2, 2, bounds)).toThrow()
})

test("the index codec: the index in the low 29 bits, Tiled's three flags above, -1 clears in every array width", () => {
  expect(indexOf(5)).toBe(5)
  expect(indexOf(TILE_FLIP_X | 5)).toBe(5)
  expect(indexOf((TILE_FLIP_X | TILE_FLIP_Y | TILE_TRANSPOSE | 7) >>> 0)).toBe(7)
  expect(flagsOf(5)).toEqual({ flipX: false, flipY: false, transpose: false })
  expect(flagsOf(TILE_FLIP_X | 5)).toEqual({ flipX: true, flipY: false, transpose: false })
  expect(flagsOf(TILE_FLIP_Y | 5)).toEqual({ flipX: false, flipY: true, transpose: false })
  expect(flagsOf(TILE_TRANSPOSE | 5)).toEqual({ flipX: false, flipY: false, transpose: true })
  // The same bits through the typed arrays a world is held in.
  let u32 = new Uint32Array([TILE_FLIP_X | TILE_TRANSPOSE | 9, -1])
  expect(indexOf(u32[0]!)).toBe(9)
  expect(flagsOf(u32[0]!)).toEqual({ flipX: true, flipY: false, transpose: true })
  expect(isClearIndex(u32[1]!)).toBe(true)
  let u16 = new Uint16Array([-1, 3])
  expect(isClearIndex(u16[0]!)).toBe(true)
  expect(isClearIndex(u16[1]!)).toBe(false)
  expect(isClearIndex(-1)).toBe(true)
  expect(isClearIndex(0)).toBe(false)
})

// The shader as the symmetry test models it: a texel at frame fraction
// (s, t) is sampled by the quad corner whose uv interpolates to it, that
// corner sits at (aPos * frame size) and the quad turns by rot (the
// glsl.ts rotation, (x, y) -> (x c - y s, x s + y c)).
function shaderPlaces(fw: number, fh: number, flipU: boolean, flipV: boolean, rot: number, s: number, t: number): [number, number] {
  let ax = (flipU ? 1 - s : s) - 0.5
  let ay = (flipV ? 1 - t : t) - 0.5
  let x = ax * fw
  let y = ay * fh
  let c = Math.cos(rot)
  let sn = Math.sin(rot)
  return [x * c - y * sn, x * sn + y * c]
}

// Tiled's rule: the diagonal flip first, then the horizontal, then the
// vertical, over the texel's pixel offset from the frame's center.
function oracle(fw: number, fh: number, flipX: boolean, flipY: boolean, transpose: boolean, s: number, t: number): [number, number] {
  let x = (s - 0.5) * fw
  let y = (t - 0.5) * fh
  if (transpose) [x, y] = [y, x]
  if (flipX) x = -x
  if (flipY) y = -y
  return [x, y]
}

test("cellSymmetry: the UV swaps and the quarter turn reproduce all eight symmetries in Tiled's order, on a non-square frame", () => {
  const FW = 8
  const FH = 12
  for (let bits = 0; bits < 8; bits++) {
    let flipX = (bits & 4) !== 0
    let flipY = (bits & 2) !== 0
    let transpose = (bits & 1) !== 0
    let sym = cellSymmetry(flipX, flipY, transpose)
    let [footW, footH] = footprint(FW, FH, transpose)
    for (let [s, t] of [
      [0, 0],
      [1, 0],
      [0, 1],
      [1, 1],
      [0.25, 0.75],
    ]) {
      let got = shaderPlaces(FW, FH, sym.flipU, sym.flipV, sym.rot, s!, t!)
      let want = oracle(FW, FH, flipX, flipY, transpose, s!, t!)
      if (Math.abs(got[0] - want[0]) > 1e-9 || Math.abs(got[1] - want[1]) > 1e-9) {
        fail(`flags x${flipX} y${flipY} d${transpose}: texel (${s}, ${t}) lands at (${got}) not (${want})`)
      }
      if (Math.abs(got[0]) > footW / 2 + 1e-9 || Math.abs(got[1]) > footH / 2 + 1e-9) fail(`flags x${flipX} y${flipY} d${transpose}: texel outside the ${footW} x ${footH} footprint`)
    }
  }
  expect(footprint(8, 12, false)).toEqual([8, 12])
  expect(footprint(8, 12, true)).toEqual([12, 8])
})
