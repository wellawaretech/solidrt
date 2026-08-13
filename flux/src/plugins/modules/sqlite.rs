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

use rquickjs::class::Trace;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, Exception, IntoJs, JsLifetime, Object, TypedArray, Value};

use crate::plugins::js_error::{err_message, JsResult};
use crate::plugins::marshal::{with_pending, OptArg};
use forge::sqlite::{FirstRow, Rows, RunResult, SqlValue, SqliteConnection, TxResults};

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
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<JsRunResult>>>> {
    let conn = self.conn.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { conn.run(sql, bound, false).await.map(JsRunResult) }))
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
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<JsTxResults>>>> {
    let parsed = extract_statements(statements).map_err(|m| Exception::throw_message(&ctx, &m))?;
    let conn = self.conn.clone();
    Ok(with_pending(&ctx, async move { conn.transaction(parsed).await.map(JsTxResults) }))
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
  /// All matching rows, as an array of plain objects.
  pub fn all<'js>(
    &self,
    ctx: Ctx<'js>,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<JsRows>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { conn.query(sql, bound, true).await.map(JsRows) }))
  }

  /// The first matching row as a plain object, or `undefined` if there are none.
  pub fn get<'js>(
    &self,
    ctx: Ctx<'js>,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<JsFirstRow>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { conn.get(sql, bound, true).await.map(JsFirstRow) }))
  }

  /// Execute as a write. Resolves to `{ changes, lastInsertRowid }`.
  pub fn run<'js>(
    &self,
    ctx: Ctx<'js>,
    params: OptArg<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<JsRunResult>>>> {
    let conn = self.conn.clone();
    let sql = self.sql.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { conn.run(sql, bound, true).await.map(JsRunResult) }))
  }
}

// ---- JS <-> SqlValue marshalling -------------------------------------------

/// Convert a JS array of bind parameters into owned SqlValues.
fn extract_params(params: Option<Array<'_>>) -> Result<Vec<SqlValue>, String> {
  let Some(arr) = params else {
    return Ok(Vec::new());
  };
  let mut out = Vec::with_capacity(arr.len());
  for v in arr.iter::<Value>() {
    let v = v.map_err(err_message)?;
    out.push(js_to_sql(v)?);
  }
  Ok(out)
}

/// Convert a JS array of `[sql, params]` entries into owned statements.
fn extract_statements(arr: Array<'_>) -> Result<Vec<(String, Vec<SqlValue>)>, String> {
  let mut out = Vec::with_capacity(arr.len());
  for entry in arr.iter::<Value>() {
    let entry = entry.map_err(err_message)?;
    let Some(pair) = entry.into_array() else {
      return Err("each transaction statement must be an array [sql, params?]".to_string());
    };
    let sql: String = pair.get(0).map_err(err_message)?;
    let params = pair.get::<Array>(1).ok();
    out.push((sql, extract_params(params)?));
  }
  Ok(out)
}

fn js_to_sql(v: Value<'_>) -> Result<SqlValue, String> {
  if v.is_null() || v.is_undefined() {
    Ok(SqlValue::Null)
  } else if let Some(b) = v.as_bool() {
    Ok(SqlValue::Int(b as i64))
  } else if let Some(i) = v.as_int() {
    Ok(SqlValue::Int(i as i64))
  } else if let Some(f) = v.as_float() {
    Ok(SqlValue::Real(f))
  } else if let Some(s) = v.as_string() {
    Ok(SqlValue::Text(s.to_string().map_err(err_message)?))
  } else if let Ok(ta) = TypedArray::<u8>::from_value(v.clone()) {
    Ok(SqlValue::Blob(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default()))
  } else {
    Err("unsupported SQL parameter type".to_string())
  }
}

// Marshalling newtypes over the engine-free `forge::sqlite` result types. The
// `IntoJs` impls live on these wrappers, not on the forge types directly, so
// that once forge is its own crate, converting its types to JS stays inside this
// crate rather than tripping the orphan rule (a foreign `IntoJs` on a foreign
// type). The forge methods return the bare types; call sites `.map(JsX)` them.
struct JsSqlValue(SqlValue);
// `pub` (not re-exported) only to satisfy `private_interfaces`: these appear in
// the rquickjs `#[methods]` return types, which are `pub fn`s.
pub struct JsRows(Rows);
pub struct JsFirstRow(FirstRow);
pub struct JsRunResult(RunResult);
pub struct JsTxResults(TxResults);

impl<'js> IntoJs<'js> for JsSqlValue {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self.0 {
      SqlValue::Null => Ok(Value::new_null(ctx.clone())),
      SqlValue::Int(i) => i.into_js(ctx),
      SqlValue::Real(f) => f.into_js(ctx),
      SqlValue::Text(s) => s.into_js(ctx),
      SqlValue::Blob(b) => TypedArray::new(ctx.clone(), b).map(|ta| ta.into_value()),
    }
  }
}

/// Build a plain object from a row's (name, value) cells.
fn row_to_object<'js>(ctx: &Ctx<'js>, cells: Vec<(String, SqlValue)>) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  for (name, val) in cells {
    obj.set(name, JsSqlValue(val))?;
  }
  Ok(obj)
}

impl<'js> IntoJs<'js> for JsRows {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (idx, row) in self.0 .0.into_iter().enumerate() {
      arr.set(idx, row_to_object(ctx, row)?)?;
    }
    Ok(arr.into_value())
  }
}

impl<'js> IntoJs<'js> for JsFirstRow {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self.0 .0 {
      Some(cells) => Ok(row_to_object(ctx, cells)?.into_value()),
      None => Ok(Value::new_undefined(ctx.clone())),
    }
  }
}

impl<'js> IntoJs<'js> for JsRunResult {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("changes", self.0.changes)?;
    obj.set("lastInsertRowid", self.0.last_insert_rowid)?;
    Ok(obj.into_value())
  }
}

impl<'js> IntoJs<'js> for JsTxResults {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (idx, r) in self.0 .0.into_iter().enumerate() {
      arr.set(idx, JsRunResult(r))?;
    }
    Ok(arr.into_value())
  }
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
