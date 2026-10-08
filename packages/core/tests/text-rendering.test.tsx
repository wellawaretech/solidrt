// textRendering: the policy read back as the runtime resolved it for this
// display. A setting reads back as set, a default reads back as the
// value the display scale gives it (so a settings screen shows what is in
// effect, never null), setting the read-back changes nothing, and text
// draws under every mode the policy can take.

import { test, expect } from "@solidrt/test"
import { env, setTextRendering, textRendering } from "@solidrt/core"
import type { TextRenderingPolicy } from "@solidrt/core"

const SIZE = 16
// Below this display scale the defaults hint light; at and above it they
// leave the outline alone (the runtime's low-DPI policy).
const LOW_DPI_SCALE = 2
// Values a single-precision float holds exactly (the runtime stores the
// policy in f32), so a read-back compares equal.
const GAMMA = 2.0
const DARKEN = 0.03125

function inked(pixels: { width: number; height: number; data: Uint8Array }): number {
  let count = 0
  for (let i = 3; i < pixels.data.length; i += 4) if (pixels.data[i]! > 0) count++
  return count
}

test("a default policy reads back resolved for the display", async () => {
  setTextRendering({ darken: null, hint: null })
  let policy = textRendering()
  expect(policy.coverage).toBe("directWrite")
  expect(policy.darken).toBe(0)
  expect(policy.hint).toBe(env.displayScale < LOW_DPI_SCALE ? "light" : false)
})

test("a setting reads back as set and a round trip changes nothing", async () => {
  setTextRendering({ coverage: "linearLight", gamma: GAMMA, darken: DARKEN, hint: "full" })
  let set = textRendering()
  expect(set).toEqual({ coverage: "linearLight", gamma: GAMMA, contrast: set.contrast, darken: DARKEN, hint: "full" })
  setTextRendering(set)
  expect(textRendering()).toEqual(set)
  setTextRendering({ hint: false })
  expect(textRendering().hint).toBe(false)
  setTextRendering({ coverage: "directWrite", gamma: set.gamma, darken: null, hint: null })
})

test("text draws under every hinting mode", async app => {
  let text = await app.mount(() => (
    <text fontSize={SIZE} color="#ffffff">
      Hinted
    </text>
  ))
  let modes: TextRenderingPolicy["hint"][] = [false, "light", "full"]
  for (let hint of modes) {
    setTextRendering({ hint })
    await app.settle()
    expect(textRendering().hint).toBe(hint)
    expect(inked(text.pixels())).toBeGreaterThan(0)
  }
  setTextRendering({ hint: null })
})
