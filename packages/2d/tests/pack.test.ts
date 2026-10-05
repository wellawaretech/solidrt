// Tests for the shelf packer (pack.ts): over random size sets, every size
// is placed inside the sheet, no two placements overlap, placements come
// back in input order, the height is the shelves' reach, sizes that are
// multiples of g land on multiples of g (what extrusion's mip alignment
// relies on), the packing is deterministic, and the validation throws.
// Pure-module input only, so it runs headless on flux: `sol test
// packages/2d`. The random inputs come from Math.random, which `sol test`
// seeds: the same on every run, and `--seed <n>` tries others.

import { test } from "flux:test"
import { packRects, packWidth } from "../src/pack.ts"
import type { Size } from "../src/pack.ts"

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

// Size sets per sweep, and the largest set.
const SWEEPS = 300
const MAX_RECTS = 40
// The largest side a random size takes, in units of the alignment g.
const MAX_SIDE = 16

test("packRects: every size inside the sheet, none overlapping, input order kept, height the shelves' reach", () => {
  for (let s = 0; s < SWEEPS; s++) {
    // Sizes as multiples of g, so alignment can be checked too.
    let g = 1 << int(0, 3)
    let sizes: Size[] = []
    let n = int(1, MAX_RECTS)
    for (let i = 0; i < n; i++) sizes.push({ width: g * int(1, MAX_SIDE), height: g * int(1, MAX_SIDE) })
    let width = packWidth(sizes)
    let p = packRects(sizes, width)
    if (p.width !== width) fail(`packing width ${p.width} is not the asked ${width}`)
    if (p.rects.length !== n) fail(`${p.rects.length} placements for ${n} sizes`)
    let reach = 0
    for (let i = 0; i < n; i++) {
      let r = p.rects[i]!
      let z = sizes[i]!
      if (!(r.x >= 0 && r.y >= 0 && r.x + z.width <= width)) fail(`size ${i} (${z.width} x ${z.height}) placed at (${r.x}, ${r.y}) outside the ${width} sheet`)
      if (r.x % g !== 0 || r.y % g !== 0) fail(`size ${i} placed at (${r.x}, ${r.y}), not on the ${g} alignment`)
      reach = Math.max(reach, r.y + z.height)
      for (let j = 0; j < i; j++) {
        let q = p.rects[j]!
        let y = sizes[j]!
        let apart = r.x >= q.x + y.width || q.x >= r.x + z.width || r.y >= q.y + y.height || q.y >= r.y + z.height
        if (!apart) fail(`sizes ${i} and ${j} overlap: (${r.x}, ${r.y}, ${z.width}, ${z.height}) and (${q.x}, ${q.y}, ${y.width}, ${y.height})`)
      }
    }
    if (p.height !== reach) fail(`packing height ${p.height} is not the shelves' reach ${reach}`)
    // Deterministic: the same sizes pack the same way.
    let again = packRects(sizes, width)
    for (let i = 0; i < n; i++) {
      if (again.rects[i]!.x !== p.rects[i]!.x || again.rects[i]!.y !== p.rects[i]!.y) fail("packing is not deterministic")
    }
  }
})

test("packWidth: a power of two, at least the widest size and the square root of the area", () => {
  for (let s = 0; s < SWEEPS; s++) {
    let sizes: Size[] = []
    let n = int(1, MAX_RECTS)
    let widest = 0
    let area = 0
    for (let i = 0; i < n; i++) {
      let z = { width: int(1, 200), height: int(1, 200) }
      sizes.push(z)
      widest = Math.max(widest, z.width)
      area += z.width * z.height
    }
    let w = packWidth(sizes)
    if ((w & (w - 1)) !== 0) fail(`packWidth ${w} is not a power of two`)
    if (w < widest) fail(`packWidth ${w} is narrower than the widest size ${widest}`)
    if (w < Math.sqrt(area)) fail(`packWidth ${w} is below the square root of the area ${area}`)
    if (w / 2 >= widest && w / 2 >= Math.sqrt(area)) fail(`packWidth ${w} is not the smallest that fits`)
  }
})

test("packRects: validation throws", () => {
  assertThrows("zero width", () => packRects([{ width: 4, height: 4 }], 0))
  assertThrows("fractional width", () => packRects([{ width: 4, height: 4 }], 7.5))
  assertThrows("non-positive size", () => packRects([{ width: 0, height: 4 }], 16))
  assertThrows("fractional size", () => packRects([{ width: 2.5, height: 4 }], 16))
  assertThrows("wider than the sheet", () => packRects([{ width: 32, height: 4 }], 16))
  // The empty set packs to an empty sheet.
  let p = packRects([], 16)
  if (p.height !== 0 || p.rects.length !== 0) fail("the empty packing is not empty")
})
