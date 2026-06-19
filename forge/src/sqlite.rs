//! Engine-free SQLite core.
//!
//! The scripting-engine-independent half of `flux:sqlite`: the connection actor,
//! the command protocol, query execution, and the owned value/result types that
//! cross the channel. It names no scripting-engine types; the marshalling layer
//! (`plugins/flux/sqlite.rs`) decodes JS args into these types, drives the
//! `SqliteConnection` methods, and encodes the results back to JS. Destined for
//! the `forge` crate (see REDESIGN.md).
//!
//! Each `SqliteConnection` owns a dedicated OS thread that holds the
//! `rusqlite::Connection` for its whole life. A method marshals a `Command` over
//! a channel, the thread runs it synchronously on the connection, and the reply
//! comes back over a oneshot the async method awaits. So queries run off the JS
//! thread without blocking it, and the connection is never shared across threads.
//! This actor shape is also what a future transaction feature needs: a
//! transaction can "check out" the connection thread for the duration of
//! BEGIN..COMMIT while other commands wait, which a shared-pool model cannot do
//! safely across `await` points.

use std::sync::mpsc::{Receiver, Sender};

use rusqlite::{Connection, OpenFlags};
use tokio::sync::oneshot;

/// An owned SQLite value, used both for bound parameters (JS -> SQL) and for
/// decoded result cells (SQL -> JS). Owned so it can cross the channel.
pub enum SqlValue {
  Null,
  Int(i64),
  Real(f64),
  Text(String),
  Blob(Vec<u8>),
}

/// Result rows: each row is a list of (column name, value) pairs.
pub struct Rows(pub Vec<Vec<(String, SqlValue)>>);

/// A single row, or none. Returned by `get`.
pub struct FirstRow(pub Option<Vec<(String, SqlValue)>>);

/// The outcome of a write: rows changed and the last inserted rowid.
pub struct RunResult {
  pub changes: i64,
  pub last_insert_rowid: i64,
}

/// One RunResult per statement in a transaction batch.
pub struct TxResults(pub Vec<RunResult>);

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

/// A handle to a database connection running on its own thread. Cheaply cloned
/// (it is just the command channel), so the JS `Database` and each `Statement`
/// share one connection.
#[derive(Clone)]
pub struct SqliteConnection {
  cmd_tx: Sender<Command>,
}

impl SqliteConnection {
  /// Open a database. `mode` selects access: `None`/`"ro"` (read-only, file must
  /// exist), `"rw"` (read-write, must exist), `"rw+"` (read-write, create if
  /// missing). Spawns the connection thread and waits for it to open.
  pub async fn connect(path: String, mode: Option<String>) -> Result<Self, String> {
    let flags = open_flags(mode)?;
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
    let (open_tx, open_rx) = oneshot::channel::<rusqlite::Result<()>>();
    std::thread::Builder::new()
      .name("flux-sqlite".to_string())
      .spawn(move || actor_main(path, flags, cmd_rx, open_tx))
      .map_err(sqlite_err)?;
    match open_rx.await {
      Ok(Ok(())) => Ok(SqliteConnection { cmd_tx }),
      Ok(Err(e)) => Err(sqlite_err(e)),
      Err(_) => Err(sqlite_err("sqlite thread terminated before opening")),
    }
  }

  /// All matching rows.
  pub async fn query(&self, sql: String, params: Vec<SqlValue>, cached: bool) -> Result<Rows, String> {
    self.call(|reply| Command::Query { sql, params, cached, first_only: false, reply }).await
  }

  /// The first matching row, or none.
  pub async fn get(&self, sql: String, params: Vec<SqlValue>, cached: bool) -> Result<FirstRow, String> {
    let rows = self.call(|reply| Command::Query { sql, params, cached, first_only: true, reply }).await?;
    Ok(FirstRow(rows.0.into_iter().next()))
  }

  /// Execute a write. `cached` uses the connection's prepared-statement cache.
  pub async fn run(&self, sql: String, params: Vec<SqlValue>, cached: bool) -> Result<RunResult, String> {
    self.call(|reply| Command::Run { sql, params, cached, reply }).await
  }

  /// Run a multi-statement script with no parameters.
  pub async fn exec(&self, sql: String) -> Result<(), String> {
    self.call(|reply| Command::Exec { sql, reply }).await
  }

  /// Run a batch of `[sql, params]` statements in one BEGIN/COMMIT.
  pub async fn transaction(&self, statements: Vec<(String, Vec<SqlValue>)>) -> Result<TxResults, String> {
    self.call(|reply| Command::Transaction { statements, reply }).await.map(TxResults)
  }

  /// Close the connection, releasing it. Safe to call more than once.
  pub async fn close(&self) {
    let (reply, rx) = oneshot::channel();
    // A send error means the thread already exited: treat as closed.
    if self.cmd_tx.send(Command::Close { reply }).is_ok() {
      let _ = rx.await;
    }
  }

  /// Send a command and await its reply, mapping a closed channel or a sqlite
  /// failure to a message string. The one request/reply roundtrip every fallible
  /// method shares.
  async fn call<T>(&self, make: impl FnOnce(oneshot::Sender<rusqlite::Result<T>>) -> Command) -> Result<T, String> {
    let (reply, rx) = oneshot::channel();
    self.cmd_tx.send(make(reply)).map_err(|_| closed_err())?;
    rx.await.map_err(|_| closed_err())?.map_err(sqlite_err)
  }
}

/// Map a `mode` string to open flags. Default (`None`/`"ro"`) is read-only.
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

fn sqlite_err(e: impl std::fmt::Display) -> String {
  e.to_string()
}

fn closed_err() -> String {
  "database is closed".to_string()
}