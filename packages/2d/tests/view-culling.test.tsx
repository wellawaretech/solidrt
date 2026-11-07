// View culling per cell (the `cells` option of createSpriteLayer and
// createRecordLayer, over the core's cell-culled instance order): a view
// draws only the instances of the cells its camera meets, and what it
// draws is exactly what the same layer without cells draws. Every check
// renders a cells layer beside a plain twin under the same camera and
// compares the textures byte for byte, then reads the instances the
// view's entry submitted from the GPU inventory. GPU state throughout, so
// this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, destroy, setSprite } from "../src/index.ts"
import type { SpriteHandle as Sprite, SpriteLayerOptions, ViewHandle } from "../src/index.ts"
import type { CameraUpdate } from "../src/camera.ts"

const VIEW = 200
/** The grid: GRID x GRID sprites STEP apart, SPRITE wide, the first
 * centered at STEP / 2. */
const GRID = 30
const STEP = 100
const SPRITE = 40
const CELL = 256
const ATLAS: DecodedImage = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

type Draw = { id: number; instanceCount?: number; instanceSpans?: [number, number][]; instancesDrawn?: number }
type Pipeline = { kind: string; label?: string; draws: Draw[] }

// The instances the view target `label`'s one entry submits: its spans'
// sum, or its whole instance count without spans.
function drawn(app: TestApp, label: string): number {
  let pipelines = app.gpu({ label }).pipelines as Pipeline[]
  let found = pipelines.find(p => p.kind === "draws" && p.label === label)
  if (found === undefined) throw new Error("draw target not in the inventory: " + label)
  let draw = found.draws[0]!
  return draw.instancesDrawn ?? draw.instanceCount ?? 1
}

// The two views' textures, byte for byte.
function same(a: ViewHandle, b: ViewHandle): void {
  let pa = readTexture(a.texture)
  let pb = readTexture(b.texture)
  let painted = 0
  let differing = 0
  for (let i = 0; i < pa.data.length; i++) {
    if (pa.data[i] !== pb.data[i]) differing++
    if (i % 4 === 3 && pa.data[i]! > 0) painted++
  }
  expect(painted).toBeGreaterThan(0)
  expect(differing).toBe(0)
}

// A tint per grid position, so a sprite drawn in the wrong place or
// order shows.
function tint(i: number, j: number): [number, number, number, number] {
  return [((i * 7) % 16) / 15, ((j * 5) % 16) / 15, ((i + j) % 4) / 3, 1]
}

type Twin = { cells: ViewHandle; plain: ViewHandle; sprites: Sprite[][]; camera(update: CameraUpdate): void }

// A grid layer with cells and its plain twin, each with one view, the
// grid and the cameras `offset` px out on both axes.
function sprites(label: string, opts: SpriteLayerOptions = {}, offset = 0): Twin {
  let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
  let made: Sprite[][] = [[], []]
  let views: ViewHandle[] = []
  for (let [k, cells] of [[0, { size: CELL }], [1, undefined]] as const) {
    let layer = createSpriteLayer([atlas], { ...opts, capacity: GRID * GRID, label: `${label}-${k}`, cells })
    let camera = { x: offset, y: offset }
    views.push(layer.createView({ width: VIEW, height: VIEW, camera, clearColor: [0, 0, 0, 0], label: `${label}-${k}` }))
    for (let i = 0; i < GRID; i++) {
      for (let j = 0; j < GRID; j++) {
        made[k]!.push(addSprite(layer, { x: offset + i * STEP + STEP / 2, y: offset + j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) }))
      }
    }
  }
  return {
    cells: views[0]!,
    plain: views[1]!,
    sprites: made,
    camera(update) {
      for (let v of views) v.setCamera(update)
    },
  }
}

function mountTwin(app: TestApp, twin: () => Twin): Promise<Twin> {
  let t!: Twin
  return app
    .mount(() => {
      t = twin()
      return (
        <view>
          <texture src={t.cells.texture} width={VIEW} height={VIEW} />
          <texture src={t.plain.texture} width={VIEW} height={VIEW} />
        </view>
      )
    })
    .then(() => t)
}

test("a cells layer draws only the cells its view meets, and the same pixels as without cells", async app => {
  let t = await mountTwin(app, () => sprites("grid"))
  await app.frame()
  same(t.cells, t.plain)
  // The view [0, 200] meets cell (0, 0), [0, 256): the sprites centered
  // at 50, 150 and 250 on each axis.
  expect(drawn(app, "grid-0")).toBe(9)
  expect(drawn(app, "grid-1")).toBe(GRID * GRID)
  // A pan into the middle of the grid re-selects with no record upload.
  t.camera({ x: 1000, y: 1400 })
  await app.frame()
  same(t.cells, t.plain)
  let middle = drawn(app, "grid-0")
  expect(middle).toBeGreaterThan(9)
  expect(middle).toBeLessThan(GRID * GRID / 4)
  // A pan off the world draws nothing at all.
  t.camera({ x: -5000, y: -5000 })
  await app.frame()
  expect(drawn(app, "grid-0")).toBe(0)
})

test("sixteen million pixels out the cells cull exactly as at zero", async app => {
  // 2^24 px: past f32's whole numbers, and a multiple of the origin cell
  // the primary view snaps on (views.ts), where the records and the fed
  // view are relative to.
  let far = 2 ** 24
  let t = await mountTwin(app, () => sprites("far", {}, far))
  await app.frame()
  same(t.cells, t.plain)
  expect(drawn(app, "far-0")).toBe(9)
  t.camera({ x: far + 1000, y: far + 1400 })
  await app.frame()
  same(t.cells, t.plain)
  let middle = drawn(app, "far-0")
  expect(middle).toBeGreaterThan(9)
  expect(middle).toBeLessThan(GRID * GRID / 4)
})

test("hidden, destroyed and zero-size sprites are not submitted", async app => {
  let t = await mountTwin(app, () => sprites("empties"))
  await app.frame()
  expect(drawn(app, "empties-0")).toBe(9)
  // Three of the nine in view: one hidden, one destroyed, one collapsed.
  for (let k of [0, 1]) {
    setSprite(t.sprites[k]![0]!, { visible: false })
    destroy(t.sprites[k]![1]!)
    setSprite(t.sprites[k]![GRID]!, { w: 0 })
  }
  await app.frame()
  same(t.cells, t.plain)
  expect(drawn(app, "empties-0")).toBe(6)
})

test("a sprite moving into the view joins its new cell", async app => {
  let t = await mountTwin(app, () => sprites("moves"))
  await app.frame()
  // The far corner's sprite moves to the view's middle.
  for (let k of [0, 1]) setSprite(t.sprites[k]![GRID * GRID - 1]!, { x: 100, y: 120 })
  await app.frame()
  same(t.cells, t.plain)
  expect(drawn(app, "moves-0")).toBe(10)
})

test("a screen-size floor reaching into the view keeps its cell drawn when zoomed out", async app => {
  let t = await mountTwin(app, () => sprites("floor"))
  // Zoomed out to 0.05 the view spans 4000 px: the whole grid. One
  // sprite past it, a world pixel wide but floored at 60 px (30 view px
  // from its center, 600 world px), reaches into the view from 300 world
  // px beyond its right edge.
  t.camera({ zoom: 0.05 })
  for (let k of [0, 1]) setSprite(t.sprites[k]![0]!, { x: 4300, y: 2000, w: 1, h: 1, minScreenPx: 60 })
  await app.frame()
  same(t.cells, t.plain)
  expect(drawn(app, "floor-0")).toBe(GRID * GRID)
  // Without the floor it is out of every visible cell.
  for (let k of [0, 1]) setSprite(t.sprites[k]![0]!, { minScreenPx: 0 })
  await app.frame()
  same(t.cells, t.plain)
  expect(drawn(app, "floor-0")).toBe(GRID * GRID - 1)
})

test("under orderBy y the cells are bands and the y order stays exact", async app => {
  // Overlapping sprites (wider than their step) whose draw order is
  // their y: any slip shows as a differing pixel.
  let t = await mountTwin(app, () => {
    let twin = sprites("bands", { orderBy: "y" })
    for (let k of [0, 1]) for (let sprite of twin.sprites[k]!) setSprite(sprite, { w: STEP * 1.5, h: STEP * 1.5 })
    return twin
  })
  await app.frame()
  same(t.cells, t.plain)
  // A band is a row of the whole width: the view meets band 0 (y below
  // 256) and the band below it through the sprites' overhang.
  let banded = drawn(app, "bands-0")
  expect(banded).toBeGreaterThan(0)
  expect(banded).toBeLessThan(GRID * GRID / 4)
  t.camera({ x: 300, y: 900 })
  await app.frame()
  same(t.cells, t.plain)
})

test("every view of a cells layer selects under its own camera, and survives the ordering view going", async app => {
  let atlas = createAtlas(ATLAS, { label: "views-atlas" })
  let views!: { a: ViewHandle; b: ViewHandle; plain: ViewHandle }
  await app.mount(() => {
    let layer = createSpriteLayer([atlas], { capacity: GRID * GRID, label: "views", cells: { size: CELL } })
    let plain = createSpriteLayer([atlas], { capacity: GRID * GRID, label: "views-plain" })
    for (let i = 0; i < GRID; i++) {
      for (let j = 0; j < GRID; j++) {
        let opts = { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) }
        addSprite(layer, opts)
        addSprite(plain, opts)
      }
    }
    let a = layer.createView({ width: VIEW, height: VIEW, clearColor: [0, 0, 0, 0], label: "views-a" })
    let b = layer.createView({ width: VIEW, height: VIEW, camera: { x: 2000, y: 600 }, clearColor: [0, 0, 0, 0], label: "views-b" })
    let p = plain.createView({ width: VIEW, height: VIEW, camera: { x: 2000, y: 600 }, clearColor: [0, 0, 0, 0], label: "views-p" })
    views = { a, b, plain: p }
    return (
      <view>
        <texture src={a.texture} width={VIEW} height={VIEW} />
        <texture src={b.texture} width={VIEW} height={VIEW} />
        <texture src={p.texture} width={VIEW} height={VIEW} />
      </view>
    )
  })
  await app.frame()
  expect(drawn(app, "views-a")).toBe(9)
  let second = drawn(app, "views-b")
  expect(second).toBeGreaterThan(0)
  expect(second).toBeLessThan(GRID * GRID / 4)
  same(views.b, views.plain)
  // The view declaring the order goes: the other takes it over and keeps
  // culling, with the same pixels.
  views.a.dispose()
  await app.frame()
  expect(drawn(app, "views-b")).toBe(second)
  same(views.b, views.plain)
  views.b.setCamera({ x: 600, y: 2000 })
  views.plain.setCamera({ x: 600, y: 2000 })
  await app.frame()
  same(views.b, views.plain)
})

test("a record layer culls by its records' own fields", async app => {
  let atlas = createAtlas(ATLAS, { label: "records-atlas" })
  let views: ViewHandle[] = []
  await app.mount(() => {
    for (let [k, cells] of [[0, { size: CELL }], [1, undefined]] as const) {
      let layer = createRecordLayer([atlas], { capacity: GRID * GRID, label: `records-${k}`, cells })
      for (let i = 0; i < GRID; i++) {
        for (let j = 0; j < GRID; j++) {
          addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: SPRITE, h: SPRITE, rotation: 0.3, tint: tint(i, j) })
        }
      }
      views.push(layer.createView({ width: VIEW, height: VIEW, camera: { x: 700, y: 300 }, clearColor: [0, 0, 0, 0], label: `records-${k}` }))
    }
    return (
      <view>
        {views.map(v => (
          <texture src={v.texture} width={VIEW} height={VIEW} />
        ))}
      </view>
    )
  })
  await app.frame()
  same(views[0]!, views[1]!)
  let culled = drawn(app, "records-0")
  expect(culled).toBeGreaterThan(0)
  expect(culled).toBeLessThan(GRID * GRID / 4)
  expect(drawn(app, "records-1")).toBe(GRID * GRID)
})

test("cells refuse a world-wide order they cannot keep", async app => {
  let atlas = createAtlas(ATLAS, { label: "refuse-atlas" })
  await app.mount(() => <view />)
  expect(() => createSpriteLayer([atlas], { orderBy: "renderOrder", cells: { size: CELL }, autoFree: false })).toThrow("y order only")
  expect(() => createRecordLayer([atlas], { orderBy: { field: 9 }, cells: { size: CELL }, autoFree: false })).toThrow("y order only")
  expect(() => createSpriteLayer([atlas], { cells: { size: 0 }, autoFree: false })).toThrow("cells.size")
})
