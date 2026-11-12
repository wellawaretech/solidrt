//! The platform's tick timeline: SDL's nanosecond tick count, the instant
//! it counts from, and the instant an event timestamp on it stands for.
//! `SDL_GetTicksNS` needs no SDL_Init and is thread safe.

use sdl3::sys::timer::SDL_GetTicksNS;

/// The instant an SDL event timestamp stands for. SDL stamps events in
/// nanoseconds on its own tick base (the platform's event time where the
/// backend has one, the receipt in the pump otherwise, never later than
/// now); `epoch` is that base as an Instant (see `ticks_epoch`).
pub fn event_instant(epoch: std::time::Instant, timestamp_ns: u64) -> std::time::Instant {
  // Windows is the exception: its event time is the message tick, which
  // moves in steps of the system timer, coarser than the receipt. Input is
  // not batched on a desktop and the pump blocks on the queue, so the
  // receipt is the better reading there.
  if cfg!(target_os = "windows") {
    return std::time::Instant::now();
  }
  (epoch + std::time::Duration::from_nanos(timestamp_ns)).min(std::time::Instant::now())
}

/// Now on SDL's tick timeline, ns since `ticks_epoch`: what SDL stamps
/// camera frames with, and what a session's open is stamped with.
pub fn ticks_ns() -> u64 {
  // SAFETY: SDL_GetTicksNS takes no arguments and only reads the
  // platform's monotonic clock; it is safe from any thread, before or
  // after SDL_Init.
  unsafe { SDL_GetTicksNS() }
}

/// SDL's tick base as an Instant: the instant SDL_GetTicksNS counts from.
pub fn ticks_epoch() -> std::time::Instant {
  let now = std::time::Instant::now();
  let ticks = ticks_ns();
  now.checked_sub(std::time::Duration::from_nanos(ticks)).unwrap_or(now)
}
