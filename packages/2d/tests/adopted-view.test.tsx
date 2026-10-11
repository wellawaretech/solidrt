// An adopted <SpriteLayer layer={...}> makes a view of its own over a layer
// that outlives it, so the component must free that view itself: a layer
// of its own takes its views along when the owner disposes it, an adopted
// one frees nothing of the component's. The view's draw target is the
// texture carrying the component's label, read off the GPU inventory, so
// this is an app test.

import { test, expect } from "@solidrt/test"
import { createSignal, Show } from "@solidrt/core"
import { createAtlas, createSpriteLayer, SpriteLayer } from "../src/index.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// The adopted view's label, and so its target texture's.
const PIP = "pip"

test("an adopted layer's own view is disposed with the component", async app => {
  let [shown, setShown] = createSignal(true)
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "adopt-atlas" })
    let layer = createSpriteLayer([atlas], { label: "adopt" })
    return (
      <view width={SIZE} height={SIZE}>
        <Show when={shown()}>
          <SpriteLayer layer={layer} width={SIZE} height={SIZE} label={PIP} />
        </Show>
      </view>
    )
  })
  // The inventory's lists are untyped records; the length is what counts.
  let targets = () => (app.gpu({ label: PIP }).textures as unknown[]).length
  expect(targets()).toBeGreaterThan(0)
  setShown(false)
  await app.settle()
  expect(targets()).toBe(0)
})
