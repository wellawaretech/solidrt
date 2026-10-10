// The screen-size clamp (SpriteOptions.minScreenPx/maxScreenPx) and the
// layer blend mode, pinned against running layers: a floored sprite draws
// at the floor when the zoom would shrink it below, a capped one at the
// ceiling when the zoom would grow it past, equal bounds hold a constant
// size, all in the pixels of a view; `pick` hits the drawn rect at the
// zoom it is asked for - on the node layer (through group scale, the
// world-matrix path) and on the record layer (the JS walk) alike, with
// `view.pick` passing its camera's zoom; the clamp pair is validated
// against the stored bounds before any write; and a material's "add"
// accumulates overlapping sprites where "alpha" composites them. GPU
// state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { readTexture } from "@solidrt/core/gpu"
import type { BlendMode, TextureId } from "@solidrt/core/gpu"
import { addGroup, addSprite, createAtlas, createRecordLayer, createSpriteLayer, createTileLayer, destroySprite, fullFrame, getSprite, setSprite, unlit } from "../src/index.ts"

const SIZE = 64
// The atlas: one opaque white texel, tinted per sprite.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// The floor every floored sprite here uses, view pixels.
const FLOOR = 16
// The sprite's world size: a quarter of the floor, so the floor is in
// force at zoom 1 and released at zoom 4.
const SPRITE = 4
// A big sprite's world size: four times the ceiling, so the ceiling is in
// force at zoom 1 and released at zoom 1/8.
const BIG = 64
// Half-alpha white, premultiplied: two over each other add to opaque white
// under "add" and to three quarters under "alpha".
const HALF: [number, number, number, number] = [0.5, 0.5, 0.5, 0.5]
// 8-bit rounding slack on a read-back channel: 0.75 is 191.25, and GPUs
// round it either way.
const CHANNEL_SLACK = 1

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={SIZE} height={SIZE} />
  })
  return built
}

/** The painted span of the row through `y`, in pixels (alpha above zero). */
function paintedWidth(texture: TextureId, y: number): number {
  let { width, data } = readTexture(texture)
  let painted = 0
  for (let x = 0; x < width; x++) if (data[(y * width + x) * 4 + 3]! > 0) painted++
  return painted
}

function pixel(texture: TextureId, x: number, y: number): number[] {
  let { width, data } = readTexture(texture)
  let at = (y * width + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

/** Every channel of `got` within the rounding slack of `want`. */
function expectPixel(got: number[], want: number[]): void {
  for (let i = 0; i < 4; i++) {
    if (Math.abs(got[i]! - want[i]!) > CHANNEL_SLACK) throw new Error(`pixel [${got}] is not [${want}] within ${CHANNEL_SLACK}`)
  }
}

test("a floored sprite draws at the floor when zoom would shrink it below, and releases past it", async app => {
  let { view } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "floor-atlas" })
    let layer = createSpriteLayer([atlas], { capacity: 8, label: "floor" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "floor" })
    addSprite(layer, { x: 32, y: 32, w: SPRITE, h: SPRITE, minScreenPx: FLOOR })
    return { layer, view, texture: view.texture }
  })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  // Zooming out leaves the sprite at the floor; without one it would be a
  // single pixel.
  view.setCamera({ zoom: 0.25, x: 32, y: 32, pivotX: 32, pivotY: 32 })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  // Past the floor the sprite is its world size.
  view.setCamera({ zoom: 8 })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(SPRITE * 8)
})

test("pick hits the drawn rect at the zoom it is asked for, through group scale, and view.pick passes its camera's", async app => {
  let { layer, view, ring, pin, plain } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "floor-pick-atlas" })
    let layer = createSpriteLayer([atlas], { capacity: 8, label: "floor-pick" })
    let view = layer.createView({ width: SIZE, height: SIZE, label: "floor-pick" })
    let ring = addSprite(layer, { x: 32, y: 32, w: SPRITE, h: SPRITE, minScreenPx: FLOOR })
    // A 2:1 pin under a group scaled 2x: world 8 x 4, floored to 32 x 16
    // at zoom 1, turned a quarter so its long axis runs down the view.
    let group = addGroup(layer, { x: 100, y: 100, scale: 2 })
    let pin = addSprite(layer, { x: 0, y: 0, w: SPRITE, h: SPRITE / 2, rotation: Math.PI / 2, minScreenPx: FLOOR, parent: group })
    let plain = addSprite(layer, { x: 200, y: 200, w: SPRITE, h: SPRITE })
    return { layer, view, ring, pin, plain, texture: view.texture }
  })
  // At zoom 1 the ring is 16 world px: a point 6 px off its center hits.
  expect(layer.pick(38, 32)).toEqual([ring])
  expect(layer.pick(38, 32, 1)).toEqual([ring])
  // At zoom 4 it is its world size, 4 px: the same point misses, 1 px hits.
  expect(layer.pick(38, 32, 4)).toEqual([])
  expect(layer.pick(33, 32, 4)).toEqual([ring])
  // At zoom 0.25 it reaches 64 world px: 30 px off still hits.
  expect(layer.pick(62, 32, 0.25)).toEqual([ring])
  // The pin: 32 x 16 drawn, turned, so 14 px down the long axis hits and
  // 14 px across does not; the unfloored sprite picks at its world size.
  expect(layer.pick(100, 114)).toEqual([pin])
  expect(layer.pick(114, 100)).toEqual([])
  expect(layer.pick(100, 103, 4)).toEqual([pin])
  expect(layer.pick(100, 106, 4)).toEqual([])
  expect(layer.pick(201, 201)).toEqual([plain])
  expect(layer.pick(206, 200)).toEqual([])
  // view.pick: the view pixel unprojected, its zoom applied.
  view.setCamera({ zoom: 4, x: 32, y: 32, pivotX: 32, pivotY: 32 })
  expect(view.pick(32 + 1 * 4, 32)).toEqual([ring])
  expect(view.pick(32 + 6 * 4, 32)).toEqual([])
  expect(() => layer.pick(32, 32, 0)).toThrow()
  expect(() => addSprite(layer, { minScreenPx: -1 })).toThrow()
})

test("the record layer floors and picks the same way", async app => {
  let { layer, view, dot } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "floor-records-atlas" })
    let layer = createRecordLayer([atlas], { capacity: 8, label: "floor-records" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "floor-records" })
    let dot = addSprite(layer, { x: 32, y: 32, w: SPRITE, h: SPRITE, minScreenPx: FLOOR })
    addSprite(layer, { x: 200, y: 200, w: SPRITE, h: SPRITE })
    return { layer, view, dot, texture: view.texture }
  })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  expect(layer.pick(38, 32)).toEqual([dot])
  expect(layer.pick(38, 32, 4)).toEqual([])
  expect(layer.pick(33, 32, 4)).toEqual([dot])
  expect(layer.pick(206, 200)).toEqual([])
  view.setCamera({ zoom: 4, x: 32, y: 32, pivotX: 32, pivotY: 32 })
  expect(view.pick(32 + 1 * 4, 32)).toEqual([dot])
  expect(view.pick(32 + 6 * 4, 32)).toEqual([])
  expect(() => addSprite(layer, { minScreenPx: Number.NaN })).toThrow()
})

/** Two half-alpha sprites over each other on a layer whose material
 * blends with `blend`: the center pixel of the view. */
async function overlapped(app: TestApp, blend: BlendMode): Promise<number[]> {
  let { view } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: `${blend}-atlas` })
    let layer = createSpriteLayer([atlas], { capacity: 8, material: unlit({ blend }), label: blend })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: blend })
    addSprite(layer, { x: 32, y: 32, w: 16, h: 16, tint: HALF })
    addSprite(layer, { x: 32, y: 32, w: 16, h: 16, tint: HALF })
    return { view, texture: view.texture }
  })
  await app.frame()
  return pixel(view.texture, 32, 32)
}

test("blend: \"alpha\" composites overlapping sprites to three quarters", async app => {
  expectPixel(await overlapped(app, "alpha"), [191, 191, 191, 191])
})

test("blend: \"add\" accumulates overlapping sprites to opaque white", async app => {
  expectPixel(await overlapped(app, "add"), [255, 255, 255, 255])
})

test("a tile layer takes the material too, baking its cells with its blend", async app => {
  let { layer } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "tiles-add-atlas" })
    let layer = createTileLayer(2, 2, 8, 8, [atlas], { material: unlit({ blend: "add" }), label: "tiles-add" })
    layer.setTile(0, 0, fullFrame(atlas), { tint: HALF })
    return { layer, texture: layer.chunks[0]!.texture }
  })
  await app.frame()
  // Over the transparent clear, "add" leaves the cell's own half alpha.
  expectPixel(pixel(layer.chunks[0]!.texture, 4, 4), [128, 128, 128, 128])
  expect(pixel(layer.chunks[0]!.texture, 12, 12)).toEqual([0, 0, 0, 0])
})

test("a floored sprite destroyed, or its floor cleared, lets the search bound shrink back", async app => {
  let { layer } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "floor-bound-atlas" })
    let layer = createSpriteLayer([atlas], { capacity: 8, label: "floor-bound" })
    let view = layer.createView({ width: SIZE, height: SIZE, label: "floor-bound" })
    return { layer, texture: view.texture }
  })
  // A wide floored pin reaches far; a point beside a small floored dot is
  // a candidate either way and must resolve by the dot's own drawn rect.
  let pin = addSprite(layer, { x: 32, y: 32, w: 64, h: 4, minScreenPx: FLOOR })
  let dot = addSprite(layer, { x: 32, y: 32, w: SPRITE, h: SPRITE, minScreenPx: FLOOR })
  expect(layer.pick(32 + 100, 32)).toEqual([pin])
  expect(layer.pick(38, 32)).toEqual([dot, pin])
  destroySprite(pin)
  expect(layer.pick(32 + 100, 32)).toEqual([])
  expect(layer.pick(38, 32)).toEqual([dot])
  // Clearing the last floor returns the layer to world-size picking.
  setSprite(dot, { minScreenPx: 0 })
  expect(layer.pick(38, 32)).toEqual([])
  expect(layer.pick(33, 32)).toEqual([dot])
  expect(() => setSprite(dot, { tint: [1, Number.NaN, 1, 1] })).toThrow()
})

test("a capped sprite draws at the ceiling when zoom would grow it past, equal bounds hold a constant size, and pick agrees", async app => {
  let { layer, view, label, gizmo } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "ceiling-atlas" })
    let layer = createSpriteLayer([atlas], { capacity: 8, label: "ceiling" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "ceiling" })
    let label = addSprite(layer, { x: 32, y: 32, w: BIG, h: BIG, maxScreenPx: FLOOR })
    let gizmo = addSprite(layer, { x: 200, y: 200, w: 1, h: 2, minScreenPx: FLOOR, maxScreenPx: FLOOR })
    return { layer, view, label, gizmo, texture: view.texture }
  })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  view.setCamera({ zoom: 4, x: 32, y: 32, pivotX: 32, pivotY: 32 })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  // Below the ceiling the sprite is its world size: 8 px at zoom 1/8.
  view.setCamera({ zoom: 1 / 8 })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(BIG / 8)
  // Picking: 16 world px drawn at zoom 1 (20 px off misses, 6 hits); the
  // full 64 at zoom 1/8 (30 px off hits).
  expect(layer.pick(52, 32)).toEqual([])
  expect(layer.pick(38, 32)).toEqual([label])
  expect(layer.pick(62, 32, 1 / 8)).toEqual([label])
  expect(layer.pick(52, 32, 4)).toEqual([])
  // The gizmo: 1 x 2 world, 16 x 32 on screen at any zoom, so in world
  // pixels 16/zoom wide: 6 px off its center hits at zoom 1 and not at
  // zoom 2 (8 wide), where 3 px off does.
  view.setCamera({ zoom: 0.25, x: 200, y: 200, pivotX: 32, pivotY: 32 })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  view.setCamera({ zoom: 8 })
  await app.frame()
  expect(paintedWidth(view.texture, 32)).toBe(FLOOR)
  expect(layer.pick(206, 200)).toEqual([gizmo])
  expect(layer.pick(206, 200, 2)).toEqual([])
  expect(layer.pick(203, 200, 2)).toEqual([gizmo])
  expect(layer.pick(206, 200, 4)).toEqual([])
  expect(view.pick(32 + 6 * 8, 32)).toEqual([])
  expect(view.pick(32 + 7, 32)).toEqual([gizmo])
})

test("the clamp pair is validated against the stored bounds before any write, on both layers", async app => {
  let { nodes, records } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "clamp-check-atlas" })
    let nodes = createSpriteLayer([atlas], { capacity: 8, label: "clamp-check" })
    let records = createRecordLayer([atlas], { capacity: 8, label: "clamp-check-records" })
    let view = nodes.createView({ width: SIZE, height: SIZE, label: "clamp-check" })
    return { nodes, records, texture: view.texture }
  })
  for (let layer of [nodes, records]) {
    expect(() => addSprite(layer, { minScreenPx: 16, maxScreenPx: 8 })).toThrow()
    expect(() => addSprite(layer, { maxScreenPx: -1 })).toThrow()
    let s = addSprite(layer, { x: 10, y: 10, w: 4, h: 4, minScreenPx: 16 })
    // A ceiling below the stored floor is refused and nothing changes.
    expect(() => setSprite(s, { x: 20, maxScreenPx: 8 })).toThrow()
    expect(getSprite(s)!.x).toBe(10)
    expect(getSprite(s)!.maxScreenPx).toBe(0)
    // Raising both at once is fine, as is clearing the floor first.
    setSprite(s, { minScreenPx: 20, maxScreenPx: 24 })
    expect(getSprite(s)!.maxScreenPx).toBe(24)
    setSprite(s, { minScreenPx: 0 })
    setSprite(s, { maxScreenPx: 8 })
    expect(getSprite(s)!.minScreenPx).toBe(0)
    expect(getSprite(s)!.maxScreenPx).toBe(8)
  }
})
