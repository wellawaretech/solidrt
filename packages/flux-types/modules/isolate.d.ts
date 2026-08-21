declare module "flux:isolate" {
  /**
   * A value that can cross to or from an isolate: null (undefined becomes
   * null), boolean, number, string, any typed-array view (arrives as a copy
   * of the same kind: a Float32Array stays a Float32Array) or ArrayBuffer
   * (arrives as a Uint8Array copy), arrays and plain objects of these.
   * Anything else (functions, class instances, Date/Map/Set, BigInt, symbols)
   * throws a TypeError as an argument and rejects the call as a result.
   */
  type Sendable =
    | null
    | undefined
    | boolean
    | number
    | string
    | ArrayBuffer
    | ArrayBufferView
    | Sendable[]
    | { [key: string]: Sendable }

  /** Options for {@link isolate}. */
  type IsolateOptions = {
    /** The child's `flux:process` `argv`. Default `[]`. */
    args?: string[]
    /**
     * Heap limit in bytes for the child runtime. Once reached, allocations in
     * the child fail with an out-of-memory error where they happen instead of
     * growing the process; an exit this causes is observable via `exited`.
     * Applies to this child only (not to isolates it spawns itself). Default:
     * unlimited.
     */
    memoryLimit?: number
  }

  /**
   * The isolate view of a module's exports: every function returns a Promise
   * of what it returns in the isolate (an async function stays as it is); an
   * async generator returns a stream to iterate with `for await` (one item is
   * pulled per step; `break` ends the generator in the isolate); plus
   * `terminate()`. Non-function exports are not reachable.
   */
  type Isolated<T> = {
    [K in keyof T as T[K] extends (...args: any[]) => any ? K : never]: T[K] extends (...args: infer A) => infer R
      ? 0 extends 1 & R // an `any` result (untyped module) is a plain call, not a stream
        ? (...args: A) => Promise<any>
        : R extends AsyncIterable<infer Y>
          ? (...args: A | [...A, AbortSignal]) => AsyncIterableIterator<Y>
          : R extends Generator<any, any, any> // sync generators do not stream: the call rejects
            ? never
            : (...args: A | [...A, AbortSignal]) => Promise<Awaited<R>>
      : never
  } & {
    /**
     * Kill the child now: busy JS is interrupted, the child runtime is
     * dropped, pending and later calls reject. A handle that never called
     * anything never spawned.
     */
    terminate(): void
    /**
     * Settles once the child is gone: with the uncaught error that ended it,
     * or `null` after `terminate()` or a clean end. Reading `exited` is a
     * first use (it starts the child like a call does) and keeps the runtime
     * watching the child - the loop stays open until the child exits, so an
     * exit is noticed with no call in flight. Each read returns an
     * equivalent promise.
     */
    readonly exited: Promise<string | null>
  }

  /**
   * A handle on an isolate module: a `"use isolate"` module in a SolidRT
   * project (id = its path relative to the source root, without extension),
   * or `isolates/<id>.bin`/`.js` next to the entry under standalone flux. Each property is
   * an async function that runs the export of that name in a second runtime
   * on its own thread (own heap, own event loop, the non-gui `flux:*`
   * modules). Arguments and results are copied ({@link Sendable}).
   *
   * The child starts on first use (a call, or reading `exited`) and lives
   * until `terminate()` or the parent's end; module state persists between
   * calls; each `isolate()` call is its own instance. Calls start in call order and run concurrently, as
   * the same functions would in-process: a sync export runs to completion
   * before anything else (one thread), an async export lets other calls and
   * stream steps run at each `await`; an export that must not interleave with
   * itself serialises inside the module. A throw in the export rejects that
   * call (a throw in a generator rejects the pending step) with the error
   * rebuilt from its data: `name`, `message` and `stack` carry over, `e
   * instanceof RangeError` holds for the standard error types (a custom error
   * class arrives as an `Error` with its `name`), and the `cause` chain
   * carries over - each cause another rebuilt error or a {@link Sendable}
   * value (an unsendable cause is dropped; the chain is capped). A thrown
   * non-Error rejects with the thrown value itself when it is sendable, else
   * with an `Error` describing it. An uncaught
   * error that ends the child rejects pending and later calls with a message
   * naming it. Awaiting a stream call rejects; iterating a plain call rejects. An
   * open stream keeps both runtimes alive until it ends, `break`s, or the
   * child is terminated. A sync generator export rejects when called: only
   * async generators stream.
   *
   * An `AbortSignal` among a call's arguments (anywhere in the list; at most
   * one, more throw) is consumed as the call's signal rather than sent: the
   * export sees only the other arguments. On a plain call, aborting stops
   * the waiting - the call rejects with `signal.reason` and the eventual
   * result is dropped - but does not interrupt the export; interrupting is
   * `terminate()`'s job. On a stream, aborting acts as `return()`: the
   * generator ends in the isolate (its `finally` runs) and the `for await`
   * loop finishes cleanly, like a `break` from outside it. A call on an
   * already-aborted signal rejects without sending anything (or starting the
   * child).
   *
   * Reserved names: `terminate`, `exited`, `then`.
   */
  export function isolate<T = Record<string, (...args: any[]) => any>>(id: string, opts?: IsolateOptions): Isolated<T>
}
