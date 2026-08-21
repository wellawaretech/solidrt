// The paced clock's two readings (see paced_clock.rs): the smoothed
// animation reading is allowed to lag the wall clock under slow frames; the
// timer reading, which virtual timer deadlines advance against, is not.

use crate::paced_clock::PacedClock;

const PERIOD: f64 = 1000.0 / 60.0;
const EPS: f64 = 1e-6;

// Frame signals at 46.5 ms (the measured slow-paint cadence behind the
// timer-lag report) instead of the 16.7 ms refresh period: the animation
// reading settles into its (signal period - refresh period) / GAIN
// equilibrium lag, while the timer reading stays exactly on the wall clock.
#[test]
fn timer_reading_tracks_wall_under_slow_frames() {
  let clock = PacedClock::new();
  let mut raw = 0.0;
  let mut max_lag: f64 = 0.0;
  for i in 0..100 {
    raw += 46.5;
    clock.tick(raw, 1.0);
    // The first tick anchors at 0; from there the reading tracks raw deltas
    // exactly.
    let expected = raw - 46.5;
    assert!((clock.timer_now_ms() - expected).abs() < EPS, "timer reading drifted at tick {i}: {} vs {expected}", clock.timer_now_ms());
    max_lag = max_lag.max(raw - clock.now_ms());
  }
  // The animation reading's lag sawtooths toward its equilibrium (bounded by
  // the present model's stall snap); the exact peak is a constants question,
  // but a substantial lag is the reason the timer reading exists.
  assert!(max_lag > 300.0, "animation reading never lagged, max {max_lag}");
}

// The raw stretch before the first tick (engine build and eval at cold
// start) is anchored away, not lived through: module-init timers must not
// see seconds of startup counted against their delay and fire on the first
// frame.
#[test]
fn first_tick_anchors_instead_of_living_through_startup() {
  let clock = PacedClock::new();
  clock.tick(2000.0, 1.0);
  assert!((clock.timer_now_ms() - 0.0).abs() < EPS, "startup stretch leaked: {}", clock.timer_now_ms());
  clock.tick(2000.0 + PERIOD, 1.0);
  assert!((clock.timer_now_ms() - PERIOD).abs() < EPS, "tracking after anchor broke: {}", clock.timer_now_ms());
}

// The live reading between ticks: the stepped reading before the first tick,
// raw-tracking after it, and at most one tick gap past the frozen reading
// while paused (the offset re-anchors per tick).
#[test]
fn live_reading_is_fresh_between_ticks() {
  let clock = PacedClock::new();
  assert!((clock.timer_live_ms(500.0) - 0.0).abs() < EPS, "live read before first tick leaked raw");
  clock.tick(PERIOD, 1.0);
  assert!((clock.timer_live_ms(PERIOD + 5.0) - 5.0).abs() < EPS, "live read did not track raw");
  clock.tick(2.0 * PERIOD, 1.0);
  assert!((clock.timer_live_ms(2.0 * PERIOD + 3.0) - (PERIOD + 3.0)).abs() < EPS, "live read stale after tick");
  clock.tick(3.0 * PERIOD, 0.0);
  let frozen = clock.timer_now_ms();
  let live = clock.timer_live_ms(3.0 * PERIOD + 10.0);
  assert!(live >= frozen - EPS && live <= frozen + PERIOD + 10.0, "paused live read out of bounds: {live} vs {frozen}");
}

// A pause (scale 0) freezes the timer reading, and the return to scale 1
// continues from the frozen value without replaying the paused stretch.
#[test]
fn pause_accrues_into_offset_and_resumes_jump_free() {
  let clock = PacedClock::new();
  let mut raw = 0.0;
  for _ in 0..10 {
    raw += PERIOD;
    clock.tick(raw, 1.0);
  }
  let frozen = clock.timer_now_ms();
  for _ in 0..30 {
    raw += PERIOD;
    clock.tick(raw, 0.0);
  }
  assert!((clock.timer_now_ms() - frozen).abs() < EPS, "paused timer reading moved");
  raw += PERIOD;
  clock.tick(raw, 1.0);
  assert!(
    (clock.timer_now_ms() - (frozen + PERIOD)).abs() < EPS,
    "resume jumped: {} vs {}",
    clock.timer_now_ms(),
    frozen + PERIOD
  );
}

// A suspension (raw jump beyond the stall threshold) is skipped by the
// animation reading but lived through by the timer reading, so timers that
// came due while suspended fire on the resume tick.
#[test]
fn suspension_skipped_by_animation_lived_by_timers() {
  let clock = PacedClock::new();
  let mut raw = 0.0;
  for _ in 0..10 {
    raw += PERIOD;
    clock.tick(raw, 1.0);
  }
  let (anim_before, timer_before) = (clock.now_ms(), clock.timer_now_ms());
  raw += 10_000.0;
  clock.tick(raw, 1.0);
  assert!(
    (clock.now_ms() - (anim_before + PERIOD)).abs() < EPS,
    "animation reading should advance one period across a suspension, moved {}",
    clock.now_ms() - anim_before
  );
  assert!(
    (clock.timer_now_ms() - (timer_before + 10_000.0)).abs() < EPS,
    "timer reading should live through a suspension, moved {}",
    clock.timer_now_ms() - timer_before
  );
}
