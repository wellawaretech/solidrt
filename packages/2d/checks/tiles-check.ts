// Check for the tile-grid math (tiles-math.ts): a cell's chunk and record
// slot against a brute-force oracle, the chunk walk of a rect covering
// every cell of the rect exactly once and nothing outside it, each slice
// inside its own chunk, and the cell/rect validation throws. Pure-module
// input only (tiles-math.ts imports nothing), so it runs headless on flux,
// bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/tiles-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end, and the flux binary
// exits 1 on the uncaught throw, so a CI step can gate on the exit code.

import { argv } from "flux:process"
import { checkCell, checkRect, chunkOf, eachChunkSlice, slotOf } from "../src/tiles-math.ts"

let seed = Number(argv[0] ?? Math.floor(Math.random() * 0xffffffff))
console.log("seed", seed)

let s = seed >>> 0
function rand(): number {
  s = (Math.imul(s, 1664525) + 1013904223) >>> 0
  return s / 4294967296
}
function int(lo: number, hi: number): number {
  return lo + Math.floor(rand() * (hi - lo + 1))
}

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}
let throws = (what: string, f: () => void) => {
  try {
    f()
    fail(`${what} must throw`)
  } catch (err) {
    if (!(err instanceof Error)) fail(`${what}: unexpected ${err}`)
  }
}

// Random grids per sweep, and rects walked per grid.
const GRIDS = 200
const RECTS = 20
// The record width the slot oracle assumes (any positive integer works;
// the layer's own is FLOATS_PER_SPRITE).
const FLOATS = 13

// --- chunkOf and slotOf against the oracle: the chunk holding a cell is
// the one whose chunkTiles-square contains it, and inside the chunk the
// cells are row-major ---
for (let g = 0; g < GRIDS; g++) {
  let chunkTiles = int(1, 16)
  let cols = int(1, 100)
  let rows = int(1, 100)
  let chunkCols = Math.ceil(cols / chunkTiles)
  for (let k = 0; k < RECTS; k++) {
    let col = int(0, cols - 1)
    let row = int(0, rows - 1)
    let index = chunkOf(col, row, chunkTiles, chunkCols)
    let cc = index % chunkCols
    let cr = Math.floor(index / chunkCols)
    if (!(cc * chunkTiles <= col && col < (cc + 1) * chunkTiles && cr * chunkTiles <= row && row < (cr + 1) * chunkTiles)) {
      fail(`chunkOf(${col}, ${row}) = ${index} does not contain the cell (chunkTiles ${chunkTiles}, chunkCols ${chunkCols})`)
    }
    let slot = slotOf(col, row, chunkTiles, FLOATS)
    let want = ((row - cr * chunkTiles) * chunkTiles + (col - cc * chunkTiles)) * FLOATS
    if (slot !== want) fail(`slotOf(${col}, ${row}) = ${slot}, want ${want} (chunkTiles ${chunkTiles})`)
    if (slot < 0 || slot >= chunkTiles * chunkTiles * FLOATS) fail(`slotOf(${col}, ${row}) = ${slot} outside the chunk's records`)
  }
}

// --- eachChunkSlice: the slices cover the rect exactly once, stay inside
// the grid and their own chunk, and arrive chunk rows outer ---
for (let g = 0; g < GRIDS; g++) {
  let chunkTiles = int(1, 16)
  let cols = int(1, 100)
  let rows = int(1, 100)
  let chunkCols = Math.ceil(cols / chunkTiles)
  for (let k = 0; k < RECTS; k++) {
    let w = int(1, cols)
    let h = int(1, rows)
    let col = int(0, cols - w)
    let row = int(0, rows - h)
    let seen = new Uint8Array(cols * rows)
    let lastIndex = -1
    let slices = 0
    eachChunkSlice(col, row, w, h, chunkTiles, chunkCols, (index, colA, colB, rowA, rowB) => {
      slices++
      if (index <= lastIndex) fail(`slices arrive in chunk order: ${index} after ${lastIndex}`)
      lastIndex = index
      if (colA >= colB || rowA >= rowB) fail(`empty slice ${colA}..${colB} x ${rowA}..${rowB} for chunk ${index}`)
      for (let y = rowA; y < rowB; y++) {
        for (let x = colA; x < colB; x++) {
          if (chunkOf(x, y, chunkTiles, chunkCols) !== index) fail(`cell ${x}, ${y} sliced into chunk ${index}, lies in ${chunkOf(x, y, chunkTiles, chunkCols)}`)
          if (!(x >= col && x < col + w && y >= row && y < row + h)) fail(`cell ${x}, ${y} outside the rect ${col}, ${row} of ${w} x ${h}`)
          seen[y * cols + x]! += 1
        }
      }
    })
    let want = (Math.floor((col + w - 1) / chunkTiles) - Math.floor(col / chunkTiles) + 1) * (Math.floor((row + h - 1) / chunkTiles) - Math.floor(row / chunkTiles) + 1)
    if (slices !== want) fail(`rect ${col}, ${row} of ${w} x ${h} touches ${want} chunks, walked ${slices}`)
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        let inside = x >= col && x < col + w && y >= row && y < row + h
        let n = seen[y * cols + x]!
        if (n !== (inside ? 1 : 0)) fail(`cell ${x}, ${y} visited ${n} times (rect ${col}, ${row} of ${w} x ${h}, chunkTiles ${chunkTiles})`)
      }
    }
  }
}

// --- Validation: cells and rects outside the grid, fractions, empty rects ---
checkCell("t", 0, 0, 8, 4)
checkCell("t", 7, 3, 8, 4)
throws("checkCell col past the edge", () => checkCell("t", 8, 0, 8, 4))
throws("checkCell row past the edge", () => checkCell("t", 0, 4, 8, 4))
throws("checkCell negative", () => checkCell("t", -1, 0, 8, 4))
throws("checkCell fraction", () => checkCell("t", 1.5, 0, 8, 4))
checkRect("t", 0, 0, 8, 4, 8, 4)
checkRect("t", 7, 3, 1, 1, 8, 4)
throws("checkRect past the right edge", () => checkRect("t", 1, 0, 8, 4, 8, 4))
throws("checkRect past the bottom edge", () => checkRect("t", 0, 1, 8, 4, 8, 4))
throws("checkRect empty", () => checkRect("t", 0, 0, 0, 1, 8, 4))
throws("checkRect negative origin", () => checkRect("t", -1, 0, 2, 2, 8, 4))
throws("checkRect fraction", () => checkRect("t", 0, 0, 2.5, 2, 8, 4))

if (failures === 0) console.log(`PASS: chunk/slot oracle and rect walk over ${GRIDS} grids x ${RECTS} rects hold`)
else throw new Error(`${failures} FAILURES (seed ${seed})`)
