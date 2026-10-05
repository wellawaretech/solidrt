// A write made in a frame is in that frame's paint: every layer kind
// publishes its pending batch in the publish pass of onBeforeRender, ahead
// of renderFrame(), so a sprite added, a record written or a tile set from
// onFrame is drawn in the same frame, never the next. The layers are GPU
// state, so this is an app test, and the probe is the frame itself: a
// capture of the leaf showing the layer, requested from inside the frame
// after the write, is serviced by that frame's own paint (a readback would
// re-render a view first and see the write wherever it landed).

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { onFrame } from "@solidrt/core"
import { captureSnapshot } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, createTileLayer, fullFrame } from "../src/index.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// The sprite: a square around the view's center.
const SPRITE = 16
// The tile layer: one chunk of 2 x 2 tiles filling the 64 px leaf.
const TILES = 2
const TILE = SIZE / TILES
const WHITE = [255, 255, 255, 255]
const CLEAR = [0, 0, 0, 255]
const TRANSPARENT = [0, 0, 0, 0]

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T & { leaf: RefLocator }> {
  let leaf = app.ref()
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture ref={leaf} src={built.texture} width={SIZE} height={SIZE} />
  })
  return { ...built, leaf }
}

/**
 * Runs `write` among the next frame's callbacks and captures `leaf` from
 * inside that frame: what that frame's own paint drew. The capture's
 * promise settles with the frame after, so two frames run.
 */
async function painted(app: TestApp, leaf: RefLocator, write: () => void): Promise<Pixels> {
  let shot!: Promise<Pixels>
  let stop = onFrame(() => {
    stop()
    write()
    shot = captureSnapshot(leaf.record.id)
  })
  await app.frame()
  await app.frame()
  return shot
}

/** The pixel at a fraction of the way across and down `pixels`. */
function at(pixels: Pixels, fx: number, fy: number): number[] {
  let x = Math.floor(fx * pixels.width)
  let y = Math.floor(fy * pixels.height)
  let i = (y * pixels.width + x) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

test("a sprite added from onFrame is drawn in that frame", async app => {
  let { layer, leaf } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "before-render-sprites-atlas" })
    let layer = createSpriteLayer([atlas], { capacity: 8, label: "before-render-sprites" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 1], label: "before-render-sprites" })
    return { layer, texture: view.texture }
  })
  let before = await painted(app, leaf, () => {})
  expect(at(before, 0.5, 0.5)).toEqual(CLEAR)
  let frame = await painted(app, leaf, () => addSprite(layer, { x: SIZE / 2, y: SIZE / 2, w: SPRITE, h: SPRITE }))
  expect(at(frame, 0.5, 0.5)).toEqual(WHITE)
})

test("a record sprite added from onFrame is drawn in that frame", async app => {
  let { layer, leaf } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "before-render-records-atlas" })
    let layer = createRecordLayer([atlas], { capacity: 8, label: "before-render-records" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 1], label: "before-render-records" })
    return { layer, texture: view.texture }
  })
  let before = await painted(app, leaf, () => {})
  expect(at(before, 0.5, 0.5)).toEqual(CLEAR)
  let frame = await painted(app, leaf, () => addSprite(layer, { x: SIZE / 2, y: SIZE / 2, w: SPRITE, h: SPRITE }))
  expect(at(frame, 0.5, 0.5)).toEqual(WHITE)
})

test("a tile set from onFrame is baked for that frame's paint", async app => {
  let { layer, leaf } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "before-render-tiles-atlas" })
    let layer = createTileLayer(TILES, TILES, TILE, TILE, [atlas], { chunkTiles: TILES, label: "before-render-tiles" })
    // The chunk exists once a cell in it is set; setting then clearing one
    // leaves an allocated, empty chunk for the leaf to show.
    layer.setTile(0, 0, fullFrame(layer.atlases[0]!))
    layer.setTile(0, 0, null)
    return { layer, texture: layer.chunks[0]!.texture }
  })
  let before = await painted(app, leaf, () => {})
  expect(at(before, 0.25, 0.25)).toEqual(TRANSPARENT)
  let frame = await painted(app, leaf, () => layer.setTile(0, 0, fullFrame(layer.atlases[0]!)))
  expect(at(frame, 0.25, 0.25)).toEqual(WHITE)
  expect(at(frame, 0.75, 0.75)).toEqual(TRANSPARENT)
})
