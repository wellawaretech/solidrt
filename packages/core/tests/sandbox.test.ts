// The sandbox of an app test: every test starts with an empty data folder,
// whatever the tests before it wrote or left open, and what a test file
// opens at module level is opened after the wipe, in the test's own engine.

import { test, expect } from "../src/test.ts"
import { file } from "flux:fs"
import { Database } from "flux:sqlite"

// Opened at the top level, so once per test, in the emptied sandbox.
let db = await Database.open("state.db", "rw+")
await db.exec("create table if not exists notes (body text)")

async function count(): Promise<number> {
  let rows = (await db.query("select count(*) as n from notes").all()) as { n: number }[]
  return rows[0]!.n
}

test("first: writes a file and a row, and leaves the database open", async () => {
  expect(await file("left-behind.txt").exists()).toBe(false)
  await file("left-behind.txt").write("from the first test")
  await db.exec("insert into notes values ('from the first test')")
  expect(await count()).toBe(1)
})

test("second: finds neither", async () => {
  expect(await file("left-behind.txt").exists()).toBe(false)
  expect(await count()).toBe(0)
})
