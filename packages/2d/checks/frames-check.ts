// Check rig for the atlas frame math (frames.ts): grid slicing against a
// directly-computed oracle across random sheet shapes, spacing, and margins,
// plus namedFrames and the validation throws. Pure-module input only, so it
// runs headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/frames-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end, and the flux binary
// exits 1 on the uncaught throw, so a CI step can gate on the exit code.

import { argv } from "flux:process"
import { grid, namedFrames, writeFrame, FULL_FRAME } from "../src/frames.ts"

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

function close(a: number, b: number): boolean {
  return Math.abs(a - b) < 1e-9
}

function assertThrows(what: string, fn: () => void) {
  try {
    fn()
    fail(`${what}: expected a throw`)
  } catch {
    // expected
  }
}

// Hand-written: a 2x2 grid over a 32x32 sheet is quarters.
{
  let frames = grid(2, 2, { width: 32, height: 32 })
  if (frames.length !== 4) fail(`2x2 grid has ${frames.length} frames`)
  let f = frames[3]!
  if (!(close(f.u0, 0.5) && close(f.v0, 0.5) && close(f.u1, 1) && close(f.v1, 1))) {
    fail(`2x2 grid frame 3 is (${f.u0}, ${f.v0})-(${f.u1}, ${f.v1}), expected the bottom-right quarter`)
  }
}
// Row-major order: frame[cols] starts the second row.
{
  let frames = grid(3, 2, { width: 48, height: 32 })
  let second = frames[3]!
  if (!(close(second.u0, 0) && close(second.v0, 0.5))) fail("grid is not row-major")
}
// FULL_FRAME is the unit rect.
if (!(FULL_FRAME.u0 === 0 && FULL_FRAME.v0 === 0 && FULL_FRAME.u1 === 1 && FULL_FRAME.v1 === 1)) {
  fail("FULL_FRAME is not the unit rect")
}

// Validation throws.
assertThrows("zero cols", () => grid(0, 2, { width: 32, height: 32 }))
assertThrows("fractional rows", () => grid(2, 1.5, { width: 32, height: 32 }))
assertThrows("non-positive sheet", () => grid(2, 2, { width: 0, height: 32 }))
assertThrows("cells eaten by spacing", () => grid(8, 1, { width: 8, height: 8, spacing: 4 }))
assertThrows("named non-positive frame", () => namedFrames(32, 32, { bad: [0, 0, 0, 4] }))
assertThrows("named non-positive atlas", () => namedFrames(0, 32, { a: [0, 0, 4, 4] }))

// namedFrames maps pixel rects to UVs.
{
  let frames = namedFrames(64, 32, { hero: [16, 8, 32, 16] })
  let f = frames.hero
  if (!(close(f.u0, 0.25) && close(f.v0, 0.25) && close(f.u1, 0.75) && close(f.v1, 0.75))) {
    fail(`namedFrames hero is (${f.u0}, ${f.v0})-(${f.u1}, ${f.v1})`)
  }
}

// Randomized sweep: every frame's pixel rect, reconstructed from its UVs,
// must land exactly where the oracle places the cell.
const SWEEPS = 2000
let checked = 0
for (let i = 0; i < SWEEPS; i++) {
  let cols = int(1, 12)
  let rows = int(1, 12)
  let cellW = int(1, 32)
  let cellH = int(1, 32)
  let spacing = int(0, 4)
  let marginX = int(0, 6)
  let marginY = int(0, 6)
  let width = marginX * 2 + cols * cellW + (cols - 1) * spacing
  let height = marginY * 2 + rows * cellH + (rows - 1) * spacing
  let frames = grid(cols, rows, { width, height, cellW, cellH, spacing, marginX, marginY })
  if (frames.length !== cols * rows) {
    fail(`grid(${cols}, ${rows}) returned ${frames.length} frames`)
    continue
  }
  let col = int(0, cols - 1)
  let row = int(0, rows - 1)
  let f = frames[row * cols + col]!
  let x = marginX + col * (cellW + spacing)
  let y = marginY + row * (cellH + spacing)
  if (
    !(
      close(f.u0 * width, x) &&
      close(f.v0 * height, y) &&
      close(f.u1 * width, x + cellW) &&
      close(f.v1 * height, y + cellH)
    )
  ) {
    fail(
      `grid(${cols}x${rows}, cell ${cellW}x${cellH}, spacing ${spacing}, margin ${marginX}/${marginY}) ` +
        `cell (${col}, ${row}): UV rect maps to (${f.u0 * width}, ${f.v0 * height}), expected (${x}, ${y})`,
    )
  }
  checked++
}

// writeFrame: the UV-side mirror. flipX swaps u0/u1, flipY v0/v1, both is
// both, and toggling the changed axes on the stored floats undoes it (an
// involution), which is what a flip write without a new frame relies on.
{
  let data = new Float32Array(8)
  let f = { u0: 0.1, v0: 0.2, u1: 0.3, v1: 0.4 }
  let expect = (what: string, u0: number, v0: number, u1: number, v1: number) => {
    let same = (i: number, v: number) => data[i] === Math.fround(v)
    if (!(same(4, u0) && same(5, v0) && same(6, u1) && same(7, v1))) {
      fail(`writeFrame ${what}: got [${data[4]}, ${data[5]}, ${data[6]}, ${data[7]}], expected [${u0}, ${v0}, ${u1}, ${v1}]`)
    }
  }
  writeFrame(data, 4, f.u0, f.v0, f.u1, f.v1, false, false)
  expect("plain", 0.1, 0.2, 0.3, 0.4)
  writeFrame(data, 4, f.u0, f.v0, f.u1, f.v1, true, false)
  expect("flipX", 0.3, 0.2, 0.1, 0.4)
  writeFrame(data, 4, f.u0, f.v0, f.u1, f.v1, false, true)
  expect("flipY", 0.1, 0.4, 0.3, 0.2)
  writeFrame(data, 4, f.u0, f.v0, f.u1, f.v1, true, true)
  expect("flipXY", 0.3, 0.4, 0.1, 0.2)
  // Toggle in place: X off again, then Y off again -> back to plain.
  writeFrame(data, 4, data[4]!, data[5]!, data[6]!, data[7]!, true, false)
  expect("toggle X back", 0.1, 0.4, 0.3, 0.2)
  writeFrame(data, 4, data[4]!, data[5]!, data[6]!, data[7]!, false, true)
  expect("toggle Y back", 0.1, 0.2, 0.3, 0.4)
  if (data[0] !== 0 || data[3] !== 0) fail("writeFrame wrote outside its four floats")
}

if (failures === 0) console.log(`PASS: ${checked} random grids + edge cases`)
else throw new Error(`${failures} FAILURES (seed ${seed})`)
