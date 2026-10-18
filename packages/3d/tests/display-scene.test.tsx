// A blendSpace "display" scene has no resolve stage (it blends straight
// into the displayable rgba8 target), so the knobs that live on the
// resolve refuse by name - except turning them OFF, which is already the
// state: `setBloom(null)`, `setExposure(1)` and `setToneMapping("none")`
// are no-ops there. The Scene component mirrors absent props as exactly
// those values whenever its deferred prop effects run (the splat probe's
// rung remount made them run on mount, and on the Android client they
// run on every mount; a display scene threw there until 2026-10-18), so
// a display scene with none of those props must mount and paint wherever
// it is mounted.

import { test, expect } from "@solidrt/test"
import type { RefLocator } from "@solidrt/test"
import { createMemo, Loading, Show } from "@solidrt/core"
import { createScene, Scene } from "../src/index.ts"
import type { SceneHandle } from "../src/index.ts"

const SIZE = 32
const BLACK = [0, 0, 0, 255]

test("a display scene takes the resolve knobs' off values as no-ops and refuses the rest by name", async () => {
  let scene = createScene(SIZE, SIZE, { blendSpace: "display", label: "display-knobs" })
  // The Scene component mirrors absent props as these very values.
  expect(() => scene.setBloom(null)).not.toThrow()
  expect(() => scene.setExposure(1)).not.toThrow()
  expect(() => scene.setToneMapping("none")).not.toThrow()
  expect(() => scene.setBloom({ threshold: 1 })).toThrow('blendSpace "display"')
  expect(() => scene.setExposure(2)).toThrow('blendSpace "display"')
  expect(() => scene.setToneMapping("aces")).toThrow('blendSpace "display"')
  scene.dispose()
})

test("the Scene component mounts a display scene with no bloom prop and paints it", async app => {
  // Mounted under an async boundary, the way a viewer mounts a scene
  // once its data has loaded.
  let leaf: RefLocator = app.ref()
  let scene!: SceneHandle
  await app.mount(() => {
    let loaded = createMemo(async () => {
      await Promise.resolve()
      return { label: "display-mount" }
    })
    return (
      <view ref={leaf} width={SIZE} height={SIZE}>
        <Loading fallback={<text>loading</text>}>
          <Show when={loaded()} keyed>
            {data => <Scene width={SIZE} height={SIZE} blendSpace="display" clearColor={[0, 0, 0, 1]} label={data.label} ref={s => (scene = s)} />}
          </Show>
        </Loading>
      </view>
    )
  })
  await app.settle()
  expect(scene.size()).toEqual({ width: SIZE, height: SIZE })
  let pixels = await app.painted(leaf)
  let i = ((SIZE / 2) * SIZE + SIZE / 2) * 4
  expect([pixels.data[i], pixels.data[i + 1], pixels.data[i + 2], pixels.data[i + 3]]).toEqual(BLACK)
})
