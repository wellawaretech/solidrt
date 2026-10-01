// Scene fog through the picture, the tier the pure tests cannot reach
// (GLSL plus a scene write): the fog example loaded whole, its camera
// parked and its mode set through the debug commands it registers, then
// one pixel on each of its two suns, found through its `suns` and
// `project` commands. They sit side by side on the far ridge: the left
// one is `unlit({ fog: false })` and keeps the sun's color in every mode,
// the right one fogs like everything else and reads as sky, except under
// height fog, which the suns stand above. The example's frame callback
// keeps the frames coming, so the test steps frames rather than settling.

import { test, expect } from "@solidrt/core/test"
import type { Locator, TestApp } from "@solidrt/core/test"

type Point = [number, number, number]
type Screen = { x: number; y: number } | null

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
  let suns = (await app.debug("suns")) as { lit: Point; fogged: Point }
  let scene = root.find({ kind: "texture" })
  let at = async (point: Point) => {
    let screen = (await app.debug("project", { point })) as Screen
    expect(screen).not.toBeNull()
    return scene.pixel(Math.round(screen!.x), Math.round(screen!.y))
  }
  return { scene, lit: await at(suns.lit), fogged: await at(suns.fogged) }
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

test("height fog fills the valley and thins on the way up: both suns, high above it, read as the sun", async app => {
  let { lit, fogged } = await loaded(app, "height")
  expect(sunlit(lit)).toBe(true)
  expect(sunlit(fogged)).toBe(true)
})

test("fog off: both suns read as the sun", async app => {
  let { lit, fogged } = await loaded(app, "off")
  expect(sunlit(lit)).toBe(true)
  expect(sunlit(fogged)).toBe(true)
  expect(fogged).toEqual(lit)
})
