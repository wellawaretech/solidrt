// Timers, microtask scheduling, and the monotonic clock. flux's timers differ
// from the browser in two ways: the delay is required, and no extra callback
// arguments are forwarded.
//
// In a GUI runtime the timers are FRAME-QUANTIZED but WALL-ACCURATE: a
// deadline is measured against the real clock from the moment of
// registration, and firing happens on frame boundaries. So a timer fires at
// the first frame at or after its deadline - at least `ms` after
// registration, at most one frame late (~16 ms at 60 Hz; a setTimeout of 0
// runs on the next frame) - and deadlines do not drift when frames run
// slow. An interval fires at most once per frame (missed periods collapse
// instead of storming). Pausing the runtime clock (the dev tools'
// set_time_scale 0) freezes timers and frame callbacks together; a timer
// that came due while the app was suspended (backgrounded) fires on the
// resume frame. performance.now() is real elapsed time, for measuring
// work; the onFrame / requestAnimationFrame timestamp is a separate paced
// animation timeline. Date.now() is calendar time. Headless flux (scripts,
// servers) keeps ordinary wall-clock timers.

/**
 * Run `callback` after at least `ms` milliseconds. Returns a timer id for
 * {@link clearTimeout}.
 */
declare function setTimeout(callback: () => void, ms: number): number
/** Cancel a pending timeout. Unknown or missing ids are ignored. */
declare function clearTimeout(id?: number): void
/**
 * Run `callback` every `ms` milliseconds. Returns a timer id for
 * {@link clearInterval}.
 */
declare function setInterval(callback: () => void, ms: number): number
/** Cancel a running interval. Unknown or missing ids are ignored. */
declare function clearInterval(id?: number): void
/**
 * Queue `callback` to run as a microtask: after the current job finishes, before
 * any timer fires.
 */
declare function queueMicrotask(callback: () => void): void

declare let performance: {
  /**
   * Milliseconds elapsed since the runtime started (high-resolution,
   * monotonic, sub-millisecond). Real time: it keeps advancing across
   * synchronous work and while the runtime clock is paused, so it is the
   * clock for measuring durations. For frame time use the onFrame /
   * requestAnimationFrame timestamp; for calendar time use Date.now().
   */
  now(): number
  /**
   * Wall-clock time (ms since the Unix epoch) when the runtime started, so
   * timeOrigin + now() tracks Date.now().
   */
  readonly timeOrigin: number
}