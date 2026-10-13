// The fill-mode density knobs and the output leaf contract
// (okf/done/scene-fill-extensions.md): a fill target follows its leaf's
// on-screen box in device pixels, scaled by `renderScale`, held under
// `maxPixels` by one factor on both axes, and an `output` leaf that refs
// itself is followed the same way, its reactive `size` tracking the
// target. The leaf's box read off the tree times the display scale is the
// expectation, so no display scale is assumed. A `<View3d>` fills by the
// same rule. The same test one dimension down is
// packages/2d/tests/fill-knobs.test.tsx.

import { test, expect } from "@solidrt/test"
import { createEffect, createSignal, displayScale, pct } from "@solidrt/core"
import { Scene, useScene, View3d } from "../src/index.ts"
import type { OutputLeaf, SceneHandle, ViewHandle } from "../src/index.ts"

const WIDTH = 200
const HEIGHT = 100
const SHORT = 60
// Four times the pixels: the supersampling scale.
const SCALE = 2
// A quarter of the 200 x 100 leaf's pixels at display scale 1; the cap's
// factor is 1 / displayScale on each axis, so the result is scale-free.
const BUDGET = (WIDTH * HEIGHT) / 4

/** The target a leaf box should produce at `scale`, each axis rounded. */
function expected(box: { width: number; height: number }, scale: number) {
  let s = displayScale() * scale
  return { width: Math.round(box.width * s), height: Math.round(box.height * s) }
}

test("renderScale multiplies the tracked device-pixel size, reactively", async app => {
  let scene!: SceneHandle
  let [scale, setScale] = createSignal(1)
  await app.mount(() => (
    <view width={WIDTH} height={HEIGHT}>
      <Scene label="knobs-scale" renderScale={scale()} ref={s => (scene = s)} />
    </view>
  ))
  await app.settle()
  let leaf = app.find({ kind: "texture" })
  expect(scene.size()).toEqual(expected(leaf.box, 1))

  setScale(SCALE)
  await app.settle()
  expect(scene.size()).toEqual(expected(leaf.box, SCALE))
})

test("maxPixels caps the pixel count with one factor on both axes", async app => {
  let scene!: SceneHandle
  await app.mount(() => (
    <view width={WIDTH} height={HEIGHT}>
      <Scene label="knobs-budget" maxPixels={BUDGET} ref={s => (scene = s)} />
    </view>
  ))
  await app.settle()
  let size = scene.size()
  expect(size.width * size.height).toBeLessThanOrEqual(BUDGET + size.width + size.height)
  expect(size).toEqual({ width: WIDTH / 2, height: HEIGHT / 2 })
})

test("an output leaf that refs itself is followed, and leaf.size tracks the target", async app => {
  let scene!: SceneHandle
  let leaf!: OutputLeaf
  let viewportSize!: () => { width: number; height: number }
  let seen: { width: number; height: number }[] = []
  let [height, setHeight] = createSignal(HEIGHT)
  await app.mount(() => (
    <view width={WIDTH} height={height()}>
      <Scene
        label="knobs-output"
        ref={s => (scene = s)}
        output={(texture, l) => {
          leaf = l
          viewportSize = useScene().viewportSize
          createEffect(
            () => l.size(),
            size => {
              seen.push(size)
            },
          )
          return <texture ref={l.ref} src={texture} width={pct(100)} height={pct(100)} contain="size" />
        }}
      />
    </view>
  ))
  await app.settle()
  let node = app.find({ kind: "texture" })
  expect(node.box.height).toBe(HEIGHT)
  expect(scene.size()).toEqual(expected(node.box, 1))
  expect(leaf.size()).toEqual(scene.size())
  expect(leaf.deviceSize()).toEqual(scene.size())
  expect(seen[seen.length - 1]).toEqual(scene.size())
  // The children's reactive read is the same size.
  expect(viewportSize()).toEqual(scene.size())

  setHeight(SHORT)
  await app.settle()
  expect(node.box.height).toBe(SHORT)
  expect(scene.size()).toEqual(expected(node.box, 1))
  expect(seen[seen.length - 1]).toEqual(scene.size())
})

test("a View3d without sizes fills its box like the scene", async app => {
  let view!: ViewHandle
  let pane = app.ref()
  await app.mount(() => (
    <view width={WIDTH} height={HEIGHT} flexDirection="column">
      <Scene label="knobs-views">
        <view ref={pane} width={WIDTH / 2} height={HEIGHT / 2} flexShrink={0}>
          <View3d label="knobs-view" renderScale={SCALE} ref={v => (view = v)} />
        </view>
      </Scene>
    </view>
  ))
  await app.settle()
  // The view's leaf fills the pane, so the pane's box is the leaf's.
  expect(pane.box.width).toBe(WIDTH / 2)
  expect(view.size()).toEqual(expected(pane.box, SCALE))
})
