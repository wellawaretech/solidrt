declare global {
  type FluxDirEntry = {
    name: string
    type: "file" | "directory" | "symlink" | "other"
  }

  type FluxDir = {
    path: string
    entries(): Promise<FluxDirEntry[]>
    exists(): Promise<boolean>
  }

  type FluxFileStat = {
    size: number
    type: string
    mtime?: number
  }

  type FluxFile = {
    path: string
    text(): Promise<string>
    bytes(): Promise<Uint8Array>
    json(): Promise<any>
    exists(): Promise<boolean>
    stat(): Promise<FluxFileStat>
  }

  type FluxServeOptions = {
    port: number
    fetch?: (req: Request) => Response | string | Promise<Response | string>
  }

  let Flux: {
    on(event: string, callback: (data: any) => void): () => void
    once(event: string, callback: (data: any) => void): () => void
    dir(path: string): FluxDir
    file(path: string): FluxFile
    write(path: string, data: string | Uint8Array): Promise<void>
    serve(options: FluxServeOptions): void
  }
}

declare module "flux:sqlite" {
  // Values accepted as bound parameters. booleans bind as 0/1.
  type SqlParam = null | boolean | number | string | Uint8Array
  // Values returned in result rows. BLOB comes back as Uint8Array.
  type SqlValue = null | number | string | Uint8Array
  type Row = Record<string, SqlValue>

  // The outcome of a write.
  type RunResult = { changes: number; lastInsertRowid: number }

  // A reusable prepared statement. Created with db.query(sql); its executions
  // reuse the compiled statement (cached on the connection).
  export class Statement {
    all(params?: SqlParam[]): Promise<Row[]>
    get(params?: SqlParam[]): Promise<Row | undefined>
    run(params?: SqlParam[]): Promise<RunResult>
  }

  // Open mode: "ro" (default, read-only, must exist), "rw" (read-write, must
  // exist), "rw+" (read-write, create if missing).
  type OpenMode = "ro" | "rw" | "rw+"

  export class Database {
    static connect(path: string, mode?: OpenMode): Promise<Database>
    // Create a reusable prepared statement (synchronous; compiles on first run).
    query(sql: string): Statement
    // One-shot write; uses plain prepare (no caching).
    run(sql: string, params?: SqlParam[]): Promise<RunResult>
    // Run a multi-statement script (no params), e.g. schema setup / migrations.
    exec(sql: string): Promise<void>
    // Run a batch of [sql, params] statements in one transaction (BEGIN/COMMIT,
    // ROLLBACK on any error). Resolves to one result per statement. Statements
    // must be writes/DDL. Cannot branch on intermediate results.
    transaction(statements: [string, SqlParam[]?][]): Promise<RunResult[]>
    close(): Promise<void>
  }
}

export {}