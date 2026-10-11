// An open ContextMenu is one step of the back stack: Escape closes it
// selecting nothing, as a press outside does. A right-click opens it, so the
// whole path is the client's.

import { test, expect } from "@solidrt/test"
import { ContextMenu, Text } from "../src/index.ts"

// The secondary mouse button, which opens the menu.
const RIGHT_BUTTON = 2

test("Escape closes an open menu selecting nothing", async app => {
  let picked: string[] = []
  await app.mount(() => (
    <ContextMenu items={[{ label: "Rename", onSelect: () => picked.push("rename") }]}>
      <Text>notes.txt</Text>
    </ContextMenu>
  ))
  await app.tap(app.find({ text: "notes.txt" }), { button: RIGHT_BUTTON })
  await app.settle()
  expect(app.find({ text: "Rename" }).visible).toBe(true)

  await app.key("Escape")
  await app.settle()
  expect(app.find({ text: "Rename" }).visible).toBe(false)
  expect(picked).toEqual([])
})

// More items than any test window is tall.
const MANY = Array.from({ length: 40 }, (_, i) => ({ label: `Action ${i + 1}` }))
// One mouse wheel notch, in the units the runtime reports.
const WHEEL_NOTCH = 100

test("a long menu caps itself to the window and scrolls", async app => {
  let root = await app.mount(() => (
    <ContextMenu items={MANY}>
      <Text>notes.txt</Text>
    </ContextMenu>
  ))
  await app.tap(app.find({ text: "notes.txt" }), { button: RIGHT_BUTTON })
  await app.settle()
  expect(app.find({ text: "Action 1" }).visible).toBe(true)
  expect(app.find({ text: "Action 40" }).visible).toBe(false)
  let shown = app.findAll({ text: /^Action / }).filter((r) => r.visible)
  expect(shown.length).toBeGreaterThan(1)
  for (let r of shown) expect(r.box.y + r.box.height).toBeLessThanOrEqual(root.box.height)

  let first = app.find({ text: "Action 1" }).box
  await app.input([{ type: "wheel", x: first.x + first.width / 2, y: first.y + first.height / 2, deltaX: 0, deltaY: WHEEL_NOTCH * MANY.length }])
  await app.settle()
  expect(app.find({ text: "Action 40" }).visible).toBe(true)
})
