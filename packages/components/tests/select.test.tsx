// An open Select is one step of the back stack: Escape closes it without a
// change, as a press outside does. The picker opens on a tap, so the whole
// path is the client's. A list taller than the room it has caps itself to
// the room and scrolls, opening with the selected option in view; the
// dropdown takes the side of the trigger with the room and stays inside the
// window. Placement and clipping are layout, so these are app tests.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { Select } from "../src/index.ts"

const OPTIONS = [
  { value: "s", label: "Small" },
  { value: "m", label: "Medium" },
]

// More rows than any test window is tall.
const MANY = Array.from({ length: 40 }, (_, i) => ({ value: `o${i + 1}`, label: `Option ${i + 1}` }))
// One mouse wheel notch, in the units the runtime reports.
const WHEEL_NOTCH = 100

// The option rows' boxes, in window coordinates, visible or not.
let rows = (app: TestApp) => app.findAll({ text: /^Option / }).map((r) => r.box)

test("Escape closes an open picker without a change", async app => {
  let changes: string[] = []
  await app.mount(() => <Select options={OPTIONS} onChange={v => changes.push(v)} placeholder="Size" />)
  await app.tap(app.find({ text: "Size" }))
  await app.settle()
  expect(app.find({ text: "Medium" }).visible).toBe(true)

  await app.key("Escape")
  await app.settle()
  expect(app.find({ text: "Medium" }).visible).toBe(false)
  expect(changes).toEqual([])
})

test("a long list caps itself to the room below the trigger and scrolls", async app => {
  let root = await app.mount(() => <Select options={MANY} placeholder="Pick" />)
  await app.tap(app.find({ text: "Pick" }))
  await app.settle()
  expect(app.find({ text: "Option 1" }).visible).toBe(true)
  expect(app.find({ text: "Option 40" }).visible).toBe(false)
  // Every row the list shows ends inside the window.
  let shown = app.findAll({ text: /^Option / }).filter((r) => r.visible)
  expect(shown.length).toBeGreaterThan(1)
  for (let r of shown) expect(r.box.y + r.box.height).toBeLessThanOrEqual(root.box.height)

  let first = app.find({ text: "Option 1" }).box
  await app.input([{ type: "wheel", x: first.x + first.width / 2, y: first.y + first.height / 2, deltaX: 0, deltaY: WHEEL_NOTCH * MANY.length }])
  await app.settle()
  expect(app.find({ text: "Option 40" }).visible).toBe(true)
  expect(app.find({ text: "Option 1" }).visible).toBe(false)
})

test("a long list opens with the selected option in view", async app => {
  await app.mount(() => <Select options={MANY} value="o40" />)
  await app.tap(app.find({ text: "Option 40" }))
  await app.settle()
  // The trigger shows the label too: the row is the one inside the list.
  let shown = app.findAll({ text: "Option 40" }).filter((r) => r.visible)
  expect(shown.length).toBe(2)
  expect(app.find({ text: "Option 1" }).visible).toBe(false)
})

test("a trigger at the bottom opens its list above, inside the window", async app => {
  await app.mount(() => (
    <view position="absolute" left={0} bottom={0}>
      <Select options={MANY} placeholder="Pick" />
    </view>
  ))
  let trigger = app.find({ text: "Pick" }).box
  await app.tap(app.find({ text: "Pick" }))
  await app.settle()
  expect(app.find({ text: "Option 1" }).visible).toBe(true)
  let shown = app.findAll({ text: /^Option / }).filter((r) => r.visible)
  expect(shown.length).toBeGreaterThan(1)
  for (let r of shown) {
    expect(r.box.y).toBeGreaterThanOrEqual(0)
    expect(r.box.y + r.box.height).toBeLessThanOrEqual(trigger.y)
  }
  // The rows that exist stay the full list: capped by the viewport, not cut.
  expect(rows(app).length).toBe(MANY.length)
})
