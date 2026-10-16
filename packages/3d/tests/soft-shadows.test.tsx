// Contact-hardening soft shadows through the picture: the soft-shadows
// example loaded whole, its lights and quality set through its debug
// commands, then a row of ground pixels read across each shadow's +z edge
// at two places - beside the caster's foot and under its top edge - and
// the width of the lit-to-shadowed transition measured on each. With a
// sized light the foot stays as sharp as the one hardware tap while the
// tip widens with the caster-to-receiver distance; with size 0, or the
// hard tier, both are the one hardware tap the example's lights pin as
// their floor, and a light handed back to the tier filters at the tier's
// width at both. The sun (an orthographic map, as a box and
// as cascades), the spot (a perspective map) and the lamp (a point
// light's six face maps) each get the pair, and a row inside the band
// where two cascades blend reads the same width as either cascade alone,
// because the penumbra is estimated in world units before each map
// converts it.

import { test, expect } from "@solidrt/test"
import type { Locator, Pixels, TestApp } from "@solidrt/test"

type Edge = { x: number; z: number }
type Pair = { foot: Edge; tip: Edge }
type Cascade = { edge: number; inner: number; band: number }
type Layout = { sun: Pair; spot: Pair; lamp: Pair; cascade: Cascade | null }
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
// A light without a size or a radius filters at the tier's width (2.5
// texels at medium, 9 cm on the sun's box map) at foot and tip alike:
// both wider than the one tap (over UNIFORM_MIN), under UNIFORM_MAX,
// and within UNIFORM_SPREAD of each other.
const UNIFORM_MIN = 0.08
const UNIFORM_MAX = 0.3
const UNIFORM_SPREAD = 0.06
// The cascade split is searched between these fractions of the sun's
// distance, by bisection in this many steps, to put the first map's
// edge, band or inner band side at a wanted x on the ground.
const SPLIT_LO = 0.05
const SPLIT_HI = 0.95
const SPLIT_STEPS = 12
// The band row crosses the wall's shadow midway between foot and tip; the
// single-cascade reads put the map's edge this far past the row (inside
// cascade 0 with the band clear of it) or this far short of it (outside,
// so cascade 1 alone has it), and the band's width may differ from
// either single read by this much (a few samples).
const CLEAR = 0.3
const CONTINUITY = 0.06

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

async function pair(app: TestApp, scene: Locator, edges: Pair): Promise<{ foot: number; tip: number }> {
  await app.settle()
  let px = scene.pixels()
  let w = { foot: await width(app, scene, px, edges.foot), tip: await width(app, scene, px, edges.tip) }
  console.log("Transition widths: foot " + w.foot.toFixed(3) + ", tip " + w.tip.toFixed(3))
  return w
}

async function single(app: TestApp, scene: Locator, edge: Edge): Promise<number> {
  await app.settle()
  let w = await width(app, scene, scene.pixels(), edge)
  console.log("Transition width: " + w.toFixed(3))
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

function expectUniform(w: { foot: number; tip: number }): void {
  expect(w.foot).toBeGreaterThan(UNIFORM_MIN)
  expect(w.foot).toBeLessThanOrEqual(UNIFORM_MAX)
  expect(w.tip).toBeLessThanOrEqual(UNIFORM_MAX)
  expect(Math.abs(w.tip - w.foot)).toBeLessThanOrEqual(UNIFORM_SPREAD)
}

/** Sets the two-cascade split that puts the first map's `key` at x =
 * `target` on the ground (the layout replays the scene's cascade fit),
 * and returns it. */
async function splitFor(app: TestApp, key: keyof Cascade, target: number): Promise<number> {
  let at = async (s: number) => {
    await app.debug("sun", { split: s })
    let cascade = ((await app.debug("layout")) as Layout).cascade
    expect(cascade).not.toBeNull()
    return cascade![key] - target
  }
  let lo = SPLIT_LO
  let hi = SPLIT_HI
  let flo = await at(lo)
  expect(flo * (await at(hi))).toBeLessThan(0)
  for (let i = 0; i < SPLIT_STEPS; i++) {
    let mid = (lo + hi) / 2
    let f = await at(mid)
    if (f * flo > 0) {
      lo = mid
      flo = f
    } else hi = mid
  }
  let s = (lo + hi) / 2
  await app.debug("sun", { split: s })
  return s
}

test("a sized sun: the wall's shadow is sharp at its foot and wide under its top edge", async app => {
  let { scene, layout } = await loaded(app)
  expectSoft(await pair(app, scene, layout.sun))
})

// Two cascades and the lamp's six face maps overflow the scene's shadow
// budget together: the cascade reads turn the lamp's shadow off.
async function cascaded(app: TestApp): Promise<void> {
  await app.debug("lamp", { castShadow: false })
  await app.debug("sun", { cascades: 2 })
}

test("the same sun as two cascades: the penumbra comes out of the cascade fit's metric too", async app => {
  let { scene, layout } = await loaded(app)
  await cascaded(app)
  expectSoft(await pair(app, scene, layout.sun))
})

test("a spot with a source radius: the post's shadow hardens at its foot and softens toward its tip", async app => {
  let { scene, layout } = await loaded(app)
  expectSoft(await pair(app, scene, layout.spot))
})

test("a point light with a source radius: its face map gives the block's shadow the same pair", async app => {
  let { scene, layout } = await loaded(app)
  expectSoft(await pair(app, scene, layout.lamp))
})

test("size 0 is a point source: both edges are the one hardware tap", async app => {
  let { scene, layout } = await loaded(app)
  await app.debug("sun", { angularSize: 0 })
  await app.debug("spot", { sourceRadius: 0 })
  await app.debug("lamp", { sourceRadius: 0 })
  expectHard(await pair(app, scene, layout.sun))
  expectHard(await pair(app, scene, layout.spot))
  expectHard(await pair(app, scene, layout.lamp))
})

test("no radius: a light without a size filters at the tier's width, foot and tip alike", async app => {
  let { scene, layout } = await loaded(app)
  await app.debug("sun", { angularSize: 0, radius: null })
  expectUniform(await pair(app, scene, layout.sun))
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

test("the cascade blend band: a row where two cascades mix reads the width either cascade reads alone", async app => {
  let { scene, layout } = await loaded(app)
  await cascaded(app)
  let row: Edge = { x: (layout.sun.foot.x + layout.sun.tip.x) / 2, z: layout.sun.foot.z }
  // Cascade 0 alone: its band starts past the row.
  await splitFor(app, "inner", row.x - CLEAR)
  let first = await single(app, scene, row)
  // Cascade 1 alone: cascade 0's map ends before the row.
  await splitFor(app, "edge", row.x + CLEAR)
  let second = await single(app, scene, row)
  // The band's middle on the row: half of each.
  await splitFor(app, "band", row.x)
  let mixed = await single(app, scene, row)
  expect(mixed).toBeGreaterThanOrEqual(Math.min(first, second) - CONTINUITY)
  expect(mixed).toBeLessThanOrEqual(Math.max(first, second) + CONTINUITY)
  expect(mixed).toBeGreaterThan(HARD_MAX)
})
