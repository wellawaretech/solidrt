// A layer's atlases, pinned against running layers: sprites from several
// sheets draw in ONE layer (the multi-texture batch), interleaving in slot
// order, on the node layer, the record layer and the tile layer alike; a
// frame from a sheet the layer did not declare throws at the write and
// leaves the sprite as it was; the list itself is validated (empty,
// duplicate, past the device's sampler units); and an extruded, mipmapped
// sheet samples clean at a cell edge where the plain sheet's mip chain
// bleeds the neighbour in. GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { limits, readTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, createTileLayer, extrudeGrid, fullFrame, getSprite, grid, setSprite } from "../src/index.ts"
import type { Atlas } from "../src/index.ts"

const SIZE = 64
// Sprites drawn side by side: their size and the column spacing.
const SPRITE = 8
const STEP = 16
// 8-bit rounding slack on a read-back channel.
const CHANNEL_SLACK = 1
// The mip test's sheet: two solid 8 px cells (red, green) side by side,
// extruded with a 4-texel gutter; the sprite draws a cell 2 px wide, so
// the GPU samples mip level 2 (4 texels per pixel), where a plain sheet's
// chain has already mixed the cells across their shared edge.
const CELL = 8
const GUTTER = 4
const DRAWN = 2
// The sprite center sits 0.6 px past a pixel edge, so the right column's
// center lands at 95% of the quad: inside the frame, clamped to its edge.
const EDGE_OFFSET = 0.6
// A mixed edge texel shows at least this much of the neighbour's channel.
const MIXED = 32

/** A one-texel opaque sheet of the given colour. */
function texel(r: number, g: number, b: number): DecodedImage {
  return { width: 1, height: 1, data: new Uint8Array([r, g, b, 255]) }
}

/** `cols` solid 8 px cells in a row: red, green, blue. */
function cells(cols: number): DecodedImage {
  let data = new Uint8Array(cols * CELL * CELL * 4)
  for (let col = 0; col < cols; col++) {
    for (let y = 0; y < CELL; y++) {
      for (let x = 0; x < CELL; x++) {
        let i = (y * cols * CELL + col * CELL + x) * 4
        data[i] = col === 0 ? 255 : 0
        data[i + 1] = col === 1 ? 255 : 0
        data[i + 2] = col === 2 ? 255 : 0
        data[i + 3] = 255
      }
    }
  }
  return { width: cols * CELL, height: CELL, data }
}

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={SIZE} height={SIZE} />
  })
  return built
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

const RED = [255, 0, 0, 255]
const GREEN = [0, 255, 0, 255]
const BLUE = [0, 0, 255, 255]

function threeAtlases(label: string): [Atlas, Atlas, Atlas] {
  return [createAtlas(texel(255, 0, 0), { label: `${label}-red` }), createAtlas(texel(0, 255, 0), { label: `${label}-green` }), createAtlas(texel(0, 0, 255), { label: `${label}-blue` })]
}

test("a sprite layer draws from several atlases in one draw, sprites from different sheets interleaving", async app => {
  let { view, sprites, atlases } = await mounted(app, () => {
    let atlases = threeAtlases("multi")
    let [red, green, blue] = atlases
    let layer = createSpriteLayer(atlases, { capacity: 8, label: "multi" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "multi" })
    // Green, red, blue, green across one row: no sheet is contiguous.
    let order = [green, red, blue, green]
    let sprites = order.map((atlas, i) => addSprite(layer, { x: STEP / 2 + i * STEP, y: SIZE / 2, w: SPRITE, h: SPRITE, frame: fullFrame(atlas) }))
    return { layer, view, sprites, atlases, texture: view.texture }
  })
  await app.frame()
  expectPixel(pixel(view.texture, STEP / 2, SIZE / 2), GREEN)
  expectPixel(pixel(view.texture, STEP / 2 + STEP, SIZE / 2), RED)
  expectPixel(pixel(view.texture, STEP / 2 + 2 * STEP, SIZE / 2), BLUE)
  expectPixel(pixel(view.texture, STEP / 2 + 3 * STEP, SIZE / 2), GREEN)
  // The frame reads back with its texture, and the list is exposed.
  expect(getSprite(sprites[1]!)!.frame.texture).toBe(atlases[0].texture)
  expect(getSprite(sprites[2]!)!.frame.texture).toBe(atlases[2].texture)
  // A re-frame onto another sheet moves the sprite's sampler.
  setSprite(sprites[0]!, { frame: fullFrame(atlases[2]) })
  await app.frame()
  expectPixel(pixel(view.texture, STEP / 2, SIZE / 2), BLUE)
})

test("the record layer and the tile layer draw from several atlases too", async app => {
  let { view, tiles, atlases } = await mounted(app, () => {
    let atlases = threeAtlases("multi-records")
    let [red, green, blue] = atlases
    let layer = createRecordLayer(atlases, { capacity: 8, label: "multi-records" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "multi-records" })
    for (let [i, atlas] of [blue, green, red].entries()) addSprite(layer, { x: STEP / 2 + i * STEP, y: SIZE / 2, w: SPRITE, h: SPRITE, frame: fullFrame(atlas) })
    // A 2 x 1 tile world in one chunk, each cell from another sheet, and a
    // frames table spanning two sheets for the index form.
    let tiles = createTileLayer(2, 1, SIZE / 2, SIZE, atlases, { chunkTiles: 2, frames: [fullFrame(red), fullFrame(blue)], label: "multi-tiles" })
    tiles.setTile(0, 0, fullFrame(green))
    tiles.setTiles(1, 0, 1, 1, [1])
    return { layer, view, tiles, atlases, texture: view.texture }
  })
  await app.frame()
  expectPixel(pixel(view.texture, STEP / 2, SIZE / 2), BLUE)
  expectPixel(pixel(view.texture, STEP / 2 + STEP, SIZE / 2), GREEN)
  expectPixel(pixel(view.texture, STEP / 2 + 2 * STEP, SIZE / 2), RED)
  let chunk = tiles.chunks[0]!.texture
  expectPixel(pixel(chunk, SIZE / 4, SIZE / 2), GREEN)
  expectPixel(pixel(chunk, (SIZE * 3) / 4, SIZE / 2), BLUE)
  expect(tiles.getTile(0, 0)!.texture).toBe(atlases[1].texture)
  expect(tiles.getTile(1, 0)!.texture).toBe(atlases[2].texture)
})

test("a frame from a sheet the layer did not declare throws at the write and changes nothing, on every layer kind", async app => {
  let { nodes, records, tiles, stranger, red } = await mounted(app, () => {
    let red = createAtlas(texel(255, 0, 0), { label: "declared" })
    let stranger = createAtlas(texel(0, 255, 0), { label: "undeclared" })
    let nodes = createSpriteLayer([red], { capacity: 8, label: "strict" })
    let records = createRecordLayer([red], { capacity: 8, label: "strict-records" })
    let tiles = createTileLayer(2, 2, 8, 8, [red], { label: "strict-tiles" })
    let view = nodes.createView({ width: SIZE, height: SIZE, label: "strict" })
    return { nodes, records, tiles, stranger, red, texture: view.texture }
  })
  for (let layer of [nodes, records]) {
    expect(() => addSprite(layer, { frame: fullFrame(stranger) })).toThrow()
    let s = addSprite(layer, { x: 10, y: 10, w: 4, h: 4 })
    expect(getSprite(s)!.frame.texture).toBe(red.texture)
    expect(() => setSprite(s, { x: 20, frame: fullFrame(stranger) })).toThrow()
    expect(getSprite(s)!.x).toBe(10)
    expect(() => setSprite(s, { frame: { u0: 0, v0: 0, u1: 1, v1: 1 } as never })).toThrow()
  }
  expect(() => tiles.setTile(0, 0, fullFrame(stranger))).toThrow()
  expect(() => tiles.setTiles(0, 0, 2, 1, [fullFrame(red), fullFrame(stranger)])).toThrow()
  expect(tiles.getTile(0, 0)).toBe(null)
  expect(() => createTileLayer(2, 2, 8, 8, [red], { frames: [fullFrame(stranger)], label: "strict-table" })).toThrow()
})

test("the atlas list is validated: empty, a duplicate, past the device's sampler units", async app => {
  await mounted(app, () => {
    let red = createAtlas(texel(255, 0, 0), { label: "list-red" })
    expect(() => createSpriteLayer([], { label: "none" })).toThrow()
    expect(() => createSpriteLayer([red, red], { label: "twice" })).toThrow()
    expect(() => createRecordLayer([{ width: 1, height: 1 } as never], { label: "no-texture" })).toThrow()
    let many: Atlas[] = []
    for (let i = 0; i <= limits.maxTextureUnits; i++) many.push(createAtlas(texel(i, 0, 0), { label: `many-${i}` }))
    expect(() => createSpriteLayer(many, { label: "too-many" })).toThrow()
    let layer = createSpriteLayer(many.slice(0, limits.maxTextureUnits), { label: "just-enough" })
    let view = layer.createView({ width: SIZE, height: SIZE, label: "just-enough" })
    expect(layer.atlases.length).toBe(limits.maxTextureUnits)
    return { texture: view.texture }
  })
})

test("an extruded, mipmapped sheet samples clean at a cell edge where the plain sheet bleeds", async app => {
  let { view } = await mounted(app, () => {
    let sheet = cells(2)
    let plain = createAtlas(sheet, { mipmap: true, label: "plain-mip" })
    let extruded = extrudeGrid(sheet, 2, 1, GUTTER)
    let clean = createAtlas(extruded.image, { mipmap: true, label: "extruded-mip" })
    let plainFrames = grid(plain, 2, 1)
    let cleanFrames = grid(clean, 2, 1, extruded.options)
    let layer = createSpriteLayer([plain, clean], { capacity: 8, label: "mip" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "mip" })
    // The red cell of each sheet, drawn 2 px wide in its own row.
    addSprite(layer, { x: SIZE / 2 + EDGE_OFFSET, y: STEP, w: DRAWN, h: DRAWN, frame: plainFrames[0] })
    addSprite(layer, { x: SIZE / 2 + EDGE_OFFSET, y: 3 * STEP, w: DRAWN, h: DRAWN, frame: cleanFrames[0] })
    return { view, texture: view.texture }
  })
  await app.frame()
  // The right column of each sprite: the pixel whose sample is clamped
  // onto the cell's right edge.
  let column = SIZE / 2 + 1
  let bled = pixel(view.texture, column, STEP)
  let kept = pixel(view.texture, column, 3 * STEP)
  expect(bled[3]).toBe(255)
  expect(bled[1]! >= MIXED).toBe(true)
  expectPixel(kept, RED)
})
