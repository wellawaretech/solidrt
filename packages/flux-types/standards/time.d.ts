// Timers, task scheduling, idle callbacks, microtask scheduling, and the
// monotonic clock. flux's timers differ from the browser in one way: no
// extra callback arguments are forwarded.
//
// Two kinds of scheduling. What names a point in time (setTimeout,
// setInterval, requestAnimationFrame) runs on the app's frame clock, as
// described next. What names a turn (queueMicrotask, setImmediate) runs on
// the engine loop whenever it is ready: between frames in a GUI runtime,
// never waiting for one, and not held by a paused clock. requestIdleCallback
// sits on the frame side: its idle period is what is left of a frame.
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
 * Run `callback` after at least `ms` milliseconds (default 0, as on the
 * web: the next frame in a GUI runtime). Returns a timer id for
 * {@link clearTimeout}.
 */
declare function setTimeout(callback: () => void, ms?: number): number
/** Cancel a pending timeout. Unknown or missing ids are ignored. */
declare function clearTimeout(id?: number): void
/**
 * Run `callback` every `ms` milliseconds (default 0: once per frame in a
 * GUI runtime). Returns a timer id for {@link clearInterval}.
 */
declare function setInterval(callback: () => void, ms?: number): number
/** Cancel a running interval. Unknown or missing ids are ignored. */
declare function clearInterval(id?: number): void
/**
 * Run `callback` on the next turn of the engine loop: after the current
 * task and its microtasks, with no delay floor. Not a timer: in a GUI
 * runtime it runs between frames (a chain of immediates makes progress at
 * CPU speed while frames keep coming) and a paused clock does not hold it;
 * it always runs before a `setTimeout(fn, 0)` registered with it there.
 * Returns an id for {@link clearImmediate}.
 */
declare function setImmediate(callback: () => void): number
/** Cancel a pending immediate. Unknown or missing ids are ignored. */
declare function clearImmediate(id?: number): void

/** What an idle callback is handed: how much of the idle period is left. */
interface IdleDeadline {
  /** Milliseconds left in the idle period; 0 once it has passed. */
  timeRemaining(): number
  /** Whether the `timeout` ran the callback instead of an idle period. */
  readonly didTimeout: boolean
}

interface IdleRequestOptions {
  /**
   * If no idle period has run the callback after this many ms, run it as
   * a task anyway, with `didTimeout` true and no time remaining. That
   * task is a timer, so in a GUI runtime it fires on a frame boundary,
   * like `setTimeout`.
   */
  timeout?: number
}

/**
 * Run `callback` in an idle period, oldest registration first, with an
 * {@link IdleDeadline}. In a GUI runtime an idle period is what is left of a
 * frame after its work: it begins once the frame's callbacks, flush and
 * draw are done and its microtasks have run, and ends when the frame is
 * due on screen, at most 50 ms later. Callbacks that do not fit wait for
 * the next frame's period, and one registered during a period waits for
 * the next. A paused clock holds the frames and with them the idle
 * periods. Headless flux has no frames: the callback runs on the next
 * turn with the full 50 ms. Returns an id for {@link cancelIdleCallback}.
 */
declare function requestIdleCallback(callback: (deadline: IdleDeadline) => void, options?: IdleRequestOptions): number
/** Cancel a pending idle callback. Unknown or missing ids are ignored. */
declare function cancelIdleCallback(id?: number): void
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