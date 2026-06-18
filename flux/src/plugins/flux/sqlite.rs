//! The `flux:sqlite` module: an async SQLite binding for flux.
//!
//! The API is loosely modeled on Bun's `bun:sqlite`, but is NOT a drop-in
//! match. The differences below are intentional (current scope) or fundamental
//! (async vs sync). Keep this list current as the plugin grows.
//!
//! Architecture: each `Database` owns a dedicated OS thread that holds the
//! `rusqlite::Connection` for its whole life. JS calls marshal a command over a
//! channel, the thread runs it synchronously on the connection, and the result
//! comes back over a oneshot which the async method awaits (wrapped in
//! `Promised`). So queries run off the JS thread without blocking it, and the
//! connection is never shared across threads. This actor shape is also what a
//! future transaction feature needs: a transaction can "check out" the
//! connection thread for the duration of BEGIN..COMMIT while other commands
//! wait, which a shared-pool model cannot do safely across `await` points.
//!
//! Fundamental difference: Bun's API is fully synchronous (`new Database()`,
//! `query.all()`); ours is async. So construction is `await Database.connect()`
//! and `new Database()` throws. Bun's `db.transaction(fn)` wraps a SYNC callback;
//! that does not translate to our async model (the callback would yield between
//! statements). Instead `db.transaction(statements)` takes a DECLARATIVE batch -
//! an array of `[sql, params]` run in one BEGIN/COMMIT (ROLLBACK on any error).
//! It cannot branch on intermediate results; a programmable async-callback form
//! may be layered on later (the connection-actor below already supports it).
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
//! Open mode is a positional second arg to `connect`: `"ro"` (DEFAULT,
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
//! - Construction and all executions are async (return promises).
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

use std::sync::mpsc::{Receiver, Sender};

use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, Exception, IntoJs, JsLifetime, Object, TypedArray, Value};
use rusqlite::{Connection, OpenFlags};
use tokio::sync::oneshot;

use crate::plugins::js_error::{err_message, JsResult};
use crate::plugins::marshal::with_pending;

/// An owned SQLite value, used both for bound parameters (JS -> SQL) and for
/// decoded result cells (SQL -> JS). Owned so it can cross the channel.
enum SqlValue {
  Null,
  Int(i64),
  Real(f64),
  Text(String),
  Blob(Vec<u8>),
}

/// Result rows: each row is a list of (column name, value) pairs.
pub struct Rows(Vec<Vec<(String, SqlValue)>>);

/// A single row, or none. Returned by `stmt.get()`.
pub struct FirstRow(Option<Vec<(String, SqlValue)>>);

/// The outcome of a write: rows changed and the last inserted rowid.
pub struct RunResult {
  changes: i64,
  last_insert_rowid: i64,
}

/// One RunResult per statement in a transaction batch.
pub struct TxResults(Vec<RunResult>);

/// A command sent from a JS call to the connection thread. Each variant carries
/// a oneshot sender the thread replies on.
enum Command {
  Query {
    sql: String,
    params: Vec<SqlValue>,
    cached: bool,
    first_only: bool,
    reply: oneshot::Sender<rusqlite::Result<Rows>>,
  },
  Run {
    sql: String,
    params: Vec<SqlValue>,
    cached: bool,
    reply: oneshot::Sender<rusqlite::Result<RunResult>>,
  },
  Exec {
    sql: String,
    reply: oneshot::Sender<rusqlite::Result<()>>,
  },
  Transaction {
    statements: Vec<(String, Vec<SqlValue>)>,
    reply: oneshot::Sender<rusqlite::Result<Vec<RunResult>>>,
  },
  Close {
    reply: oneshot::Sender<()>,
  },
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Database")]
pub struct Database {
  #[qjs(skip_trace)]
  cmd_tx: Sender<Command>,
}

#[rquickjs::methods]
impl Database {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<Database> {
    Err(ctx.throw(rquickjs::String::from_str(ctx.clone(), "use Database.connect() to open a database")?.into()))
  }

  /// Open a database. `mode` selects access: `"ro"` (default, read-only, file
  /// must exist), `"rw"` (read-write, must exist), `"rw+"` (read-write, create
  /// if missing).
  #[qjs(static)]
  pub fn connect<'js>(
    ctx: Ctx<'js>,
    path: String,
    mode: Opt<String>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Database>>>> {
    let flags = open_flags(mode.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { spawn_database(path, flags).await }))
  }

  /// Create a reusable prepared statement. Construction is synchronous and
  /// cheap (it only stores the SQL); the compile happens on first execution and
  /// is then cached on the connection, so repeated `all`/`get`/`run` reuse it.
  pub fn query<'js>(&self, ctx: Ctx<'js>, sql: String) -> rquickjs::Result<Class<'js, Statement>> {
    Class::instance(ctx, Statement { cmd_tx: self.cmd_tx.clone(), sql })
  }

  /// One-shot write. Uses plain `prepare` (no caching). Resolves to
  /// `{ changes, lastInsertRowid }`.
  pub fn run<'js>(
    &self,
    ctx: Ctx<'js>,
    sql: String,
    params: Opt<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<RunResult>>>> {
    let cmd_tx = self.cmd_tx.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { exec_roundtrip(&cmd_tx, sql, bound, false).await }))
  }

  /// Run a batch of statements (separated by `;`) with no parameters. Intended
  /// for schema setup / migrations. Resolves to undefined.
  pub fn exec<'js>(
    &self,
    ctx: Ctx<'js>,
    sql: String,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<()>>>> {
    let cmd_tx = self.cmd_tx.clone();
    Ok(with_pending(&ctx, async move { batch_roundtrip(&cmd_tx, sql).await }))
  }

  /// Run a batch of `[sql, params]` statements in a single transaction. All run
  /// in one BEGIN/COMMIT; any error rolls the whole batch back and rejects.
  /// Resolves to one `{ changes, lastInsertRowid }` per statement.
  pub fn transaction<'js>(
    &self,
    ctx: Ctx<'js>,
    statements: Array<'js>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<TxResults>>>> {
    let parsed = extract_statements(statements).map_err(|m| Exception::throw_message(&ctx, &m))?;
    let cmd_tx = self.cmd_tx.clone();
    Ok(with_pending(&ctx, async move { transaction_roundtrip(&cmd_tx, parsed).await }))
  }

  /// Close the connection, releasing it. Safe to call more than once; later
  /// queries on a closed database will reject.
  pub fn close<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<()>>>> {
    let cmd_tx = self.cmd_tx.clone();
    Ok(with_pending(&ctx, async move {
      let (reply, rx) = oneshot::channel();
      // A send error means the thread already exited: treat as closed.
      if cmd_tx.send(Command::Close { reply }).is_ok() {
        let _ = rx.await;
      }
      Ok::<(), String>(())
    }))
  }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Statement")]
pub struct Statement {
  #[qjs(skip_trace)]
  cmd_tx: Sender<Command>,
  #[qjs(skip_trace)]
  sql: String,
}

#[rquickjs::methods]
impl Statement {
  /// All matching rows, as an array of plain objects.
  pub fn all<'js>(
    &self,
    ctx: Ctx<'js>,
    params: Opt<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<Rows>>>> {
    let cmd_tx = self.cmd_tx.clone();
    let sql = self.sql.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { query_roundtrip(&cmd_tx, sql, bound, true, false).await }))
  }

  /// The first matching row as a plain object, or `undefined` if there are none.
  pub fn get<'js>(
    &self,
    ctx: Ctx<'js>,
    params: Opt<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<FirstRow>>>> {
    let cmd_tx = self.cmd_tx.clone();
    let sql = self.sql.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move {
      query_roundtrip(&cmd_tx, sql, bound, true, true).await.map(|rows| FirstRow(rows.0.into_iter().next()))
    }))
  }

  /// Execute as a write. Resolves to `{ changes, lastInsertRowid }`.
  pub fn run<'js>(
    &self,
    ctx: Ctx<'js>,
    params: Opt<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = JsResult<RunResult>>>> {
    let cmd_tx = self.cmd_tx.clone();
    let sql = self.sql.clone();
    let bound = extract_params(params.0).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(with_pending(&ctx, async move { exec_roundtrip(&cmd_tx, sql, bound, true).await }))
  }
}

/// Map a JS `mode` string to open flags. Default (`None`/`"ro"`) is read-only.
fn open_flags(mode: Option<String>) -> Result<OpenFlags, String> {
  let base = OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI;
  let access = match mode.as_deref() {
    None | Some("ro") => OpenFlags::SQLITE_OPEN_READ_ONLY,
    Some("rw") => OpenFlags::SQLITE_OPEN_READ_WRITE,
    Some("rw+") => OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    Some(other) => return Err(format!("unknown database mode {other:?}, expected \"ro\", \"rw\", or \"rw+\"")),
  };
  Ok(base | access)
}

/// Spawn the connection thread and wait for it to open the database.
async fn spawn_database(path: String, flags: OpenFlags) -> Result<Database, String> {
  let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
  let (open_tx, open_rx) = oneshot::channel::<rusqlite::Result<()>>();
  std::thread::Builder::new()
    .name("flux-sqlite".to_string())
    .spawn(move || actor_main(path, flags, cmd_rx, open_tx))
    .map_err(sqlite_err)?;
  match open_rx.await {
    Ok(Ok(())) => Ok(Database { cmd_tx }),
    Ok(Err(e)) => Err(sqlite_err(e)),
    Err(_) => Err(sqlite_err("sqlite thread terminated before opening")),
  }
}

/// The connection thread: owns the `Connection` and serves commands serially.
fn actor_main(
  path: String,
  flags: OpenFlags,
  cmd_rx: Receiver<Command>,
  open_tx: oneshot::Sender<rusqlite::Result<()>>,
) {
  let mut conn = match Connection::open_with_flags(&path, flags) {
    Ok(c) => c,
    Err(e) => {
      let _ = open_tx.send(Err(e));
      return;
    }
  };
  // If the caller went away while we were opening, drop the connection and stop.
  if open_tx.send(Ok(())).is_err() {
    return;
  }

  let mut close_reply = None;
  while let Ok(cmd) = cmd_rx.recv() {
    match cmd {
      Command::Query { sql, params, cached, first_only, reply } => {
        let _ = reply.send(do_query(&conn, &sql, &params, cached, first_only));
      }
      Command::Run { sql, params, cached, reply } => {
        let _ = reply.send(do_run(&conn, &sql, &params, cached));
      }
      Command::Exec { sql, reply } => {
        let _ = reply.send(conn.execute_batch(&sql));
      }
      Command::Transaction { statements, reply } => {
        let _ = reply.send(do_transaction(&mut conn, &statements));
      }
      Command::Close { reply } => {
        close_reply = Some(reply);
        break;
      }
    }
  }
  // Finalize the connection before acking close, so a subsequent open of the
  // same file sees a fully released database.
  drop(conn);
  if let Some(reply) = close_reply {
    let _ = reply.send(());
  }
}

async fn query_roundtrip(
  cmd_tx: &Sender<Command>,
  sql: String,
  params: Vec<SqlValue>,
  cached: bool,
  first_only: bool,
) -> Result<Rows, String> {
  let (reply, rx) = oneshot::channel();
  cmd_tx.send(Command::Query { sql, params, cached, first_only, reply }).map_err(|_| closed_err())?;
  rx.await.map_err(|_| closed_err())?.map_err(sqlite_err)
}

async fn exec_roundtrip(
  cmd_tx: &Sender<Command>,
  sql: String,
  params: Vec<SqlValue>,
  cached: bool,
) -> Result<RunResult, String> {
  let (reply, rx) = oneshot::channel();
  cmd_tx.send(Command::Run { sql, params, cached, reply }).map_err(|_| closed_err())?;
  rx.await.map_err(|_| closed_err())?.map_err(sqlite_err)
}

async fn batch_roundtrip(cmd_tx: &Sender<Command>, sql: String) -> Result<(), String> {
  let (reply, rx) = oneshot::channel();
  cmd_tx.send(Command::Exec { sql, reply }).map_err(|_| closed_err())?;
  rx.await.map_err(|_| closed_err())?.map_err(sqlite_err)
}

async fn transaction_roundtrip(
  cmd_tx: &Sender<Command>,
  statements: Vec<(String, Vec<SqlValue>)>,
) -> Result<TxResults, String> {
  let (reply, rx) = oneshot::channel();
  cmd_tx.send(Command::Transaction { statements, reply }).map_err(|_| closed_err())?;
  rx.await.map_err(|_| closed_err())?.map(TxResults).map_err(sqlite_err)
}

fn do_query(
  conn: &Connection,
  sql: &str,
  params: &[SqlValue],
  cached: bool,
  first_only: bool,
) -> rusqlite::Result<Rows> {
  if cached {
    let mut stmt = conn.prepare_cached(sql)?;
    query_with(&mut stmt, params, first_only)
  } else {
    let mut stmt = conn.prepare(sql)?;
    query_with(&mut stmt, params, first_only)
  }
}

fn query_with(stmt: &mut rusqlite::Statement, params: &[SqlValue], first_only: bool) -> rusqlite::Result<Rows> {
  let col_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
  let n = col_names.len();
  let mut out = Vec::new();
  let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
  while let Some(row) = rows.next()? {
    let mut cells = Vec::with_capacity(n);
    for i in 0..n {
      cells.push((col_names[i].clone(), sqlvalue_from_ref(row.get_ref(i)?)));
    }
    out.push(cells);
    if first_only {
      break;
    }
  }
  Ok(Rows(out))
}

fn do_run(conn: &Connection, sql: &str, params: &[SqlValue], cached: bool) -> rusqlite::Result<RunResult> {
  let changes = if cached {
    let mut stmt = conn.prepare_cached(sql)?;
    stmt.execute(rusqlite::params_from_iter(params.iter()))?
  } else {
    let mut stmt = conn.prepare(sql)?;
    stmt.execute(rusqlite::params_from_iter(params.iter()))?
  };
  Ok(RunResult { changes: changes as i64, last_insert_rowid: conn.last_insert_rowid() })
}

/// Run all statements in a single transaction. The rusqlite `Transaction` guard
/// rolls back on drop, so an error on any statement (via `?`) discards the lot.
fn do_transaction(conn: &mut Connection, statements: &[(String, Vec<SqlValue>)]) -> rusqlite::Result<Vec<RunResult>> {
  let tx = conn.transaction()?;
  let mut results = Vec::with_capacity(statements.len());
  for (sql, params) in statements {
    let changes = tx.execute(sql, rusqlite::params_from_iter(params.iter()))?;
    results.push(RunResult { changes: changes as i64, last_insert_rowid: tx.last_insert_rowid() });
  }
  tx.commit()?;
  Ok(results)
}

fn sqlvalue_from_ref(v: rusqlite::types::ValueRef<'_>) -> SqlValue {
  use rusqlite::types::ValueRef;
  match v {
    ValueRef::Null => SqlValue::Null,
    ValueRef::Integer(i) => SqlValue::Int(i),
    ValueRef::Real(f) => SqlValue::Real(f),
    ValueRef::Text(t) => SqlValue::Text(String::from_utf8_lossy(t).into_owned()),
    ValueRef::Blob(b) => SqlValue::Blob(b.to_vec()),
  }
}

impl rusqlite::types::ToSql for SqlValue {
  fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
    use rusqlite::types::{ToSqlOutput, Value as V, ValueRef};
    Ok(match self {
      SqlValue::Null => ToSqlOutput::Owned(V::Null),
      SqlValue::Int(i) => ToSqlOutput::Owned(V::Integer(*i)),
      SqlValue::Real(f) => ToSqlOutput::Owned(V::Real(*f)),
      SqlValue::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
      SqlValue::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
    })
  }
}

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

fn sqlite_err(e: impl std::fmt::Display) -> String {
  e.to_string()
}

fn closed_err() -> String {
  "database is closed".to_string()
}

impl<'js> IntoJs<'js> for SqlValue {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self {
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
    obj.set(name, val)?;
  }
  Ok(obj)
}

impl<'js> IntoJs<'js> for Rows {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (idx, row) in self.0.into_iter().enumerate() {
      arr.set(idx, row_to_object(ctx, row)?)?;
    }
    Ok(arr.into_value())
  }
}

impl<'js> IntoJs<'js> for FirstRow {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self.0 {
      Some(cells) => Ok(row_to_object(ctx, cells)?.into_value()),
      None => Ok(Value::new_undefined(ctx.clone())),
    }
  }
}

impl<'js> IntoJs<'js> for RunResult {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("changes", self.changes)?;
    obj.set("lastInsertRowid", self.last_insert_rowid)?;
    Ok(obj.into_value())
  }
}

impl<'js> IntoJs<'js> for TxResults {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (idx, r) in self.0.into_iter().enumerate() {
      arr.set(idx, r)?;
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
