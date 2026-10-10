// An open Select is one step of the back stack: Escape closes it without a
// change, as a press outside does. The picker opens on a tap, so the whole
// path is the client's.

import { test, expect } from "@solidrt/test"
import { Select } from "../src/index.ts"

const OPTIONS = [
  { value: "s", label: "Small" },
  { value: "m", label: "Medium" },
]

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
