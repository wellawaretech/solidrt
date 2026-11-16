// The focus navigation brings the control it focuses into view: a Tab onto
// a row below a ScrollView's viewport scrolls the list to it, so a pad or
// remote can walk a list longer than its box. Scroll offsets and clipping
// are GPU-side tree state, so this is an app test.

import { test, expect } from "@solidrt/test"
import { For } from "@solidrt/core"
import { Button, NavShell, ScrollView, Text, createFocusNav, createPress, theme } from "../src/index.ts"

// A viewport three rows tall over ten rows.
const ROW = 40
const VIEWPORT = 3 * ROW
const ROWS = Array.from({ length: 10 }, (_, i) => `Row ${i + 1}`)
// Tabs from nothing focused: the first lands on Row 1, the rest step down.
const TABS = 6

test("a Tab onto a row below the viewport scrolls it into view", async app => {
  await app.mount(() => {
    let nav = createFocusNav()
    return (
      <window {...nav.handlers}>
        <ScrollView layout={{ width: 200, height: VIEWPORT }}>
          <For each={ROWS}>{(label) => <Button layout={{ height: ROW }}>{label}</Button>}</For>
        </ScrollView>
      </window>
    )
  })
  expect(app.find({ text: `Row ${TABS}` }).visible).toBe(false)
  for (let i = 0; i < TABS; i++) await app.key("Tab")
  await app.settle()
  expect(app.find({ text: `Row ${TABS}` }).visible).toBe(true)
  expect(app.find({ text: "Row 1" }).visible).toBe(false)
})

// A `select` activation flashes the focused control's pressed state for the
// feedback duration (theme.motion.fast), as a tap would show it.
test("a select activation shows the pressed state briefly", async app => {
  let presses = 0
  let Probe = () => {
    let press = createPress({ onPress: () => presses++ })
    return (
      <view ref={press.ref} focusable {...press.handlers} width={100} height={ROW}>
        <text>{press.pressed() ? "Down" : "Up"}</text>
      </view>
    )
  }
  await app.mount(() => {
    let nav = createFocusNav()
    return (
      <window {...nav.handlers}>
        <Probe />
      </window>
    )
  })
  await app.key("Tab")
  await app.settle()
  expect(app.find({ text: "Up" }).visible).toBe(true)
  await app.key("Enter")
  expect(presses).toBe(1)
  await app.frame(1)
  expect(app.find({ text: "Down" }).visible).toBe(true)
  await app.advance(theme.motion.fast)
  await app.settle()
  expect(app.find({ text: "Up" }).visible).toBe(true)
})

// The flash's end hands the state back to a pointer press in flight: a press
// held and dragged outside shows released, not the flash's pressed.
test("a select flash during a pointer press dragged outside ends released", async app => {
  let Probe = () => {
    let press = createPress({})
    return (
      <view ref={press.ref} focusable {...press.handlers} width={100} height={ROW}>
        <text>{press.pressed() ? "Down" : "Up"}</text>
      </view>
    )
  }
  await app.mount(() => {
    let nav = createFocusNav()
    return (
      <window {...nav.handlers}>
        <Probe />
      </window>
    )
  })
  await app.key("Tab")
  await app.settle()
  await app.input([
    { type: "pointer", action: "down", x: 50, y: ROW / 2, pointerType: "touch" },
    { type: "pointer", action: "move", x: 50, y: ROW * 4, pointerType: "touch" },
  ])
  expect(app.find({ text: "Up" }).visible).toBe(true)
  await app.key("Enter")
  await app.frame(1)
  expect(app.find({ text: "Down" }).visible).toBe(true)
  await app.advance(theme.motion.fast)
  await app.frame(1)
  expect(app.find({ text: "Up" }).visible).toBe(true)
  await app.input([{ type: "pointer", action: "up", x: 50, y: ROW * 4, pointerType: "touch" }])
})

// NavShell's items are focusable: a Tab reaches one and `select` picks it,
// through the nav action registry the press registers.
test("a Tab and Enter select a NavShell item", async app => {
  let picked: unknown[] = []
  await app.mount(() => {
    let nav = createFocusNav()
    return (
      <window {...nav.handlers}>
        <NavShell items={[{ value: "home", label: "Home" }, { value: "settings", label: "Settings" }]} defaultValue="home" onChange={v => picked.push(v)}>
          <Text>Page</Text>
        </NavShell>
      </window>
    )
  })
  await app.key("Tab")
  await app.key("Tab")
  await app.key("Enter")
  await app.settle()
  expect(picked).toEqual(["settings"])
})
