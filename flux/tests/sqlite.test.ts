// flux:sqlite over in-memory databases: every test opens its own, so they
// are isolated and deterministic.
import { expect, test } from "flux:test"
import { Database } from "flux:sqlite"

test("insert and query all", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
  await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Alice", 9.5])
  await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Bob", 7])
  let rows = await db.query("SELECT name, score FROM users ORDER BY id").all()
  expect(rows).toEqual([
    { name: "Alice", score: 9.5 },
    { name: "Bob", score: 7 },
  ])
  await db.close()
})

test("run returns the changes and the rowid", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
  let r = await db.run("INSERT INTO t (v) VALUES (?)", ["x"])
  expect(r).toEqual({ changes: 1, lastInsertRowid: 1 })
  await db.close()
})

test("exec runs a multi-statement script", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec(`
    CREATE TABLE a (x INTEGER);
    CREATE TABLE b (y INTEGER);
    INSERT INTO a VALUES (1);
    INSERT INTO b VALUES (2);
  `)
  let row = await db.query("SELECT (SELECT x FROM a) AS ax, (SELECT y FROM b) AS bee").get()
  expect(row).toEqual({ ax: 1, bee: 2 })
  await db.close()
})

test("a reusable statement rebinds its params", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
  await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Alice", 9.5])
  await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Bob", 7])
  let q = db.query("SELECT name FROM users WHERE score > ? ORDER BY id")
  expect(await q.all([5])).toEqual([{ name: "Alice" }, { name: "Bob" }])
  expect(await q.all([8])).toEqual([{ name: "Alice" }])
  await db.close()
})

test("get returns the first row or undefined", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
  await db.run("INSERT INTO users (name) VALUES (?)", ["Alice"])
  let found = await db.query("SELECT name FROM users WHERE name = ?").get(["Alice"])
  let missing = await db.query("SELECT name FROM users WHERE name = ?").get(["Nobody"])
  expect(found).toEqual({ name: "Alice" })
  expect(missing).toBe(undefined)
  await db.close()
})

test("a blob round-trips as a Uint8Array", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE blobs (id INTEGER PRIMARY KEY, data BLOB)")
  await db.run("INSERT INTO blobs (data) VALUES (?)", [new Uint8Array([1, 2, 3, 255])])
  let row = await db.query("SELECT data FROM blobs").get()
  let bytes = row!.data
  expect(bytes instanceof Uint8Array).toBe(true)
  expect(bytes).toEqual(new Uint8Array([1, 2, 3, 255]))
  await db.close()
})

test("a transaction commits its batch", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
  await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Bob", 7])
  let res = await db.transaction([
    ["INSERT INTO users (name, score) VALUES (?, ?)", ["Carol", 8.2]],
    ["UPDATE users SET score = score + ? WHERE name = ?", [1, "Bob"]],
  ])
  // One RunResult per statement; both report the connection's last insert
  // rowid (Carol = 2).
  expect(res).toEqual([
    { changes: 1, lastInsertRowid: 2 },
    { changes: 1, lastInsertRowid: 2 },
  ])
  expect(await db.query("SELECT name, score FROM users ORDER BY id").all()).toEqual([
    { name: "Bob", score: 8 },
    { name: "Carol", score: 8.2 },
  ])
  await db.close()
})

test("a transaction rolls back on an error", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
  let count = async () => (await db.query("SELECT COUNT(*) AS n FROM users").get())!.n
  let before = await count()
  await expect(
    db.transaction([
      ["INSERT INTO users (name) VALUES (?)", ["Dave"]],
      ["INSERT INTO nonexistent VALUES (1)", []],
    ]),
  ).rejects.toThrow()
  expect(await count()).toBe(before)
  await db.close()
})

test("bad SQL rejects", async () => {
  let db = await Database.open(":memory:", "rw+")
  await expect(db.query("SELECT * FROM nonexistent").all()).rejects.toThrow()
  await db.close()
})

test("a query after close rejects", async () => {
  let db = await Database.open(":memory:", "rw+")
  await db.close()
  await expect(db.query("SELECT 1").all()).rejects.toThrow()
})

test("an unknown mode rejects", async () => {
  // @ts-expect-error not a mode; open has to say so at runtime too
  await expect(Database.open(":memory:", "bogus")).rejects.toThrow("unknown database mode")
})

test("the constructor throws", () => {
  expect(() => new Database()).toThrow("Database.open()")
})

test("the default mode is read-only", async () => {
  let db = await Database.open(":memory:")
  await expect(db.exec("CREATE TABLE t (x INTEGER)")).rejects.toThrow()
})
