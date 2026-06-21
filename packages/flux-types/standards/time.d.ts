// Timers, microtask scheduling, and the monotonic clock. flux's timers differ
// from the browser in two ways: the delay is required, and no extra callback
// arguments are forwarded.

/**
 * Run `callback` after at least `ms` milliseconds. Returns a timer id for
 * {@link clearTimeout}.
 */
declare function setTimeout(callback: () => void, ms: number): number
/** Cancel a pending timeout. Throws on an unknown id. */
declare function clearTimeout(id: number): void
/**
 * Run `callback` every `ms` milliseconds. Returns a timer id for
 * {@link clearInterval}.
 */
declare function setInterval(callback: () => void, ms: number): number
/** Cancel a running interval. Throws on an unknown id. */
declare function clearInterval(id: number): void
/**
 * Queue `callback` to run as a microtask: after the current job finishes, before
 * any timer fires.
 */
declare function queueMicrotask(callback: () => void): void

declare let performance: {
  /**
   * Milliseconds since a monotonic origin (high-resolution, not wall-clock). Use
   * for measuring durations, not for calendar time.
   */
  now(): number
}