use crate::PresentClock;

const P60: f64 = 1000.0 / 60.0;

#[test]
fn steady_cadence_tracks_exact_presents() {
  let clock = PresentClock::new();
  assert!((clock.period_ms() - P60).abs() < 1e-9);
  // Raw readings landing exactly one period apart: the model reports exact
  // multiples of the period (gap is zero, no correction).
  for n in 1..=100u32 {
    let t = clock.on_present(n as f64 * P60);
    assert!((t - n as f64 * P60).abs() < 1e-9, "tick {n}: {t}");
  }
}

#[test]
fn jitter_is_smoothed_and_drift_converges() {
  let clock = PresentClock::new();
  // Raw readings a constant 100 ms ahead of the modeled clock (well under the
  // stall threshold): each present corrects a GAIN fraction of the error, so
  // spacing stays close to one period and the error dies out geometrically.
  let mut prev = 0.0;
  let mut t = 0.0;
  for n in 1..=200u32 {
    let raw = n as f64 * P60 + 100.0;
    t = clock.on_present(raw);
    let spacing = t - prev;
    prev = t;
    assert!(spacing >= P60 - 1e-9 && spacing <= P60 + 5.0 + 1e-9, "tick {n}: spacing {spacing}");
  }
  let err = (200.0 * P60 + 100.0) - t;
  assert!(err.abs() < 0.1, "residual drift {err}");
}

#[test]
fn extra_calls_within_a_period_do_not_advance_the_clock() {
  let clock = PresentClock::new();
  for n in 1..=10u32 {
    clock.on_present(n as f64 * P60);
  }
  let settled = clock.on_present(11.0 * P60);
  // Frame signals that are not presents arrive between them (an app whose
  // content changes at half the refresh rate ticks in between), and the
  // timeline must not count them as time.
  for _ in 0..5 {
    let t = clock.on_present(11.0 * P60);
    assert!((t - settled).abs() < 1e-9, "an extra call at the same instant advanced the clock to {t}");
  }
  // A raw reading a quarter period on is still the same period.
  let t = clock.on_present(11.0 * P60 + P60 / 4.0);
  assert!(t < settled + P60, "a call inside the period advanced a whole one: {t} vs {settled}");
  // The next real period advances normally.
  let t = clock.on_present(12.0 * P60);
  assert!((t - 12.0 * P60).abs() < 0.5, "cadence not resumed: {t}");
}

#[test]
fn stall_snaps_to_raw() {
  let clock = PresentClock::new();
  for n in 1..=10u32 {
    clock.on_present(n as f64 * P60);
  }
  // Presents stop for 5 seconds (app backgrounded): the next reading is a
  // stall, and the model reports the jump like a real timestamp source
  // would, instead of creeping through the gap.
  let resumed = 11.0 * P60 + 5000.0;
  let t = clock.on_present(resumed);
  assert_eq!(t, resumed);
  // The stream is back on cadence from the snapped point.
  let t = clock.on_present(resumed + P60);
  assert!((t - (resumed + P60)).abs() < 1e-9);
}

#[test]
fn set_hz_ignores_non_positive() {
  let clock = PresentClock::new();
  clock.set_hz(120.0);
  assert!((clock.period_ms() - 1000.0 / 120.0).abs() < 1e-9);
  clock.set_hz(0.0);
  clock.set_hz(-30.0);
  assert!((clock.period_ms() - 1000.0 / 120.0).abs() < 1e-9);
}
