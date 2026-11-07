// View culling (the `cull` option of createSpriteLayer and
// createRecordLayer, on by default, over the core's per-view draw lists):
// a view draws only the instances its camera meets, and what it draws is
// exactly what the same layer with `cull: false` draws, in the same
// order. Every check renders a culled layer beside a plain twin under the
// same camera and compares the textures byte for byte, then reads the
// instances the view's draw list holds, and how it got there (rebuilt or
// patched), from the GPU inventory. The pick order under orderBy closes
// the file. GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { glsl, readTexture, SCREEN_SIZE_GLSL } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, destroy, records, setSprite, shaderMaterial, updateRecords } from "../src/index.ts"
import { SPRITE_VARYINGS, SPRITE_VERTEX_BODY, unlitFragment } from "../src/glsl.ts"
import type { RecordLayerHandle, RecordLayerOptions, SpriteHandle as Sprite, SpriteLayerOptions, ViewHandle } from "../src/index.ts"
import type { CameraUpdate } from "../src/camera.ts"

const VIEW = 200
/** The grid: GRID x GRID sprites STEP apart, SPRITE wide, the first
 * centered at STEP / 2. */
const GRID = 30
const STEP = 100
const SPRITE = 40
const CELL = 256
const ATLAS: DecodedImage = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

type ListFacts = { population: number; visible: number; rebuilds: number; patches: number }
type Draw = { id: number; instanceCount?: number; list?: ListFacts }
type Pipeline = { kind: string; label?: string; draws: Draw[] }

// The view target `label`'s one entry, as the GPU inventory reports it.
function entry(app: TestApp, label: string): Draw {
  let pipelines = app.gpu({ label }).pipelines as Pipeline[]
  let found = pipelines.find(p => p.kind === "draws" && p.label === label)
  if (found === undefined) throw new Error("draw target not in the inventory: " + label)
  return found.draws[0]!
}

// The instances the view target `label`'s one entry submits: its draw
// list's visible count, or its whole instance count without a list.
function drawn(app: TestApp, label: string): number {
  let draw = entry(app, label)
  return draw.list?.visible ?? draw.instanceCount ?? 1
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

type Twin = { culled: ViewHandle; plain: ViewHandle; sprites: Sprite[][]; camera(update: CameraUpdate): void }

// A grid layer culled by CELL cells and its `cull: false` twin, each with one view, the
// grid and the cameras `offset` px out on both axes.
function sprites(label: string, opts: SpriteLayerOptions = {}, offset = 0): Twin {
  let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
  let made: Sprite[][] = [[], []]
  let views: ViewHandle[] = []
  for (let [k, cull] of [[0, { cellSize: CELL }], [1, false]] as const) {
    let layer = createSpriteLayer([atlas], { ...opts, capacity: GRID * GRID, label: `${label}-${k}`, cull })
    let camera = { x: offset, y: offset }
    views.push(layer.createView({ width: VIEW, height: VIEW, camera, clearColor: [0, 0, 0, 0], label: `${label}-${k}` }))
    for (let i = 0; i < GRID; i++) {
      for (let j = 0; j < GRID; j++) {
        made[k]!.push(addSprite(layer, { x: offset + i * STEP + STEP / 2, y: offset + j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) }))
      }
    }
  }
  return {
    culled: views[0]!,
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
          <texture src={t.culled.texture} width={VIEW} height={VIEW} />
          <texture src={t.plain.texture} width={VIEW} height={VIEW} />
        </view>
      )
    })
    .then(() => t)
}

test("a culled layer draws only the sprites its view meets, and the same pixels as without culling", async app => {
  let t = await mountTwin(app, () => sprites("grid"))
  await app.frame()
  same(t.culled, t.plain)
  // The view [0, 200] meets cell (0, 0), [0, 256), an edge cell whose
  // records are tested one by one: the sprites centered at 50 and 150 on
  // each axis (the one at 250 starts at 230).
  expect(drawn(app, "grid-0")).toBe(4)
  expect(drawn(app, "grid-1")).toBe(GRID * GRID)
  // A pan into the middle of the grid rebuilds the list with no record
  // upload.
  t.camera({ x: 1000, y: 1400 })
  await app.frame()
  same(t.culled, t.plain)
  // [1000, 1200] x [1400, 1600]: again two sprites on each axis.
  expect(drawn(app, "grid-0")).toBe(4)
  // A pan off the world draws nothing at all.
  t.camera({ x: -5000, y: -5000 })
  await app.frame()
  expect(drawn(app, "grid-0")).toBe(0)
})

test("sixteen million pixels out the view culls exactly as at zero", async app => {
  // 2^24 px: past f32's whole numbers, and a multiple of the origin cell
  // the primary view snaps on (views.ts), where the records and the fed
  // view are relative to.
  let far = 2 ** 24
  let t = await mountTwin(app, () => sprites("far", {}, far))
  await app.frame()
  same(t.culled, t.plain)
  expect(drawn(app, "far-0")).toBe(4)
  t.camera({ x: far + 1000, y: far + 1400 })
  await app.frame()
  same(t.culled, t.plain)
  expect(drawn(app, "far-0")).toBe(4)
})

test("hidden, destroyed and zero-size sprites are not submitted", async app => {
  let t = await mountTwin(app, () => sprites("empties"))
  await app.frame()
  expect(drawn(app, "empties-0")).toBe(4)
  // Three of the four in view: one hidden, one destroyed, one collapsed.
  for (let k of [0, 1]) {
    setSprite(t.sprites[k]![0]!, { visible: false })
    destroy(t.sprites[k]![1]!)
    setSprite(t.sprites[k]![GRID]!, { w: 0 })
  }
  await app.frame()
  same(t.culled, t.plain)
  expect(drawn(app, "empties-0")).toBe(1)
})

test("a sprite moving into the view joins its draw list", async app => {
  let t = await mountTwin(app, () => sprites("moves"))
  await app.frame()
  // The far corner's sprite moves to the view's middle.
  for (let k of [0, 1]) setSprite(t.sprites[k]![GRID * GRID - 1]!, { x: 100, y: 120 })
  await app.frame()
  same(t.culled, t.plain)
  expect(drawn(app, "moves-0")).toBe(5)
})

test("a screen-size floor reaching into the view keeps its sprite drawn when zoomed out", async app => {
  let t = await mountTwin(app, () => sprites("floor"))
  // Zoomed out to 0.05 the view spans 4000 px: the whole grid. One
  // sprite past it, a world pixel wide but floored at 60 px (30 view px
  // from its center, 600 world px), reaches into the view from 300 world
  // px beyond its right edge.
  t.camera({ zoom: 0.05 })
  for (let k of [0, 1]) setSprite(t.sprites[k]![0]!, { x: 4300, y: 2000, w: 1, h: 1, minScreenPx: 60 })
  await app.frame()
  same(t.culled, t.plain)
  expect(drawn(app, "floor-0")).toBe(GRID * GRID)
  // Without the floor it is out of the view.
  for (let k of [0, 1]) setSprite(t.sprites[k]![0]!, { minScreenPx: 0 })
  await app.frame()
  same(t.culled, t.plain)
  expect(drawn(app, "floor-0")).toBe(GRID * GRID - 1)
})

// Overlapping sprites (wider than their step), so any slip in the draw
// order shows as a differing pixel; the camera puts cell boundaries
// through the view.
function overlapping(twin: Twin): Twin {
  for (let k of [0, 1]) for (let sprite of twin.sprites[k]!) setSprite(sprite, { w: STEP * 1.5, h: STEP * 1.5 })
  twin.camera({ x: 156, y: 156 })
  return twin
}

for (let [name, opts] of [
  ["slot order", {}],
  ["orderBy y", { orderBy: "y" }],
  ["orderBy renderOrder", { orderBy: "renderOrder" }],
] as [string, SpriteLayerOptions][]) {
  test(`culling keeps the painter order across cell boundaries under ${name}`, async app => {
    let label = name.replace(/ /g, "-")
    let t = await mountTwin(app, () => {
      let twin = overlapping(sprites(label, opts))
      // A renderOrder that is neither slot nor y order.
      if (opts.orderBy === "renderOrder") {
        for (let k of [0, 1]) twin.sprites[k]!.forEach((sprite, n) => setSprite(sprite, { renderOrder: (n * 37) % 11 }))
      }
      return twin
    })
    await app.frame()
    same(t.culled, t.plain)
    let culled = drawn(app, `${label}-0`)
    expect(culled).toBeGreaterThan(0)
    expect(culled).toBeLessThan(GRID * GRID / 4)
    t.camera({ x: 300, y: 900 })
    await app.frame()
    same(t.culled, t.plain)
  })
}

test("every view of a culled layer culls under its own camera, and keeps culling when another view goes", async app => {
  let atlas = createAtlas(ATLAS, { label: "views-atlas" })
  let views!: { a: ViewHandle; b: ViewHandle; plain: ViewHandle }
  await app.mount(() => {
    let layer = createSpriteLayer([atlas], { capacity: GRID * GRID, label: "views", cull: { cellSize: CELL } })
    let plain = createSpriteLayer([atlas], { capacity: GRID * GRID, label: "views-plain", cull: false })
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
  expect(drawn(app, "views-a")).toBe(4)
  let second = drawn(app, "views-b")
  expect(second).toBeGreaterThan(0)
  expect(second).toBeLessThan(GRID * GRID / 4)
  same(views.b, views.plain)
  // The first view goes: the other keeps its own list, with the same
  // pixels.
  views.a.dispose()
  await app.frame()
  expect(drawn(app, "views-b")).toBe(second)
  same(views.b, views.plain)
  views.b.setCamera({ x: 600, y: 2000 })
  views.plain.setCamera({ x: 600, y: 2000 })
  await app.frame()
  same(views.b, views.plain)
})

// A record layer culled by CELL cells and its `cull: false` twin over the grid, `size` per
// sprite, each with one view at `camera`.
function recordTwin(app: TestApp, label: string, opts: RecordLayerOptions, size: number, camera: CameraUpdate): Promise<ViewHandle[]> {
  let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
  let views: ViewHandle[] = []
  return app.mount(() => {
    for (let [k, cull] of [[0, { cellSize: CELL }], [1, false]] as const) {
      let layer = createRecordLayer([atlas], { ...opts, capacity: GRID * GRID, label: `${label}-${k}`, cull })
      for (let i = 0; i < GRID; i++) {
        for (let j = 0; j < GRID; j++) {
          addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: size, h: size, rotation: 0.3, tint: tint(i, j) })
        }
      }
      views.push(layer.createView({ width: VIEW, height: VIEW, camera, clearColor: [0, 0, 0, 0], label: `${label}-${k}` }))
    }
    return (
      <view>
        {views.map(v => (
          <texture src={v.texture} width={VIEW} height={VIEW} />
        ))}
      </view>
    )
  }).then(() => views)
}

test("a record layer culls by its records' own fields", async app => {
  let views = await recordTwin(app, "records", {}, SPRITE, { x: 700, y: 300 })
  await app.frame()
  same(views[0]!, views[1]!)
  let culled = drawn(app, "records-0")
  expect(culled).toBeGreaterThan(0)
  expect(culled).toBeLessThan(GRID * GRID / 4)
  expect(drawn(app, "records-1")).toBe(GRID * GRID)
})

test("a culled record layer keeps a descending field order across cell boundaries", async app => {
  // The key is the tint's red (field 9), descending: neither record nor
  // y order, over overlapping records.
  let views = await recordTwin(app, "fields", { orderBy: { field: 9, descending: true } }, STEP * 1.5, { x: 156, y: 156 })
  await app.frame()
  same(views[0]!, views[1]!)
  expect(drawn(app, "fields-0")).toBeLessThan(GRID * GRID / 4)
})

test("a sprite layer culls by default, cull: false draws every sprite, and a record layer culls when asked", async app => {
  let atlas = createAtlas(ATLAS, { label: "default-atlas" })
  let views: ViewHandle[] = []
  await app.mount(() => {
    let culled = createSpriteLayer([atlas], { capacity: GRID * GRID, label: "default-0" })
    let plain = createSpriteLayer([atlas], { capacity: GRID * GRID, label: "default-1", cull: false })
    let records = createRecordLayer([atlas], { capacity: GRID * GRID, label: "default-2" })
    let culledRecords = createRecordLayer([atlas], { capacity: GRID * GRID, label: "default-3", cull: true })
    for (let layer of [culled, plain, records, culledRecords]) {
      for (let i = 0; i < GRID; i++) {
        for (let j = 0; j < GRID; j++) addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) })
      }
    }
    views = [culled, plain, records, culledRecords].map((layer, k) => layer.createView({ width: VIEW, height: VIEW, clearColor: [0, 0, 0, 0], label: `default-${k}` }))
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
  same(views[2]!, views[1]!)
  same(views[3]!, views[1]!)
  expect(drawn(app, "default-0")).toBe(4)
  expect(drawn(app, "default-1")).toBe(GRID * GRID)
  expect(drawn(app, "default-2")).toBe(GRID * GRID)
  expect(drawn(app, "default-3")).toBe(4)
})

test("a write that keeps a sprite drawn patches the list in place", async app => {
  let t = await mountTwin(app, () => sprites("patch"))
  await app.frame()
  let before = entry(app, "patch-0").list!
  // A drawn sprite retinted and nudged within the view: the list keeps its
  // slots, and only that record is re-gathered.
  for (let k of [0, 1]) setSprite(t.sprites[k]![0]!, { x: 60, tint: [1, 1, 1, 1] })
  await app.frame()
  same(t.culled, t.plain)
  let patched = entry(app, "patch-0").list!
  expect(patched.patches).toBe(before.patches + 1)
  expect(patched.rebuilds).toBe(before.rebuilds)
  expect(patched.visible).toBe(4)
  // Moved out of the view: the drawn set changes, so the list rebuilds.
  for (let k of [0, 1]) setSprite(t.sprites[k]![0]!, { x: 2500 })
  await app.frame()
  same(t.culled, t.plain)
  let rebuilt = entry(app, "patch-0").list!
  expect(rebuilt.rebuilds).toBe(patched.rebuilds + 1)
  expect(rebuilt.visible).toBe(3)
})

// The red tint channel of a record (agents/records.md).
const TINT_R = 9

test("a publish over a whole range patches only the records it changed", async app => {
  let atlas = createAtlas(ATLAS, { label: "range-atlas" })
  let layers: RecordLayerHandle[] = []
  let views: ViewHandle[] = []
  await app.mount(() => {
    for (let [k, cull] of [[0, true], [1, false]] as const) {
      // Overlapping records in y order, so a stale or misplaced patch shows.
      let layer = createRecordLayer([atlas], { capacity: GRID * GRID, label: `range-${k}`, orderBy: "y", cull })
      for (let i = 0; i < GRID; i++) {
        for (let j = 0; j < GRID; j++) addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: STEP * 1.5, h: STEP * 1.5, tint: tint(i, j) })
      }
      layers.push(layer)
      views.push(layer.createView({ width: VIEW, height: VIEW, camera: { x: 156, y: 156 }, clearColor: [0, 0, 0, 0], label: `range-${k}` }))
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
  let before = entry(app, "range-0").list!
  // The whole range republished as it is: nothing changed, nothing to do.
  for (let layer of layers) updateRecords(layer)
  await app.frame()
  let same0 = entry(app, "range-0").list!
  expect(same0.patches).toBe(before.patches)
  expect(same0.rebuilds).toBe(before.rebuilds)
  // Two drawn records far apart retinted, published as the whole range:
  // one patch, no rebuild, and the pixels of the unculled twin.
  for (let layer of layers) {
    let r = records(layer)
    for (let slot of [2 * GRID + 2, 3 * GRID + 3]) r[slot * (r.length / (GRID * GRID)) + TINT_R] = 1
    updateRecords(layer)
  }
  await app.frame()
  same(views[0]!, views[1]!)
  let patched = entry(app, "range-0").list!
  expect(patched.patches).toBe(before.patches + 1)
  expect(patched.rebuilds).toBe(before.rebuilds)
})

// The view-pixel distance the margin test's vertex stage moves every
// sprite to the left of its rect, and the margin that covers it.
const SHIFT = 60
const MARGIN = 64

test("a material's cullMargin keeps a sprite its vertex stage moves into view drawn", async app => {
  // The stock stage with every sprite drawn SHIFT pixels left of its rect.
  let vertex = glsl`
    in vec2 iPos;
    in float iRot;
    in vec2 iScale;
    in vec4 iUv;
    in vec4 iTint;
    in vec2 iScreenPx;
    in float iAtlas;
    in vec4 iOutline;
    ${SPRITE_VARYINGS}
    ${SCREEN_SIZE_GLSL}
    void main() {
      float clampScale = screenSizeScale(iScale, uCamera.z, iScreenPx);
      vec2 size = iScale * clampScale;
      vec2 unit = spriteCorner(size, uCamera.z, iOutline.a * uCamera.z * clampScale, iUv);
      vec2 corner = unit * size;
      vec2 center = iPos - vec2(${SHIFT.toFixed(1)}, 0.0);
      float rot = iRot;
      ${SPRITE_VERTEX_BODY}
    }
  `
  let atlas = createAtlas(ATLAS, { label: "margin-atlas" })
  let views: ViewHandle[] = []
  await app.mount(() => {
    let materials = [
      shaderMaterial({ vertex, fragment: unlitFragment(), cullMargin: MARGIN, label: "margin-0" }),
      shaderMaterial({ vertex, fragment: unlitFragment(), label: "margin-1" }),
      shaderMaterial({ vertex, fragment: unlitFragment(), label: "margin-2" }),
    ]
    materials.forEach((material, k) => {
      let layer = createSpriteLayer([atlas], { capacity: 1, material, label: `margin-${k}`, cull: k === 2 ? false : undefined })
      // Its rect just right of the view, its drawn quad inside it.
      addSprite(layer, { x: VIEW + SPRITE / 2 + 10, y: VIEW / 2, w: SPRITE, h: SPRITE })
      views.push(layer.createView({ width: VIEW, height: VIEW, clearColor: [0, 0, 0, 0], label: `margin-${k}` }))
    })
    return (
      <view>
        {views.map(v => (
          <texture src={v.texture} width={VIEW} height={VIEW} />
        ))}
      </view>
    )
  })
  await app.frame()
  // With the margin it draws as unculled; without, the rect alone culls it.
  same(views[0]!, views[2]!)
  expect(drawn(app, "margin-0")).toBe(1)
  expect(drawn(app, "margin-1")).toBe(0)
  expect(() => shaderMaterial({ vertex, fragment: unlitFragment(), cullMargin: -1 })).toThrow("cullMargin")
})

test("cull refuses a cell size that is not a positive number", async app => {
  let atlas = createAtlas(ATLAS, { label: "refuse-atlas" })
  await app.mount(() => <view />)
  expect(() => createSpriteLayer([atlas], { cull: { cellSize: 0 }, autoFree: false })).toThrow("cull.cellSize")
  expect(() => createRecordLayer([atlas], { cull: { cellSize: Number.NaN }, autoFree: false })).toThrow("cull.cellSize")
})

// Pick follows the draw order: two overlapping sprites, the first added
// (slot 0) keyed to draw on top, so the topmost hit is it, not slot 1.
test("pick orders its hits by the layer's order, ties on slot", async app => {
  let atlas = createAtlas(ATLAS, { label: "pick-atlas" })
  await app.mount(() => <view />)
  let below = { x: 100, y: 100, w: SPRITE, h: SPRITE }
  let above = { x: 100, y: 110, w: SPRITE, h: SPRITE }
  let order = (layer: { pick(x: number, y: number): Sprite[] }, a: Sprite, b: Sprite) => {
    let hits = layer.pick(100, 105)
    expect(hits.length).toBe(2)
    expect(hits[0]).toBe(a)
    expect(hits[1]).toBe(b)
  }
  // No order: slot order, the later slot on top.
  let plain = createSpriteLayer([atlas], { autoFree: false })
  let p0 = addSprite(plain, above)
  let p1 = addSprite(plain, below)
  order(plain, p1, p0)
  plain.dispose()
  // y: the larger y draws later.
  let byY = createSpriteLayer([atlas], { orderBy: "y", autoFree: false })
  let y0 = addSprite(byY, above)
  let y1 = addSprite(byY, below)
  order(byY, y0, y1)
  byY.dispose()
  // renderOrder, and a tie on it falling back to slot order.
  let byKey = createSpriteLayer([atlas], { orderBy: "renderOrder", autoFree: false })
  let k0 = addSprite(byKey, { ...below, renderOrder: 2 })
  let k1 = addSprite(byKey, { ...below, renderOrder: 1 })
  order(byKey, k0, k1)
  setSprite(k0, { renderOrder: 1 })
  order(byKey, k1, k0)
  byKey.dispose()
  // A record layer's y, and a descending field.
  let records = createRecordLayer([atlas], { orderBy: "y", autoFree: false })
  let r0 = addSprite(records, above)
  let r1 = addSprite(records, below)
  order(records, r0, r1)
  records.dispose()
  let fields = createRecordLayer([atlas], { orderBy: { field: 9, descending: true }, autoFree: false })
  let f0 = addSprite(fields, { ...below, tint: [0, 0, 0, 1] })
  let f1 = addSprite(fields, { ...below, tint: [1, 0, 0, 1] })
  order(fields, f0, f1)
  fields.dispose()
})
