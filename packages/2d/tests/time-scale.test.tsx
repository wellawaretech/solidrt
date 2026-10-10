// The time scale contract (AGENTS.md Retargeted motion and Frame
// animation) pinned against a running layer: a frozen layer holds a
// sprite's transition where it is, asks for no frames (`settle` resolves)
// and resumes from there; a group declaring its own rate keeps running
// inside it; a frame animation given the layer as its clock holds with it,
// one without keeps stepping. GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { TextureId } from "@solidrt/core/gpu"
import { addGroup, addSprite, createAnimation, createAtlas, createSpriteLayer, fullFrame, setGroupTimeScale, setSprite, setSpriteTransition, timeRate, worldPosition } from "../src/index.ts"
import type { Sprite, SpriteGroup, SpriteLayer } from "../src/layer.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// A linear tween over a second to x = 100: app time in ms maps to x / 10.
const DURATION = 1000
const TARGET_X = 100
// Frames land on the 60 fps grid and the mount frame only anchors the
// clock, so a read is within a frame and a half of its time.
const SLACK = (TARGET_X / 60) * 1.5
// The flipbook: four frames at 10 fps, one every 100 ms.
const FLIP_FPS = 10
const FLIP_FRAMES = 4

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={SIZE} height={SIZE} />
  })
  return built
}

function world() {
  let atlas = createAtlas(ATLAS, { label: "time-scale-atlas" })
  let layer = createSpriteLayer([atlas], { capacity: 8, label: "time-scale" })
  let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "time-scale" })
  return { atlas, layer, view, texture: view.texture }
}

/** A sprite under `parent` whose position tweens linearly, sent to x = 100. */
function mover(layer: SpriteLayer, parent: SpriteGroup | null = null): Sprite {
  let sprite = addSprite(layer, { x: 0, y: 0, w: 4, h: 4, parent })
  setSpriteTransition(sprite, { position: { duration: DURATION, curve: "linear" } })
  setSprite(sprite, { x: TARGET_X })
  return sprite
}

let x = (sprite: Sprite) => worldPosition(sprite)![0]

test("a frozen layer holds its motion, idles, and resumes from where it was", async app => {
  let { layer } = await mounted(app, world)
  let sprite = mover(layer)
  await app.advance(DURATION / 2)
  expect(Math.abs(x(sprite) - TARGET_X / 2)).toBeLessThan(SLACK)

  layer.setTimeScale(0)
  expect(layer.timeRate()).toBe(0)
  expect(timeRate(sprite)).toBe(0)
  let held = x(sprite)
  await app.advance(DURATION)
  expect(x(sprite)).toBe(held)
  // A frozen track is not frame demand: the app is at rest.
  await app.settle({ maxMs: 100 })
  expect(x(sprite)).toBe(held)

  layer.setTimeScale(null)
  await app.advance(DURATION / 4)
  expect(Math.abs(x(sprite) - (TARGET_X * 3) / 4)).toBeLessThan(SLACK)
  await app.settle()
  expect(x(sprite)).toBe(TARGET_X)
})

test("a group declaring its own rate keeps running inside a frozen layer", async app => {
  let { layer } = await mounted(app, world)
  let frozen = mover(layer)
  let preview = addGroup(layer)
  setGroupTimeScale(preview, 1)
  let spinning = mover(layer, preview)
  layer.setTimeScale(0)
  expect(timeRate(frozen)).toBe(0)
  expect(timeRate(spinning)).toBe(1)
  await app.advance(DURATION / 2)
  expect(x(frozen)).toBe(0)
  expect(Math.abs(x(spinning) - TARGET_X / 2)).toBeLessThan(SLACK)
})

test("a frame animation clocked on the layer holds with it; one on app time keeps stepping", async app => {
  let { atlas, layer } = await mounted(app, world)
  let frames = Array.from({ length: FLIP_FRAMES }, () => fullFrame(atlas))
  let clocked = createAnimation(frames, FLIP_FPS, { clock: layer })
  let free = createAnimation(frames, FLIP_FPS)
  clocked.add(addSprite(layer, { x: 10, y: 10, w: 4, h: 4 }))
  free.add(addSprite(layer, { x: 20, y: 20, w: 4, h: 4 }))
  // The first frame anchors the clocks; the rest of the 250 ms steps two
  // frames in.
  await app.advance(250)
  expect(clocked.frame()).toBe(2)
  expect(free.frame()).toBe(2)
  layer.setTimeScale(0)
  await app.advance(500)
  expect(clocked.frame()).toBe(2)
  expect(free.frame()).toBe((2 + 5) % FLIP_FRAMES)
  layer.setTimeScale(null)
  await app.advance(100)
  expect(clocked.frame()).toBe(3)
})
