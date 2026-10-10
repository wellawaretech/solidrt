// A component's `ref` runs outside any owner, like an element's: a signal
// setter passed straight in writes without tripping Solid's owned-scope
// write guard, and the handle is held before the mount frame lands.
// Pinned on <SpriteLayer>, which delivers through core's callRef like every
// 2d and 3d component; a write that tripped the guard would throw the app
// into the root error window.

import { test, expect } from "@solidrt/test"
import { createSignal } from "@solidrt/core"
import { createAtlas, SpriteLayer } from "../src/index.ts"
import type { SpriteLayerHandle } from "../src/index.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

test("a signal setter as a component ref", async app => {
  let [layer, setLayer] = createSignal<SpriteLayerHandle>()
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "ref-setter-atlas" })
    return <SpriteLayer atlases={[atlas]} width={SIZE} height={SIZE} ref={setLayer} label="ref-setter" />
  })
  expect(layer()).not.toBe(undefined)
  expect(layer()!.pickRect(0, 0, SIZE, SIZE)).toEqual([])
})
