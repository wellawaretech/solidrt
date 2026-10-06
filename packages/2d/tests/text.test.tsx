// Text runs and the distance-field decode, pinned against running layers:
// a sprite font over the engine's mask cells draws its glyphs where the
// layout puts them (ink in the glyph's box, nothing beside it), a run is a
// group of reused sprites that moves and re-lays out, a glyph the atlas
// lacks draws nothing at its advance until the cell lands, and an atlas
// declared `sdf` decodes a synthetic field: a disc's edge is sharp at 2x
// magnification and an outline paints its colour in a ring. GPU state
// throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createSignal, Show } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { addText, createAtlas, createSpriteFont, createSpriteLayer, destroyText, fullFrame, addSprite, setText, SpriteLayer, Text2d, worldPosition } from "../src/index.ts"
import type { SpriteFont, SpriteLayerHandle, TextRun } from "../src/index.ts"

const SIZE = 128
// The mask font's pixel size: cells at this many texels per em, drawn 1:1.
const FONT_PX = 32
// 8-bit rounding slack on a read-back channel.
const CHANNEL_SLACK = 2
// A texel counts as inked past this alpha.
const INK = 64

// The synthetic field: a disc in a 64-square sheet, range 8 texels, drawn
// at 2x so the one-screen-pixel edge is half a texel wide.
const FIELD = 64
const RADIUS = 20
const RANGE = 8
const ZOOM = 2
// Pixels either side of the drawn edge that must be fully in and fully out.
const EDGE_MARGIN = 3
const OUTLINE_PX = 6

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
  let at = (Math.round(y) * width + Math.round(x)) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

/** Alpha summed over a rect of the texture. */
function inkIn(texture: TextureId, x0: number, y0: number, x1: number, y1: number): number {
  let { width, data } = readTexture(texture)
  let sum = 0
  for (let y = Math.floor(y0); y < Math.ceil(y1); y++) for (let x = Math.floor(x0); x < Math.ceil(x1); x++) sum += data[(y * width + x) * 4 + 3]!
  return sum
}

/** A disc's multi-channel field: the signed distance to the circle in
 * every channel (a true SDF is a valid MTSDF: its median is itself). */
function disc(): DecodedImage {
  let data = new Uint8Array(FIELD * FIELD * 4)
  for (let y = 0; y < FIELD; y++) {
    for (let x = 0; x < FIELD; x++) {
      let d = RADIUS - Math.hypot(x + 0.5 - FIELD / 2, y + 0.5 - FIELD / 2)
      let v = Math.round(Math.min(Math.max(0.5 + d / RANGE, 0), 1) * 255)
      data.set([v, v, v, v], (y * FIELD + x) * 4)
    }
  }
  return { data, width: FIELD, height: FIELD }
}

test("a text run draws its glyphs from the engine's cells where the layout puts them", async app => {
  let { view, run, layer } = await mounted(app, () => {
    let font = createSpriteFont({ fontFamily: "sans", fontSize: FONT_PX, fontWeight: 700 }, { cells: "mask", chars: false, label: "text-mask" })
    let layer = createSpriteLayer([font.atlas], { capacity: 32, label: "text" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "text" })
    let run = addText(layer, { font, text: "HI", x: 8, y: 8, tint: [1, 0, 0, 1] })
    return { font, layer, view, run, texture: view.texture }
  })
  // The cells are made off the frame: until they land the glyphs are
  // hidden sprites at their places.
  expect(run.sprites.length).toBe(2)
  expect(run.sprites.every(s => !s._visible)).toBe(true)
  await app.settle()
  await app.frame()
  expect(run.sprites.every(s => s._visible)).toBe(true)
  // Ink inside the run's box, in the tint, none below it.
  expect(run.width).toBeGreaterThan(FONT_PX)
  expect(inkIn(view.texture, 8, 8, 8 + run.width, 8 + run.height)).toBeGreaterThan(0)
  expect(inkIn(view.texture, 8, 8 + run.height + 2, 8 + run.width, SIZE)).toBe(0)
  // The H's left stem: a column inside its first glyph is red. A glyph
  // sprite's own x/y are local to the run's group; its layer position is
  // the composed one.
  let stem = run.sprites[0]!
  let [sx, sy] = worldPosition(stem)!
  let p = pixel(view.texture, sx - stem._w / 2 + 2, sy)
  expect(p[3]!).toBeGreaterThan(INK)
  expect(p[0]!).toBeGreaterThan(INK)
  expect(p[1]!).toBeLessThanOrEqual(CHANNEL_SLACK)
  expect(p[2]!).toBeLessThanOrEqual(CHANNEL_SLACK)
  // A re-set reuses the pool: three glyphs, the first two sprites kept.
  let before = run.sprites.slice()
  setText(run, { text: "HIP" })
  expect(run.sprites.length).toBe(3)
  expect(run.sprites[0]).toBe(before[0])
  expect(run.sprites[1]).toBe(before[1])
  await app.settle()
  // Shorter again: the extra sprite goes, the layer's count follows.
  setText(run, { text: "H" })
  expect(run.sprites.length).toBe(1)
  expect(layer.count).toBe(1)
  destroyText(run)
  expect(layer.count).toBe(0)
  expect(run.group.layer).toBe(null)
})

test("a run's pose is its group's, and a run anchors and wraps as laid out", async app => {
  let { view, run } = await mounted(app, () => {
    let font = createSpriteFont({ fontFamily: "sans", fontSize: FONT_PX }, { cells: "mask", chars: false, label: "text-pose" })
    let layer = createSpriteLayer([font.atlas], { capacity: 32, label: "text-pose" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "text-pose" })
    let run = addText(layer, { font, text: "a b", x: SIZE / 2, y: SIZE / 2, anchor: "middle", anchorY: "middle" })
    return { layer, view, run, texture: view.texture }
  })
  await app.settle()
  await app.frame()
  // Centered on the view: ink on both sides of the middle, none in a
  // corner.
  expect(inkIn(view.texture, 0, 0, SIZE / 2, SIZE)).toBeGreaterThan(0)
  expect(inkIn(view.texture, SIZE / 2, 0, SIZE, SIZE)).toBeGreaterThan(0)
  expect(inkIn(view.texture, 0, 0, 16, 16)).toBe(0)
  // Moving the run moves its group; the sprites keep their local places.
  let local = run.sprites.map(s => [s._x, s._y])
  setText(run, { x: 16, y: 16, anchor: "start", anchorY: "top" })
  expect(run.group._x).toBe(16)
  expect(run.sprites.map(s => [s._x, s._y])).not.toEqual(local)
  await app.frame()
  expect(inkIn(view.texture, 16, 16, 16 + run.width, 16 + run.height)).toBeGreaterThan(0)
  // Wrapping at the first word's width makes two lines.
  setText(run, { maxWidth: run.width / 2 + 1 })
  expect(run.lines).toBe(2)
})

test("an sdf atlas decodes its field: a sharp edge at 2x and an outline ring", async app => {
  let { view, layer, sprite } = await mounted(app, () => {
    let atlas = createAtlas(disc(), { label: "field" })
    atlas.sdf = { range: RANGE }
    let layer = createSpriteLayer([atlas], { capacity: 4, label: "sdf" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "sdf" })
    let sprite = addSprite(layer, { x: SIZE / 2, y: SIZE / 2, w: FIELD * ZOOM, h: FIELD * ZOOM, frame: fullFrame(atlas), tint: [0, 0, 1, 1] })
    return { layer, view, sprite, texture: view.texture }
  })
  await app.frame()
  let c = SIZE / 2
  let edge = RADIUS * ZOOM
  // Inside: the tint, opaque. Outside: nothing. Each a few pixels from the
  // edge, where a bitmap drawn at 2x would still be blurred.
  expect(pixel(view.texture, c, c)).toEqual([0, 0, 255, 255])
  expect(pixel(view.texture, c + edge - EDGE_MARGIN, c)[3]).toBeGreaterThanOrEqual(255 - CHANNEL_SLACK)
  expect(pixel(view.texture, c + edge + EDGE_MARGIN, c)[3]).toBeLessThanOrEqual(CHANNEL_SLACK)
  // The transition spans about one screen pixel: the two pixels straddling
  // the edge are partial, the next ones out are not.
  let across = [-1, 0, 1].map(d => pixel(view.texture, c + edge + d, c)[3]!)
  expect(across.some(a => a > CHANNEL_SLACK && a < 255 - CHANNEL_SLACK)).toBe(true)
  // An outline in red, OUTLINE_PX WORLD pixels wide (the camera is at
  // zoom 1, so as many screen pixels; the sprite's own magnification of
  // its cell does not widen it): a ring outside the disc, blue inside it
  // still.
  layer._outline(sprite, 1, 0, 0, OUTLINE_PX)
  await app.frame()
  let ring = pixel(view.texture, c + edge + OUTLINE_PX / 2, c)
  expect(ring[0]).toBeGreaterThanOrEqual(255 - CHANNEL_SLACK)
  expect(ring[2]).toBeLessThanOrEqual(CHANNEL_SLACK)
  expect(ring[3]).toBeGreaterThanOrEqual(255 - CHANNEL_SLACK)
  expect(pixel(view.texture, c, c)).toEqual([0, 0, 255, 255])
  expect(pixel(view.texture, c + edge + OUTLINE_PX + EDGE_MARGIN, c)[3]).toBeLessThanOrEqual(CHANNEL_SLACK)
  // A colour atlas layer still draws as before beside it: the field as
  // plain texels would be a grey disc, which is not what an sdf layer
  // shows.
  expect(layer.atlases[0]!.sdf).toEqual({ range: RANGE })
})

test("<Text2d> is a run under props: text and pose follow signals, the run dies with the component", async app => {
  let [text, setTextValue] = createSignal("ab")
  let [x, setX] = createSignal(10)
  let [shown, setShown] = createSignal(true)
  let run!: TextRun
  await app.mount(() => {
    let font = createSpriteFont({ fontFamily: "sans", fontSize: FONT_PX }, { cells: "mask", chars: false, label: "text2d" })
    return (
      <SpriteLayer atlases={[font.atlas]} width={SIZE} height={SIZE} clearColor={[0, 0, 0, 0]}>
        <Show when={shown()}>
          <Text2d ref={r => (run = r)} font={font} text={text()} x={x()} y={10} />
        </Show>
      </SpriteLayer>
    )
  })
  expect(run.sprites.length).toBe(2)
  expect(run.group._x).toBe(10)
  setTextValue("abc")
  setX(20)
  await app.frame()
  expect(run.sprites.length).toBe(3)
  expect(run.text).toBe("abc")
  expect(run.group._x).toBe(20)
  let layer = run.layer
  setShown(false)
  await app.frame()
  expect(run.group.layer).toBe(null)
  expect(layer.count).toBe(0)
})

test("<SpriteLayer layer> adopts an imperative layer: its children populate it, and the component's end is not the layer's", async app => {
  let [shown, setShown] = createSignal(true)
  let run!: TextRun
  let font!: SpriteFont
  let layer!: SpriteLayerHandle
  await app.mount(() => {
    font = createSpriteFont({ fontFamily: "sans", fontSize: FONT_PX }, { cells: "mask", chars: false, label: "adopt" })
    layer = createSpriteLayer([font.atlas], { capacity: 32, label: "adopt" })
    return (
      <view>
        <Show when={shown()}>
          <SpriteLayer layer={layer} output={false}>
            <Text2d ref={r => (run = r)} font={font} text="ab" x={10} y={10} />
          </SpriteLayer>
        </Show>
      </view>
    )
  })
  expect(run.layer).toBe(layer)
  expect(layer.count).toBe(2)
  setShown(false)
  await app.frame()
  expect(run.group.layer).toBe(null)
  expect(layer.count).toBe(0)
  // Its maker's layer still: usable after the component is gone.
  addText(layer, { font, text: "c", x: 0, y: 0 })
  expect(layer.count).toBe(1)
})

test("addText refuses a layer that does not declare the font's atlas", async app => {
  await mounted(app, () => {
    let font = createSpriteFont({ fontFamily: "sans", fontSize: FONT_PX }, { cells: "mask", chars: false, label: "text-refuse" })
    let other = createAtlas({ data: new Uint8Array([255, 255, 255, 255]), width: 1, height: 1 }, { label: "other" })
    let layer: SpriteLayerHandle = createSpriteLayer([other], { capacity: 4, label: "text-refuse" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "text-refuse" })
    expect(() => addText(layer, { font, text: "x" })).toThrow()
    return { texture: view.texture }
  })
})
