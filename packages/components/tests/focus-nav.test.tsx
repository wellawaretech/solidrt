// The focus navigation brings the control it focuses into view: a Tab onto
// a row below a ScrollView's viewport scrolls the list to it, so a pad or
// remote can walk a list longer than its box. Scroll offsets and clipping
// are GPU-side tree state, so this is an app test.

import { test, expect } from "@solidrt/test"
import { For } from "@solidrt/core"
import { Button, ScrollView, createFocusNav } from "../src/index.ts"

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
