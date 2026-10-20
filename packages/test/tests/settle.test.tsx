// app.settle(): the app run until it is at rest. Work in flight is waited
// for with no app time passing, a demanded frame and a due timer are run,
// and what never comes to rest fails at the cap with its name. Behind an
// app that never rests, settle({ frames: false }) waits for its work alone
// and app.until() for a condition.

import { serve } from "flux:http"
import { test, expect } from "../src/index.ts"
import { createSignal, onFrame } from "@solidrt/core"
import { isolate } from "flux:isolate"

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

test("settle waits for each step of an isolate stream the app iterates", async app => {
  let [last, setLast] = createSignal(0)
  let root = await app.mount(() => <text>{`count ${last()}`}</text>)
  let side = isolate<{ count(n: number): AsyncGenerator<number> }>("fixtures/counting")
  let run = async () => {
    for await (let n of side.count(3)) setLast(n)
    side.terminate()
  }
  run()
  await app.settle()
  expect(root.find({ text: "count 3" }).visible).toBe(true)
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

// A frame loop that never stops, as a loading screen's spinner runs one.
function Spinner() {
  let [angle, setAngle] = createSignal(0)
  onFrame(() => setAngle(angle() + 1))
  return <text>{String(angle())}</text>
}

test("settle({ frames: false }) waits for a fetch behind a frame loop, and ends though frames are still demanded", async app => {
  let server = serve({ port: 0, host: "127.0.0.1", fetch: () => new Response("loaded") })
  let [status, setStatus] = createSignal("loading")
  let root = await app.mount(() => (
    <view>
      <Spinner />
      <text>{status()}</text>
    </view>
  ))
  let load = async () => {
    let response = await fetch(`http://127.0.0.1:${server.port}/`)
    setStatus(await response.text())
  }
  load()
  let before = app.time
  await app.settle({ frames: false })
  expect(root.find({ text: "loaded" }).exists).toBe(true)
  // The wait on the fetch passed no app time: one frame after the landing.
  expect(app.time - before).toBeLessThan(50)
  server.close()
})

test("settle({ frames: false }) follows work a landing starts, and an app at rest returns at once", async app => {
  let server = serve({ port: 0, host: "127.0.0.1", fetch: (req) => new Response(req.url.slice(req.url.lastIndexOf("/"))) })
  let [first, setFirst] = createSignal("")
  let [second, setSecond] = createSignal("")
  await app.mount(() => (
    <view>
      <Spinner />
      <text>{first() + second()}</text>
    </view>
  ))
  // The second load starts only on the frame after the first lands, the
  // way a mounted child starts its own loads.
  let load = async () => {
    let response = await fetch(`http://127.0.0.1:${server.port}/one`)
    setFirst(await response.text())
    let stop = onFrame(async () => {
      stop()
      let more = await fetch(`http://127.0.0.1:${server.port}/two`)
      setSecond(await more.text())
    })
  }
  load()
  await app.settle({ frames: false })
  expect(app.find({ text: "/one/two" }).exists).toBe(true)
  server.close()
})

test("settle({ frames: false }) on an app at rest runs no frame", async app => {
  await app.mount(() => <text>still</text>)
  await app.settle()
  let before = app.time
  await app.settle({ frames: false })
  expect(app.time).toBe(before)
})

test("settle({ frames: false }) fails at the cap when work keeps starting, naming that and not the frames", async app => {
  let server = serve({ port: 0, host: "127.0.0.1", fetch: () => new Response("tick") })
  let [polls, setPolls] = createSignal(0)
  // A poll per frame: every frame starts work, so the work is never done.
  function Poller() {
    onFrame(() => {
      fetch(`http://127.0.0.1:${server.port}/`).then(() => setPolls(polls() + 1))
    })
    return <text>{String(polls())}</text>
  }
  await app.mount(() => <Poller />)
  let message = await failure(() => app.settle({ frames: false, maxMs: 200 }))
  expect(message).toContain("work did not finish within 200 ms")
  expect(message).toContain("work kept starting")
  expect(message).not.toContain("onFrame")
  expect(polls()).toBeGreaterThan(5)
  server.close()
})

test("until runs frames until the condition holds, and waits out work in flight with no app time", async app => {
  let [step, setStep] = createSignal("start")
  await app.mount(() => (
    <view>
      <Spinner />
      <text>{step()}</text>
    </view>
  ))
  setTimeout(() => setStep("late"), 300)
  await app.until(() => app.find({ text: "late" }).exists)
  expect(app.time).toBeGreaterThan(290)
  expect(app.time).toBeLessThan(360)

  let server = serve({ port: 0, host: "127.0.0.1", fetch: () => new Response("loaded") })
  let load = async () => {
    let response = await fetch(`http://127.0.0.1:${server.port}/`)
    setStep(await response.text())
  }
  load()
  let before = app.time
  await app.until(() => step() === "loaded")
  expect(app.time - before).toBeLessThan(50)
  server.close()
})

test("until fails at the cap saying what the app was doing, and with the condition's own error", async app => {
  await app.mount(() => <Spinner />)
  let message = await failure(() => app.until(() => false, { maxMs: 100 }))
  expect(message).toContain("did not hold within 100 ms")
  expect(message).toContain("onFrame")
  let thrown = await failure(() =>
    app.until(() => {
      throw new Error("the read broke")
    }),
  )
  expect(thrown).toBe("the read broke")
})
