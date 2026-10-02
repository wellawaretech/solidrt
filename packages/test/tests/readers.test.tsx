// The verbs an app test has beside naming nodes and sending pointer input:
// a link, a debug command, a synthetic gamepad, and the readers past a
// node's record: the subtree as text, its pixels, the GPU inventory.

import { registerDebug } from "sol:dev"
import { test, expect } from "../src/index.ts"
import { createSignal, gamepads, onLink } from "@solidrt/core"

// The error `run` rejects with; fails when it fulfills.
async function failure(run: () => Promise<unknown>): Promise<string> {
  try {
    await run()
  } catch (err) {
    return String((err as Error).message)
  }
  throw new Error("Expected the call to fail, and it passed")
}

test("a link reaches onLink, and an app without a handler fails the verb", async app => {
  let message = await failure(() => app.link("app://early"))
  expect(message).toContain("does not listen for links")
  let [screen, setScreen] = createSignal("home")
  onLink(link => setScreen(link))
  await app.mount(() => <text>{screen()}</text>)
  await app.link("app://settings/display")
  expect(app.find({ text: "app://settings/display" }).exists).toBe(true)
})

test("a debug command is called with its args and returns its value", async app => {
  let [level, setLevel] = createSignal(1)
  registerDebug("level", (next: number) => {
    setLevel(next)
    return { was: 1, now: next }
  })
  await app.mount(() => <text>{`level ${level()}`}</text>)
  expect(await app.debug("level", 7)).toEqual({ was: 1, now: 7 })
  expect(app.find({ text: "level 7" }).exists).toBe(true)
  expect(await failure(() => app.debug("warp"))).toContain("no debug command 'warp' (registered: level)")
})

test("a synthetic gamepad connects, holds a state and leaves", async app => {
  await app.mount(() => <text>{gamepads().map(pad => (pad ? pad.buttons.join("+") || "idle" : "none")).join(",") || "no pads"}</text>)
  expect(app.find({ text: "no pads" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "connect" }])
  expect(app.find({ text: "idle" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "set", slot: 0, buttons: ["south", "dpadUp"] }])
  expect(app.find({ text: "south+dpadUp" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "set", slot: 0, buttons: ["east"], holdMs: 100 }])
  expect(app.find({ text: "idle" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "disconnect", slot: 0 }])
  expect(app.find({ text: "none" }).exists).toBe(true)
  expect(await failure(() => app.input([{ type: "gamepad", action: "set", slot: 3 }]))).toContain("slot 3 holds no synthetic pad")
})

test("every test starts with no pad seated", async app => {
  await app.mount(() => <text>{String(gamepads().length)}</text>)
  await app.frame()
  expect(app.find({ text: "0" }).exists).toBe(true)
})

test("outline() is the subtree as text, without ids", async app => {
  let root = await app.mount(() => (
    <view label="card" width={200} height={80} flexDirection="column" opacity={0.5}>
      <text>Title</text>
      <view label="bar" width={50} height={10} />
    </view>
  ))
  let card = root.find({ label: "card" })
  let lines = card.outline().split("\n")
  expect(lines[0]).toBe("view [card] 0,0 200x80")
  expect(/^  text "Title" 0,0 \d+(\.\d+)?x\d+(\.\d+)?$/.test(lines[1]!)).toBe(true)
  expect(/^  view \[bar\] 0,\d+(\.\d+)? 50x10$/.test(lines[2]!)).toBe(true)
  expect(lines.length).toBe(3)
  expect(card.outline({ props: true }).split("\n")[0]).toContain('"opacity":0.5')
})

test("pixel reads what a node paints, where a transition stands", async app => {
  let [on, setOn] = createSignal(false)
  let swatch = app.ref()
  await app.mount(() => (
    <view width={200} height={100}>
      <rect width={200} height={100} color="#000000" />
      <rect ref={swatch} position="absolute" left={0} top={0} width={40} height={40} color={on() ? "#000000" : "#ff0000"} transition="100ms linear" />
    </view>
  ))
  expect(swatch.pixel(20, 20)).toEqual([255, 0, 0, 255])
  let image = swatch.pixels()
  expect([image.width, image.height, image.data.length]).toEqual([40, 40, 40 * 40 * 4])
  let before = app.time
  setOn(true)
  await app.advance(50)
  let half = swatch.pixel(20, 20)
  expect(half[0]).toBeGreaterThan(40)
  expect(half[0]).toBeLessThan(220)
  // Reading drew the tree as it stood and passed no time.
  expect(app.time - before).toBeLessThan(70)
  expect(() => swatch.pixel(40, 0)).toThrow("outside")
})

test("gpu is the inventory the control API reports", async app => {
  await app.mount(() => <text>plain</text>)
  let inventory = app.gpu()
  expect(Object.keys(inventory).sort()).toEqual(["buffers", "pipelines", "programs", "renderPipelines", "textures"])
  expect(app.gpu({ label: "no such label" }).textures).toEqual([])
})
