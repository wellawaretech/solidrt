//! The `flux:sqlite` module: an async SQLite binding for flux.
//!
//! The API is loosely modeled on Bun's `bun:sqlite`, but is NOT a drop-in
//! match. The differences below are intentional (current scope) or fundamental
//! (async vs sync). Keep this list current as the plugin grows.
//!
//! Fundamental difference: Bun's API is fully synchronous (`new Database()`,
//! `query.all()`); ours is async, running queries off the JS thread via tokio.
//! So construction is `await Database.connect(path)` and `new Database()`
//! throws. Bun's `db.transaction(fn)` wraps a SYNC callback and therefore does
//! not translate directly to our model; any transaction support we add will
//! need a different (async) shape.
//!
//! Matches Bun:
//! - Named export `Database`, in-memory (`:memory:`) databases.
//! - Creates the file if missing.
//! - Positional `?` parameters; rows returned as plain objects.
//! - `BLOB` <-> `Uint8Array`.
//!
//! Differs from Bun (current scope - may change):
//! - Construction is async: `await Database.connect(path)`, not `new Database()`.
//! - Single method `db.query(sql, params)` always returns ALL rows as objects.
//!   No statement/execution split, so we lack Bun's `.get()` (first row),
//!   `.run()`, `.values()` (array-of-arrays), and `.iterate()`.
//! - No `lastInsertRowid` / `changes` from writes (consequence of no `.run()`).
//!   Cannot retrieve an auto-generated INSERT id yet.
//! - No prepared-statement reuse/caching; each `query()` recompiles the SQL.
//! - Params are passed as an array (`query(sql, [1, 2])`), not spread args.
//! - Positional `?` only; no named params (`$p` / `:p` / `@p`) or object binding.
//! - Integers always go i64 -> JS number; no `safeIntegers`/bigint mode, so
//!   values above 2^53 lose precision silently.
//! - Errors surface as generic `IO Error: ...`, not a typed SQLite error.
//!
//! Not implemented at all: transactions, `close()`, `constants`,
//! `serialize()`/`deserialize()`, `loadExtension()`, `.as(Class)`.

use std::io;
use std::str::FromStr;

use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, IntoJs, JsLifetime, Object, TypedArray, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};

/// An owned SQLite value, used both for bound parameters (JS -> SQL) and for
/// decoded result cells (SQL -> JS). Owned so it can cross the async boundary.
enum SqlValue {
  Null,
  Int(i64),
  Real(f64),
  Text(String),
  Blob(Vec<u8>),
}

/// Result rows: each row is a list of (column name, value) pairs.
pub struct Rows(Vec<Vec<(String, SqlValue)>>);

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Database")]
pub struct Database {
  #[qjs(skip_trace)]
  pool: SqlitePool,
}

#[rquickjs::methods]
impl Database {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<Database> {
    Err(ctx.throw(
      rquickjs::String::from_str(ctx.clone(), "use Database.connect() to open a database")?
        .into(),
    ))
  }

  #[qjs(static)]
  pub fn connect<'js>(
    ctx: Ctx<'js>,
    path: String,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = rquickjs::Result<Database>>>> {
    let pending = ctx.userdata::<crate::pending::PendingOps>().expect("pending ops").clone();
    Ok(Promised(async move {
      pending.hold();
      let r = open_pool(&path).await;
      pending.release();
      r
    }))
  }

  pub fn query<'js>(
    &self,
    ctx: Ctx<'js>,
    sql: String,
    params: Opt<Array<'js>>,
  ) -> rquickjs::Result<Promised<impl std::future::Future<Output = rquickjs::Result<Rows>>>> {
    let pool = self.pool.clone();
    let bound = extract_params(params.0)?;
    let pending = ctx.userdata::<crate::pending::PendingOps>().expect("pending ops").clone();
    Ok(Promised(async move {
      pending.hold();
      let r = run_query(&pool, &sql, bound).await;
      pending.release();
      r
    }))
  }
}

async fn open_pool(path: &str) -> rquickjs::Result<Database> {
  let opts = SqliteConnectOptions::from_str(path)
    .map_err(sqlite_err)?
    .create_if_missing(true);
  let pool = SqlitePoolOptions::new()
    .connect_with(opts)
    .await
    .map_err(sqlite_err)?;
  Ok(Database { pool })
}

async fn run_query(pool: &SqlitePool, sql: &str, params: Vec<SqlValue>) -> rquickjs::Result<Rows> {
  let mut q = sqlx::query(sql);
  for p in params {
    q = match p {
      SqlValue::Null => q.bind(None::<i64>),
      SqlValue::Int(i) => q.bind(i),
      SqlValue::Real(f) => q.bind(f),
      SqlValue::Text(s) => q.bind(s),
      SqlValue::Blob(b) => q.bind(b),
    };
  }

  let rows = q.fetch_all(pool).await.map_err(sqlite_err)?;

  let mut out = Vec::with_capacity(rows.len());
  for row in &rows {
    let mut cells = Vec::with_capacity(row.len());
    for col in row.columns() {
      let i = col.ordinal();
      let name = col.name().to_string();
      let (is_null, type_name) = {
        let raw = row.try_get_raw(i).map_err(sqlite_err)?;
        (raw.is_null(), raw.type_info().name().to_string())
      };
      let val = if is_null {
        SqlValue::Null
      } else {
        match type_name.as_str() {
          "INTEGER" | "BIGINT" | "INT8" | "INT" => {
            SqlValue::Int(row.try_get(i).map_err(sqlite_err)?)
          }
          "REAL" | "DOUBLE" | "FLOAT" | "NUMERIC" => {
            SqlValue::Real(row.try_get(i).map_err(sqlite_err)?)
          }
          "BLOB" => SqlValue::Blob(row.try_get(i).map_err(sqlite_err)?),
          // TEXT and anything else: decode as text.
          _ => SqlValue::Text(row.try_get(i).map_err(sqlite_err)?),
        }
      };
      cells.push((name, val));
    }
    out.push(cells);
  }
  Ok(Rows(out))
}

/// Convert a JS array of bind parameters into owned SqlValues.
fn extract_params(params: Option<Array<'_>>) -> rquickjs::Result<Vec<SqlValue>> {
  let Some(arr) = params else {
    return Ok(Vec::new());
  };
  let mut out = Vec::with_capacity(arr.len());
  for v in arr.iter::<Value>() {
    let v = v?;
    out.push(js_to_sql(v)?);
  }
  Ok(out)
}

fn js_to_sql(v: Value<'_>) -> rquickjs::Result<SqlValue> {
  if v.is_null() || v.is_undefined() {
    Ok(SqlValue::Null)
  } else if let Some(b) = v.as_bool() {
    Ok(SqlValue::Int(b as i64))
  } else if let Some(i) = v.as_int() {
    Ok(SqlValue::Int(i as i64))
  } else if let Some(f) = v.as_float() {
    Ok(SqlValue::Real(f))
  } else if let Some(s) = v.as_string() {
    Ok(SqlValue::Text(s.to_string()?))
  } else if let Ok(ta) = TypedArray::<u8>::from_value(v.clone()) {
    Ok(SqlValue::Blob(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default()))
  } else {
    Err(rquickjs::Error::Io(io::Error::new(
      io::ErrorKind::InvalidInput,
      "unsupported SQL parameter type",
    )))
  }
}

fn sqlite_err(e: impl std::fmt::Display) -> rquickjs::Error {
  rquickjs::Error::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
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

impl<'js> IntoJs<'js> for Rows {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (idx, row) in self.0.into_iter().enumerate() {
      let obj = Object::new(ctx.clone())?;
      for (name, val) in row {
        obj.set(name, val)?;
      }
      arr.set(idx, obj)?;
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
    let ctor = Class::<Database>::create_constructor(ctx)?
      .expect("Database class has a constructor");
    exports.export("Database", ctor)?;
    Ok(())
  }
}