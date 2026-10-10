// ScrollView's scrollRef hands out the scroll handle the way every ref
// does: during mount, outside any owner, so a signal setter passed straight
// in writes without tripping Solid's owned-scope guard (core's callRef).
// The handle is then the live geometry: the range follows the content's
// overflow, an instant scrollTo lands in the next frame. Layout and
// scroll offsets are GPU-side tree state, so this is an app test.

import { test, expect } from "@solidrt/test"
import { createSignal } from "@solidrt/core"
import type { Scroll } from "@solidrt/core"
import { ScrollView } from "../src/index.ts"

// The viewport and the content it clips: three viewports of overflow.
const VIEWPORT = 100
const CONTENT = 400
const TARGET_Y = 50

test("scrollRef takes a signal setter and hands out the live handle at mount", async app => {
  let [scroll, setScroll] = createSignal<Scroll>()
  await app.mount(() => (
    <ScrollView scrollRef={setScroll} layout={{ width: VIEWPORT, height: VIEWPORT }}>
      <view width={VIEWPORT} height={CONTENT} />
    </ScrollView>
  ))
  expect(scroll()).toBeDefined()
  expect(scroll()!.range()).toEqual({ x: 0, y: CONTENT - VIEWPORT })
  expect(scroll()!.offset()).toEqual({ x: 0, y: 0 })

  scroll()!.scrollTo({ y: TARGET_Y, behavior: "instant" })
  await app.settle()
  expect(scroll()!.offset()).toEqual({ x: 0, y: TARGET_Y })
})
