declare module "flux:process" {
  /**
   * The arguments the app was started with; empty when there are none.
   * App arguments only: no executable path, no script path (deliberately
   * simpler than Node/Bun's two leading entries), so `argv[0]` is the first
   * argument.
   */
  export let argv: string[]
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
   * High-resolution real-time clock for timing synchronous work (Node's
   * `process.hrtime`; only the `bigint()` form is offered, not the legacy
   * `[seconds, nanoseconds]` tuple). Unlike `performance.now()`, which in a
   * GUI runtime is the paced app timeline and does not advance within a
   * frame, this is monotonic wall time at nanosecond resolution.
   *
   * @example
   * let t0 = hrtime.bigint()
   * // ... synchronous work ...
   * let ms = Number(hrtime.bigint() - t0) / 1e6
   */
  export let hrtime: {
    /** Nanoseconds since an arbitrary fixed origin, as a bigint. */
    bigint(): bigint
  }
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