// Contact-hardening soft shadows through the picture: the soft-shadows
// example loaded whole, its lights and quality set through its debug
// commands, then a row of ground pixels read across each shadow's +z edge
// at two places - beside the caster's foot and under its top edge - and
// the width of the lit-to-shadowed transition measured on each. With a
// sized light the foot stays as sharp as the one hardware tap while the
// tip widens with the caster-to-receiver distance; with size 0, or the
// hard tier, both are sharp. The sun (an orthographic map, as a box and
// as cascades) and the spot (a perspective map) each get the pair.

import { test, expect } from "@solidrt/test"
import type { Locator, Pixels, TestApp } from "@solidrt/test"

type Edge = { x: number; z: number }
type Layout = { sun: { foot: Edge; tip: Edge }; spot: { foot: Edge; tip: Edge } }
type Screen = { x: number; y: number } | null

// The probe row: this far either side of the edge along z, in this many
// samples.
const HALF = 0.6
const SAMPLES = 61
const STEP = (2 * HALF) / (SAMPLES - 1)
// A sample is in transition when its brightness sits this far inside the
// row's lit-to-shadowed range from both ends (the disc's ramp eases in
// and out, so the width read is about three quarters of the penumbra).
const BAND = 0.1
// The row must span at least this much brightness (r + g + b, 0..765) or
// it did not cross a shadow edge at all.
const CONTRAST = 60
// A hard edge spans at most this many world units (the hardware tap's
// bilinear texel plus the pixel grid). A sized light's foot may be a
// little wider: the blocker search averages the caster's side up to the
// search radius, so a contact under a tall caster is near-hard, not
// hard (PCSS in every engine). A soft tip spans at least SOFT_MIN, and
// at least SOFT_RATIO times its foot.
const HARD_MAX = 0.15
const FOOT_MAX = 0.25
const SOFT_MIN = 0.2
const SOFT_RATIO = 1.5

async function loaded(app: TestApp): Promise<{ scene: Locator; layout: Layout }> {
  let root = await app.load(() => import("../examples/soft-shadows.tsx"))
  await app.settle()
  let scene = root.find({ kind: "texture" })
  let layout = (await app.debug("layout")) as Layout
  return { scene, layout }
}

function brightness(px: Pixels, scale: number, p: { x: number; y: number }): number {
  let x = Math.round(p.x * scale)
  let y = Math.round(p.y * scale)
  let i = (y * px.width + x) * 4
  return px.data[i]! + px.data[i + 1]! + px.data[i + 2]!
}

/** The transition's width across `edge` in world units, from one read of
 * the scene's pixels. */
async function width(app: TestApp, scene: Locator, px: Pixels, edge: Edge): Promise<number> {
  let points = (await app.debug("samples", { x: edge.x, z0: edge.z - HALF, z1: edge.z + HALF, n: SAMPLES })) as Screen[]
  let scale = px.width / scene.box.width
  let values = points.map(p => {
    expect(p).not.toBeNull()
    return brightness(px, scale, p!)
  })
  let lo = Math.min(...values)
  let hi = Math.max(...values)
  expect(hi - lo).toBeGreaterThan(CONTRAST)
  let inner = values.filter(v => v > lo + (hi - lo) * BAND && v < hi - (hi - lo) * BAND).length
  return inner * STEP
}

async function pair(app: TestApp, scene: Locator, edges: { foot: Edge; tip: Edge }): Promise<{ foot: number; tip: number }> {
  await app.settle()
  let px = scene.pixels()
  let w = { foot: await width(app, scene, px, edges.foot), tip: await width(app, scene, px, edges.tip) }
  console.log("Transition widths: foot " + w.foot.toFixed(3) + ", tip " + w.tip.toFixed(3))
  return w
}

function expectSoft(w: { foot: number; tip: number }): void {
  expect(w.foot).toBeLessThanOrEqual(FOOT_MAX)
  expect(w.tip).toBeGreaterThanOrEqual(SOFT_MIN)
  expect(w.tip).toBeGreaterThanOrEqual(w.foot * SOFT_RATIO)
}

function expectHard(w: { foot: number; tip: number }): void {
  expect(w.foot).toBeLessThanOrEqual(HARD_MAX)
  expect(w.tip).toBeLessThanOrEqual(HARD_MAX)
}

test("a sized sun: the wall's shadow is sharp at its foot and wide under its top edge", async app => {
  let { scene, layout } = await loaded(app)
  expectSoft(await pair(app, scene, layout.sun))
})

test("the same sun as two cascades: the penumbra comes out of the cascade fit's metric too", async app => {
  let { scene, layout } = await loaded(app)
  await app.debug("sun", { cascades: 2 })
  expectSoft(await pair(app, scene, layout.sun))
})

test("a spot with a source radius: the post's shadow hardens at its foot and softens toward its tip", async app => {
  let { scene, layout } = await loaded(app)
  expectSoft(await pair(app, scene, layout.spot))
})

test("size 0 is a point source: both edges are the one hardware tap", async app => {
  let { scene, layout } = await loaded(app)
  await app.debug("sun", { angularSize: 0 })
  await app.debug("spot", { sourceRadius: 0 })
  expectHard(await pair(app, scene, layout.sun))
  expectHard(await pair(app, scene, layout.spot))
})

test("the quality tier: hard flattens every light to the one tap, low and high keep the pair", async app => {
  let { scene, layout } = await loaded(app)
  await app.debug("quality", { quality: "hard" })
  expectHard(await pair(app, scene, layout.sun))
  expectHard(await pair(app, scene, layout.spot))
  await app.debug("quality", { quality: "low" })
  expectSoft(await pair(app, scene, layout.sun))
  await app.debug("quality", { quality: "high" })
  expectSoft(await pair(app, scene, layout.spot))
})
