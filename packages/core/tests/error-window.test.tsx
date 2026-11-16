// The root error boundary on a client: a user effect that throws replaces
// the app's window with the error window, Reset brings the same window back
// with its state intact (the boundary keeps the subtree and recomputes only
// the failed computation), and a second throw after the reset shows the
// error window again.

import { test, expect } from "@solidrt/test"
import { createEffect, createSignal } from "@solidrt/core"

// Whether the effect throws on its next run: plain, not reactive, so a reset
// recomputes the effect without it throwing again.
let armed = false

function App() {
  let [count, setCount] = createSignal(0)
  let [tick, setTick] = createSignal(0)
  createEffect(
    () => tick(),
    () => {
      if (armed) throw new Error("Effect failed on purpose")
    },
  )
  return (
    <view flexDirection="column" gap={10} padding={20}>
      <text>{`Count: ${count()}`}</text>
      <view label="increment" width={120} height={40} onPointerDown={() => setCount(count() + 1)}>
        <text>Increment</text>
      </view>
      <view label="break" width={120} height={40} onPointerDown={() => {
        armed = true
        setTick(tick() + 1)
      }}>
        <text>Break</text>
      </view>
    </view>
  )
}

test("reset brings the app's window back with its state, and a second error shows the error window again", async app => {
  let main = await app.mount(() => <App />)
  await app.tap(app.find({ text: "Increment" }))
  await app.tap(app.find({ text: "Increment" }))
  expect(app.find({ text: "Count: 2" }).visible).toBe(true)

  await app.tap(app.find({ text: "Break" }))
  await app.settle()
  expect(app.find({ text: "Effect failed on purpose" }).visible).toBe(true)
  expect(main.visible).toBe(false)

  armed = false
  await app.tap(app.find({ text: "Reset" }))
  await app.settle()
  expect(app.find({ text: "Reset" }).exists).toBe(false)
  expect(main.visible).toBe(true)
  expect(main.find({ text: "Count: 2" }).visible).toBe(true)
  await app.tap(main.find({ text: "Increment" }))
  expect(main.find({ text: "Count: 3" }).visible).toBe(true)

  await app.tap(main.find({ text: "Break" }))
  await app.settle()
  expect(app.find({ text: "Effect failed on purpose" }).visible).toBe(true)
  expect(main.visible).toBe(false)
})
