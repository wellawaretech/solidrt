// A text is hinted for the scale it rests at from the frame that draws it
// (okf/notes/pixel-snap-motion-findings.md). Below a display scale of 2 a
// text's cells are hinted, and a raster made while the scale moves is not,
// so a moving text's x-height does not pop between rows. Nothing but
// consecutive frames tells a text its scale rests, and an app at rest
// draws none: a text whose scale is not the display's (under a design-size
// fit) must not wait for them, or it changes when something else on the
// screen first animates. Read as pictures: what a text shows once the app
// rests is what it still shows after frames have been drawn around it.

import { test, expect } from "@solidrt/test"
import type { Pixels, TestApp } from "@solidrt/test"
import { createSignal } from "@solidrt/core"

// A design-size fit of 1.5 at display scale 1: the text rasters at 1.5.
const DESIGN: [number, number] = [200, 100]
const FIT = 1.5
const LABEL_PX = 30
// A direct write takes the text's scale past the bound within which a
// raster is kept and stretched, in one frame.
const JUMP = 2

/** How many bytes differ between two pictures of one size. */
function differing(a: Pixels, b: Pixels): number {
  let count = 0
  for (let i = 0; i < a.data.length; i++) if (a.data[i] !== b.data[i]) count++
  return count
}

test("a text shows at rest what it still shows after frames are drawn around it", async app => {
  let [label, setLabel] = createSignal("Hinted n")
  let [zoom, setZoom] = createSignal(1)
  let [dim, setDim] = createSignal(false)
  let fit = app.ref()
  await app.mount(() => (
    <view flex={1}>
      <view ref={fit} width={DESIGN[0] * FIT} height={DESIGN[1] * FIT} designSize={DESIGN}>
        <view scale={zoom()} originX={0} originY={0}>
          <text color="#ffffff" fontSize={LABEL_PX}>
            {label()}
          </text>
        </view>
      </view>
      <view width={10} height={10} opacity={dim() ? 0.5 : 1} transition={{ opacity: "200ms linear" }}>
        <rect detached color="#ffffff" />
      </view>
    </view>
  ))
  // Frames drawn around the text: a fade elsewhere on the screen walks the
  // whole tree for its length, the text included, and changes nothing of it.
  let looks = async (app: TestApp) => {
    setDim(!dim())
    await app.settle()
  }

  // As mounted, in an app that rests after its first frame.
  await app.settle()
  let mounted = fit.pixels()
  await looks(app)
  expect(differing(mounted, fit.pixels())).toBe(0)

  // After its content changed, in an app at rest.
  setLabel("Other m")
  await app.settle()
  let changed = fit.pixels()
  expect(differing(mounted, changed)).toBeGreaterThan(0)
  await looks(app)
  expect(differing(changed, fit.pixels())).toBe(0)

  // After a scale written directly, in one step: the app comes to rest
  // with the text as it stays.
  setZoom(JUMP)
  await app.settle()
  let jumped = fit.pixels()
  expect(differing(changed, jumped)).toBeGreaterThan(0)
  await looks(app)
  expect(differing(jumped, fit.pixels())).toBe(0)
})
