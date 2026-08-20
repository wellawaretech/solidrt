// The web-standard abort primitives. A deliberate subset: an `onabort`
// handler property only (no addEventListener), a plain-object event (not an
// Event instance), and no `AbortSignal.timeout`/`any`. Without `DOMException`
// the default abort reason is an `Error` whose `name` is "AbortError".

/** The event passed to {@link AbortSignal.onabort}. */
interface AbortEvent {
  type: "abort"
}

interface AbortSignal {
  /** True once the signal has been aborted. */
  readonly aborted: boolean
  /** The abort reason; `undefined` until aborted. */
  readonly reason: any
  /** Called once when the signal aborts. */
  onabort: ((event: AbortEvent) => void) | null
  /** Throws `reason` if the signal is aborted; no-op otherwise. */
  throwIfAborted(): void
}

declare let AbortSignal: {
  prototype: AbortSignal
  /** An already-aborted signal. */
  abort(reason?: any): AbortSignal
}

declare class AbortController {
  /** The controller's signal; the same object on every read. */
  readonly signal: AbortSignal
  /**
   * Abort the signal with `reason` (default: an `Error` named "AbortError")
   * and fire its `onabort`. Aborting an already-aborted signal is a no-op.
   */
  abort(reason?: any): void
}
