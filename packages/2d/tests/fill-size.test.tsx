// A fill-mode <SpriteLayer> adds no size to its ancestors
// (okf/done/fill-leaf-intrinsic-size.md): its leaf is size-contained, so
// once the view has followed the leaf's box, a relayout that shrinks the
// column shrinks the leaf with it, instead of the view's pixel size pinning
// the column at the old height through the flex automatic minimum. Layout
// is the probe: the leaf's and the footer's boxes, read off the tree, and
// compared with the column's so no display scale enters. The same test one
// dimension up is packages/3d/tests/fill-size.test.tsx.

import { test, expect } from "@solidrt/test"
import type { RefLocator } from "@solidrt/test"
import { createSignal } from "@solidrt/core"
import { createAtlas, SpriteLayer } from "../src/index.ts"

const WIDTH = 200
const TALL = 300
const SHORT = 150
const BAR = 40
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

/** The leaf takes what the bars leave, and the footer ends where the column ends. */
function expectFilled(column: RefLocator, header: RefLocator, leaf: { box: { height: number } }, footer: RefLocator) {
  expect(leaf.box.height).toBe(column.box.height - header.box.height - footer.box.height)
  expect(footer.box.y + footer.box.height).toBe(column.box.y + column.box.height)
}

test("a fill sprite layer follows its column down: the footer stays in view", async app => {
  let column = app.ref()
  let header = app.ref()
  let footer = app.ref()
  let [height, setHeight] = createSignal(TALL)
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "fill-size-atlas" })
    return (
      <view ref={column} width={WIDTH} height={height()} flexDirection="column">
        <view ref={header} height={BAR} flexShrink={0} />
        <SpriteLayer atlases={[atlas]} label="fill-size" />
        <view ref={footer} height={BAR} flexShrink={0} />
      </view>
    )
  })
  let leaf = app.find({ kind: "texture" })
  expectFilled(column, header, leaf, footer)
  expect(leaf.box.height).toBeGreaterThan(0)

  // The view now holds the leaf's first box; a shorter column must not be
  // held there by it.
  setHeight(SHORT)
  await app.settle()
  expectFilled(column, header, leaf, footer)
  expect(column.box.height).toBeLessThan(TALL)
})
