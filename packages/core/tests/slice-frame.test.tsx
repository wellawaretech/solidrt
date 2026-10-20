// The slice queue in the frame: a job queued outside a frame demands
// one and runs first thing in it, settling in that frame, and a job
// whose steps the frame cannot time (the app clock of a test is frozen,
// so every step is free) drains whole. The pacing itself is the
// headless test's (slice.test.ts).

import { test, expect } from "@solidrt/test"
import { slice, slicesQueued } from "@solidrt/core"

test("a queued job runs first thing in the next frame and settles there", async app => {
  await app.mount(() => <view width={8} height={8} />)
  let ran = 0
  let settledInFrame = false
  let job = slice(
    (function* () {
      ran++
      yield
      ran++
    })(),
    { onSettle: () => (settledInFrame = true) },
  )
  expect(slicesQueued()).toBe(1)
  expect(ran).toBe(0)
  await app.frame()
  expect([ran, slicesQueued(), settledInFrame, job.state]).toEqual([2, 0, true, "done"])
  await job.done
  let stats = app.stats() as { app: { sliceSteps: number; sliceQueued: number } }
  expect(stats.app.sliceQueued).toBe(0)
  expect(stats.app.sliceSteps >= 2).toBe(true)
})
