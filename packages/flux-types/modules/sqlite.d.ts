declare module "flux:sqlite" {
  /** Values accepted as bound parameters. booleans bind as 0/1. */
  export type SqlParam = null | boolean | number | string | Uint8Array
  /** Values returned in result rows. BLOB comes back as Uint8Array. */
  export type SqlValue = null | number | string | Uint8Array
  export type Row = Record<string, SqlValue>

  /** The outcome of a write. */
  export type RunResult = { changes: number; lastInsertRowid: number }

  /**
   * A reusable prepared statement. Created with {@link Database.query}; its
   * executions reuse the compiled statement (cached on the connection).
   */
  export class Statement {
    /** Run the statement and resolve to all matching rows. */
    all(params?: SqlParam[]): Promise<Row[]>
    /** Run the statement and resolve to the first row, or `undefined`. */
    get(params?: SqlParam[]): Promise<Row | undefined>
    /** Run the statement as a write and resolve to its {@link RunResult}. */
    run(params?: SqlParam[]): Promise<RunResult>
    /**
     * The tables this statement reads, sorted (SQLite's authorizer, captured
     * during a compile; the statement is never run). Includes tables reached
     * through views and subqueries. Pair with {@link Database.onWrite} to know
     * when a re-read could return different rows.
     */
    tables(): Promise<string[]>
  }

  /**
   * Open mode: "ro" (default, read-only, must exist), "rw" (read-write, must
   * exist), "rw+" (read-write, create if missing).
   */
  export type OpenMode = "ro" | "rw" | "rw+"

  export class Database {
    /**
     * Open a connection to the database at `path`.
     *
     * @param path  Database file path.
     * @param mode  Open mode; defaults to "ro".
     */
    static open(path: string, mode?: OpenMode): Promise<Database>
    /** Create a reusable prepared statement (synchronous; compiles on first run). */
    query(sql: string): Statement
    /** One-shot write; uses plain prepare (no caching). */
    run(sql: string, params?: SqlParam[]): Promise<RunResult>
    /** Run a multi-statement script (no params), e.g. schema setup / migrations. */
    exec(sql: string): Promise<void>
    /**
     * Run a batch of [sql, params] statements in one transaction (BEGIN/COMMIT,
     * ROLLBACK on any error). Resolves to one result per statement. Statements
     * must be writes/DDL. Cannot branch on intermediate results.
     */
    transaction(statements: [string, SqlParam[]?][]): Promise<RunResult[]>
    /**
     * Subscribe to writes on this connection. After each command that changed
     * rows, `callback` gets one call with the sorted names of the tables
     * touched (SQLite's update hook, so trigger and cascade writes are
     * included). Returns an unsubscribe function.
     *
     * Contract: only THIS connection's writes are seen (another connection or
     * process writing the same file does not report); SQLite does not fire
     * the hook for WITHOUT ROWID tables; a rolled-back transaction may still
     * report its tables (a spurious re-read, never a stale one). A full-table
     * `DELETE FROM t` reports correctly: the connection disables SQLite's
     * truncate optimization, trading row-by-row deletion for a hook that
     * cannot be silently skipped.
     */
    onWrite(callback: (tables: string[]) => void): () => void
    /** Close the connection. */
    close(): Promise<void>
  }
}