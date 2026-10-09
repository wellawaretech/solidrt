declare module "flux:subprocess" {
  /** Options for {@link command}. */
  type CommandOptions = {
    /** Working directory for the child. */
    cwd?: string
    /** Extra env vars, added to / overriding the inherited environment. */
    env?: Record<string, string>
    /**
     * Bytes written to the child's stdin first. With {@link Command.output}
     * stdin is closed after them; with {@link Command.spawn} it stays open for
     * the child's `stdin` stream.
     */
    stdin?: string | Uint8Array
    /** Kill the child if it has not exited within this many milliseconds. */
    timeoutMs?: number
    /**
     * "buffer" returns stdout/stderr as `Uint8Array`; the default returns them
     * as UTF-8 strings.
     */
    encoding?: "buffer" | "utf8"
    /**
     * `spawn()` only: the child outlives this engine and this process. It is
     * never killed on drop or reload, has no stdin/stdout/stderr pipes (all
     * null: `stdout`/`stderr` end at once, the first write to `stdin`
     * rejects) and runs in
     * its own process group, so a Ctrl+C to the parent does not reach it.
     * `pid`, `kill()` and `status()` still work. Cannot combine with `stdin`.
     * What a dev tool uses to launch another runtime instance.
     */
    detached?: boolean
  }

  /** The buffered result of a child run to completion via {@link Command.output}. */
  type CommandOutput = {
    /** Exit code, or `null` if the child was killed by a signal. */
    code: number | null
    /** Signal name that killed the child (Unix), or `null`. */
    signal: string | null
    /** `true` when `code` is 0. */
    success: boolean
    /** Captured stdout. `Uint8Array` when `encoding` is "buffer", else a string. */
    stdout: string | Uint8Array
    /** Captured stderr. `Uint8Array` when `encoding` is "buffer", else a string. */
    stderr: string | Uint8Array
  }

  /** The exit status of a spawned child (the {@link CommandOutput} shape without buffered streams). */
  type CommandStatus = {
    /** Exit code, or `null` if the child was killed by a signal. */
    code: number | null
    /** Signal name that killed the child (Unix), or `null`. */
    signal: string | null
    /** `true` when `code` is 0. */
    success: boolean
  }

  /** A running child process, returned by {@link Command.spawn}. */
  type Child = {
    /** The OS process id, if available. */
    pid: number | undefined
    /** Live stdout as a stream of byte chunks, read as far as it is consumed. */
    stdout: ReadableStream<Uint8Array>
    /** Live stderr as a stream of byte chunks, read as far as it is consumed. */
    stderr: ReadableStream<Uint8Array>
    /**
     * The child's stdin as a stream: strings (as UTF-8) or `Uint8Array`s,
     * written after any {@link CommandOptions.stdin}. A write resolves once
     * the pipe took the bytes, so a producer faster than the child reads
     * sees backpressure. `close()` (and `abort()`) closes the pipe after the
     * writes so far, so the child sees EOF; a later write rejects. A body
     * piped in (`response.body.pipeTo(child.stdin)`) closes it at the end.
     */
    stdin: WritableStream<string | Uint8Array>
    /** Request termination (portable; SIGKILL / TerminateProcess). */
    kill(): void
    /** Resolves with the exit status when the child exits. */
    status(): Promise<CommandStatus>
  }

  /** A parsed, reusable command spec. Created with {@link command}; runnable more than once. */
  type Command = {
    cmd: string
    args: string[]
    /** Run the child to completion, buffering stdout/stderr. */
    output(): Promise<CommandOutput>
    /** Spawn the child and return a handle with live stdout/stderr, a stdin stream, and control. */
    spawn(): Child
  }

  /**
   * Build a command. Arguments are always passed as an array and never through a
   * shell, so there is no shell quoting or injection to reason about, and the JS
   * is identical on every OS.
   *
   * @param cmd   The program to run.
   * @param args  Arguments, passed verbatim (no shell).
   * @param opts  cwd, env, stdin, timeoutMs, encoding.
   */
  export function command(cmd: string, args?: string[], opts?: CommandOptions): Command

  /**
   * Cross-platform PATH lookup (handles Windows PATHEXT / .exe).
   *
   * @param cmd  Binary name to resolve.
   * @returns The absolute path to the resolved executable, or `null` if not found.
   */
  export function which(cmd: string): string | null
}