// app.settle(): the app run until it is at rest. Work in flight is waited
// for with no app time passing, a demanded frame and a due timer are run,
// and what never comes to rest fails at the cap with its name.

import { serve } from "flux:http"
import { test, expect } from "../src/test.ts"
import { createSignal, onFrame } from "../src/index.ts"

// The error `run` rejects with; fails when it fulfills.
async function failure(run: () => Promise<void>): Promise<string> {
  try {
    await run()
  } catch (err) {
    return String((err as Error).message)
  }
  throw new Error("Expected the call to fail, and it passed")
}

test("settle waits for a fetch and draws what its answer wrote", async app => {
  let server = serve({ port: 0, host: "127.0.0.1", fetch: () => new Response("loaded") })
  let [status, setStatus] = createSignal("idle")
  let load = async () => {
    let response = await fetch(`http://127.0.0.1:${server.port}/`)
    setStatus(await response.text())
  }
  let root = await app.mount(() => (
    <view label="load" width={120} height={40} onPointerDown={load}>
      <text>{status()}</text>
    </view>
  ))
  await app.tap(app.find({ label: "load" }))
  let before = app.time
  await app.settle()
  expect(root.find({ text: "loaded" }).visible).toBe(true)
  // The wait itself passed no app time: only the frame that drew the answer.
  expect(app.time - before).toBeLessThan(50)
  server.close()
})

test("settle on an app at rest runs no frame", async app => {
  await app.mount(() => <text>still</text>)
  await app.settle()
  let before = app.time
  await app.settle()
  expect(app.time).toBe(before)
})

test("settle plays a running transition to its end", async app => {
  let [dim, setDim] = createSignal(false)
  let panel = app.ref()
  await app.mount(() => (
    <view ref={panel} width={100} height={100} opacity={dim() ? 0.25 : 1} transition="200ms linear" onPointerDown={() => setDim(true)} />
  ))
  await app.settle()
  await app.tap(panel)
  let before = app.time
  await app.settle()
  expect(panel.props.opacity).toBe(0.25)
  expect(app.time - before).toBeGreaterThan(150)
  expect(app.time - before).toBeLessThan(300)
})

test("settle fires a timer that is due, and leaves one that is not", async app => {
  let [step, setStep] = createSignal("start")
  await app.mount(() => <text>{step()}</text>)
  setTimeout(() => setStep("deferred"), 0)
  setTimeout(() => setStep("late"), 1000)
  await app.settle()
  expect(app.find({ text: "deferred" }).exists).toBe(true)
  expect(app.time).toBeLessThan(100)
  await app.advance(1000)
  await app.settle()
  expect(app.find({ text: "late" }).exists).toBe(true)
})

test("a frame callback that never stops fails the settle, by name", async app => {
  function Spinner() {
    let [angle, setAngle] = createSignal(0)
    onFrame(() => setAngle(angle() + 1))
    return <text>{String(angle())}</text>
  }
  await app.mount(() => <Spinner />)
  let message = await failure(() => app.settle({ maxMs: 200 }))
  expect(message).toContain("did not come to rest within 200 ms")
  expect(message).toContain("onFrame")
  expect(app.time).toBeGreaterThan(190)
  expect(app.time).toBeLessThan(260)
})

test("a timer that re-arms with no delay fails the settle, by name", async app => {
  let spin = () => {
    setTimeout(spin, 0)
  }
  spin()
  let message = await failure(() => app.settle({ maxMs: 100 }))
  expect(message).toContain("a timer is due at every frame")
})
