declare module "flux:process" {
  /**
   * The arguments the app was started with; empty when there are none.
   * App arguments only: no executable path, no script path (deliberately
   * simpler than Node/Bun's two leading entries), so `argv[0]` is the first
   * argument.
   */
  export let argv: string[]
  /** The OS process id of this process (what a registry record or a `kill` names). */
  export let pid: number
  /** The host OS: "darwin", "win32", "linux", "android", ... */
  export let platform: string
  /** The CPU architecture: "x64", "arm64", ... */
  export let arch: string
  /**
   * Current-process memory usage. `rss` is the resident set size in bytes.
   * (Node also reports heapTotal/heapUsed/external/arrayBuffers; only rss is
   * provided for now.)
   */
  export function memoryUsage(): { rss: number }
  /**
   * The current user's home directory, or `null` when the environment does
   * not name one (HOME on unix, USERPROFILE on Windows).
   */
  export function homedir(): string | null
  /**
   * Terminate another process. Portable (SIGKILL / TerminateProcess), so
   * there is no signal argument, unlike Node's `process.kill(pid, signal)`.
   *
   * @param pid  The OS process id.
   * @returns `true` when the process was terminated; `false` when it does not
   *          exist or the OS refused.
   */
  export function kill(pid: number): boolean
  /**
   * Whether a process with `pid` exists. The `process.kill(pid, 0)` idiom
   * under its own name. A zombie (exited, not yet reaped) counts as gone.
   *
   * @param pid  The OS process id.
   */
  export function alive(pid: number): boolean
  /**
   * The process environment, snapshotted when the module is evaluated: a
   * plain object, not Node's live and writable `process.env`.
   */
  export let env: Record<string, string | undefined>
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