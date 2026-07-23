#![cfg(feature = "compile")]

mod common;

use common::run_source;

// These mirror the manual examples/sqlite_test.js walkthrough, but as automated
// assertions. Every test uses its own in-memory database, so they are isolated
// and deterministic.

#[tokio::test]
async fn insert_and_query_all() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
            await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Alice", 9.5]);
            await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Bob", 7]);
            let rows = await db.query("SELECT name, score FROM users ORDER BY id").all();
            console.log(JSON.stringify(rows));
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), r#"[{"name":"Alice","score":9.5},{"name":"Bob","score":7}]"#);
}

#[tokio::test]
async fn run_returns_changes_and_rowid() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)");
            let r = await db.run("INSERT INTO t (v) VALUES (?)", ["x"]);
            console.log(r.changes, r.lastInsertRowid);
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "1 1");
}

#[tokio::test]
async fn exec_runs_multi_statement_script() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec(`
                CREATE TABLE a (x INTEGER);
                CREATE TABLE b (y INTEGER);
                INSERT INTO a VALUES (1);
                INSERT INTO b VALUES (2);
            `);
            let row = await db.query("SELECT (SELECT x FROM a) AS ax, (SELECT y FROM b) AS bee").get();
            console.log(row.ax, row.bee);
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "1 2");
}

#[tokio::test]
async fn reusable_statement_rebinds_params() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
            await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Alice", 9.5]);
            await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Bob", 7]);
            let q = db.query("SELECT name FROM users WHERE score > ? ORDER BY id");
            console.log(JSON.stringify(await q.all([5])));
            console.log(JSON.stringify(await q.all([8])));
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "[{\"name\":\"Alice\"},{\"name\":\"Bob\"}]\n[{\"name\":\"Alice\"}]");
}

#[tokio::test]
async fn get_returns_first_row_or_undefined() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
            await db.run("INSERT INTO users (name) VALUES (?)", ["Alice"]);
            let found = await db.query("SELECT name FROM users WHERE name = ?").get(["Alice"]);
            let missing = await db.query("SELECT name FROM users WHERE name = ?").get(["Nobody"]);
            console.log(JSON.stringify(found));
            console.log(missing === undefined);
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "{\"name\":\"Alice\"}\ntrue");
}

#[tokio::test]
async fn blob_roundtrips_as_uint8array() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE blobs (id INTEGER PRIMARY KEY, data BLOB)");
            await db.run("INSERT INTO blobs (data) VALUES (?)", [new Uint8Array([1, 2, 3, 255])]);
            let row = await db.query("SELECT data FROM blobs").get();
            let bytes = row.data;
            console.log(bytes instanceof Uint8Array, bytes.length, bytes[0], bytes[3]);
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true 4 1 255");
}

#[tokio::test]
async fn transaction_commits_batch() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
            await db.run("INSERT INTO users (name, score) VALUES (?, ?)", ["Bob", 7]);
            let res = await db.transaction([
                ["INSERT INTO users (name, score) VALUES (?, ?)", ["Carol", 8.2]],
                ["UPDATE users SET score = score + ? WHERE name = ?", [1, "Bob"]],
            ]);
            console.log(JSON.stringify(res));
            console.log(JSON.stringify(await db.query("SELECT name, score FROM users ORDER BY id").all()));
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  // One RunResult per statement; both report the connection's last insert rowid (Carol = 2).
  let expected = "[{\"changes\":1,\"lastInsertRowid\":2},{\"changes\":1,\"lastInsertRowid\":2}]\n\
                  [{\"name\":\"Bob\",\"score\":8},{\"name\":\"Carol\",\"score\":8.2}]";
  assert_eq!(out.log(), expected);
}

#[tokio::test]
async fn transaction_rolls_back_on_error() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
            let before = (await db.query("SELECT COUNT(*) AS n FROM users").get()).n;
            let threw = false;
            try {
                await db.transaction([
                    ["INSERT INTO users (name) VALUES (?)", ["Dave"]],
                    ["INSERT INTO nonexistent VALUES (1)", []],
                ]);
            } catch (e) {
                threw = true;
            }
            let after = (await db.query("SELECT COUNT(*) AS n FROM users").get()).n;
            console.log(threw, before === after);
            await db.close();
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true true");
}

#[tokio::test]
async fn bad_sql_rejects() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            let msg = "no error";
            try {
                await db.query("SELECT * FROM nonexistent").all();
            } catch (e) {
                msg = "rejected";
            }
            console.log(msg);
            await db.close();
            "#,
  )
  .await;
  assert_eq!(out.log(), "rejected");
}

#[tokio::test]
async fn query_after_close_rejects() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let db = await Database.open(":memory:", "rw+");
            await db.close();
            let msg = "no error";
            try {
                await db.query("SELECT 1").all();
            } catch (e) {
                msg = "rejected";
            }
            console.log(msg);
            "#,
  )
  .await;
  assert_eq!(out.log(), "rejected");
}

#[tokio::test]
async fn unknown_mode_rejects() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let msg = "no error";
            try {
                await Database.open(":memory:", "bogus");
            } catch (e) {
                msg = String(e.message || e);
            }
            console.log(msg);
            "#,
  )
  .await;
  assert!(out.log().contains("unknown database mode"), "got: {}", out.log());
}

#[tokio::test]
async fn constructor_throws() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let msg = "no throw";
            try {
                new Database();
            } catch (e) {
                msg = String(e.message || e);
            }
            console.log(msg);
            "#,
  )
  .await;
  assert!(out.log().contains("Database.open()"), "got: {}", out.log());
}

#[tokio::test]
async fn default_mode_is_read_only() {
  let out = run_source(
    r#"
            import { Database } from "flux:sqlite";
            let msg = "wrote";
            try {
                let db = await Database.open(":memory:");
                await db.exec("CREATE TABLE t (x INTEGER)");
            } catch (e) {
                msg = "readonly";
            }
            console.log(msg);
            "#,
  )
  .await;
  assert_eq!(out.log(), "readonly");
}
