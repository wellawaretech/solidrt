// The paced clock's readings (see paced_clock.rs): the animation reading
// advances by the refreshes alloy counted for each frame signal, so it keeps
// wall rate under slow frames without ever lagging; the timer reading, which
// virtual timer deadlines advance against, is the wall clock itself; the
// input reading stamps input events.

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

// The input reading (okf/plans/event-timestamp.md): the moves of
// consecutive frames are stamped whole periods apart whatever the wall did
// to the signals, as the slot positions they carry are.
#[test]
fn move_stamps_are_whole_periods_apart() {
  let clock = PacedClock::new();
  let mut counter = RefreshCounter::new();
  let mut raw = 0.0;
  let mut last = None;
  for i in 0..60 {
    // Signals jittered around a two-period cadence.
    raw += 2.0 * PERIOD + if i % 2 == 0 { 3.0 } else { -3.0 };
    clock.tick(raw, counter.on_signal(raw), Advance::Run(1.0));
    let stamp = clock.input_slot_ms();
    if let Some(last) = last {
      let periods: f64 = (stamp - last) / PERIOD;
      assert!((periods - periods.round()).abs() < EPS, "tick {i}: moves {} ms apart", stamp - last);
    }
    last = Some(stamp);
  }
}

// An event that arrives between two ticks is stamped with when it arrived:
// the latched reading plus the wall time since that tick. Before the first
// tick there is no anchor.
#[test]
fn an_arrival_between_ticks_reads_its_own_time() {
  let clock = PacedClock::new();
  assert!((clock.input_arrival_ms(500.0) - 0.0).abs() < EPS, "arrival before the first tick leaked raw");
  clock.tick(1000.0, 1, Advance::Run(1.0));
  let slot = clock.input_slot_ms();
  assert!((clock.input_arrival_ms(1040.0) - (slot + 40.0)).abs() < EPS, "arrival 40 ms into a slow frame");
}

// Stamps never go back: an arrival a little past the next tick's latched
// reading holds the next move's stamp at its own, and the reading itself is
// not moved, so the frame after is on the period grid again.
#[test]
fn a_stamp_is_never_earlier_than_the_one_before() {
  let clock = PacedClock::new();
  clock.tick(0.0, 1, Advance::Run(1.0));
  let slot = clock.input_slot_ms();
  let arrival = clock.input_arrival_ms(PERIOD + 2.0);
  assert!((arrival - (slot + PERIOD + 2.0)).abs() < EPS, "arrival stamp {arrival}");
  clock.tick(PERIOD + 3.0, 1, Advance::Run(1.0));
  assert!((clock.input_slot_ms() - arrival).abs() < EPS, "the move after an overshooting arrival went back");
  clock.tick(2.0 * PERIOD + 3.0, 1, Advance::Run(1.0));
  assert!((clock.input_slot_ms() - (slot + 2.0 * PERIOD)).abs() < EPS, "the overshoot moved the reading itself");
}

// The input reading lives through a suspension, where the animation
// reading skips it: two taps on either side of a background stretch are
// far apart.
#[test]
fn input_reading_lives_through_a_suspension() {
  let clock = PacedClock::new();
  let mut raw = 0.0;
  for _ in 0..10 {
    raw += PERIOD;
    clock.tick(raw, 1, Advance::Run(1.0));
  }
  let before = clock.input_arrival_ms(raw + 1.0);
  raw += 10_000.0;
  clock.tick(raw, 600, Advance::Run(1.0));
  let after = clock.input_arrival_ms(raw + 1.0);
  assert!((after - before - 600.0 * PERIOD).abs() < EPS, "taps across a suspension read {} ms apart", after - before);
}

// The dev clock control governs the input reading as it does the other
// two: a pause holds it still between ticks as well, a step moves it one
// period, a scale advances it at that rate.
#[test]
fn input_reading_follows_pause_step_and_scale() {
  let clock = PacedClock::new();
  clock.tick(0.0, 1, Advance::Run(1.0));
  clock.tick(PERIOD, 1, Advance::Paused);
  let paused = clock.input_slot_ms();
  assert!((clock.input_arrival_ms(PERIOD + 500.0) - paused).abs() < EPS, "an arrival moved a paused input reading");
  clock.tick(2.0 * PERIOD, 1, Advance::Step);
  assert!((clock.input_slot_ms() - (paused + PERIOD)).abs() < EPS, "a step moved the input reading by other than a period");
  assert!((clock.input_arrival_ms(2.0 * PERIOD + 500.0) - (paused + PERIOD)).abs() < EPS, "an arrival after a step moved it");
  clock.tick(3.0 * PERIOD, 3, Advance::Run(0.5));
  let scaled = clock.input_slot_ms();
  assert!((scaled - (paused + 1.5 * PERIOD)).abs() < EPS, "scaled input step {}", scaled - paused);
  assert!((clock.input_arrival_ms(3.0 * PERIOD + 10.0) - (scaled + 5.0)).abs() < EPS, "scaled arrival");
}

// An event that arrived before a tick and is stamped after it (it waited in
// the channel behind a frame) reads its own time, before that tick's
// reading: a press 30 ms long across a 50 ms frame is 30 ms, not the frame.
#[test]
fn an_arrival_stamped_after_the_next_tick_keeps_its_own_time() {
  let clock = PacedClock::new();
  clock.tick(1000.0, 1, Advance::Run(1.0));
  let slot = clock.input_slot_ms();
  // A frame of three periods: the down arrives 10 ms into it and the up
  // 30 ms later; both are stamped only after the frame's end, past the tick.
  clock.tick(1000.0 + 3.0 * PERIOD, 3, Advance::Run(1.0));
  let down = clock.input_arrival_ms(1010.0);
  let up = clock.input_arrival_ms(1040.0);
  assert!((down - (slot + 10.0)).abs() < EPS, "down read {}", down - slot);
  assert!((up - down - 30.0).abs() < EPS, "a 30 ms press read {} ms", up - down);
  // The frame's own moves, stamped after those, are not before them.
  assert!(clock.input_slot_ms() >= up);
}
