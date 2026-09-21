// The paced clock's two readings (see paced_clock.rs): the animation reading
// advances by the refreshes alloy counted for each frame signal, so it keeps
// wall rate under slow frames without ever lagging; the timer reading, which
// virtual timer deadlines advance against, is the wall clock itself.

use crate::paced_clock::{Advance, PacedClock};
use alloy::RefreshCounter;

const PERIOD: f64 = 1000.0 / 60.0;
const EPS: f64 = 1e-6;

// Frame signals at 46.5 ms (the measured slow-paint cadence behind the
// timer-lag report) instead of the 16.7 ms refresh period, counted by the
// real refresh counter: the animation reading stays within two periods of
// the wall clock throughout (the old model lagged by up to half a second
// here), every step is a whole number of periods, and the timer reading
// stays exactly on the wall clock.
#[test]
fn animation_reading_keeps_wall_rate_under_slow_frames() {
  let clock = PacedClock::new();
  let mut counter = RefreshCounter::new();
  let mut raw = 0.0;
  let mut offsets: Vec<f64> = Vec::new();
  let mut last_now = clock.now_ms();
  for i in 0..100 {
    raw += 46.5;
    let refreshes = counter.on_signal(raw);
    clock.tick(raw, refreshes, Advance::Run(1.0));
    // The first tick anchors at 0; from there the reading tracks raw deltas
    // exactly.
    let expected = raw - 46.5;
    assert!((clock.timer_now_ms() - expected).abs() < EPS, "timer reading drifted at tick {i}: {} vs {expected}", clock.timer_now_ms());
    let step = clock.now_ms() - last_now;
    last_now = clock.now_ms();
    let periods = step / PERIOD;
    assert!((periods - periods.round()).abs() < EPS, "tick {i}: step {step} is not whole periods");
    let periods = periods.round();
    // The first signal has nothing to measure against and counts one.
    if i > 0 {
      assert!(periods == 2.0 || periods == 3.0, "tick {i}: {periods} periods for a 2.79-period frame");
      offsets.push(raw - clock.now_ms());
    }
  }
  let spread = offsets.iter().cloned().fold(f64::MIN, f64::max) - offsets.iter().cloned().fold(f64::MAX, f64::min);
  assert!(spread < 2.0 * PERIOD, "animation reading wandered {spread} ms against the wall clock");
}

// The raw stretch before the first tick (engine build and eval at cold
// start) is anchored away, not lived through: module-init timers must not
// see seconds of startup counted against their delay and fire on the first
// frame.
#[test]
fn first_tick_anchors_instead_of_living_through_startup() {
  let clock = PacedClock::new();
  clock.tick(2000.0, 1, Advance::Run(1.0));
  assert!((clock.timer_now_ms() - 0.0).abs() < EPS, "startup stretch leaked: {}", clock.timer_now_ms());
  clock.tick(2000.0 + PERIOD, 1, Advance::Run(1.0));
  assert!((clock.timer_now_ms() - PERIOD).abs() < EPS, "tracking after anchor broke: {}", clock.timer_now_ms());
}

// The live reading between ticks: the stepped reading before the first tick,
// raw-tracking after it, and at most one tick gap past the frozen reading
// while paused (the offset re-anchors per tick).
#[test]
fn live_reading_is_fresh_between_ticks() {
  let clock = PacedClock::new();
  assert!((clock.timer_live_ms(500.0) - 0.0).abs() < EPS, "live read before first tick leaked raw");
  clock.tick(PERIOD, 1, Advance::Run(1.0));
  assert!((clock.timer_live_ms(PERIOD + 5.0) - 5.0).abs() < EPS, "live read did not track raw");
  clock.tick(2.0 * PERIOD, 1, Advance::Run(1.0));
  assert!((clock.timer_live_ms(2.0 * PERIOD + 3.0) - (PERIOD + 3.0)).abs() < EPS, "live read stale after tick");
  clock.tick(3.0 * PERIOD, 1, Advance::Paused);
  let frozen = clock.timer_now_ms();
  let live = clock.timer_live_ms(3.0 * PERIOD + 10.0);
  assert!(live >= frozen - EPS && live <= frozen + PERIOD + 10.0, "paused live read out of bounds: {live} vs {frozen}");
}

// A pause freezes both readings, a step moves both exactly one period, and
// the return to scale 1 continues from there without replaying the paused
// stretch.
#[test]
fn pause_and_step_move_both_readings_in_lockstep() {
  let clock = PacedClock::new();
  let mut raw = 0.0;
  for _ in 0..10 {
    raw += PERIOD;
    clock.tick(raw, 1, Advance::Run(1.0));
  }
  let (anim, timer) = (clock.now_ms(), clock.timer_now_ms());
  for _ in 0..30 {
    raw += PERIOD;
    clock.tick(raw, 1, Advance::Paused);
  }
  assert!((clock.now_ms() - anim).abs() < EPS, "paused animation reading moved");
  assert!((clock.timer_now_ms() - timer).abs() < EPS, "paused timer reading moved");
  raw += PERIOD;
  clock.tick(raw, 1, Advance::Step);
  assert!((clock.now_ms() - (anim + PERIOD)).abs() < EPS, "step moved animation by {}", clock.now_ms() - anim);
  assert!((clock.timer_now_ms() - (timer + PERIOD)).abs() < EPS, "step moved timers by {}", clock.timer_now_ms() - timer);
  raw += PERIOD;
  clock.tick(raw, 1, Advance::Run(1.0));
  assert!((clock.now_ms() - (anim + 2.0 * PERIOD)).abs() < EPS, "resume jumped the animation reading");
  assert!((clock.timer_now_ms() - (timer + 2.0 * PERIOD)).abs() < EPS, "resume jumped the timer reading");
}

// A scaled clock advances both readings by period * scale per signal
// whatever the refresh count says.
#[test]
fn scale_advances_period_times_scale() {
  let clock = PacedClock::new();
  clock.tick(0.0, 1, Advance::Run(1.0));
  let (anim, timer) = (clock.now_ms(), clock.timer_now_ms());
  clock.tick(PERIOD, 3, Advance::Run(0.5));
  assert!((clock.now_ms() - anim - 0.5 * PERIOD).abs() < EPS, "scaled animation step {}", clock.now_ms() - anim);
  assert!((clock.timer_now_ms() - timer - 0.5 * PERIOD).abs() < EPS, "scaled timer step {}", clock.timer_now_ms() - timer);
}

// A suspension (a count worth more than the threshold) is skipped by the
// animation reading but lived through by the timer reading, so timers that
// came due while suspended fire on the resume tick.
#[test]
fn suspension_skipped_by_animation_lived_by_timers() {
  let clock = PacedClock::new();
  let mut raw = 0.0;
  for _ in 0..10 {
    raw += PERIOD;
    clock.tick(raw, 1, Advance::Run(1.0));
  }
  let (anim_before, timer_before) = (clock.now_ms(), clock.timer_now_ms());
  raw += 10_000.0;
  clock.tick(raw, 600, Advance::Run(1.0));
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

// A count of zero (a duplicate or stray frame signal) leaves the animation
// reading where it is: no refresh passed, so no time did.
#[test]
fn zero_refreshes_advance_nothing() {
  let clock = PacedClock::new();
  clock.tick(0.0, 1, Advance::Run(1.0));
  clock.tick(PERIOD, 1, Advance::Run(1.0));
  let before = clock.now_ms();
  clock.tick(PERIOD + 1.0, 0, Advance::Run(1.0));
  assert!((clock.now_ms() - before).abs() < EPS, "a zero count moved the animation reading");
}
