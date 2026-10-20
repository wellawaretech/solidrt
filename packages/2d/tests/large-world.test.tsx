// Large-world precision one dimension down (views.ts, the layer's render
// origin over flux:spatial setInstanceOrigin): a sprite layer whose
// camera and sprites sit sixteen million pixels out - past f32's whole
// numbers - must render exactly like the same layer at zero and pick the
// same, because the arena keeps the sprites in f64 and the pose records
// and the camera go to the GPU relative to the origin the primary view
// snapped. GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createSpriteLayer } from "../src/index.ts"
import type { SpriteLayerHandle, ViewHandle } from "../src/index.ts"

const SIZE = 64
/** The far layer's offset: 2^24 px, where f32 has no fraction left and
 * a multiple of the origin cell the view snaps on. */
const FAR = 2 ** 24
const SPRITE = 16
const AT = 20
const ATLAS: DecodedImage = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

function build(offset: number, label: string): { layer: SpriteLayerHandle; view: ViewHandle } {
  let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
  let layer = createSpriteLayer([atlas], { capacity: 4, label })
  let view = layer.createView({ width: SIZE, height: SIZE, camera: { x: offset, y: offset }, clearColor: [0, 0, 0, 0], label })
  addSprite(layer, { x: offset + AT, y: offset + AT, w: SPRITE, h: SPRITE })
  addSprite(layer, { x: offset + AT * 2.25, y: offset + AT * 1.5, w: SPRITE / 2, h: SPRITE, rotation: 0.4 })
  return { layer, view }
}

test("a sprite layer sixteen million pixels out renders byte for byte like the one at zero and picks the same", async app => {
  let near!: { layer: SpriteLayerHandle; view: ViewHandle }
  let far!: { layer: SpriteLayerHandle; view: ViewHandle }
  await app.mount(() => {
    near = build(0, "near")
    far = build(FAR, "far")
    return (
      <view>
        <texture src={near.view.texture} width={SIZE} height={SIZE} />
        <texture src={far.view.texture} width={SIZE} height={SIZE} />
      </view>
    )
  })
  await app.frame()
  let pa = readTexture(near.view.texture)
  let pb = readTexture(far.view.texture)
  expect(pb.width).toBe(pa.width)
  let differing = 0
  let painted = 0
  for (let i = 0; i < pa.data.length; i++) {
    if (pa.data[i] !== pb.data[i]) differing++
    if (i % 4 === 3 && pa.data[i]! > 0) painted++
  }
  expect(painted).toBeGreaterThan(0)
  expect(differing).toBe(0)
  // The pick through the far view's camera finds the sprite under it.
  expect(far.view.pick(AT, AT).length).toBe(1)
  expect(near.view.pick(AT, AT).length).toBe(1)
  expect(far.view.pick(AT + SPRITE, AT + SPRITE).length).toBe(0)
})
