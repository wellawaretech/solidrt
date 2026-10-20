// A component's `ref` runs outside any owner, like an element's: a signal
// setter passed straight in writes without tripping Solid's owned-scope
// write guard, and the handle is held before the mount frame lands.
// Pinned on <Scene>, which delivers through core's callRef like every 3d
// and 2d component; a write that tripped the guard would throw the app
// into the root error window. The same test one dimension down is
// packages/2d/tests/ref-setter.test.tsx.

import { test, expect } from "@solidrt/test"
import { createSignal } from "@solidrt/core"
import { Scene } from "../src/index.ts"
import type { SceneHandle } from "../src/index.ts"

const SIZE = 64

test("a signal setter as a component ref", async app => {
  let [scene, setScene] = createSignal<SceneHandle>()
  await app.mount(() => <Scene width={SIZE} height={SIZE} ref={setScene} label="ref-setter" />)
  expect(scene()).not.toBe(undefined)
  expect(scene()!.pick(SIZE / 2, SIZE / 2)).toEqual([])
})
