// A frame that draws text has every glyph it draws: a text in a style
// nothing warmed is complete in the first frame that paints it, so what a
// frame writes (a headless render, a snapshot) never depends on how fast
// the cells were made (okf/done/text-complete-frames.md).

import { test, expect } from "@solidrt/test"

// A size no test or component warms, with enough distinct glyphs to be
// well past any per-frame cell budget.
const SIZE = 23
const LINES = 12
const LOREM =
  "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore. Quis nostrud 0123456789!"

function inked(pixels: { width: number; height: number; data: Uint8Array }): number {
  let count = 0
  for (let i = 3; i < pixels.data.length; i += 4) if (pixels.data[i]! > 0) count++
  return count
}

test("a text in a cold style is complete in the first frame that paints it", async app => {
  let block = await app.mount(() => (
    <view>
      {Array.from({ length: LINES }, (_, i) => (
        <text fontSize={SIZE} color="#ffffff">{`${i} ${LOREM}`}</text>
      ))}
    </view>
  ))
  // What the first frame painted, before anything could land later.
  let first = inked(await app.painted(block))
  expect(first).toBeGreaterThan(0)
  // At rest, with every warm-up landed, the same picture.
  await app.settle()
  expect(inked(block.pixels())).toBe(first)
})
