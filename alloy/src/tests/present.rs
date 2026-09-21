// RefreshCounter (present.rs): the display refreshes a frame signal covers,
// estimated from noisy reference instants. The properties here are the ones
// okf/design/frame-timing.md D1 promises: exactly one per signal at full
// rate under swap jitter (the guard against the rejected per-sample
// round()), the true cadence below the refresh rate, recovery from
// duplicate and stray signals, and a bounded drift against the measurement
// over any span.

use crate::present::{RefreshCounter, RefreshCounting, SignalLedger, SignalRecord};

const P60: f64 = 1000.0 / 60.0;

// Deterministic uniform noise in [-amplitude, amplitude] (an LCG; the test
// wants reproducible jitter, not randomness).
struct Noise(u64);

impl Noise {
  fn next(&mut self, amplitude: f64) -> f64 {
    self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let unit = (self.0 >> 11) as f64 / (1u64 << 53) as f64;
    (unit * 2.0 - 1.0) * amplitude
  }
}

// Drive the counter with references at `interval` ms apart plus noise;
// returns the counts from the second signal on, and the largest gap between
// the counted total and the measured elapsed refreshes.
fn drive(counter: &mut RefreshCounter, interval: f64, signals: usize, noise: f64, seed: u64) -> (Vec<u32>, f64) {
  let mut rng = Noise(seed);
  let first = 1000.0 + rng.next(noise);
  counter.on_signal(first);
  let mut counts = Vec::new();
  let mut counted = 1.0;
  let mut worst: f64 = 0.0;
  for k in 1..signals {
    let reference = 1000.0 + k as f64 * interval + rng.next(noise);
    let n = counter.on_signal(reference);
    counts.push(n);
    counted += n as f64;
    // Measured elapsed refreshes since the first reference, plus the one
    // the first signal counted.
    let measured = (reference - first) / counter.period_ms() + 1.0;
    worst = worst.max((counted - measured).abs());
  }
  (counts, worst)
}

#[test]
fn steady_cadence_counts_one_per_signal() {
  let mut counter = RefreshCounter::new();
  assert!((counter.period_ms() - P60).abs() < 1e-9);
  let (counts, worst) = drive(&mut counter, P60, 200, 0.0, 1);
  assert!(counts.iter().all(|&n| n == 1), "exact grid counted {counts:?}");
  assert!(worst < 1e-6, "drift on an exact grid: {worst}");
}

#[test]
fn jitter_within_tolerance_still_counts_one_per_signal() {
  // Swap returns landing up to half a period early or late around a 1:1
  // cadence: the per-sample round() this replaces flipped between 0, 1 and
  // 2 here; the cumulative count must stay at exactly one once the anchor
  // has warmed up, whatever the seed.
  for seed in 1..=20u64 {
    let mut counter = RefreshCounter::new();
    let (counts, worst) = drive(&mut counter, P60, 400, 0.5 * P60, seed);
    let settled = &counts[16..];
    assert!(settled.iter().all(|&n| n == 1), "seed {seed}: jittered 1:1 counted {:?}", &settled[..40]);
    assert!(worst < 1.75, "seed {seed}: drift {worst} periods");
  }
}

#[test]
fn half_rate_counts_two_from_the_second_signal() {
  let mut counter = RefreshCounter::new();
  let (counts, worst) = drive(&mut counter, 2.0 * P60, 50, 0.0, 1);
  assert!(counts.iter().all(|&n| n == 2), "2:1 cadence counted {counts:?}");
  assert!(worst < 1.75, "drift {worst}");
}

#[test]
fn fractional_cadence_settles_into_the_display_pattern() {
  // 4.3 refreshes per frame (21 fps on a 90 Hz panel): whole counts of 4
  // and 5, in the display's own proportion, with the drift bounded.
  let p90 = 1000.0 / 90.0;
  let mut counter = RefreshCounter::new();
  counter.set_hz(90.0);
  let (counts, worst) = drive(&mut counter, 4.3 * p90, 300, 0.2 * p90, 3);
  assert!(counts.iter().all(|&n| n == 4 || n == 5), "4.3:1 cadence counted {:?}", &counts[..30]);
  let mean = counts.iter().map(|&n| n as f64).sum::<f64>() / counts.len() as f64;
  assert!((mean - 4.3).abs() < 0.05, "mean count {mean}");
  assert!(worst < 1.75, "drift {worst}");
}

#[test]
fn duplicate_and_stray_signals_count_zero_and_recover() {
  let mut counter = RefreshCounter::new();
  for n in 0..10u32 {
    counter.on_signal(n as f64 * P60);
  }
  // Two emissions at one instant (several presents released together): the
  // second covers no refresh.
  assert_eq!(counter.on_signal(10.0 * P60), 1);
  assert_eq!(counter.on_signal(10.0 * P60), 0);
  // A stray signal a tenth of a period on is the same refresh.
  assert_eq!(counter.on_signal(10.1 * P60), 0);
  // Cadence resumes without a catch-up.
  assert_eq!(counter.on_signal(11.0 * P60), 1);
  assert_eq!(counter.on_signal(12.0 * P60), 1);
}

#[test]
fn a_dropped_frame_is_counted_when_the_measurement_shows_it() {
  let mut counter = RefreshCounter::new();
  for n in 0..20u32 {
    counter.on_signal(n as f64 * P60);
  }
  // One frame shown for two refreshes, then back on cadence: the count
  // reports the 2 and does not owe anything afterwards.
  assert_eq!(counter.on_signal(21.0 * P60), 2);
  assert_eq!(counter.on_signal(22.0 * P60), 1);
  assert_eq!(counter.on_signal(23.0 * P60), 1);
}

#[test]
fn rate_change_re_anchors_without_a_jump() {
  let mut counter = RefreshCounter::new();
  for n in 0..30u32 {
    counter.on_signal(n as f64 * P60);
  }
  counter.set_hz(120.0);
  let p120 = 1000.0 / 120.0;
  let base = 29.0 * P60;
  for k in 1..=30u32 {
    assert_eq!(counter.on_signal(base + k as f64 * p120), 1, "signal {k} after the rate change");
  }
  // A non-positive rate is ignored.
  counter.set_hz(0.0);
  assert!((counter.period_ms() - p120).abs() < 1e-9);
}

#[test]
fn a_stall_reports_the_whole_count() {
  let mut counter = RefreshCounter::new();
  for n in 0..10u32 {
    counter.on_signal(n as f64 * P60);
  }
  // Presents stop for 5 seconds (app backgrounded): the count says how many
  // refreshes passed; what that means is the consumer's policy.
  assert_eq!(counter.on_signal(10.0 * P60 + 5000.0), 301);
  assert_eq!(counter.on_signal(11.0 * P60 + 5000.0), 1);
}

// Missed presents from the count (RefreshCounting::count): a demanded
// present that took two refreshes missed one; an idle stretch before a
// present is not judged; the Ticks a busy JS thread lets through between
// two presents fold into the present's interval.
#[test]
fn misses_are_the_extra_refreshes_of_a_demanded_interval() {
  let mut c = RefreshCounting::new();
  let mut t = 0.0;
  // A present at `t`, then the reference moves on by `gap` ms.
  let mut present = |c: &mut RefreshCounting, t: &mut f64, demanded: bool, gap: f64| {
    let counted = c.count(*t, 0, true, demanded);
    *t += gap;
    counted.missed
  };
  // Full rate, demanded throughout: no misses.
  for _ in 0..10 {
    assert_eq!(present(&mut c, &mut t, true, P60), 0);
  }
  // One frame shown for two refreshes while demanded: one miss.
  assert_eq!(present(&mut c, &mut t, true, 2.0 * P60), 0);
  assert_eq!(present(&mut c, &mut t, true, P60), 1);
  assert_eq!(present(&mut c, &mut t, true, P60), 0);
  // The app goes idle (no demand at the last present), comes back three
  // seconds later: an idle gap, not jank.
  assert_eq!(present(&mut c, &mut t, false, 3000.0), 0);
  assert_eq!(present(&mut c, &mut t, true, 3.0 * P60), 0);
  // Demanded again, and slow: a 3-refresh frame missed two.
  assert_eq!(present(&mut c, &mut t, true, P60), 2);
}

#[test]
fn ticks_between_presents_fold_into_the_interval() {
  let mut c = RefreshCounting::new();
  assert_eq!(c.count(0.0, 0, true, true).missed, 0);
  // JS is busy for three refreshes; the loop ticks twice meanwhile, then
  // the present lands on the third refresh: the present missed two.
  assert_eq!(c.count(P60, 1, false, false).missed, 0);
  assert_eq!(c.count(2.0 * P60, 1, false, false).missed, 0);
  assert_eq!(c.count(3.0 * P60, 1, true, true).missed, 2);
  // The loop feeds the rate every iteration; an unchanged rate must not
  // drop the interval (it did once: every present read as undemanded).
  c.set_hz(60.0);
  assert_eq!(c.count(5.0 * P60, 2, true, true).missed, 1);
  // A rate change drops the interval in flight.
  c.set_hz(120.0);
  assert_eq!(c.count(5.0 * P60 + 5.0 * 1000.0 / 120.0, 3, true, true).missed, 0);
}

#[test]
fn ledger_keeps_the_newest_records_oldest_first() {
  let mut ledger = SignalLedger::new();
  for frame in 0..600u64 {
    ledger.push(SignalRecord { frame, reference_ms: frame as f64, refreshes: 1, presented: true, demanded: true, missed: 0 });
  }
  let frames: Vec<u64> = ledger.records().map(|r| r.frame).collect();
  assert_eq!(frames.len(), 512);
  assert_eq!(frames[0], 88);
  assert_eq!(*frames.last().expect("records"), 599);
  assert!(frames.windows(2).all(|w| w[1] == w[0] + 1), "records out of order");
}
