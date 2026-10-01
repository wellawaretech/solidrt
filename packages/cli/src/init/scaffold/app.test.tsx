import { test, expect } from "@solidrt/test"

//Usually you import dependencies at the top, this is an exception
test("the app starts and shows its title", async app => {
  let root = await app.load(() => import("@/index.tsx"))
  expect(root.find({ text: "The Solid Runtime" }).visible).toBe(true)
})
