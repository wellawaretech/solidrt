// The records layer's raw path, as a painted frame sees it: records(layer)
// is the mirror and updateRecords publishes a RANGE of it (a record left
// out keeps drawing what the GPU holds), setRecordCount dials the drawn
// prefix and pick ignores what is past it, a destroy publishes the records
// it shifted, growth draws the grown population whole in its own frame,
// and an ordered layer gathers any range into key order. The layer is GPU
// state, so this is an app test, and the probe is the frame itself:
// `app.painted` reads the leaf showing the layer as that frame's own paint
// drew it. The same verbs, one dimension up, are pinned by
// packages/3d/tests/records.test.tsx.

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { onFrame } from "@solidrt/core"
import type { TextureId } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, destroySprite, records, setRecordCount, updateRecords, INSTANCE_FLOATS, SPRITE_FLOATS } from "../src/index.ts"
import type { RecordLayerHandle, RecordLayerOptions, SpriteLayerHandle, SpriteLayerOptions } from "../src/index.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// Squares a quarter of the view wide, centred a quarter, half and three
// quarters of the way across the view's middle row.
const SPRITE = SIZE / 4
const LEFT = SIZE / 4
const MID = SIZE / 2
const RIGHT = (3 * SIZE) / 4
// Record field offsets (see records()).
const FIELD_Y = 1
const FIELD_W = 2
// Sprite record field offsets of the tint and the renderOrder key (the
// node layer's records()).
const STYLE_TINT = 4
const STYLE_KEY = 8
const WHITE = [255, 255, 255, 255]
const RED = [255, 0, 0, 255]
const GREEN = [0, 255, 0, 255]
const CLEAR = [0, 0, 0, 255]

/** A records layer of `capacity` sprites with one view, under the
 * mounted app's root; the view's texture is the leaf. */
function world(opts: RecordLayerOptions = {}): { layer: RecordLayerHandle; texture: TextureId } {
  let atlas = createAtlas(ATLAS, { label: "records-atlas" })
  let layer = createRecordLayer([atlas], { capacity: 2, label: "records", ...opts })
  let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 1], label: "records" })
  return { layer, texture: view.texture }
}

async function mounted(app: TestApp, opts: RecordLayerOptions = {}): Promise<{ layer: RecordLayerHandle; leaf: RefLocator }> {
  let leaf = app.ref()
  let built!: ReturnType<typeof world>
  await app.mount(() => {
    built = world(opts)
    return <texture ref={leaf} src={built.texture} width={SIZE} height={SIZE} />
  })
  return { layer: built.layer, leaf }
}

/** Runs `write` among the next frame's callbacks; what that frame drew of `leaf`. */
function painted(app: TestApp, leaf: RefLocator, write: () => void): Promise<Pixels> {
  let stop = onFrame(() => {
    stop()
    write()
  })
  return app.painted(leaf)
}

/** The pixel under view point (x, y), whatever the leaf's device size. */
function at(pixels: Pixels, x: number, y: number): number[] {
  let px = Math.floor((x / SIZE) * pixels.width)
  let py = Math.floor((y / SIZE) * pixels.height)
  let i = (py * pixels.width + px) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

let square = (x: number, tint?: [number, number, number, number]) => ({ x, y: MID, w: SPRITE, h: SPRITE, ...(tint ? { tint } : {}) })

test("updateRecords publishes the range it names; a record left out keeps drawing what the GPU holds", async app => {
  let { layer, leaf } = await mounted(app)
  addSprite(layer, square(LEFT))
  addSprite(layer, square(RIGHT))
  let before = await painted(app, leaf, () => {})
  expect(at(before, LEFT, MID)).toEqual(WHITE)
  expect(at(before, RIGHT, MID)).toEqual(WHITE)
  // Both records collapse in the mirror; only the second publishes.
  let partial = await painted(app, leaf, () => {
    let r = records(layer)
    r[0 * INSTANCE_FLOATS + FIELD_W] = 0
    r[1 * INSTANCE_FLOATS + FIELD_W] = 0
    updateRecords(layer, { first: 1, count: 1 })
  })
  expect(at(partial, LEFT, MID)).toEqual(WHITE)
  expect(at(partial, RIGHT, MID)).toEqual(CLEAR)
  let whole = await painted(app, leaf, () => updateRecords(layer))
  expect(at(whole, LEFT, MID)).toEqual(CLEAR)
  expect(() => updateRecords(layer, { first: 1, count: 2 })).toThrow()
})

test("setRecordCount draws the first n sprites and pick ignores the rest; the dial outlives an add past it", async app => {
  let { layer, leaf } = await mounted(app)
  let a = addSprite(layer, square(LEFT))
  addSprite(layer, square(RIGHT))
  let dialed = await painted(app, leaf, () => setRecordCount(layer, 1))
  expect(at(dialed, LEFT, MID)).toEqual(WHITE)
  expect(at(dialed, RIGHT, MID)).toEqual(CLEAR)
  expect(layer.pick(LEFT, MID)[0]).toBe(a)
  expect(layer.pick(RIGHT, MID)).toEqual([])
  expect(layer.count).toBe(2)
  // A third sprite past the dial (and past capacity: the layer grows)
  // stays undrawn until the dial moves.
  let added = await painted(app, leaf, () => addSprite(layer, square(MID)))
  expect(at(added, MID, MID)).toEqual(CLEAR)
  expect(layer.count).toBe(3)
  let all = await painted(app, leaf, () => setRecordCount(layer, 3))
  expect(at(all, LEFT, MID)).toEqual(WHITE)
  expect(at(all, MID, MID)).toEqual(WHITE)
  expect(at(all, RIGHT, MID)).toEqual(WHITE)
  expect(() => setRecordCount(layer, -1)).toThrow()
})

test("destroySprite shifts the later records down and publishes them", async app => {
  let { layer, leaf } = await mounted(app, { capacity: 4 })
  let a = addSprite(layer, square(LEFT))
  addSprite(layer, square(MID))
  addSprite(layer, square(RIGHT))
  let frame = await painted(app, leaf, () => destroySprite(a))
  expect(at(frame, LEFT, MID)).toEqual(CLEAR)
  expect(at(frame, MID, MID)).toEqual(WHITE)
  expect(at(frame, RIGHT, MID)).toEqual(WHITE)
  expect(layer.count).toBe(2)
})

test("an add past capacity draws the grown population in that frame, and records() hands out the new mirror", async app => {
  let { layer, leaf } = await mounted(app, { capacity: 1 })
  addSprite(layer, square(LEFT))
  let before = await painted(app, leaf, () => {})
  expect(at(before, LEFT, MID)).toEqual(WHITE)
  let old = records(layer)
  let frame = await painted(app, leaf, () => addSprite(layer, square(RIGHT)))
  expect(at(frame, LEFT, MID)).toEqual(WHITE)
  expect(at(frame, RIGHT, MID)).toEqual(WHITE)
  expect(records(layer)).not.toBe(old)
  expect(records(layer).length).toBe(2 * INSTANCE_FLOATS)
  // The hoisted view is a dead copy: a write through it publishes nothing.
  let dead = await painted(app, leaf, () => {
    old[0 * INSTANCE_FLOATS + FIELD_W] = 0
    updateRecords(layer)
  })
  expect(at(dead, LEFT, MID)).toEqual(WHITE)
})

test("an ordered layer gathers a published range into key order", async app => {
  let { layer, leaf } = await mounted(app, { orderBy: "y" })
  // Two overlapping squares at the centre: the red one sits higher, so
  // the green one (larger y) draws later and wins the overlap.
  addSprite(layer, { ...square(MID, [1, 0, 0, 1]), y: MID - 2 })
  addSprite(layer, { ...square(MID, [0, 1, 0, 1]), y: MID + 2 })
  let before = await painted(app, leaf, () => {})
  expect(at(before, MID, MID)).toEqual(GREEN)
  // The red record moves below the green one in the mirror; publishing
  // its record alone re-gathers the pair.
  let after = await painted(app, leaf, () => {
    records(layer)[0 * INSTANCE_FLOATS + FIELD_Y] = MID + 4
    updateRecords(layer, { first: 0, count: 1 })
  })
  expect(at(after, MID, MID)).toEqual(RED)
})

/** A node layer of `capacity` sprites with one view, under the mounted
 * app's root; the view's texture is the leaf. */
async function mountedNodes(app: TestApp, opts: SpriteLayerOptions = {}): Promise<{ layer: SpriteLayerHandle; leaf: RefLocator }> {
  let leaf = app.ref()
  let layer!: SpriteLayerHandle
  let texture!: TextureId
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "nodes-atlas" })
    layer = createSpriteLayer([atlas], { capacity: 2, label: "nodes", ...opts })
    texture = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 1], label: "nodes" }).texture
    return <texture ref={leaf} src={texture} width={SIZE} height={SIZE} />
  })
  return { layer, leaf }
}

test("a node layer's records() is its style mirror and updateRecords publishes the slot range it names", async app => {
  let { layer, leaf } = await mountedNodes(app)
  addSprite(layer, square(LEFT))
  addSprite(layer, square(RIGHT))
  let before = await painted(app, leaf, () => {})
  expect(at(before, LEFT, MID)).toEqual(WHITE)
  expect(at(before, RIGHT, MID)).toEqual(WHITE)
  // Both slots turn red in the mirror; only the second publishes.
  let partial = await painted(app, leaf, () => {
    let r = records(layer)
    for (let slot of [0, 1]) {
      r[slot * SPRITE_FLOATS + STYLE_TINT + 1] = 0
      r[slot * SPRITE_FLOATS + STYLE_TINT + 2] = 0
    }
    updateRecords(layer, { first: 1, count: 1 })
  })
  expect(at(partial, LEFT, MID)).toEqual(WHITE)
  expect(at(partial, RIGHT, MID)).toEqual(RED)
  let whole = await painted(app, leaf, () => updateRecords(layer))
  expect(at(whole, LEFT, MID)).toEqual(RED)
  expect(() => updateRecords(layer, { first: 1, count: 2 })).toThrow()
  // Growth replaces the mirror: the hoisted view is a dead copy.
  let old = records(layer)
  let grown = await painted(app, leaf, () => addSprite(layer, square(MID)))
  expect(at(grown, MID, MID)).toEqual(WHITE)
  expect(records(layer)).not.toBe(old)
  expect(records(layer).length).toBe(4 * SPRITE_FLOATS)
  let dead = await painted(app, leaf, () => {
    old[0 * SPRITE_FLOATS + STYLE_TINT + 1] = 1
    updateRecords(layer)
  })
  expect(at(dead, LEFT, MID)).toEqual(RED)
})

test("an ordered node layer republishes its style records whole, gathered into key order", async app => {
  let { layer, leaf } = await mountedNodes(app, { orderBy: "renderOrder" })
  // Two overlapping squares at the centre; the green one's key draws it last.
  addSprite(layer, { ...square(MID, [1, 0, 0, 1]), renderOrder: 1 })
  addSprite(layer, { ...square(MID, [0, 1, 0, 1]), renderOrder: 2 })
  let before = await painted(app, leaf, () => {})
  expect(at(before, MID, MID)).toEqual(GREEN)
  // The red record's key moves above the green one's in the mirror;
  // publishing its slot alone re-gathers the pair.
  let after = await painted(app, leaf, () => {
    records(layer)[0 * SPRITE_FLOATS + STYLE_KEY] = 3
    updateRecords(layer, { first: 0, count: 1 })
  })
  expect(at(after, MID, MID)).toEqual(RED)
})
