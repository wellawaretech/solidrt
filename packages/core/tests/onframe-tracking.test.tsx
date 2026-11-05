// The per-frame hooks run outside any computation: the runtime delivers the
// frame from its loop, not from inside a reactive pass, so a callback's reads
// subscribe nothing, wherever the hook was registered. Pinned here because a
// hook registered from an effect under a render insert was once suspected of
// being tracked by that insert; core does not wrap the hooks in untrack.

import { test, expect } from "@solidrt/test"
import { createEffect, createSignal, getObserver, untrack } from "@solidjs/signals"
import { onBeforeRender, onFrame, onLayout } from "@solidrt/core"

test("frame, layout and before-render callbacks run with no observer", async app => {
  let [value, setValue] = createSignal(0)
  let observed: unknown[] = []
  let insertRuns = 0
  let read = () => {
    observed.push(getObserver())
    value()
  }
  let Frame = (props: { children: () => any }) => (
    <view width={8} height={8}>
      {(() => {
        insertRuns++
        return untrack(() => props.children())
      })()}
    </view>
  )
  await app.mount(() => (
    <Frame>
      {() => {
        createEffect(
          () => true,
          () => onFrame(read),
        )
        onLayout(read)
        onBeforeRender(read)
        return <view width={4} height={4} />
      }}
    </Frame>
  ))
  await app.frame()
  setValue(1)
  await app.frame()
  expect(observed.length > 0).toBe(true)
  expect(observed.every(o => o === null)).toBe(true)
  expect(insertRuns).toBe(1)
})
