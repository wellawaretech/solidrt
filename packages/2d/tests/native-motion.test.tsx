// Native motion steps before the frame's JS (flux's frame protocol), so a
// transitioned sprite's worldPosition read in onFrame is this frame's
// pose, not the last frame's. GPU state, so this is an app test, and
// frames are stepped so each read is at a known app time.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { TextureId } from "@solidrt/core/gpu"
import { onFrame } from "@solidrt/core"
import { addSprite, createAtlas, createSpriteLayer, setSprite, setSpriteTransition, worldPosition } from "../src/index.ts"
import type { Sprite, SpriteLayer } from "../src/layer.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// A linear tween over a second to x = 100: app time in ms maps to x / 10.
const DURATION = 1000
const TARGET_X = 100
// A pose read is exact to float rounding; a frame-stale one is a whole
// frame off (1.7 at 60 fps), so the tolerance only absorbs rounding.
const EPSILON = 1e-3

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={SIZE} height={SIZE} />
  })
  return built
}

function world() {
  let atlas = createAtlas(ATLAS, { label: "native-motion-atlas" })
  let layer = createSpriteLayer([atlas], { capacity: 8, label: "native-motion" })
  let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "native-motion" })
  return { atlas, layer, view, texture: view.texture }
}

/** A sprite whose position tweens linearly, sent to x = 100. */
function mover(layer: SpriteLayer): Sprite {
  let sprite = addSprite(layer, { x: 0, y: 0, w: 4, h: 4 })
  setSpriteTransition(sprite, { position: { duration: DURATION, curve: "linear" } })
  setSprite(sprite, { x: TARGET_X })
  return sprite
}

let x = (sprite: Sprite) => worldPosition(sprite)![0]

test("a transitioned sprite's worldPosition read in onFrame is this frame's, not the last frame's", async app => {
  let { layer } = await mounted(app, world)
  // The mount frame is the bootstrap, which stamps no clock: a track
  // written before the first stamp is anchored at the first frame that
  // runs (the startup anchor), so one frame first, and the write below
  // is stamped at the clock as of that frame.
  await app.frame()
  let sprite = mover(layer)
  let since = app.time
  let seen: [tick: number, x: number][] = []
  let stop = onFrame(tick => {
    seen.push([tick, x(sprite)])
  })
  await app.advance(DURATION / 4)
  stop()
  expect(seen.length).toBeGreaterThan(3)
  for (let [tick, px] of seen) expect(Math.abs(px - ((tick - since) * TARGET_X) / DURATION)).toBeLessThan(EPSILON)
})
