declare let Flux: {
  version: string
}

declare module "flux:process" {
  /**
   * The program's command-line arguments. `argv[0]` is the script path;
   * `argv[1]` onward are the user-supplied arguments.
   */
  export let argv: string[]
  /**
   * Listen for an OS signal. The callback receives the signal name. Returns an
   * unsubscribe function. Unix only; a no-op elsewhere.
   *
   * @param signal  One of "SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT", "SIGUSR1",
   *                "SIGUSR2".
   * @param callback  Invoked on each delivery with the signal name.
   * @returns An unsubscribe function.
   */
  export function on(signal: string, callback: (signal: string) => void): () => void
  /**
   * Like {@link on}, but the listener fires at most once and then unsubscribes.
   *
   * @param signal  One of "SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT", "SIGUSR1",
   *                "SIGUSR2".
   * @param callback  Invoked once with the signal name.
   * @returns An unsubscribe function.
   */
  export function once(signal: string, callback: (signal: string) => void): () => void
}

declare module "flux:path" {
  /**
   * Resolves `path` against the trusted base directory `base`, returning the
   * absolute result only if it stays inside `base`; otherwise `null`. Fusing
   * normalization and containment means a `..`-laden or absolute `path` that
   * would escape `base` is rejected rather than silently resolved.
   *
   * Purely lexical: it does not resolve symlinks, so a symlink inside `base`
   * pointing out of it is not caught.
   *
   * @param base  Trusted root directory. Relative values resolve against cwd.
   * @param path  Untrusted path to place within `base`.
   * @returns The contained absolute path, or `null` if it would escape `base`.
   *
   * @example
   * let target = resolveWithin(".", req.params.page)
   * if (!target) return new Response("Not found", { status: 404 })
   */
  export function resolveWithin(base: string, path: string): string | null

  /**
   * Joins and normalizes path `segments`. Lexical only, with no containment
   * guarantee; use `resolveWithin` when a segment is untrusted.
   */
  export function join(...segments: string[]): string
}

declare module "flux:http" {
  /** Path parameters captured from a route pattern (e.g. ":page"). */
  type RouteParams = Record<string, string>

  /** The request passed to a route handler, with captured route params. */
  type FluxRequest = Request & {
    params: RouteParams
  }

  /** Handles a matched route, returning a `Response` (or a promise of one). */
  type RouteHandler = (req: FluxRequest) => Response | Promise<Response>

  type ServeOptions = {
    /** Port to listen on. */
    port?: number
    /** Hostname/interface to bind. Defaults to all interfaces. */
    hostname?: string
    /**
     * Route table keyed by path pattern. Patterns may contain `:name`
     * segments, exposed on `req.params`.
     */
    routes: Record<string, RouteHandler>
  }

  type Server = {
    port: number
    hostname: string
    /** Stop accepting connections and shut the server down. */
    stop(): void
  }

  /**
   * Start an HTTP server with the given route table.
   *
   * @param options  Port, hostname, and routes.
   * @returns The running {@link Server}.
   */
  export function serve(options: ServeOptions): Server
}

declare module "flux:fs" {
  type DirEntry = {
    name: string
    type: "file" | "directory" | "symlink" | "other"
  }

  type FileStat = {
    size: number
    type: string
    mtime?: number
  }

  type FluxFile = {
    path: string
    /** Read the whole file as UTF-8 text. */
    text(): Promise<string>
    /** Read the whole file as raw bytes. */
    bytes(): Promise<Uint8Array>
    /** Read and parse the file as JSON. */
    json(): Promise<any>
    /** Resolve to whether the file exists. */
    exists(): Promise<boolean>
    /** Resolve to the file's metadata (size, type, mtime). */
    stat(): Promise<FileStat>
    /** Write `data`, replacing any existing contents. */
    write(data: string | Uint8Array): Promise<void>
  }

  type FluxDir = {
    path: string
    /** List the directory's immediate entries (non-recursive). */
    entries(): Promise<DirEntry[]>
    /** Resolve to whether the directory exists. */
    exists(): Promise<boolean>
  }

  /**
   * Reference a file by path. Lazy: no I/O happens until a method is called.
   *
   * @param path  Path to the file.
   */
  export function file(path: string): FluxFile
  /**
   * Reference a directory by path. Lazy: no I/O happens until a method is
   * called.
   *
   * @param path  Path to the directory.
   */
  export function dir(path: string): FluxDir
}

declare module "flux:sqlite" {
  /** Values accepted as bound parameters. booleans bind as 0/1. */
  type SqlParam = null | boolean | number | string | Uint8Array
  /** Values returned in result rows. BLOB comes back as Uint8Array. */
  type SqlValue = null | number | string | Uint8Array
  type Row = Record<string, SqlValue>

  /** The outcome of a write. */
  type RunResult = { changes: number; lastInsertRowid: number }

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
  }

  /**
   * Open mode: "ro" (default, read-only, must exist), "rw" (read-write, must
   * exist), "rw+" (read-write, create if missing).
   */
  type OpenMode = "ro" | "rw" | "rw+"

  export class Database {
    /**
     * Open a connection to the database at `path`.
     *
     * @param path  Database file path.
     * @param mode  Open mode; defaults to "ro".
     */
    static connect(path: string, mode?: OpenMode): Promise<Database>
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
    /** Close the connection. */
    close(): Promise<void>
  }
}