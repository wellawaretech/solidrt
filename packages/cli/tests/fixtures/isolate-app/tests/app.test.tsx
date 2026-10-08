// The app's isolates under sol test (okf/done/sol-test-isolates.md): an app
// test is bundled from this file, but the isolates staged with it are the
// project's, found under its source root and named as `sol run` names
// them, so they run from the app and from the test alike.

import { test, expect } from "@solidrt/test"
import { isolate } from "flux:isolate"
import type * as Mesher from "../src/mesher.ts"

test("the app's isolate runs under the test, named as sol run names it", async (app) => {
  let root = await app.load(() => import("../src/index.tsx"))
  await app.settle()
  expect(root.find({ text: "sum: 6" }).visible).toBe(true)
})

test("the test calls the app's isolate itself", async () => {
  let mesher = isolate<typeof Mesher>("mesher")
  expect(await mesher.sum([4, 5])).toBe(9)
  mesher.terminate()
})
