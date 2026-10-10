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
