// Scene fog through the picture, the tier the pure tests cannot reach
// (GLSL plus a scene write): the fog example loaded whole, its camera
// parked and its mode set through the debug commands it registers, then
// one pixel on each of its two suns. They sit side by side on the far
// ridge: the left one is `unlit({ fog: false })` and keeps the sun's
// color in every mode, the right one fogs like everything else and reads
// as sky. The example's frame callback keeps the frames coming, so the
// test steps frames rather than settling.

import { test, expect } from "@solidrt/core/test"
import type { Locator, TestApp } from "@solidrt/core/test"

type Suns = { lit: { x: number; y: number } | null; fogged: { x: number; y: number } | null }

// The sun's color is [1, 0.92, 0.6]: red above blue. The sky, and a sun
// fogged into it, is [0.72, 0.78, 0.86]: blue above red. The channel
// order tells the two apart whatever the rendering's color space.
function sunlit(pixel: [number, number, number, number]): boolean {
  return pixel[0] > pixel[2]
}

async function loaded(app: TestApp, mode: string): Promise<{ scene: Locator; lit: [number, number, number, number]; fogged: [number, number, number, number] }> {
  let root = await app.load(() => import("../examples/fog.tsx"))
  await app.debug("pan", { t: 0 })
  await app.debug("fog", { mode })
  await app.frame()
  let suns = (await app.debug("suns")) as Suns
  expect(suns.lit).not.toBeNull()
  expect(suns.fogged).not.toBeNull()
  let scene = root.find({ kind: "texture" })
  let at = (p: { x: number; y: number }) => scene.pixel(Math.round(p.x), Math.round(p.y))
  return { scene, lit: at(suns.lit!), fogged: at(suns.fogged!) }
}

test("linear fog: the sun that opts out keeps its color, the other reads as sky", async app => {
  let { lit, fogged } = await loaded(app, "linear")
  expect(sunlit(lit)).toBe(true)
  expect(sunlit(fogged)).toBe(false)
})

test("exp2 fog: the same two suns", async app => {
  let { lit, fogged } = await loaded(app, "exp2")
  expect(sunlit(lit)).toBe(true)
  expect(sunlit(fogged)).toBe(false)
})

test("fog off: both suns read as the sun", async app => {
  let { lit, fogged } = await loaded(app, "off")
  expect(sunlit(lit)).toBe(true)
  expect(sunlit(fogged)).toBe(true)
  expect(fogged).toEqual(lit)
})
