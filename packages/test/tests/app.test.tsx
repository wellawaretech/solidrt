// The app layer of the test harness, tested with itself: mounting, naming
// nodes (text, label, ref), reading them, and input through the real
// pipeline. What is under test here is @solidrt/test and the runtime
// verbs beneath it, on plain host elements.

import { test, expect } from "../src/index.ts"
import { createSignal, setFocus } from "@solidrt/core"

function Counter() {
  let [count, setCount] = createSignal(0)
  return (
    <view flexDirection="column" gap={10} padding={20}>
      <text fontSize={20}>{String(count())}</text>
      <view label="increment" width={120} height={40} onPointerDown={() => setCount(count() + 1)}>
        <text>Increment</text>
      </view>
    </view>
  )
}

test("mount puts content in a window and returns its locator", async app => {
  let root = await app.mount(() => <Counter />)
  expect(root.record.kind).toBe("window")
  expect(app.time).toBe(0)
  expect(root.find({ text: "Increment" }).visible).toBe(true)
  expect(app.find({ label: "increment" }).box.width).toBe(120)
})

test("a tap goes through the hit test and the tree is current after it", async app => {
  let counter = await app.mount(() => <Counter />)
  expect(counter.find({ text: "0" }).exists).toBe(true)
  await app.tap(counter.find({ text: "Increment" }))
  expect(counter.find({ text: "1" }).exists).toBe(true)
  await app.tap(app.find({ label: "increment" }), { pointerType: "touch" })
  expect(counter.find({ text: "2" }).visible).toBe(true)
})

test("a ref is a locator, and layout reads through it", async app => {
  let main = app.ref()
  await app.mount(() => (
    <view flexDirection="row" width={300} height={50}>
      <view width={100} />
      <view ref={main} flexGrow={1} />
    </view>
  ))
  expect(main.box.width).toBe(200)
  expect(main.box.x).toBe(100)
})

test("find is exact, a pattern matches a part, and a locator is strict", async app => {
  await app.mount(() => (
    <view flexDirection="column">
      <text>Save</text>
      <text>Save as</text>
      <text label="status">Saved 3 files</text>
    </view>
  ))
  expect(app.find({ text: "Save" }).text).toBe("Save")
  expect(app.findAll({ text: /^Save/ }).length).toBe(3)
  expect(app.find({ text: /\d+ files/ }).record.label).toBe("status")
  expect(app.find({ label: "status", kind: "text" }).exists).toBe(true)
  expect(app.find({ text: "save" }).exists).toBe(false)
  expect(() => app.find({ text: /^Save/ }).text).toThrow("3 nodes match")
  expect(() => app.find({ text: "Open" }).text).toThrow('texts there: "Save", "Save as", "Saved 3 files"')
  expect(() => app.find({ name: "x" } as never)).toThrow('unknown query field "name"')
})

test("visible is painted: clipped away, faded out and unmounted are not", async app => {
  let [shown, setShown] = createSignal(true)
  await app.mount(() => (
    <view flexDirection="column" width={200} height={100} overflow="hidden">
      <text>inside</text>
      <view height={200} flexShrink={0} />
      <text>below the fold</text>
      <view opacity={0}>
        <text>faded</text>
      </view>
      {shown() && <text>conditional</text>}
    </view>
  ))
  expect(app.find({ text: "inside" }).visible).toBe(true)
  expect(app.find({ text: "below the fold" }).exists).toBe(true)
  expect(app.find({ text: "below the fold" }).visible).toBe(false)
  expect(app.find({ text: "faded" }).visible).toBe(false)
  setShown(false)
  await app.frame()
  expect(app.find({ text: "conditional" }).exists).toBe(false)
  expect(app.find({ text: "conditional" }).visible).toBe(false)
})

test("a tap on a covered node fails and names the cover", async app => {
  let taps = 0
  await app.mount(() => (
    <view width={300} height={200}>
      <view label="button" width={100} height={40} onPointerDown={() => taps++}>
        <text>Buy</text>
      </view>
      <view label="scrim" position="absolute" x={0} y={0} width={300} height={200} pointerEvents="all" />
    </view>
  ))
  let failure = ""
  try {
    await app.tap(app.find({ label: "button" }))
  } catch (e) {
    failure = (e as Error).message
  }
  expect(failure).toContain('is covered at its center (50, 20) by view labelled "scrim"')
  expect(taps).toBe(0)
  // A point lands on whatever is there.
  await app.tap({ x: 50, y: 20 })
  expect(taps).toBe(0)
})

test("a drag moves once per frame and lifts at the end point", async app => {
  let seen: string[] = []
  await app.mount(() => (
    <view
      label="pad"
      width={300}
      height={100}
      onPointerDown={e => seen.push(`down ${e.clientX}`)}
      onPointerMove={e => seen.push(`move ${e.clientX}`)}
      onPointerUp={e => seen.push(`up ${e.clientX}`)}
    />
  ))
  let start = app.time
  await app.drag({ x: 10, y: 50 }, { x: 70, y: 50 }, { durationMs: 50, pointerType: "touch" })
  expect(seen).toEqual(["down 10", "move 30", "move 50", "move 70", "up 70"])
  // Three moves, the up and the closing frame.
  expect(Math.round((app.time - start) * 60 / 1000)).toBe(5)
})

test("a mouse tap hovers before it presses, a touch tap does not", async app => {
  let seen: string[] = []
  await app.mount(() => (
    <view
      width={100}
      height={100}
      onPointerEnter={() => seen.push("enter")}
      onPointerDown={() => seen.push("down")}
      onPointerUp={() => seen.push("up")}
    />
  ))
  await app.tap({ x: 50, y: 50 })
  expect(seen).toEqual(["enter", "down", "up"])
})

test("a tap is never 0 ms: its up is a frame after its down, or holdMs", async app => {
  let stamps: number[] = []
  await app.mount(() => <view width={100} height={100} onPointerDown={e => stamps.push(e.timeStamp)} onPointerUp={e => stamps.push(e.timeStamp)} />)
  await app.tap({ x: 50, y: 50 }, { pointerType: "touch" })
  await app.tap({ x: 50, y: 50 }, { pointerType: "touch", holdMs: 100 })
  expect(stamps.length).toBe(4)
  expect(stamps[1]! - stamps[0]!).toBe(1)
  expect(stamps[3]! - stamps[2]!).toBe(100)
}, { fps: 1000 })

test("a key and typed text reach the focused node", async app => {
  let seen: string[] = []
  let field: { id: number } | undefined
  await app.mount(() => (
    <view
      ref={(node: { id: number }) => (field = node)}
      width={100}
      height={30}
      onKeyDown={e => seen.push(`down ${e.key}`)}
      onKeyUp={e => seen.push(`up ${e.key}`)}
      onTextInput={e => seen.push(`text ${e.text}`)}
    />
  ))
  setFocus(field!.id)
  await app.key("Enter")
  await app.type("hi")
  expect(seen).toEqual(["down Enter", "up Enter", "text hi"])
})

test("load starts an entry the way the runtime does", async app => {
  let root = await app.load(() => import("./fixtures/entry.tsx"))
  expect(root.record.kind).toBe("window")
  expect(root.find({ text: "from the entry" }).visible).toBe(true)
  let failure = ""
  try {
    await app.mount(() => <view />)
  } catch (e) {
    failure = (e as Error).message
  }
  expect(failure).toContain("a test is one app")
})

test("a cancel is the pointer's end: onPointerCancel, no up, and the press under it fires nothing", async app => {
  let seen: string[] = []
  await app.mount(() => (
    <view
      width={200}
      height={200}
      onPointerDown={() => seen.push("down")}
      onPointerUp={() => seen.push("up")}
      onPointerCancel={e => seen.push(`cancel ${e.clientX},${e.clientY} ${e.pointerType}`)}
      onPointerLeave={() => seen.push("leave")}
    />
  ))
  await app.input([
    { type: "pointer", action: "down", x: 5, y: 5, pointerType: "touch" },
    { type: "pointer", action: "move", x: 50, y: 50, pointerType: "touch", delayMs: 20 },
    { type: "pointer", action: "cancel", x: 60, y: 60, pointerType: "touch", delayMs: 20 },
  ])
  expect(seen).toEqual(["down", "cancel 60,60 touch", "leave"])
})

test("raw events in the control API's shape, with app time between them", async app => {
  let seen: string[] = []
  await app.mount(() => (
    <view width={200} height={200} onPointerDown={e => seen.push(`down ${e.timeStamp}`)} onPointerUp={e => seen.push(`up ${e.timeStamp}`)} />
  ))
  await app.input([
    { type: "pointer", action: "down", x: 5, y: 5, pointerType: "touch", delayMs: 20 },
    { type: "pointer", action: "up", x: 5, y: 5, pointerType: "touch", delayMs: 250 },
  ])
  expect(seen).toEqual(["down 20", "up 270"])
  let failure = ""
  try {
    await app.input([{ type: "pointer", action: "press", x: 1, y: 1 }])
  } catch (e) {
    failure = (e as Error).message
  }
  expect(failure).toContain("pointer action must be down, up, cancel, move, tap or drag")
}, { fps: 1000 })
