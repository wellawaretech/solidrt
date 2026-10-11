// A <Camera2d> at rest follows its view's layout: a fill-mode view is
// created at a placeholder size and takes its box at the first layout, and
// the camera, created before that, re-reads the viewport when the layout
// runs instead of at its next move. With the camera on the world origin,
// the pivot (the viewport center) puts the origin mid-view, so a sprite
// there paints at the center and nothing paints in the corner. Layout and
// the view's pixels are GPU-side state, so this is an app test.

import { test, expect } from "@solidrt/test"
import { addSprite, Camera2d, createAtlas, SpriteLayer } from "../src/index.ts"
import type { Camera2d as Camera2dHandle } from "../src/camera2d.ts"

const WIDTH = 160
const HEIGHT = 120
// The sprite on the origin, world units (zoom 1: view pixels).
const SPRITE = 8
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

test("a resting camera's pivot follows the fill view's first layout", async app => {
  let cam!: Camera2dHandle
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "rest-atlas" })
    return (
      <view width={WIDTH} height={HEIGHT}>
        <SpriteLayer atlases={[atlas]} label="rest" ref={layer => addSprite(layer, { x: 0, y: 0, w: SPRITE, h: SPRITE })}>
          <Camera2d x={0} y={0} zoom={1} ref={c => (cam = c)} />
        </SpriteLayer>
      </view>
    )
  })
  await app.settle()
  let leaf = app.find({ kind: "texture" })
  expect([leaf.box.width, leaf.box.height]).toEqual([WIDTH, HEIGHT])
  expect(cam.viewRect()).toEqual({ x: -WIDTH / 2, y: -HEIGHT / 2, width: WIDTH, height: HEIGHT })
  expect(leaf.pixel(WIDTH / 2, HEIGHT / 2)[3]).toBe(255)
  expect(leaf.pixel(1, 1)[3]).toBe(0)
})
