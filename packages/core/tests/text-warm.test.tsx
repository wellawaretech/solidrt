// warmText: the type scale's styles warmed ahead of use, and a text drawn
// in a warmed style settles and has ink. Under either text engine (the
// warm-up is a no-op while Impeller draws text; on the glyph engine the
// cells are made on the worker and `settle` waits for them), so the test
// pins the contract that matters to an app: warming never throws, never
// blocks, and the text that follows draws.

import { test, expect } from "@solidrt/test"
import { warmText } from "@solidrt/core"
import type { MeasureTextOptions } from "@solidrt/core"

const SIZE = 16
const WEIGHT = 500 as const

function inked(pixels: { width: number; height: number; data: Uint8Array }): number {
  let count = 0
  for (let i = 3; i < pixels.data.length; i += 4) if (pixels.data[i]! > 0) count++
  return count
}

test("a text in a warmed style settles and draws", async app => {
  let styles: MeasureTextOptions[] = [
    { fontFamily: "sans", fontSize: SIZE, fontWeight: WEIGHT },
    { fontFamily: "sans", fontSize: SIZE * 2, fontWeight: 700 },
    { fontFamily: "mono", fontSize: SIZE },
  ]
  warmText(styles)
  let text = await app.mount(() => (
    <text fontSize={SIZE} fontWeight={WEIGHT} color="#ffffff">
      Warmed
    </text>
  ))
  await app.settle()
  let pixels = text.pixels()
  expect(pixels.width).toBeGreaterThan(0)
  expect(inked(pixels)).toBeGreaterThan(0)
})

test("warming the same styles again costs nothing and changes nothing", async app => {
  let styles: MeasureTextOptions[] = [{ fontFamily: "sans", fontSize: SIZE, fontWeight: WEIGHT }]
  warmText(styles)
  warmText(styles)
  let text = await app.mount(() => (
    <text fontSize={SIZE} fontWeight={WEIGHT} color="#ffffff">
      Twice
    </text>
  ))
  await app.settle()
  let before = inked(text.pixels())
  warmText(styles)
  await app.settle()
  expect(inked(text.pixels())).toBe(before)
})
