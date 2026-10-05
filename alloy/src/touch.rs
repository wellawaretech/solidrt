//! Android's touch path. SDL's own carries no sample times: it stamps a
//! touch when the JNI call arrives and never reads a batch's historical
//! samples, while Android delivers a time with every sample. So the
//! activity's surface (SolidRTSurface.java) hands each finger sample here
//! with its time, SDL sees no finger at all, and the main loop drains the
//! queue beside the SDL events (see app.rs). Mouse, pen and the touch
//! device list stay with SDL.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

// MotionEvent's masked actions, as the surface passes them on.
const ACTION_DOWN: i32 = 0;
const ACTION_UP: i32 = 1;
const ACTION_MOVE: i32 = 2;
const ACTION_CANCEL: i32 = 3;
const ACTION_POINTER_DOWN: i32 = 5;
const ACTION_POINTER_UP: i32 = 6;

pub(crate) enum TouchKind {
  Down,
  Move,
  Up,
  Cancel,
}

/// One finger sample: `x`/`y` normalized to the surface (0 to 1), `at` when
/// the finger was there.
pub(crate) struct TouchSample {
  pub kind: TouchKind,
  pub finger_id: u64,
  pub x: f32,
  pub y: f32,
  pub at: Instant,
}

static QUEUE: Mutex<Vec<TouchSample>> = Mutex::new(Vec::new());
static WAKE: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

/// The main loop's wake, so a sample queued while it sleeps on the SDL
/// event queue is noticed.
pub(crate) fn set_wake(wake: Option<Arc<dyn Fn() + Send + Sync>>) {
  *WAKE.lock().expect("touch wake lock poisoned") = wake;
}

/// Queue one finger sample. Called on the Android UI thread (the surface's
/// native); `time_ns` is the sample's MotionEvent time, CLOCK_MONOTONIC
/// nanoseconds. A cancel (the system taking the gesture: a back swipe, the
/// notification shade, palm rejection) arrives with every pointer's last
/// position, as the surface passes it on.
pub fn push(pointer_id: i32, action: i32, x: f32, y: f32, time_ns: i64) {
  let kind = match action {
    ACTION_DOWN | ACTION_POINTER_DOWN => TouchKind::Down,
    ACTION_MOVE => TouchKind::Move,
    ACTION_UP | ACTION_POINTER_UP => TouchKind::Up,
    ACTION_CANCEL => TouchKind::Cancel,
    _ => return,
  };
  // Finger ids start at 1, as SDL's did.
  let sample = TouchSample { kind, finger_id: (pointer_id + 1) as u64, x, y, at: instant_of(time_ns) };
  let first = {
    let mut queue = QUEUE.lock().expect("touch queue lock poisoned");
    queue.push(sample);
    queue.len() == 1
  };
  // One wake per batch: the loop drains the whole queue when it runs.
  if first {
    if let Some(wake) = WAKE.lock().expect("touch wake lock poisoned").as_ref() {
      wake();
    }
  }
}

/// The samples queued since the last drain, oldest first.
pub(crate) fn drain() -> Vec<TouchSample> {
  std::mem::take(&mut *QUEUE.lock().expect("touch queue lock poisoned"))
}

// A CLOCK_MONOTONIC reading as an Instant. Both clocks are read once, as a
// pair, and every reading is placed against that pair: two samples of one
// time get one Instant, and a later sample never lands before an earlier
// one, which a fresh pair of readings per sample would not promise.
fn instant_of(time_ns: i64) -> Instant {
  static PAIR: OnceLock<(Instant, i64)> = OnceLock::new();
  let (base, base_ns) = *PAIR.get_or_init(|| {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // CLOCK_MONOTONIC cannot fail on Android; a zero reading would only
    // place the samples far in the future, which the clamp below catches.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    (Instant::now(), now.tv_sec as i64 * 1_000_000_000 + now.tv_nsec as i64)
  });
  let at = if time_ns >= base_ns {
    base + Duration::from_nanos((time_ns - base_ns) as u64)
  } else {
    base.checked_sub(Duration::from_nanos((base_ns - time_ns) as u64)).unwrap_or(base)
  };
  // A sample cannot be from the future.
  at.min(Instant::now())
}
