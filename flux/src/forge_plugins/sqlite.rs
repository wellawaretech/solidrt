//! The `flux:sqlite` module: an async SQLite binding for flux.
//!
//! Marshalling only: decode JS args into the native types of the engine-free
//! `forge::sqlite` connection actor, drive its `SqliteConnection` methods, and
//! encode the results back to JS. The connection-actor architecture and query
//! execution live in `forge::sqlite`.
//!
//! The API is loosely modeled on Bun's `bun:sqlite`, but is NOT a drop-in match.
//! The differences below are intentional (current scope) or fundamental (async
//! vs sync). Keep this list current as the plugin grows.
//!
//! Fundamental difference: Bun's API is fully synchronous (`new Database()`,
//! `query.all()`); ours is async. So construction is `await Database.open()`
//! and `new Database()` throws. Bun's `db.transaction(fn)` wraps a SYNC callback;
//! that does not translate to our async model (the callback would yield between
//! statements). Instead `db.transaction(statements)` takes a DECLARATIVE batch -
//! an array of `[sql, params]` run in one BEGIN/COMMIT (ROLLBACK on any error).
//! It cannot branch on intermediate results; a programmable async-callback form
//! may be layered on later (the connection actor already supports it).
//!
//! Prepared statements follow flux's "raw primitive first, conveniences opt-in"
//! principle. `db.query(sql)` returns a reusable `Statement` whose executions go
//! through `prepare_cached` (the connection caches the compiled statement), so
//! reuse is explicit: you opt in by creating a Statement. The one-shot
//! `db.run(sql, params)` uses plain `prepare` with no caching. We do NOT offer
//! Bun's `db.prepare()` (an uncached but compile-once held statement): rusqlite's
//! `Statement` borrows the Connection and cannot be stored as a long-lived JS
//! object, so our Statement holds only the SQL and recompiles via the cache.
//!
//! Open mode is a positional second arg to `open`: `"ro"` (DEFAULT,
//! read-only, file must exist), `"rw"` (read-write, must exist), `"rw+"`
//! (read-write, create if missing). Note this differs from Bun, which defaults
//! to read-write+create; we default to read-only as a safe default, so writing
//! or creating a database is an explicit opt-in (`"rw"` / `"rw+"`).
//!
//! Matches Bun:
//! - Named export `Database`, in-memory (`:memory:`) databases.
//! - `db.query(sql)` -> reusable Statement; `stmt.all/get/run`.
//! - `db.run(sql, params)` -> `{ changes, lastInsertRowid }`.
//! - `db.exec(sql)` runs a multi-statement script (no params).
//! - Positional `?` parameters; rows returned as plain objects.
//! - Bind params as a positional array (`all([1, 2])`) - one of Bun's forms.
//! - `BLOB` <-> `Uint8Array`.
//!
//! Differs from Bun (current scope - may change):
//! - Construction and all executions are async (return promises). An invalid
//!   open mode rejects the `open` promise rather than throwing synchronously.
//! - No `db.prepare()` (see above), no `stmt.values()`/`iterate()`/`finalize()`/
//!   `.as(Class)`.
//! - `db.transaction` is a declarative batch (array of `[sql, params]`), not
//!   Bun's programmable sync callback. Returns one `{changes, lastInsertRowid}`
//!   per statement. Statements must be writes/DDL (they go through `execute`).
//! - Bind params are ONE explicit argument: a positional array (`all([1, 2])`,
//!   itself a valid Bun form). We deliberately do NOT also accept Bun's
//!   spread-args overload (`all(1, 2)`) - one shape, no argument-shape guessing.
//! - Named params (`$p` / `:p` / `@p`, object binding) not yet supported; when
//!   added they keep the single-argument style (an object), not an overload.
//! - Integers always go i64 -> JS number; no `safeIntegers`/bigint mode, so
//!   values above 2^53 lose precision silently.
//! - Errors surface as a generic `Error` with the SQLite message, not a typed
//!   SQLite error subclass.
//! - `close()` is async (returns a promise); Bun's is synchronous.
//!
//! SolidRT extensions (not in Bun) - the dependency-tracking pair that lets a
//! reactive layer (e.g. `@solidrt/data`) invalidate queries automatically:
//! - `stmt.tables()` resolves to the tables the statement reads (SQLite's
//!   authorizer, captured during a compile).
//! - `db.onWrite(cb)` calls back with the sorted table names touched after
//!   each command that changed rows (SQLite's update hook, so trigger and
//!   cascade writes are included). Returns an unsubscribe function.

use std::cell::Cell;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::function::MutFn;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, Exception, Function, JsLifetime, Value};

use crate::logger::report_uncaught;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{with_pending, OptArg};
use crate::plugins::value::{self, Neutral};
use forge::sqlite::{SqlValue, SqliteConnection};

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Database")]
pub struct Database {
  #[qjs(skip_trace)]
  conn: SqliteConnection,
}

#[rquickjs::methods]
impl Database {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<Database> {
    Err(ctx.throw(rquickjs::String::from_str(ctx.clone(), "use Database.open() to open a database")?.into()))
  }

  /// Open a database. `mode` selects access: `"ro"` (default, read-only, file
  /// must exist), `"rw"` (read-write, must exist), `"rw+"` (read-write, create
  /// if missing).
  #[qjs(static)]
  pub fn open<'js>(
    ctx: Ctx<'js>,
    path: String,
    mode: OptArg<String>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Database>>>> {
    Ok(with_pending(&ctx, async move { SqliteConnection::open(path, mode.0).await.map(|conn| Database { conn }) }))
  }

  /// Subscribe to writes on this connection. After each command that changed
  /// rows, `callback` gets one call with the sorted names of the tables
  /// touched, as reported by SQLite's update hook, so trigger and cascade
  /// writes are included. Returns an unsubscribe function.
  ///
  /// Each listener gets its own forge subscription and dispatch task, which
  /// holds the callback as a plain `Function` (refcounted, like the serve/
  /// websocket handlers) and exits when the connection closes. Unsubscribe
  /// flips a flag: the parked task stops calling immediately and unwinds on
  /// the next write event or on close. The task does not hold the engine loop
  /// open: write events only follow commands, and each in-flight command
  /// already holds via `with_pending`.
  ///
  /// Contract (see the type docs for the full list): only this connection's
  /// writes are seen; WITHOUT ROWID tables do not report; a rolled-back
  /// transaction may still report (a spurious re-read, never a stale one).
  #[qjs(rename = "onWrite")]
  pub fn on_write<'js>(&self, ctx: Ctx<'js>, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
    let mut rx = self.conn.subscribe_writes().map_err(|m| Exception::throw_message(&ctx, &m))?;
    let active = Rc::new(Cell::new(true));
    let flag = active.clone();
    let ctx2 = ctx.clone();
    ctx.spawn(async move {
      while let Some(tables) = rx.recv().await {
        if !flag.get() {
          break;
        }
        if let Err(e) = callback.call::<_, ()>((tables,)) {
          report_uncaught(&ctx2, e, "sqlite onWrite listener");
        }
      }
    });
    Function::new(
      ctx.clone(),
      MutFn::from(move || {
        active.set(false);
      }),
    )
  }

  /// Create a reusable prepared statement. Construction is synchronous and
  /// cheap (it only stores the SQL); the compile happens on first execution and
  /// is then cached on the connection, so repeated `all`/`get`/`run` reuse it.
  pub fn query<'js>(&self, ctx: Ctx<'js>, sql: String) -> rquickjs::Result<Class<'js, Statement>> {
    Class::instance(ctx, Statement { conn: self.conn.clone(), sql })
  }

  /// One-shot write. Uses plain `prepare` (no caching). Resolves to
  /// `{ changes, lastInsertRowid }`.
  pub fn run<'js>(
    &self,
    ctx: Ctx<'js>,
    sql: String,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Neutral>>>> {
    let conn = self.conn.clone();
    let bound = extract_params(&ctx, params.0)?;
    Ok(with_pending(&ctx, async move { conn.run(sql, bound, false).await.map(|r| Neutral(r.into())) }))
  }

  /// Run a batch of statements (separated by `;`) with no parameters. Intended
  /// for schema setup / migrations. Resolves to undefined.
  pub fn exec<'js>(
    &self,
    ctx: Ctx<'js>,
    sql: String,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<()>>>> {
    let conn = self.conn.clone();
    Ok(with_pending(&ctx, async move { conn.exec(sql).await }))
  }

  /// Run a batch of `[sql, params]` statements in a single transaction. All run
  /// in one BEGIN/COMMIT; any error rolls the whole batch back and rejects.
  /// Resolves to one `{ changes, lastInsertRowid }` per statement.
  pub fn transaction<'js>(
    &self,
    ctx: Ctx<'js>,
    statements: Array<'js>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Neutral>>>> {
    let parsed = extract_statements(&ctx, statements)?;
    let conn = self.conn.clone();
    Ok(with_pending(&ctx, async move { conn.transaction(parsed).await.map(|t| Neutral(t.into())) }))
  }

  /// Close the connection, releasing it. Safe to call more than once; later
  /// queries on a closed database will reject.
  pub fn close<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<()>>>> {
    let conn = self.conn.clone();
    Ok(with_pending(&ctx, async move {
      conn.close().await;
      Ok::<(), String>(())
    }))
  }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Statement")]
pub struct Statement {
  #[qjs(skip_trace)]
  conn: SqliteConnection,
  #[qjs(skip_trace)]
  sql: String,
}

#[rquickjs::methods]
impl Statement {
  /// The tables this statement reads, sorted, as reported by SQLite's
  /// authorizer during a compile. Includes tables reached through views and
  /// subqueries; the statement is compiled but never run. Pair with
  /// `Database.onWrite` to know when a re-read could return different rows.
  pub fn tables<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Vec<String>>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    Ok(with_pending(&ctx, async move { conn.read_set(sql).await }))
  }

  /// All matching rows, as an array of plain objects.
  pub fn all<'js>(
    &self,
    ctx: Ctx<'js>,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Neutral>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    let bound = extract_params(&ctx, params.0)?;
    Ok(with_pending(&ctx, async move { conn.query(sql, bound, true).await.map(|r| Neutral(r.into())) }))
  }

  /// The first matching row as a plain object, or `undefined` if there are none.
  pub fn get<'js>(
    &self,
    ctx: Ctx<'js>,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Option<Neutral>>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    let bound = extract_params(&ctx, params.0)?;
    Ok(with_pending(&ctx, async move { conn.get(sql, bound, true).await.map(|r| r.into_value().map(Neutral)) }))
  }

  /// Execute as a write. Resolves to `{ changes, lastInsertRowid }`.
  pub fn run<'js>(
    &self,
    ctx: Ctx<'js>,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Neutral>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    let bound = extract_params(&ctx, params.0)?;
    Ok(with_pending(&ctx, async move { conn.run(sql, bound, true).await.map(|r| Neutral(r.into())) }))
  }
}

// ---- JS <-> SqlValue marshalling -------------------------------------------

/// Convert a JS array of bind parameters into owned SqlValues: each element
/// decodes as a neutral value, then `SqlValue::try_from` restricts it to what
/// SQLite can bind (lists/maps reject).
fn extract_params<'js>(ctx: &Ctx<'js>, params: Option<Array<'js>>) -> rquickjs::Result<Vec<SqlValue>> {
  let Some(arr) = params else {
    return Ok(Vec::new());
  };
  let mut out = Vec::with_capacity(arr.len());
  for v in arr.iter::<Value>() {
    let neutral = value::from_js(ctx, v?)?;
    out.push(SqlValue::try_from(neutral).map_err(|m| Exception::throw_message(ctx, &m))?);
  }
  Ok(out)
}

/// Convert a JS array of `[sql, params]` entries into owned statements.
fn extract_statements<'js>(ctx: &Ctx<'js>, arr: Array<'js>) -> rquickjs::Result<Vec<(String, Vec<SqlValue>)>> {
  let mut out = Vec::with_capacity(arr.len());
  for entry in arr.iter::<Value>() {
    let Some(pair) = entry?.into_array() else {
      return Err(Exception::throw_message(ctx, "each transaction statement must be an array [sql, params?]"));
    };
    let sql: String = pair.get(0)?;
    let params = pair.get::<Array>(1).ok();
    out.push((sql, extract_params(ctx, params)?));
  }
  Ok(out)
}

pub struct SqliteModule;

impl ModuleDef for SqliteModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("Database")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let ctor = Class::<Database>::create_constructor(ctx)?.expect("Database class has a constructor");
    exports.export("Database", ctor)?;
    Ok(())
  }
}
