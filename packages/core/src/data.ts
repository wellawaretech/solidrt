// Reactive SQLite. A query is a reactive value: it re-runs when any table it
// reads is written on this connection, and every consumer updates through
// normal Solid reactivity. SQLite itself reports both sides of the dependency
// graph - the statement's read-set comes from `stmt.tables()` (SQLite's
// authorizer) and writes come from `db.onWrite` (SQLite's update hook, so
// trigger and cascade writes are included). No SQL parsing, no manual
// declarations, no wrapper: queries take the plain `flux:sqlite` Database.
//
// The contract, in one sentence: a query re-runs when any table it reads is
// written on this connection. Granularity is per table; writes from another
// connection or process are not seen; WITHOUT ROWID tables do not report.
//
// The imperative primitives live in the `flux:sqlite` module and are
// framework-neutral. This module is only the thin Solid binding on top (a
// version signal per table, an async memo per query).

import { createMemo, createSignal } from "@solidjs/signals"
import type { Signal, SourceAccessor } from "@solidjs/signals"
import type { Database, Row, SqlParam, Statement } from "flux:sqlite"

export { Database } from "flux:sqlite"
export type { OpenMode, Row, RunResult, SqlParam, SqlValue, Statement } from "flux:sqlite"

/** Bind parameters: a plain array, or an accessor for reactive params. */
export type Params = SqlParam[] | (() => SqlParam[])

// Per-database dependency tracking, created lazily on the first createQuery:
// one version signal per table, bumped from the connection's write events.
// The function reads (and lazily creates) the version signal of one table.
// Keyed by the plain Database so there is no wrapper type to hand around.
let trackers = new WeakMap<Database, (table: string) => void>()

function trackerFor(db: Database): (table: string) => void {
  let track = trackers.get(db)
  if (track) return track

  let versions = new Map<string, Signal<number>>()
  // A write bumps only existing signals: a signal that was never read has no
  // subscribers, so there is nothing to invalidate. The subscription lives
  // for the connection's life; close() ends it.
  db.onWrite((tables) => {
    for (let table of tables) {
      let version = versions.get(table)
      if (version) version[1]((v) => v + 1)
    }
  })
  track = (table) => {
    let version = versions.get(table)
    if (!version) {
      version = createSignal(0)
      versions.set(table, version)
    }
    version[0]()
  }
  trackers.set(db, track)
  return track
}

/**
 * A reactive query: an accessor over the matching rows that re-runs when any
 * table the statement reads is written on this connection. Reads surface as
 * pending until the first result lands - wrap in `<Loading>` or default with
 * `?? []`. Pass params as an accessor to also re-run when they change.
 */
export function createQuery(db: Database, sql: string, params?: Params): SourceAccessor<Row[]> {
  return statementQuery(db, sql, params, (stmt, p) => stmt.all(p))
}

/**
 * Like {@link createQuery}, but for single-row reads: resolves to the first
 * matching row, or `undefined` when there is none.
 */
export function createQueryRow(
  db: Database,
  sql: string,
  params?: Params,
): SourceAccessor<Row | undefined> {
  return statementQuery(db, sql, params, (stmt, p) => stmt.get(p))
}

function statementQuery<T>(
  db: Database,
  sql: string,
  params: Params | undefined,
  run: (stmt: Statement, params?: SqlParam[]) => Promise<T>,
): SourceAccessor<T> {
  let track = trackerFor(db)
  let stmt = db.query(sql)
  // The read-set arrives async (one authorizer round-trip on the connection
  // thread); computed once, it never changes for a given statement. Reads of
  // the query below surface as pending until it lands, then the query runs -
  // so the first execution already subscribes to its tables and no write can
  // slip between subscription and execution.
  let tables = createMemo(() => stmt.tables())
  return createMemo(() => {
    for (let table of tables()) track(table)
    return run(stmt, typeof params === "function" ? params() : params)
  })
}
