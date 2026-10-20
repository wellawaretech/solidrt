// The frame slice queue (src/slice.ts), headless on flux with a clock of
// its own: a job's steps run one budget of milliseconds per drain, in
// queue order; a job settles when its iterator finishes, synchronously
// through onSettle and later through `done`; a cancelled job's generator
// is closed so its `finally` runs; a step that throws fails the job and
// the next job runs; a drain with work left demands a frame; the budget
// setter validates. Pure-module input only: `sol test packages/core`.

import { test, expect } from "flux:test"
import { drainSlices, installSliceDriver, setSliceBudget, slice, slicesQueued, sliceStats } from "../src/slice.ts"

// A fake clock the steps advance by hand, and the frames demanded.
let now = 0
let demanded = 0
installSliceDriver({ now: () => now, requestFrame: () => demanded++ })
const STEP_MS = 1.5
const BUDGET_MS = 1
function cost(ms: number): void {
  now += ms
}

test("steps run one budget per drain in queue order, and a job settles when its iterator finishes", async () => {
  setSliceBudget(BUDGET_MS)
  let ran: string[] = []
  let settled: string[] = []
  let a = slice(
    (function* () {
      for (let i = 0; i < 3; i++) {
        cost(STEP_MS)
        ran.push("a" + i)
        yield
      }
    })(),
    { label: "a", onSettle: state => settled.push("a:" + state) },
  )
  let b = slice(
    (function* () {
      ran.push("b0")
      yield
      ran.push("b1")
    })(),
    { label: "b" },
  )
  expect(a.state).toBe("queued")
  expect(slicesQueued()).toBe(2)
  expect(demanded).toBe(2)
  drainSlices()
  expect(ran).toEqual(["a0"])
  expect(sliceStats().ms).toBe(STEP_MS)
  expect(demanded).toBe(3)
  drainSlices()
  expect(ran).toEqual(["a0", "a1"])
  drainSlices()
  expect(ran).toEqual(["a0", "a1", "a2"])
  expect(a.state).toBe("queued")
  // The fourth next() finishes `a` at no cost; `b` follows in the same
  // drain, its steps free.
  drainSlices()
  expect(a.state).toBe("done")
  expect(settled).toEqual(["a:done"])
  expect(b.state).toBe("done")
  expect(ran).toEqual(["a0", "a1", "a2", "b0", "b1"])
  expect(slicesQueued()).toBe(0)
  let before = demanded
  drainSlices()
  expect(demanded).toBe(before)
  await a.done
  await b.done
  expect(sliceStats().steps >= 6).toBe(true)
})

test("a cancelled job closes its generator so its finally runs; a throwing step fails the job and the next one runs", async () => {
  setSliceBudget(BUDGET_MS)
  let closed = false
  let ran: string[] = []
  let a = slice(
    (function* () {
      try {
        cost(STEP_MS)
        ran.push("a0")
        yield
        ran.push("a1")
        yield
      } finally {
        closed = true
      }
    })(),
  )
  let settled: string[] = []
  let b = slice(
    (function* () {
      ran.push("b0")
      yield
      throw new Error("boom")
    })(),
    { onSettle: state => settled.push(state) },
  )
  let c = slice(
    (function* () {
      ran.push("c0")
    })(),
  )
  drainSlices()
  expect(ran).toEqual(["a0"])
  a.cancel()
  expect(closed).toBe(true)
  expect(a.state).toBe("cancelled")
  a.cancel()
  await a.done
  let failed: unknown = null
  b.done.catch(e => (failed = e))
  drainSlices()
  expect(ran).toEqual(["a0", "b0", "c0"])
  expect(b.state).toBe("failed")
  expect(settled).toEqual(["failed"])
  expect(c.state).toBe("done")
  await c.done
  await Promise.resolve()
  expect((failed as Error).message).toBe("boom")
})

test("an iterable is accepted, Infinity runs every step at once, and the budget validates", () => {
  expect(() => setSliceBudget(0)).toThrow()
  expect(() => setSliceBudget(-1)).toThrow()
  expect(() => setSliceBudget(NaN)).toThrow()
  setSliceBudget(Infinity)
  let ran = 0
  let job = slice({
    *[Symbol.iterator]() {
      for (let i = 0; i < 5; i++) {
        cost(STEP_MS)
        ran++
        yield
      }
    },
  })
  drainSlices()
  expect(ran).toBe(5)
  expect(job.state).toBe("done")
})
