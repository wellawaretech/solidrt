//! The process clock frames are scheduled on: a monotonic reading in
//! nanoseconds from a process epoch, the same on every thread. A producer
//! that pushes frames with due times (a video player's worker, through a
//! `YuvFrameSink`) reads it through `now_ns`, and the raster thread compares
//! those due times against a frame's presentation estimate carried as an
//! `Instant` (`ns` converts it), so the two sides are one clock by
//! construction, with no shared definition to keep in step.
//!
//! In headless playback the clock is the capture's virtual frame time:
//! the playback loop sets it before each frame signal (`set_virtual_ns`),
//! `now_ns` reads it, and `stepped` says so, so a producer never sleeps
//! against it (okf/plans/video-texture-off-frame-loop.md, section 8).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// The reading `now_ns` reports while a virtual clock is in force; this
// value means none is (a capture starts at 0, never below).
const NO_VIRTUAL: i64 = -1;

static EPOCH: OnceLock<Instant> = OnceLock::new();
static VIRTUAL_NS: AtomicI64 = AtomicI64::new(NO_VIRTUAL);

fn epoch() -> Instant {
  *EPOCH.get_or_init(Instant::now)
}

/// The clock's reading now, in nanoseconds: the virtual frame time in
/// playback, real elapsed time otherwise.
pub fn now_ns() -> i64 {
  match VIRTUAL_NS.load(Ordering::Acquire) {
    NO_VIRTUAL => ns(Instant::now()),
    virtual_ns => virtual_ns,
  }
}

/// An instant as this clock reads it. An instant before the epoch reads
/// negative.
pub fn ns(at: Instant) -> i64 {
  let epoch = epoch();
  match at.checked_duration_since(epoch) {
    Some(since) => since.as_nanos() as i64,
    None => -(epoch.duration_since(at).as_nanos() as i64),
  }
}

/// The instant this clock reads as `ns` (the inverse of `ns`): how a
/// virtual frame time is carried where an `Instant` is expected.
pub fn at(ns: i64) -> Instant {
  let epoch = epoch();
  if ns >= 0 {
    epoch + Duration::from_nanos(ns as u64)
  } else {
    epoch - Duration::from_nanos(ns.unsigned_abs())
  }
}

/// The clock's reading now as an instant: the deadline for a frame drawn
/// without a frame signal behind it (the mount frame, a direct render),
/// which presents as soon as it can - at the virtual time in playback.
pub fn now() -> Instant {
  at(now_ns())
}

/// Whether the clock advances only when the playback loop steps it.
pub fn stepped() -> bool {
  VIRTUAL_NS.load(Ordering::Acquire) != NO_VIRTUAL
}

/// Step the virtual clock (the playback loop, before each frame signal).
pub(crate) fn set_virtual_ns(virtual_ns: i64) {
  VIRTUAL_NS.store(virtual_ns.max(0), Ordering::Release);
}
