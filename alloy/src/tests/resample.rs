use std::time::{Duration, Instant};

use crate::{AlloyEvent, Modifiers, PointerType};

use crate::resample::{Resampler, SharedResampler};

const KEY: (PointerType, u64) = (PointerType::Touch, 1);

// The refresh period of the tests' display, ms.
const PERIOD_MS: u64 = 10;
// RESAMPLE_LATENCY in ms: a frame's reference sits this far past the instant
// its touch is resampled to.
const LATENCY_MS: u64 = 5;

// One origin for a test's times, far enough from the clock's own start that
// reaching back from it cannot underflow.
fn origin() -> Instant {
  static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
  *ORIGIN.get_or_init(|| Instant::now() + Duration::from_secs(1))
}

fn at(ms: u64) -> Instant {
  origin() + Duration::from_millis(ms)
}

// The frame whose touch is resampled to `ms`.
fn frame(ms: u64) -> Option<(Instant, Duration)> {
  Some((at(ms + LATENCY_MS), Duration::from_millis(PERIOD_MS)))
}

fn down(r: &mut Resampler, key: (PointerType, u64), x: f32, ms: u64) {
  r.down(key, x, 0.0, Modifiers::default(), at(ms));
}

fn push(r: &mut Resampler, key: (PointerType, u64), x: f32, ms: u64) {
  r.push(key, x, 0.0, None, Modifiers::default(), at(ms));
}

// The x positions dispatched for the frame resampled to `ms`, per pointer id.
fn xs(r: &mut Resampler, ms: u64) -> Vec<(u64, f32)> {
  let mut out: Vec<(u64, f32)> = r.sample(frame(ms)).iter().map(|s| (s.pointer_id, s.x)).collect();
  out.sort_by(|a, b| a.0.cmp(&b.0));
  out
}

// The (x, time in ms) dispatched for the frame resampled to `ms`,
// single-pointer tests.
fn xt(r: &mut Resampler, ms: u64) -> Vec<(f32, u64)> {
  r.sample(frame(ms)).iter().map(|s| (s.x, s.at.duration_since(origin()).as_millis() as u64)).collect()
}

#[test]
fn steady_stream_dispatches_each_sample_at_its_time() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 10)]);
  push(&mut r, KEY, 20.0, 20);
  assert_eq!(xt(&mut r, 20), vec![(20.0, 20)]);
}

#[test]
fn target_between_samples_interpolates_at_the_target() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  push(&mut r, KEY, 20.0, 20);
  assert_eq!(xt(&mut r, 15), vec![(15.0, 15)]);
  // What lies past the target stays fresh; with no sample past the next
  // target the newest goes out as it is, at its own time.
  assert_eq!(xt(&mut r, 25), vec![(20.0, 20)]);
}

#[test]
fn newest_sample_a_hair_past_the_target_dispatches_whole() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  // Within RESAMPLE_MIN_AHEAD of the target: not split, so nothing of it is
  // left fresh and the next frame can bridge.
  assert_eq!(xt(&mut r, 9), vec![(10.0, 10)]);
  assert_eq!(xt(&mut r, 20), vec![(20.0, 20)]);
}

#[test]
fn fresh_sample_is_never_held_back() {
  let mut r = Resampler::new();
  // The down and the first move both land inside the latency window, after
  // the target: the move goes out as it is.
  down(&mut r, KEY, 0.0, 12);
  push(&mut r, KEY, 10.0, 14);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 14)]);
}

#[test]
fn paired_delivery_bridges_without_stall_or_double_step() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 10)]);
  // Empty frame: the late batch is bridged with the pointer's own travel
  // over the span behind its newest sample, at the target's time.
  assert_eq!(xt(&mut r, 20), vec![(20.0, 20)]);
  // The pair lands next frame; its newest continues in equal steps.
  push(&mut r, KEY, 20.0, 20);
  push(&mut r, KEY, 30.0, 30);
  assert_eq!(xt(&mut r, 30), vec![(30.0, 30)]);
}

#[test]
fn abrupt_stop_settles_to_real_position_then_holds() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 10)]);
  push(&mut r, KEY, 20.0, 20);
  assert_eq!(xt(&mut r, 20), vec![(20.0, 20)]);
  // First empty frame bridges; the second reveals a stop and settles back,
  // at the time the finger really got there.
  assert_eq!(xt(&mut r, 30), vec![(30.0, 30)]);
  assert_eq!(xt(&mut r, 40), vec![(20.0, 20)]);
  assert_eq!(xt(&mut r, 50), vec![]);
  assert_eq!(xt(&mut r, 60), vec![]);
}

#[test]
fn only_the_bridging_step_is_marked_predicted() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  let flags = |r: &mut Resampler, ms: u64| r.sample(frame(ms)).iter().map(|s| s.predicted).collect::<Vec<bool>>();
  assert_eq!(flags(&mut r, 10), vec![false]);
  assert_eq!(flags(&mut r, 20), vec![true]);
  // The settle-back is the real position again.
  assert_eq!(flags(&mut r, 30), vec![false]);
}

#[test]
fn sample_older_than_the_bridge_bound_is_a_stop() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 10)]);
  // Two periods on, past PREDICT_MAX_PERIODS: no late delivery looks like
  // that, so nothing is predicted.
  assert_eq!(xt(&mut r, 30), vec![]);
}

#[test]
fn slow_frames_dispatch_the_newest_sample_at_its_own_time() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  for step in 1..=4 {
    push(&mut r, KEY, step as f32 * 10.0, step * 10);
  }
  // A frame every 70 ms, a finger that stopped at 40: the move says 40.
  assert_eq!(xt(&mut r, 70), vec![(40.0, 40)]);
  assert_eq!(xt(&mut r, 140), vec![]);
}

#[test]
fn resting_sample_keeps_the_time_the_pointer_got_there() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  // A second finger moves and the batch reports this one where it was.
  push(&mut r, KEY, 10.0, 20);
  push(&mut r, KEY, 10.0, 30);
  assert_eq!(xt(&mut r, 30), vec![(10.0, 10)]);
  assert_eq!(xt(&mut r, 40), vec![]);
}

#[test]
fn down_alone_never_redispatches_and_gap_without_travel_holds() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 5.0, 0);
  // The down dispatched on arrival; sample() has nothing to add, and a
  // pointer with no travel behind it predicts nothing.
  assert_eq!(xs(&mut r, 5), vec![]);
  assert_eq!(xs(&mut r, 15), vec![]);
}

#[test]
fn move_without_down_tracks_from_first_sample() {
  let mut r = Resampler::new();
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xs(&mut r, 10), vec![(1, 10.0)]);
  // One position known: no travel to mirror, so a gap holds.
  assert_eq!(xs(&mut r, 20), vec![]);
}

#[test]
fn up_flushes_undispatched_samples() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  push(&mut r, KEY, 25.0, 20);
  let last = r.up(KEY).expect("the samples no frame dispatched");
  assert_eq!((last.x, last.dx, last.at), (25.0, 25.0, at(20)));
  assert_eq!(xs(&mut r, 30), vec![]);
}

#[test]
fn up_after_a_dispatched_move_flushes_nothing() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 10)]);
  assert!(r.up(KEY).is_none());
}

#[test]
fn up_after_a_predicted_move_settles_to_the_real_position() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xt(&mut r, 10), vec![(10.0, 10)]);
  assert_eq!(xt(&mut r, 20), vec![(20.0, 20)]);
  let last = r.up(KEY).expect("the real position behind the predicted one");
  assert_eq!((last.x, last.dx, last.at), (10.0, -10.0, at(10)));
}

#[test]
fn pointers_are_independent() {
  let key2 = (PointerType::Touch, 2);
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  down(&mut r, key2, 100.0, 0);
  push(&mut r, KEY, 10.0, 10);
  push(&mut r, key2, 90.0, 10);
  assert_eq!(xs(&mut r, 10), vec![(1, 10.0), (2, 90.0)]);
  // Pointer 1 goes quiet and bridges; pointer 2 keeps delivering.
  push(&mut r, key2, 80.0, 20);
  assert_eq!(xs(&mut r, 20), vec![(1, 20.0), (2, 80.0)]);
}

#[test]
fn clear_resets_all_histories() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  r.clear();
  assert_eq!(xs(&mut r, 10), vec![]);
}

#[test]
fn frame_without_a_time_dispatches_the_newest_and_never_predicts() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  push(&mut r, KEY, 20.0, 20);
  let xs = |r: &mut Resampler| r.sample(None).iter().map(|s| s.x).collect::<Vec<f32>>();
  assert_eq!(xs(&mut r), vec![20.0]);
  assert_eq!(xs(&mut r), Vec::<f32>::new());
}

const MOUSE: (PointerType, u64) = (PointerType::Mouse, 1);

#[test]
fn mouse_dispatches_newest_sample_per_frame_at_its_time() {
  let mut r = Resampler::new();
  // Two arrivals in one frame: only the newest dispatches, not resampled.
  push(&mut r, MOUSE, 10.0, 12);
  push(&mut r, MOUSE, 20.0, 18);
  assert_eq!(xt(&mut r, 15), vec![(20.0, 18)]);
  push(&mut r, MOUSE, 30.0, 27);
  assert_eq!(xt(&mut r, 25), vec![(30.0, 27)]);
}

#[test]
fn mouse_never_predicts_on_gap() {
  let mut r = Resampler::new();
  push(&mut r, MOUSE, 10.0, 10);
  assert_eq!(xs(&mut r, 10), vec![(1, 10.0)]);
  push(&mut r, MOUSE, 20.0, 20);
  assert_eq!(xs(&mut r, 20), vec![(1, 20.0)]);
  // A stop is a stop: no bridged overshoot, no settle-back bounce.
  assert_eq!(xs(&mut r, 30), vec![]);
  assert_eq!(xs(&mut r, 40), vec![]);
}

#[test]
fn down_collapses_buffered_move() {
  let mut r = Resampler::new();
  push(&mut r, MOUSE, 10.0, 10);
  down(&mut r, MOUSE, 15.0, 12);
  // The down dispatched on arrival with the newest position; the buffered
  // pre-down move must not dispatch stale after it.
  assert_eq!(xs(&mut r, 15), vec![]);
}

#[test]
fn touch_bridges_while_mouse_holds() {
  let mouse = (PointerType::Mouse, 2);
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  push(&mut r, mouse, 90.0, 10);
  assert_eq!(xs(&mut r, 10), vec![(1, 10.0), (2, 90.0)]);
  // Both go quiet: touch bridges one step, mouse just holds.
  assert_eq!(xs(&mut r, 20), vec![(1, 20.0)]);
}

// Movement (dx/dy) contract: hardware deltas sum per frame for pointers that
// report them; everything else diffs against the last dispatched position.

fn push_rel(r: &mut Resampler, key: (PointerType, u64), x: f32, dx: f32, ms: u64) {
  r.push(key, x, 0.0, Some((dx, 0.0)), Modifiers::default(), at(ms));
}

// The (x, dx) dispatched for the frame resampled to `ms`, single-pointer
// tests.
fn xdx(r: &mut Resampler, ms: u64) -> Vec<(f32, f32)> {
  r.sample(frame(ms)).iter().map(|s| (s.x, s.dx)).collect()
}

#[test]
fn hardware_deltas_sum_within_a_frame() {
  let mut r = Resampler::new();
  // Two arrivals in one frame: the position collapses to the newest, the
  // deltas must sum - a fast flick loses distance otherwise.
  push_rel(&mut r, MOUSE, 10.0, 10.0, 5);
  push_rel(&mut r, MOUSE, 25.0, 15.0, 10);
  assert_eq!(xdx(&mut r, 10), vec![(25.0, 25.0)]);
  // Drained: the next frame accumulates from zero.
  push_rel(&mut r, MOUSE, 30.0, 5.0, 20);
  assert_eq!(xdx(&mut r, 20), vec![(30.0, 5.0)]);
}

#[test]
fn hardware_deltas_survive_a_mid_flick_down() {
  let mut r = Resampler::new();
  push_rel(&mut r, MOUSE, 10.0, 10.0, 5);
  // A click mid-flick re-seeds the history; the accumulated motion
  // physically happened and must still dispatch.
  down(&mut r, MOUSE, 12.0, 6);
  push_rel(&mut r, MOUSE, 15.0, 3.0, 10);
  assert_eq!(xdx(&mut r, 10), vec![(15.0, 13.0)]);
}

#[test]
fn hardware_deltas_report_motion_while_position_freezes() {
  // Relative mouse mode: SDL freezes x/y and reports motion only in rel.
  let mut r = Resampler::new();
  push_rel(&mut r, MOUSE, 50.0, 8.0, 10);
  assert_eq!(xdx(&mut r, 10), vec![(50.0, 8.0)]);
  push_rel(&mut r, MOUSE, 50.0, 12.0, 20);
  assert_eq!(xdx(&mut r, 20), vec![(50.0, 12.0)]);
}

#[test]
fn derived_movement_diffs_dispatched_positions() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  // First move diffs against the down's contact seed.
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xdx(&mut r, 10), vec![(10.0, 10.0)]);
  push(&mut r, KEY, 25.0, 20);
  assert_eq!(xdx(&mut r, 20), vec![(25.0, 15.0)]);
}

#[test]
fn derived_movement_mirrors_prediction_bounce() {
  let mut r = Resampler::new();
  down(&mut r, KEY, 0.0, 0);
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xdx(&mut r, 10), vec![(10.0, 10.0)]);
  push(&mut r, KEY, 20.0, 20);
  assert_eq!(xdx(&mut r, 20), vec![(20.0, 10.0)]);
  // Bridged step and settle-back: movement bounces exactly with position.
  assert_eq!(xdx(&mut r, 30), vec![(30.0, 10.0)]);
  assert_eq!(xdx(&mut r, 40), vec![(20.0, -10.0)]);
}

#[test]
fn derived_movement_without_down_starts_at_zero() {
  let mut r = Resampler::new();
  // No down, no baseline: the first dispatch cannot claim movement.
  push(&mut r, KEY, 10.0, 10);
  assert_eq!(xdx(&mut r, 10), vec![(10.0, 0.0)]);
  push(&mut r, KEY, 16.0, 20);
  assert_eq!(xdx(&mut r, 20), vec![(16.0, 6.0)]);
}

// The producer-side feed: what travels to the UI loop and when it happened.

fn fed(r: &SharedResampler, event: AlloyEvent, ms: u64) -> Vec<(&'static str, u64)> {
  let mut sent = Vec::new();
  r.feed(event, at(ms), |e, when| {
    let kind = match e {
      AlloyEvent::PointerMove { .. } => "move",
      AlloyEvent::PointerDown { .. } => "down",
      AlloyEvent::PointerUp { .. } => "up",
      _ => "other",
    };
    sent.push((kind, when.duration_since(origin()).as_millis() as u64));
    Ok::<(), ()>(())
  })
  .expect("the test's send never fails");
  sent
}

fn pointer(kind: &str, x: f32) -> AlloyEvent {
  let (pointer_id, pointer_type, y, modifiers) = (KEY.1, KEY.0, 0.0, Modifiers::default());
  match kind {
    "down" => AlloyEvent::PointerDown { pointer_id, pointer_type, button: 0, x, y, modifiers },
    "up" => AlloyEvent::PointerUp { pointer_id, pointer_type, button: 0, x, y, modifiers },
    _ => AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, rel: None, modifiers },
  }
}

#[test]
fn feed_consumes_moves_and_flushes_the_last_ahead_of_the_up() {
  let r = SharedResampler::new();
  assert_eq!(fed(&r, pointer("down", 0.0), 0), vec![("down", 0)]);
  assert_eq!(fed(&r, pointer("move", 10.0), 10), vec![]);
  assert_eq!(fed(&r, pointer("move", 20.0), 20), vec![]);
  // No frame ran: the lift is read against the last sample, at its time.
  assert_eq!(fed(&r, pointer("up", 20.0), 90), vec![("move", 20), ("up", 90)]);
  assert_eq!(fed(&r, AlloyEvent::Back, 95), vec![("other", 95)]);
}
