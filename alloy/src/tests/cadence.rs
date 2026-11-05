// CadenceController (cadence.rs): the hold as a pure function of the
// per-present facts - a sustained share of measured intervals for stepping
// up, the predicted slot use for stepping down - with the hysteresis that
// keeps it from flapping.

use crate::cadence::{CadenceController, CadenceHold};

// A 90 Hz panel: slots of 11.11 ms. With the 2 ms down margin, a frame
// predicts a fit one slot down when max(cpu, gpu) + 2 fits
// (hold - 1) slots: 9.1 ms into one slot, 20.2 into two, 31.3 into three.
const P90: f64 = 1000.0 / 90.0;
const P60: f64 = 1000.0 / 60.0;
// The policy maximum used throughout: 50 ms, four slots at 90 Hz.
const MAX_MS: u32 = 50;
// Long intervals in the rise's window that raise the hold (UP_LONG), and
// the presents after a hold change that are not evidence (SETTLE_PRESENTS).
const RISE: usize = 10;
const SETTLE: usize = 3;

fn auto() -> CadenceController {
  let mut c = CadenceController::new();
  assert!(c.set_policy(CadenceHold::Auto { max_ms: MAX_MS }).is_none(), "Auto starts unheld");
  c
}

// Feed `n` presents shown for `interval` refreshes each, of `work` ms from
// the slot start, and return the hold after.
fn run(c: &mut CadenceController, t: &mut f64, n: usize, interval: u32, work: f32) -> u32 {
  for _ in 0..n {
    c.on_present(interval, work, 0.0, false, *t, P90);
    *t += interval as f64 * P90;
  }
  c.hold()
}

// Feed presents for `ms` of the clock (exclusive), asserting the hold stays
// at `expect` throughout.
fn run_for(c: &mut CadenceController, t: &mut f64, ms: f64, interval: u32, work: f32, expect: u32) {
  let start = *t;
  while *t - start < ms {
    assert_eq!(run(c, t, 1, interval, work), expect, "at {:.0} ms", *t - start);
  }
}

// Feed presents until the hold changes (or `max_ms` pass), returning the
// clock time that elapsed before the change.
fn until_change(c: &mut CadenceController, t: &mut f64, interval: u32, work: f32, max_ms: f64) -> Option<f64> {
  let start = *t;
  while *t - start < max_ms {
    let before = c.hold();
    run(c, t, 1, interval, work);
    if c.hold() != before {
      return Some(*t - start);
    }
  }
  None
}

fn near(elapsed: Option<f64>, expect: f64) -> bool {
  elapsed.is_some_and(|e| (e - expect).abs() < 100.0)
}

#[test]
fn off_and_fixed_do_not_learn() {
  let mut c = CadenceController::new();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 50, 4, 40.0), 1);
  assert_eq!(c.set_policy(CadenceHold::Fixed(3)), Some(3));
  assert_eq!(run(&mut c, &mut t, 50, 3, 5.0), 3);
  assert_eq!(run(&mut c, &mut t, 50, 4, 40.0), 3);
  assert_eq!(c.set_policy(CadenceHold::Off), Some(1));
  assert_eq!(c.set_policy(CadenceHold::Off), None);
}

#[test]
fn a_sustained_need_raises_the_hold() {
  let mut c = auto();
  let mut t = 0.0;
  // The display shows every frame for three refreshes: the tenth long
  // interval is the cadence.
  assert_eq!(run(&mut c, &mut t, RISE - 1, 3, 25.0), 1);
  assert_eq!(run(&mut c, &mut t, 1, 3, 25.0), 3);
  assert_eq!(run(&mut c, &mut t, 200, 3, 25.0), 3);
}

// A startup burst: a 453 ms first frame and four slow ones (a tablet's cold
// start), then a stream that makes every refresh. Under a third of the
// window, so the hold never moves.
#[test]
fn a_burst_does_not_raise_the_hold() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 1, 27, 60.0), 1);
  assert_eq!(run(&mut c, &mut t, 4, 3, 30.0), 1);
  run_for(&mut c, &mut t, 5000.0, 1, 5.0, 1);
  // A heavy mount later in the stream (a few slow frames) does not either.
  assert_eq!(run(&mut c, &mut t, 6, 2, 20.0), 1);
  run_for(&mut c, &mut t, 5000.0, 1, 5.0, 1);
}

// Long intervals scattered under the share keep the hold; at the share it
// rises, however they are spread.
#[test]
fn the_rise_counts_a_share_not_a_run() {
  let mut c = auto();
  let mut t = 0.0;
  // One long interval in four: 7 or 8 of 30, under the share.
  for _ in 0..50 {
    assert_eq!(run(&mut c, &mut t, 3, 1, 5.0), 1);
    assert_eq!(run(&mut c, &mut t, 1, 2, 15.0), 1);
  }
  // One in two: the share is reached within the window.
  let mut rose = false;
  for _ in 0..15 {
    run(&mut c, &mut t, 1, 1, 5.0);
    rose |= run(&mut c, &mut t, 1, 2, 15.0) == 2;
  }
  assert!(rose, "a long interval every other present raises the hold");
}

#[test]
fn a_rise_goes_to_the_typical_long_interval() {
  let mut c = auto();
  let mut t = 0.0;
  // One frame shown for 40 refreshes among intervals of three: the hold
  // rises to three, not to the worst (or the maximum it would clamp to).
  assert_eq!(run(&mut c, &mut t, 1, 40, 400.0), 1);
  assert_eq!(run(&mut c, &mut t, RISE - 1, 3, 25.0), 3);
}

#[test]
fn beyond_the_maximum_the_hold_stays_at_the_maximum() {
  let mut c = auto();
  let mut t = 0.0;
  // Six refreshes, over the four the policy allows: held at four.
  assert_eq!(run(&mut c, &mut t, RISE, 6, 60.0), 4);
  // A workload on the maximum (intervals of four and five) stays at four
  // rather than flapping between four and unheld.
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  for _ in 0..50 {
    assert_eq!(run(&mut c, &mut t, 2, 5, 44.0), 4);
    assert_eq!(run(&mut c, &mut t, 3, 4, 44.0), 4);
  }
}

#[test]
fn stepping_down_is_predicted_one_step_per_window() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  // The work drops to 14 ms, which predicts a fit in two slots: nothing
  // moves inside the window, then one step, not two.
  run_for(&mut c, &mut t, 990.0, 4, 14.0, 4);
  assert_eq!(run(&mut c, &mut t, 2, 4, 14.0), 3);
  // The second of those two presents opened the next window.
  t -= 4.0 * P90;
  run_for(&mut c, &mut t, 990.0, 3, 14.0, 3);
  assert_eq!(run(&mut c, &mut t, 2, 3, 14.0), 2);
  // 14 ms does not fit one slot: it stays.
  run_for(&mut c, &mut t, 5000.0, 2, 14.0, 2);
}

#[test]
fn no_step_down_while_the_prediction_says_no() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 32.0), 4);
  // 32 + 2 ms does not fit three slots (33.3): held for as long as it runs.
  run_for(&mut c, &mut t, 5000.0, 4, 32.0, 4);
}

// A step down the display disproves (the prediction said three slots, the
// frame still takes four) is reverted by the measured rule, and not tried
// again: a misjudged stream stutters once, not every window. A reset lets
// the controller try afresh.
#[test]
fn a_reverted_step_down_is_not_retried() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  // 31 + 2 fits three slots on paper: after one window the hold tries three.
  assert!(near(until_change(&mut c, &mut t, 4, 31.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 3);
  // The display disagrees: once the chain has settled, two long intervals
  // in a row put the hold back at four (the step was the controller's
  // guess, so it does not wait for the share).
  assert_eq!(run(&mut c, &mut t, SETTLE + 1, 4, 31.0), 3);
  assert_eq!(run(&mut c, &mut t, 1, 4, 31.0), 4);
  // The same prediction is not trusted again, nor a lighter one.
  run_for(&mut c, &mut t, 10000.0, 4, 31.0, 4);
  run_for(&mut c, &mut t, 5000.0, 4, 14.0, 4);
  // A reset starts a new stream: the prediction steps down again.
  assert_eq!(c.reset(), Some(1));
  assert_eq!(run(&mut c, &mut t, SETTLE + RISE, 4, 31.0), 4);
  assert!(near(until_change(&mut c, &mut t, 4, 31.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 3);
}

// A step down that holds is not a revert: a later rise from a heavier
// scene keeps the floor where it was, so the lighter scene after it steps
// down again.
#[test]
fn a_later_rise_is_not_a_revert() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  assert!(near(until_change(&mut c, &mut t, 4, 14.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 3);
  run_for(&mut c, &mut t, 3000.0, 3, 25.0, 3);
  assert_eq!(run(&mut c, &mut t, RISE, 4, 38.0), 4);
  assert!(near(until_change(&mut c, &mut t, 4, 14.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 3);
}

// The step down reads the frame as the pipeline runs it, max(cpu,
// gpu), not the sum. The tablet's tiles flight held at three (60 Hz): 9.9
// ms of CPU and 28.2 of GPU sum past two slots (33.3) but each fits them,
// so it steps to two; at two its 23.6 ms of GPU does not fit one slot, so
// it stays.
#[test]
fn the_step_down_reads_the_pipelined_frame() {
  let mut c = auto();
  let mut t = 0.0;
  for _ in 0..RISE {
    c.on_present(3, 9.9, 28.2, false, t, P60);
    t += 3.0 * P60;
  }
  assert_eq!(c.hold(), 3);
  let start = t;
  while t - start < 1100.0 {
    c.on_present(3, 9.9, 28.2, false, t, P60);
    t += 3.0 * P60;
  }
  assert_eq!(c.hold(), 2);
  let start = t;
  while t - start < 5000.0 {
    assert!(c.on_present(2, 9.1, 23.6, false, t, P60).is_none());
    t += 2.0 * P60;
  }
  assert_eq!(c.hold(), 2);
}

// The pacing delay a vsync-locked frame starts after is not slot use: a
// held frame of 3.9 ms of CPU on a 90 Hz panel steps down to unheld, as
// the phone ran it there (1896 of 1897 refreshes from 6.7 ms into the
// period); 9.2 ms (over 11.1 less the 2 ms margin) stays held.
#[test]
fn the_pacing_delay_is_not_slot_use() {
  for (work, expect) in [(3.9, 1), (9.2, 2)] {
    let mut c = auto();
    let mut t = 0.0;
    for _ in 0..RISE {
      c.on_present(2, work, 0.0, false, t, P90);
      t += 2.0 * P90;
    }
    assert_eq!(c.hold(), 2);
    let start = t;
    while t - start < 1100.0 {
      let shown = c.hold();
      c.on_present(shown, work, 0.0, false, t, P90);
      t += shown as f64 * P90;
    }
    assert_eq!(c.hold(), expect, "work {work} ms");
  }
}

// A workload jittering across the two-slot boundary, against a simulated
// display that shows each frame for the larger of the hold and the slots
// its work needs. The hold must end on the high side, a wrong step down
// must be reverted, and it is not retried, so the whole 33 s run sees a
// handful of changes (a rise to two, to three, a step down and its
// revert).
#[test]
fn a_boundary_workload_settles_on_the_high_side() {
  for seed in 1..=20u64 {
    let mut c = auto();
    let mut t = 0.0;
    let mut state = seed;
    let mut changes = 0;
    let mut reached = false;
    let mut low_frames = 0;
    for _ in 0..1000 {
      state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
      let jitter = (state >> 40) as f32 / (1u64 << 24) as f32; // 0..1
      let work = 17.0 + 6.0 * jitter; // 17..23 ms: two slots or three
      let needs = ((work + 1.0) / P90 as f32).ceil() as u32;
      let shown = needs.max(c.hold());
      if c.on_present(shown, work, 0.0, false, t, P90).is_some() {
        changes += 1;
      }
      t += shown as f64 * P90;
      reached |= c.hold() == 3;
      if reached && c.hold() == 2 {
        low_frames += 1;
      }
      assert!(c.hold() <= 3, "seed {seed}: the hold overshot");
    }
    assert_eq!(c.hold(), 3, "seed {seed}: the hold did not end on the high side");
    assert!(changes <= 4, "seed {seed}: {changes} changes");
    // A wrong step down is reverted by two long intervals in a row past the
    // settle; a workload that misses 30% of its frames at the lower hold
    // produces that pair within a few dozen frames.
    assert!(low_frames <= 40, "seed {seed}: a wrong step down was not reverted ({low_frames} frames low)");
  }
}

#[test]
fn a_policy_change_resets_the_hold() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  assert_eq!(c.set_policy(CadenceHold::Off), Some(1));
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 1);
  assert_eq!(c.set_policy(CadenceHold::Auto { max_ms: MAX_MS }), None);
  assert_eq!(run(&mut c, &mut t, SETTLE + RISE, 4, 38.0), 4);
}

// Under a pipelined chain (swap pacing, unheld) the display shows a 21 ms
// frame on a 60 Hz panel as 1,1,2: the measured rule alone never sees two
// long intervals in a row, and the pipelined estimate carries the rise.
#[test]
fn a_pipelined_chain_rises_on_the_pipelined_estimate() {
  let mut c = CadenceController::new();
  c.set_policy(CadenceHold::Auto { max_ms: MAX_MS });
  let mut t = 0.0;
  // Full rate: 1.6 ms of CPU and 0.6 of GPU, interval 1, no rise.
  for _ in 0..100 {
    assert!(c.on_present(1, 1.6, 0.6, true, t, P60).is_none());
    t += P60;
  }
  assert_eq!(c.hold(), 1);
  // A CPU half of 10 ms and a GPU half of 8 pipeline into one period:
  // max(10, 8) + 2 fits 16.7, so no rise although the sum would not fit.
  for _ in 0..100 {
    assert!(c.on_present(1, 10.0, 8.0, true, t, P60).is_none());
    t += P60;
  }
  assert_eq!(c.hold(), 1);
  // 21 ms of CPU: intervals 1,1,2 as the pipeline shows them. The first
  // long interval (the third present) wakes the estimate, which says two
  // slots for every present after it, and the tenth completes the rise.
  let pattern = [1u32, 1, 2];
  let mut rose_at = None;
  for (i, interval) in pattern.iter().cycle().take(30).enumerate() {
    if c.on_present(*interval, 21.0, 0.6, true, t, P60).is_some() {
      rose_at = Some(i);
      break;
    }
    t += *interval as f64 * P60;
  }
  assert_eq!(rose_at, Some(2 + RISE - 1));
  assert_eq!(c.hold(), 2);
  // Stretched measurements without a long interval are not evidence: a
  // pipelined chain at full rate whose fence and buffer waits read as 19 ms
  // of CPU and 20 of GPU keeps its hold of one for as long as every
  // interval is one.
  let mut c = CadenceController::new();
  c.set_policy(CadenceHold::Auto { max_ms: MAX_MS });
  for _ in 0..500 {
    assert!(c.on_present(1, 19.0, 20.0, true, t, 20.0).is_none());
    t += 20.0;
  }
  assert_eq!(c.hold(), 1);
}

#[test]
fn the_maximum_counts_whole_slots_at_60_hz() {
  let mut c = CadenceController::new();
  c.set_policy(CadenceHold::Auto { max_ms: MAX_MS });
  let mut t = 0.0;
  // 50 ms is three 16.667 ms slots: a frame shown for three is held at
  // three, not treated as beyond the maximum.
  for _ in 0..RISE {
    c.on_present(3, 41.0, 0.6, false, t, P60);
    t += 3.0 * P60;
  }
  assert_eq!(c.hold(), 3);
}

// A single present that does not predict a fit is a spike and keeps the
// down window open, as a single long interval does not raise the hold; two
// in a row close it.
#[test]
fn an_isolated_spike_does_not_close_the_down_window() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  // Three slots' worth of work with one 38 ms spike every 400 ms: the
  // window still completes after a second.
  let start = t;
  let mut next_spike = start + 400.0;
  let mut stepped = None;
  while t - start < 3000.0 && stepped.is_none() {
    let work = if t >= next_spike {
      next_spike += 400.0;
      38.0
    } else {
      14.0
    };
    if c.on_present(4, work, 0.0, false, t, P90).is_some() {
      stepped = Some(t - start);
    }
    t += 4.0 * P90;
  }
  assert!(near(stepped, 1000.0), "stepped at {stepped:?}");
  assert_eq!(c.hold(), 3);
  // Two spikes in a row close the window: no step for as long as they
  // keep coming in pairs every 600 ms.
  let start = t;
  let mut phase = 0;
  while t - start < 3000.0 {
    let work = if phase % 14 < 2 { 38.0 } else { 14.0 };
    phase += 1;
    assert!(c.on_present(3, work, 0.0, false, t, P90).is_none(), "stepped at {:.0} ms", t - start);
    t += 3.0 * P90;
  }
  assert_eq!(c.hold(), 3);
}

// The presents right after a hold change show the pipeline re-forming, not
// the workload: they are not evidence for a rise. The ones after them are.
#[test]
fn the_first_presents_after_a_change_do_not_revert_it() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  assert!(near(until_change(&mut c, &mut t, 4, 14.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 3);
  // Three long intervals immediately after the step: the transition, not
  // evidence. Two more after the settle: a real revert.
  assert_eq!(run(&mut c, &mut t, SETTLE, 4, 14.0), 3);
  assert_eq!(run(&mut c, &mut t, 1, 4, 14.0), 3);
  assert_eq!(run(&mut c, &mut t, 1, 4, 14.0), 4);
}

// An idle gap ends the stream the hold was learned on: the first present
// after it resets the hold to 1 (reporting the change), whatever interval
// the gap left it with, and the controller learns afresh from there.
#[test]
fn an_idle_gap_resets_the_hold() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, RISE, 3, 30.0), 3, "held at three after a run of long intervals");
  t += 2000.0;
  let change = c.on_present(180, 5.0, 0.0, false, t, P90);
  assert_eq!(change.map(|h| (h.from, h.to)), Some((3, 1)));
  assert_eq!(c.hold(), 1);
  // A held stream that keeps missing after the gap still rises again.
  t += P90;
  assert_eq!(run(&mut c, &mut t, SETTLE + RISE, 3, 30.0), 3);
  // A gap with the hold already at 1 changes nothing and reports nothing.
  let mut c1 = auto();
  let mut t1 = 0.0;
  run(&mut c1, &mut t1, 3, 1, 5.0);
  t1 += 2000.0;
  assert!(c1.on_present(180, 5.0, 0.0, false, t1, P90).is_none());
  assert_eq!(c1.hold(), 1);
}

// The app's forced reset ends a learned hold at once and keeps the policy;
// under Off or Fixed there is nothing learned to end.
#[test]
fn a_reset_ends_the_hold_and_keeps_the_policy() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, RISE, 3, 30.0), 3);
  assert_eq!(c.reset(), Some(1));
  assert_eq!(c.reset(), None);
  // Still Auto: a sustained need rises again (past the settle), and only
  // a sustained one.
  assert_eq!(run(&mut c, &mut t, SETTLE + RISE - 1, 3, 30.0), 1);
  assert_eq!(run(&mut c, &mut t, 1, 3, 30.0), 3);
  let mut c = CadenceController::new();
  assert_eq!(c.set_policy(CadenceHold::Fixed(2)), Some(2));
  assert_eq!(c.reset(), None);
  assert_eq!(c.hold(), 2);
}
