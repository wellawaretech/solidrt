// Native motion steps before the frame's JS (flux's frame protocol:
// `advance` steps the sprite transitions, then `deliver` hands the frame
// to JS), pinned against a running layer: a transitioned sprite's
// worldPosition read in onFrame is this frame's pose, not the last
// frame's; a sprite added with `from` starts its enter after the frame's
// callbacks, so the late pass and the paint see from, never its added
// pose; and a settle handler runs in the frame's turn ahead of its
// onFrame callbacks. The same contract one dimension up is
// packages/3d/tests/native-motion.test.tsx. GPU state, so this is an app
// test, and frames are stepped so each read is at a known app time.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { TextureId } from "@solidrt/core/gpu"
import { onBeforeRender, onFrame } from "@solidrt/core"
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
function mover(layer: SpriteLayer, duration = DURATION): Sprite {
  let sprite = addSprite(layer, { x: 0, y: 0, w: 4, h: 4 })
  setSpriteTransition(sprite, { position: { duration, curve: "linear" } })
  setSprite(sprite, { x: TARGET_X })
  return sprite
}

let x = (sprite: Sprite) => worldPosition(sprite)![0]

/** Where a linear mover started at `since` is at app time `tick`. */
let expected = (tick: number, since: number) => ((tick - since) * TARGET_X) / DURATION

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
  for (let [tick, px] of seen) expect(Math.abs(px - expected(tick, since))).toBeLessThan(EPSILON)
})

test("a sprite added with from starts after the frame's callbacks: the late pass and the paint see from, never its added pose", async app => {
  let { layer } = await mounted(app, world)
  let inCallbacks: number | null = null
  let inLatePass: number | null = null
  let sprite!: Sprite
  let stopFrame = onFrame(() => {
    if (inCallbacks === null) inCallbacks = x(sprite)
  })
  let stopLate = onBeforeRender(() => {
    if (inLatePass === null) inLatePass = x(sprite)
  })
  // The declaration lands in the tick that added the sprite, so `from`
  // is its enter value.
  sprite = addSprite(layer, { x: TARGET_X, y: 0, w: 4, h: 4 })
  setSpriteTransition(sprite, { position: { duration: DURATION, curve: "linear", from: [-TARGET_X, 0] } })
  await app.frame()
  stopFrame()
  stopLate()
  // The frame's callbacks read what the adding code wrote (Unity's Update
  // after Instantiate); the start pass then snaps to from, so the late
  // pass reads the pose the frame draws, at progress 0.
  expect(inCallbacks).toBe(TARGET_X)
  expect(inLatePass).toBe(-TARGET_X)
  expect(x(sprite)).toBe(-TARGET_X)
  await app.frame()
  // The next frame's step moved it exactly one frame along, from from:
  // the enter travels twice TARGET_X, so one frame is twice a mover's.
  let frameMs = 1000 / 60
  expect(Math.abs(x(sprite) - (-TARGET_X + 2 * expected(frameMs, 0)))).toBeLessThan(EPSILON)
})

test("a settle handler runs in the frame's turn, ahead of its onFrame callbacks", async app => {
  let { layer } = await mounted(app, world)
  let long = mover(layer)
  let short = mover(layer, DURATION / 10)
  let atSettle: number | null = null
  let atFrame: number | null = null
  short.onTransitionEnd = () => {
    atSettle = x(long)
  }
  let stop = onFrame(() => {
    if (atSettle !== null && atFrame === null) atFrame = x(long)
  })
  await app.advance(DURATION / 4)
  stop()
  expect(atSettle).not.toBeNull()
  // The handler fired after the step that moved the long one to this
  // frame's pose, so it read exactly what the frame's callbacks read; a
  // handler run after the previous frame's callbacks would be a frame of
  // progress behind them.
  expect(atFrame).toBe(atSettle)
})
