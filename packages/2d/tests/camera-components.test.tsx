// The camera component as an app wires it: `<Camera2d>` driving a sprite
// layer's view from an input map the app bound to the layer's pointer feed
// and to the keyboard. The motion itself is tested in camera2d.test.ts on
// the flux binary; what is under test here is the glue between a real
// drag or key and the pose, which exists only under the app runtime. The
// same test one dimension up is
// packages/3d/tests/camera-components.test.tsx.

import { test, expect } from "@solidrt/test"
import { createInputMap, createPointerFeed, keyboard, pct } from "@solidrt/core"
import { Camera2d, camera2dActions, camera2dBindings, createAtlas, SpriteLayer } from "../src/index.ts"
import type { Camera2dHandle } from "../src/index.ts"

// Decimals a pose component that must not move is compared to.
const DIGITS = 5
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// The scroll's key hold, long enough for the pose to move visibly.
const SCROLL_MS = 500

// The feed and the map register cleanups, so an app creates them in a
// component body; here the mount callback is that body.
test("a drag over the layer pans the camera through the input map", async app => {
  let camera!: Camera2dHandle
  let root = await app.mount(() => {
    let pointer = createPointerFeed()
    let input = createInputMap(camera2dActions)
    input.bind(camera2dBindings({ pointer }))
    let atlas = createAtlas(ATLAS, { label: "camera-components-atlas" })
    return (
      <view width={pct(100)} height={pct(100)}>
        <SpriteLayer atlases={[atlas]} pointer={pointer} label="pan">
          <Camera2d input={input} ref={c => (camera = c)} x={0} y={0} zoom={1} />
        </SpriteLayer>
      </view>
    )
  })
  let before = camera.pose()
  let leaf = root.find({ kind: "texture" })
  let { x, y, width, height } = leaf.box
  await app.drag({ x: x + width * 0.3, y: y + height / 2 }, { x: x + width * 0.7, y: y + height / 2 })
  await app.settle()
  let after = camera.pose()
  // The world follows the finger: a drag to the right shows what lay to
  // the left, so the camera's world x decreases.
  expect(after.x).toBeLessThan(before.x)
  expect(after.y).toBeCloseTo(before.y, DIGITS)
  expect(after.zoom).toBeCloseTo(before.zoom, DIGITS)
  expect(after.rotation).toBeCloseTo(before.rotation, DIGITS)
})

test("a held arrow key scrolls the camera through the input map", async app => {
  let camera!: Camera2dHandle
  await app.mount(() => {
    let input = createInputMap(camera2dActions)
    input.bind(camera2dBindings({ keyboard }))
    let atlas = createAtlas(ATLAS, { label: "camera-components-atlas" })
    return (
      <window onKeyDown={input.handlers.onKeyDown} onKeyUp={input.handlers.onKeyUp} onBlur={input.handlers.onBlur}>
        <view width={pct(100)} height={pct(100)}>
          <SpriteLayer atlases={[atlas]} label="scroll">
            <Camera2d input={input} ref={c => (camera = c)} x={0} y={0} zoom={1} />
          </SpriteLayer>
        </view>
      </window>
    )
  })
  let before = camera.pose()
  await app.key("ArrowRight", { holdMs: SCROLL_MS })
  await app.settle()
  let after = camera.pose()
  // The arrow scrolls the view right, as every map does: the camera's
  // world x increases.
  expect(after.x).toBeGreaterThan(before.x)
  expect(after.y).toBeCloseTo(before.y, DIGITS)
  expect(after.zoom).toBeCloseTo(before.zoom, DIGITS)
})
