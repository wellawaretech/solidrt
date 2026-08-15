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
          ? (...args: A) => AsyncIterableIterator<Y>
          : (...args: A) => Promise<Awaited<R>>
      : never
  } & {
    /**
     * Kill the child now: busy JS is interrupted, the child runtime is
     * dropped, pending and later calls reject. A handle that never called
     * anything never spawned.
     */
    terminate(): void
  }

  /**
   * A handle on an isolate module: a `"use isolate"` module in a SolidRT
   * project (id = its path relative to the source root, without extension),
   * or `<id>.js` next to the entry under standalone flux. Each property is
   * an async function that runs the export of that name in a second runtime
   * on its own thread (own heap, own event loop, the non-gui `flux:*`
   * modules). Arguments and results are copied ({@link Sendable}).
   *
   * The child starts on the first call and lives until `terminate()` or the
   * parent's end; module state persists between calls; each `isolate()` call
   * is its own instance. Calls start in call order and run concurrently, as
   * the same functions would in-process: a sync export runs to completion
   * before anything else (one thread), an async export lets other calls and
   * stream steps run at each `await`; an export that must not interleave with
   * itself serialises inside the module. A throw in the export rejects that
   * call (a throw in a generator rejects the pending step); an uncaught error
   * that ends the child rejects pending and later calls with a message naming
   * it. Awaiting a stream call rejects; iterating a plain call rejects. An
   * open stream keeps both runtimes alive until it ends, `break`s, or the
   * child is terminated. Reserved names: `terminate`, `then`.
   */
  export function isolate<T = Record<string, (...args: any[]) => any>>(id: string, opts?: IsolateOptions): Isolated<T>
}
