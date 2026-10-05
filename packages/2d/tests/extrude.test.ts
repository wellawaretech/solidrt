// Tests for atlas extrusion (extrude.ts): over random sheets, every pixel
// of the repacked sheet is the clamped source pixel of its cell (the edge
// replicated into the gutter), the returned GridOptions and rect table
// slice the new sheet onto exactly the cells, and the mip invariant holds
// on a box-filtered chain: at every level k <= log2(gutter), the texel on
// either side of a cell edge is the cell's own colour, which is what makes
// a bilinear tap at a clamped edge read one cell only. Plus the alignment
// throws. Pure-module input only, so it runs headless on flux: `sol test
// packages/2d`. The random inputs come from Math.random, which `sol test`
// seeds: the same on every run, and `--seed <n>` tries others.

import { test } from "flux:test"
import type { DecodedImage } from "@solidrt/core"
import type { TextureId } from "@solidrt/core/gpu"
import { extrudeGrid, extrudeRects } from "../src/extrude.ts"
import { grid, namedFrames } from "../src/frames.ts"

function int(lo: number, hi: number): number {
  return lo + Math.floor(Math.random() * (hi - lo + 1))
}

function fail(msg: string): void {
  throw new Error(msg)
}

function assertThrows(what: string, fn: () => void) {
  let threw = false
  try {
    fn()
  } catch {
    threw = true
    // expected
  }
  if (!threw) fail(`${what}: expected a throw`)
}

// Bytes per RGBA8 pixel.
const BPP = 4
// Sheets per randomized sweep.
const SWEEPS = 150
// The largest cell side in units of the gutter, and the largest grid.
const MAX_CELL = 6
const MAX_GRID = 5

/** A sheet of random pixels. */
function noise(width: number, height: number): DecodedImage {
  let data = new Uint8Array(width * height * BPP)
  for (let i = 0; i < data.length; i++) data[i] = int(0, 255)
  return { data, width, height }
}

/** The pixel at (x, y) as a number key. */
function px(image: DecodedImage, x: number, y: number): number {
  let i = (y * image.width + x) * BPP
  return (image.data[i]! << 24) | (image.data[i + 1]! << 16) | (image.data[i + 2]! << 8) | image.data[i + 3]!
}

/** Clamp `v` into [lo, hi]. */
function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), hi)
}

/** One box-filter level down: 2x2 blocks averaged (a pure colour stays
 * pure, a mixed block does not), odd edges dropped as GL does. */
function downsample(image: DecodedImage): DecodedImage {
  let width = Math.max(1, Math.floor(image.width / 2))
  let height = Math.max(1, Math.floor(image.height / 2))
  let data = new Uint8Array(width * height * BPP)
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      for (let c = 0; c < BPP; c++) {
        let sum = 0
        for (let dy = 0; dy < 2; dy++) for (let dx = 0; dx < 2; dx++) sum += image.data[((y * 2 + dy) * image.width + x * 2 + dx) * BPP + c]!
        data[(y * width + x) * BPP + c] = Math.round(sum / 4)
      }
    }
  }
  return { data, width, height }
}

/** A sheet of `cols` solid cells, each its own colour, `cell` px square. */
function solidCells(cols: number, cell: number): DecodedImage {
  let data = new Uint8Array(cols * cell * cell * BPP)
  for (let col = 0; col < cols; col++) {
    for (let y = 0; y < cell; y++) {
      for (let x = 0; x < cell; x++) {
        let i = (y * cols * cell + col * cell + x) * BPP
        data[i] = col === 0 ? 255 : 0
        data[i + 1] = col === 1 ? 255 : 0
        data[i + 2] = col === 2 ? 255 : 0
        data[i + 3] = 255
      }
    }
  }
  return { data, width: cols * cell, height: cell }
}

let texture = 1 as unknown as TextureId

test("extrudeGrid: every output pixel is the clamped source pixel of its cell, and the options slice the cells", () => {
  for (let s = 0; s < SWEEPS; s++) {
    let gutter = 1 << int(0, 3)
    let cols = int(1, MAX_GRID)
    let rows = int(1, MAX_GRID)
    let cellW = gutter * int(1, MAX_CELL)
    let cellH = gutter * int(1, MAX_CELL)
    let spacing = int(0, 3)
    let marginX = int(0, 3)
    let marginY = int(0, 3)
    let width = marginX * 2 + cols * cellW + (cols - 1) * spacing
    let height = marginY * 2 + rows * cellH + (rows - 1) * spacing
    let src = noise(width, height)
    let { image, options } = extrudeGrid(src, cols, rows, gutter, { cellW, cellH, spacing, marginX, marginY })
    let pitchX = cellW + 2 * gutter
    let pitchY = cellH + 2 * gutter
    if (image.width !== cols * pitchX || image.height !== rows * pitchY) fail(`extruded sheet is ${image.width} x ${image.height}, expected ${cols * pitchX} x ${rows * pitchY}`)
    for (let y = 0; y < image.height; y++) {
      for (let x = 0; x < image.width; x++) {
        let col = Math.floor(x / pitchX)
        let row = Math.floor(y / pitchY)
        let lx = clamp(x - col * pitchX - gutter, 0, cellW - 1)
        let ly = clamp(y - row * pitchY - gutter, 0, cellH - 1)
        let want = px(src, marginX + col * (cellW + spacing) + lx, marginY + row * (cellH + spacing) + ly)
        if (px(image, x, y) !== want) fail(`extruded pixel (${x}, ${y}) is not source cell (${col}, ${row}) pixel (${lx}, ${ly}) (gutter ${gutter}, cell ${cellW} x ${cellH})`)
      }
    }
    // The returned options slice the new sheet onto exactly the cells.
    let frames = grid({ texture, width: image.width, height: image.height }, cols, rows, options)
    let col = int(0, cols - 1)
    let row = int(0, rows - 1)
    let f = frames[row * cols + col]!
    let x0 = Math.round(f.u0 * image.width)
    let y0 = Math.round(f.v0 * image.height)
    let x1 = Math.round(f.u1 * image.width)
    let y1 = Math.round(f.v1 * image.height)
    if (!(x0 === col * pitchX + gutter && y0 === row * pitchY + gutter && x1 === x0 + cellW && y1 === y0 + cellH)) {
      fail(`options slice cell (${col}, ${row}) to [${x0}, ${y0}, ${x1 - x0}, ${y1 - y0}], expected [${col * pitchX + gutter}, ${row * pitchY + gutter}, ${cellW}, ${cellH}]`)
    }
  }
})

test("extrudeGrid: at every mip level up to log2(gutter), the texels on both sides of a cell edge are the cell's own colour", () => {
  for (let gutter of [1, 2, 4, 8]) {
    for (let cell of [gutter, gutter * 2, gutter * 3]) {
      let { image } = extrudeGrid(solidCells(3, cell), 3, 1, gutter)
      let pitch = cell + 2 * gutter
      let level = image
      for (let k = 0; (1 << k) <= gutter; k++) {
        let texel = 1 << k
        for (let col = 0; col < 3; col++) {
          // The cell's own colour at this level: its center texel.
          let own = px(level, Math.floor((col * pitch + gutter + cell / 2) / texel), 0)
          // The texel just inside and just outside each edge.
          let left = col * pitch + gutter
          let right = left + cell
          for (let x of [left - 1, left, right - 1, right]) {
            let got = px(level, Math.floor(x / texel), 0)
            if (got !== own) fail(`gutter ${gutter}, cell ${cell}, level ${k}: texel at x ${x} is not cell ${col}'s colour`)
          }
        }
        level = downsample(level)
      }
    }
  }
})

test("extrudeRects: every rect's pixels are the clamped source pixels, placements are aligned and apart, and the table names them", () => {
  for (let s = 0; s < SWEEPS; s++) {
    let gutter = 1 << int(0, 3)
    let src = noise(gutter * int(4, 12), gutter * int(4, 12))
    let n = int(1, 6)
    let rects: Record<string, [number, number, number, number]> = {}
    for (let i = 0; i < n; i++) {
      let w = gutter * int(1, Math.floor(src.width / gutter))
      let h = gutter * int(1, Math.floor(src.height / gutter))
      rects[`r${i}`] = [int(0, src.width - w), int(0, src.height - h), w, h]
    }
    let { image, rects: placed } = extrudeRects(src, rects, gutter)
    let names = Object.keys(rects)
    for (let name of names) {
      let [sx, sy, w, h] = rects[name]!
      let [dx, dy, pw, ph] = placed[name]!
      if (pw !== w || ph !== h) fail(`rect ${name} resized from ${w} x ${h} to ${pw} x ${ph}`)
      if (dx % gutter !== 0 || dy % gutter !== 0) fail(`rect ${name} placed at (${dx}, ${dy}), not on the ${gutter} alignment`)
      if (!(dx >= gutter && dy >= gutter && dx + w + gutter <= image.width && dy + h + gutter <= image.height)) fail(`rect ${name} and its gutter leave the sheet`)
      for (let y = -gutter; y < h + gutter; y++) {
        for (let x = -gutter; x < w + gutter; x++) {
          let want = px(src, sx + clamp(x, 0, w - 1), sy + clamp(y, 0, h - 1))
          if (px(image, dx + x, dy + y) !== want) fail(`rect ${name} pixel (${x}, ${y}) is not its clamped source pixel`)
        }
      }
    }
    // Extruded regions (rect plus gutter) never overlap.
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < i; j++) {
        let [ax, ay, aw, ah] = placed[names[i]!]!
        let [bx, by, bw, bh] = placed[names[j]!]!
        let apart = ax - gutter >= bx + bw + gutter || bx - gutter >= ax + aw + gutter || ay - gutter >= by + bh + gutter || by - gutter >= ay + ah + gutter
        if (!apart) fail(`extruded rects ${names[i]} and ${names[j]} overlap`)
      }
    }
    // namedFrames over the table addresses exactly the placed rects.
    let frames = namedFrames({ texture, width: image.width, height: image.height }, placed)
    let name = names[int(0, n - 1)]!
    let [dx, dy, w, h] = placed[name]!
    let f = frames[name]!
    if (!(Math.round(f.u0 * image.width) === dx && Math.round(f.v0 * image.height) === dy && Math.round(f.u1 * image.width) === dx + w && Math.round(f.v1 * image.height) === dy + h)) {
      fail(`namedFrames over the extruded table misses rect ${name}`)
    }
  }
})

test("extrudeGrid and extrudeRects: alignment and shape throws", () => {
  let sheet = noise(32, 16)
  assertThrows("gutter of 3", () => extrudeGrid(sheet, 2, 1, 3))
  assertThrows("gutter of 0", () => extrudeGrid(sheet, 2, 1, 0))
  assertThrows("cell not a multiple of the gutter", () => extrudeGrid(noise(24, 8), 2, 1, 8))
  assertThrows("grid past the sheet", () => extrudeGrid(sheet, 2, 1, 2, { cellW: 24, cellH: 16 }))
  assertThrows("fractional cols", () => extrudeGrid(sheet, 1.5, 1, 2))
  assertThrows("short data", () => extrudeGrid({ data: new Uint8Array(8), width: 4, height: 4 }, 1, 1, 1))
  assertThrows("rect not a multiple of the gutter", () => extrudeRects(sheet, { a: [0, 0, 6, 8] }, 4))
  assertThrows("rect outside the sheet", () => extrudeRects(sheet, { a: [28, 0, 8, 8] }, 4))
  assertThrows("no rects", () => extrudeRects(sheet, {}, 4))
  // The valid shapes pass: a gutter of 1 accepts any integer cell.
  extrudeGrid(noise(24, 8), 3, 1, 1)
  extrudeRects(sheet, { a: [0, 0, 7, 5] }, 1)
})
