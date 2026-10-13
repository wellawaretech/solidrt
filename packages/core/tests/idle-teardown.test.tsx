// requestIdleCallback under the stepped host: an idle period is a frame's
// spare time, so a test that steps no frame after registering leaves the
// callback pending when its engine ends. With a `timeout` the runtime arms
// a timer whose native function reaches the idle queue; pending at
// teardown, it must not keep the callback from being finalized ahead of
// the runtime's leak check, which aborts the process. The run completing is
// the assertion.

import { test, expect } from "@solidrt/test"

test("an idle callback with a timeout pending at teardown", async app => {
  await app.mount(() => <view flex={1} />)
  let ran = false
  requestIdleCallback(
    () => {
      ran = true
    },
    { timeout: 60_000 },
  )
  expect(ran).toBe(false)
})
