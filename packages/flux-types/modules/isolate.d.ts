declare module "flux:isolate" {
  /**
   * A value that can cross a port: null (undefined becomes null), boolean,
   * number, string, ArrayBuffer or any typed-array view (arrives as a
   * Uint8Array copy), arrays and plain objects of these. Anything else
   * (functions, class instances, Date/Map/Set, BigInt, symbols) throws a
   * TypeError at `send`.
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

  /** Options for {@link spawn}. */
  type SpawnOptions = {
    /** The child's `flux:process` `argv`. Default `[]`. */
    args?: string[]
  }

  /**
   * One end of a port between two runtimes. Messages are copied
   * (shared-nothing). A pending `recv()` keeps its own runtime alive; two ends
   * both waiting forever is a program bug nothing detects. `Promise.race`
   * over several `recv()`s is not a select: the losing calls still consume
   * their messages.
   */
  export class Port implements AsyncIterable<any> {
    private constructor()
    /** Copy `value` to the peer. Throws for unsendable values and once this end is closed. */
    send(value: Sendable): void
    /**
     * The next message, or `undefined` once the peer has closed (or exited)
     * and the queue is drained. Rejects with the peer's uncaught error
     * (module throw, unhandled rejection, a throw out of a callback) when one
     * happens; later calls keep receiving.
     */
    recv(): Promise<any>
    /** Stop sending from this end. The peer's `recv()` drains, then reports `undefined`. */
    close(): void
    /**
     * Kill the child now (parent's end only; a no-op on the child's): busy JS
     * is interrupted, the child runtime is dropped, this end's `recv()`
     * reports `undefined`.
     */
    terminate(): void
    /** `recv()` in a loop, ending when the peer closes. */
    [Symbol.asyncIterator](): AsyncIterator<any>
  }

  /**
   * Run `source` as a JS module in a new runtime on its own thread (own heap,
   * own event loop, the non-gui `flux:*` modules) and return this end of the
   * port to it. The child reaches the other end as `port` from this module.
   * Children die with their parent runtime. Errors starting the child surface
   * on `recv()`.
   */
  export function spawn(source: string, opts?: SpawnOptions): Port

  /** The port to the parent inside a spawned runtime; `undefined` in a runtime that was not spawned. */
  export const port: Port | undefined
}
