// The output leaf contract, View2d fill and the maxPixels budget
// (okf/done/scene-fill-extensions.md), one dimension down from
// packages/3d/tests/fill-knobs.test.tsx: a fill view sizes to the leaf it
// follows - the built-in one, or the `output` leaf that refs itself - a
// `<View2d>` without sizes fills its box by the same rule, and `maxPixels`
// bounds the auto-picked oversample below what the on-screen scale asks.
// View pixels are the leaf's layout units, so the expectations are boxes
// read off the tree and no display scale enters; the budget case
// measures through a designSize fit, where the leaf is shown at four times
// its layout size.

import { test, expect } from "@solidrt/test"
import { createEffect, createSignal, displayScale, pct } from "@solidrt/core"
import { createAtlas, SpriteLayer, useSpriteLayer, View2d } from "../src/index.ts"
import type { OutputLeaf, ViewHandle } from "../src/index.ts"

const WIDTH = 200
const HEIGHT = 100
const SHORT = 60
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// The budget case: a 100 x 100 design shown in a 400 x 400 box asks for
// four texels per view pixel; a budget of 200 x 200 texels allows two.
const DESIGN = 100
const SHOWN = 400
const BUDGET = (DESIGN * 2) * (DESIGN * 2)
const CAPPED = 2

test("an output leaf that refs itself is followed by the view", async app => {
  let view!: ViewHandle
  let leaf!: OutputLeaf
  let viewportSize!: () => { width: number; height: number }
  let seen: { width: number; height: number }[] = []
  let [height, setHeight] = createSignal(HEIGHT)
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "knobs-output-atlas" })
    return (
      <view width={WIDTH} height={height()}>
        <SpriteLayer
          atlases={[atlas]}
          label="knobs-output"
          viewRef={v => (view = v)}
          output={(texture, l) => {
            leaf = l
            viewportSize = useSpriteLayer().viewportSize
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
    )
  })
  await app.settle()
  let node = app.find({ kind: "texture" })
  expect([node.box.width, node.box.height]).toEqual([WIDTH, HEIGHT])
  expect(view.size()).toEqual({ width: WIDTH, height: HEIGHT })
  expect(typeof leaf.handlers.onPointerDown).toBe("function")
  // The target is the view times its oversample; the device size is the
  // leaf's box at the display scale.
  let texels = () => ({ width: WIDTH * view.oversample, height: view.size().height * view.oversample })
  expect(leaf.size()).toEqual(texels())
  expect(seen[seen.length - 1]).toEqual(texels())
  expect(leaf.deviceSize()).toEqual({ width: Math.round(WIDTH * displayScale()), height: Math.round(HEIGHT * displayScale()) })
  // The children's reactive read is the view's own size, in view pixels.
  expect(viewportSize()).toEqual({ width: WIDTH, height: HEIGHT })

  setHeight(SHORT)
  await app.settle()
  expect(node.box.height).toBe(SHORT)
  expect(view.size()).toEqual({ width: WIDTH, height: SHORT })
  expect(seen[seen.length - 1]).toEqual(texels())
  expect(viewportSize()).toEqual({ width: WIDTH, height: SHORT })
})

test("a View2d without sizes fills its box like the layer's own view", async app => {
  let view!: ViewHandle
  let pane = app.ref()
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "knobs-view-atlas" })
    return (
      <view width={WIDTH} height={HEIGHT} flexDirection="row">
        <SpriteLayer atlases={[atlas]} label="knobs-views" output={false}>
          <view ref={pane} width={WIDTH / 2} height={HEIGHT}>
            <View2d label="knobs-view" ref={v => (view = v)} />
          </view>
        </SpriteLayer>
      </view>
    )
  })
  await app.settle()
  // The view's leaf fills the pane, so the pane's box is the leaf's.
  expect(pane.box.width).toBe(WIDTH / 2)
  expect(view.size()).toEqual({ width: WIDTH / 2, height: HEIGHT })
})

test("maxPixels bounds the auto-picked oversample", async app => {
  let view!: ViewHandle
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "knobs-budget-atlas" })
    return (
      <view width={SHOWN} height={SHOWN} designSize={[DESIGN, DESIGN]}>
        <SpriteLayer atlases={[atlas]} label="knobs-budget" maxPixels={BUDGET} viewRef={v => (view = v)} />
      </view>
    )
  })
  await app.settle()
  expect(view.size()).toEqual({ width: DESIGN, height: DESIGN })
  // The fit alone asks for ceil(4 x displayScale) texels per view pixel;
  // the budget holds the pick at two.
  expect(Math.ceil((SHOWN / DESIGN) * displayScale())).toBeGreaterThan(CAPPED)
  expect(view.oversample).toBe(CAPPED)
})
