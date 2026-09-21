// CadenceController (cadence.rs): the hold as a pure function of the
// per-present facts - the measured interval for stepping up, the predicted
// slot use for stepping down - with the hysteresis that keeps it from
// flapping.

use crate::cadence::{CadenceController, CadenceHold};

// A 90 Hz panel: slots of 11.11 ms. With the 2 ms down margin, a frame
// predicts a fit one slot down when offset + work + 2 fits (hold - 1)
// slots: 9.1 ms into one slot, 20.2 into two, 31.3 into three.
const P90: f64 = 1000.0 / 90.0;
// The policy maximum used throughout: 50 ms, four slots at 90 Hz.
const MAX_MS: u32 = 50;

fn auto() -> CadenceController {
  let mut c = CadenceController::new();
  assert!(c.set_policy(CadenceHold::Auto { max_ms: MAX_MS }).is_none(), "Auto starts unheld");
  c
}

// Feed `n` presents shown for `interval` refreshes each, of `work` ms from
// the slot start, and return the hold after.
fn run(c: &mut CadenceController, t: &mut f64, n: usize, interval: u32, work: f32) -> u32 {
  for _ in 0..n {
    c.on_present(interval, work, 0.0, 0.0, false, *t, P90);
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
fn a_longer_interval_twice_raises_the_hold() {
  let mut c = auto();
  let mut t = 0.0;
  // The display showed the frame for three refreshes: once is a drop,
  // twice is the cadence.
  assert_eq!(run(&mut c, &mut t, 1, 3, 25.0), 1);
  assert_eq!(run(&mut c, &mut t, 1, 3, 25.0), 3);
  assert_eq!(run(&mut c, &mut t, 200, 3, 25.0), 3);
}

#[test]
fn one_dropped_frame_does_not_step_up() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 1, 2.0), 1);
  assert!(c.on_present(2, 2.0, 0.0, 0.0, false, t, P90).is_none(), "a single drop must not move the hold");
  t += 2.0 * P90;
  assert_eq!(run(&mut c, &mut t, 5, 1, 2.0), 1);
  assert_eq!(run(&mut c, &mut t, 2, 2, 2.0), 2);
}

#[test]
fn a_rise_goes_to_the_worst_of_the_run() {
  let mut c = auto();
  let mut t = 0.0;
  // Four refreshes, then three: the hold rises to four, not to the three
  // the second present happened to show.
  assert_eq!(run(&mut c, &mut t, 1, 4, 38.0), 1);
  assert_eq!(run(&mut c, &mut t, 1, 3, 25.0), 4);
}

#[test]
fn beyond_the_maximum_the_hold_stays_at_the_maximum() {
  let mut c = auto();
  let mut t = 0.0;
  // Six refreshes, over the four the policy allows: held at four.
  assert_eq!(run(&mut c, &mut t, 20, 6, 60.0), 4);
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

#[test]
fn a_reverted_step_down_backs_off() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
  // 31 + 2 fits three slots on paper: after one window the hold tries three.
  assert!(near(until_change(&mut c, &mut t, 4, 31.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 3);
  // The display disagrees (the frame still takes four): once the chain
  // has settled, two long intervals put the hold back at four, and the
  // next try waits two windows.
  assert_eq!(run(&mut c, &mut t, 3, 4, 31.0), 3);
  assert_eq!(run(&mut c, &mut t, 2, 4, 31.0), 4);
  assert!(near(until_change(&mut c, &mut t, 4, 31.0, 5000.0), 2000.0));
  assert_eq!(c.hold(), 3);
  // Reverted again: four windows.
  assert_eq!(run(&mut c, &mut t, 3, 4, 31.0), 3);
  assert_eq!(run(&mut c, &mut t, 2, 4, 31.0), 4);
  assert!(near(until_change(&mut c, &mut t, 4, 31.0, 9000.0), 4000.0));
  assert_eq!(c.hold(), 3);
  // This time it holds: once its window has passed the base window is
  // restored, so a later step down waits one window again.
  run_for(&mut c, &mut t, 4100.0, 3, 31.0, 3);
  assert!(near(until_change(&mut c, &mut t, 3, 14.0, 5000.0), 1000.0));
  assert_eq!(c.hold(), 2);
}

#[test]
fn the_start_offset_counts_as_slot_use() {
  // 2 ms of work from 6.7 ms into the slot (the vsync-locked pacing delay)
  // still predicts a fit in one slot (10.7 of 11.1 ms); 3 ms does not. The
  // display shows each frame for the hold in force.
  for (work, expect) in [(2.0, 1), (3.0, 2)] {
    let mut c = auto();
    let mut t = 0.0;
    for _ in 0..2 {
      c.on_present(2, work, 0.0, 6.7, false, t, P90);
      t += 2.0 * P90;
    }
    assert_eq!(c.hold(), 2);
    let start = t;
    while t - start < 1100.0 {
      let shown = c.hold();
      c.on_present(shown, work, 0.0, 6.7, false, t, P90);
      t += shown as f64 * P90;
    }
    assert_eq!(c.hold(), expect, "work {work} ms");
  }
}

// A workload jittering across the two-slot boundary, against a simulated
// display that shows each frame for the larger of the hold and the slots
// its work needs. The hold must end on the high side and every wrong step
// down must be reverted within two frames, with the retries backing off,
// so the whole 33 s run sees a bounded number of changes.
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
      if c.on_present(shown, work, 0.0, 0.0, false, t, P90).is_some() {
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
    assert!(changes <= 12, "seed {seed}: {changes} changes");
    // A wrong step down is reverted by two consecutive long intervals; a
    // workload that misses 30 percent of its frames at the lower hold
    // produces that pair within a few dozen frames, not two.
    assert!(low_frames <= 20 * changes, "seed {seed}: a wrong step down was not reverted ({low_frames} frames low)");
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
  assert_eq!(run(&mut c, &mut t, 10, 4, 38.0), 4);
}

// Under a pipelined chain (swap pacing, unheld) the display shows a 21 ms
// frame on a 60 Hz panel as 1,1,2: the measured rule alone never sees two
// long intervals in a row, and the pipelined estimate carries the rise.
#[test]
fn a_pipelined_chain_rises_on_the_pipelined_estimate() {
  const P60: f64 = 1000.0 / 60.0;
  let mut c = CadenceController::new();
  c.set_policy(CadenceHold::Auto { max_ms: MAX_MS });
  let mut t = 0.0;
  // Full rate: 1.6 ms of CPU and 0.6 of GPU, interval 1, no rise.
  for _ in 0..100 {
    assert!(c.on_present(1, 1.6, 0.6, 0.0, true, t, P60).is_none());
    t += P60;
  }
  assert_eq!(c.hold(), 1);
  // A CPU half of 10 ms and a GPU half of 8 pipeline into one period:
  // max(10, 8) + 2 fits 16.7, so no rise although the sum would not fit.
  for _ in 0..100 {
    assert!(c.on_present(1, 10.0, 8.0, 0.0, true, t, P60).is_none());
    t += P60;
  }
  assert_eq!(c.hold(), 1);
  // 21 ms of CPU: intervals 1,1,2 as the pipeline shows them. The first
  // long interval wakes the estimate, which says two slots, and the next
  // present completes the rise.
  let pattern = [1u32, 1, 2];
  let mut rose_at = None;
  for (i, interval) in pattern.iter().cycle().take(30).enumerate() {
    if c.on_present(*interval, 21.0, 0.6, 0.0, true, t, P60).is_some() {
      rose_at = Some(i);
      break;
    }
    t += *interval as f64 * P60;
  }
  assert_eq!(rose_at, Some(3));
  assert_eq!(c.hold(), 2);
  // Stretched measurements without a long interval are not evidence: a
  // pipelined chain at full rate whose fence and buffer waits read as 19 ms
  // of CPU and 20 of GPU keeps its hold of one for as long as every
  // interval is one.
  let mut c = CadenceController::new();
  c.set_policy(CadenceHold::Auto { max_ms: MAX_MS });
  for _ in 0..500 {
    assert!(c.on_present(1, 19.0, 20.0, 0.0, true, t, 20.0).is_none());
    t += 20.0;
  }
  assert_eq!(c.hold(), 1);
}

#[test]
fn the_maximum_counts_whole_slots_at_60_hz() {
  const P60: f64 = 1000.0 / 60.0;
  let mut c = CadenceController::new();
  c.set_policy(CadenceHold::Auto { max_ms: MAX_MS });
  let mut t = 0.0;
  // 50 ms is three 16.667 ms slots: a frame shown for three is held at
  // three, not treated as beyond the maximum.
  for _ in 0..3 {
    c.on_present(3, 41.0, 0.6, 0.0, false, t, P60);
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
    if c.on_present(4, work, 0.0, 0.0, false, t, P90).is_some() {
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
    assert!(c.on_present(3, work, 0.0, 0.0, false, t, P90).is_none(), "stepped at {:.0} ms", t - start);
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
  // Two long intervals immediately after the step: the transition.
  assert_eq!(run(&mut c, &mut t, 2, 4, 14.0), 3);
  // Two more after the settle: a real revert.
  assert_eq!(run(&mut c, &mut t, 1, 4, 14.0), 3);
  assert_eq!(run(&mut c, &mut t, 2, 4, 14.0), 4);
}

// An idle gap ends the stream the hold was learned on: the first present
// after it resets the hold to 1 (reporting the change), whatever interval
// the gap left it with, and the controller learns afresh from there.
#[test]
fn an_idle_gap_resets_the_hold() {
  let mut c = auto();
  let mut t = 0.0;
  assert_eq!(run(&mut c, &mut t, 6, 3, 30.0), 3, "held at three after a run of long intervals");
  t += 2000.0;
  let change = c.on_present(180, 5.0, 0.0, 0.0, false, t, P90);
  assert_eq!(change.map(|h| (h.from, h.to)), Some((3, 1)));
  assert_eq!(c.hold(), 1);
  // A held stream that keeps missing after the gap still rises again.
  t += P90;
  assert_eq!(run(&mut c, &mut t, 6, 3, 30.0), 3);
  // A gap with the hold already at 1 changes nothing and reports nothing.
  let mut c1 = auto();
  let mut t1 = 0.0;
  run(&mut c1, &mut t1, 3, 1, 5.0);
  t1 += 2000.0;
  assert!(c1.on_present(180, 5.0, 0.0, 0.0, false, t1, P90).is_none());
  assert_eq!(c1.hold(), 1);
}
