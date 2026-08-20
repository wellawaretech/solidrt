use crate::{Modifiers, PointerType};

use crate::resample::Resampler;

const KEY: (PointerType, u64) = (PointerType::Touch, 1);

fn push(r: &mut Resampler, key: (PointerType, u64), x: f32) {
  r.push(key, x, 0.0, None, Modifiers::default());
}

// The x positions dispatched for one frame slot, per pointer id.
fn xs(r: &mut Resampler) -> Vec<(u64, f32)> {
  let mut out: Vec<(u64, f32)> = r.sample().iter().map(|s| (s.pointer_id, s.x)).collect();
  out.sort_by(|a, b| a.0.cmp(&b.0));
  out
}

#[test]
fn steady_stream_dispatches_each_sample() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0)]);
  push(&mut r, KEY, 20.0);
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
}

#[test]
fn paired_delivery_bridges_without_stall_or_double_step() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0)]);
  // Empty vsync: the missing sample is bridged with one velocity step.
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
  // The pair lands next vsync; its newest continues in equal steps.
  push(&mut r, KEY, 20.0);
  push(&mut r, KEY, 30.0);
  assert_eq!(xs(&mut r), vec![(1, 30.0)]);
}

#[test]
fn abrupt_stop_settles_to_real_position_then_holds() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0)]);
  push(&mut r, KEY, 20.0);
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
  // First empty slot bridges; the second reveals a stop and settles back.
  assert_eq!(xs(&mut r), vec![(1, 30.0)]);
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
  assert_eq!(xs(&mut r), vec![]);
  assert_eq!(xs(&mut r), vec![]);
}

#[test]
fn down_alone_never_redispatches_and_gap_without_velocity_holds() {
  let mut r = Resampler::new();
  r.down(KEY, 5.0, 0.0, Modifiers::default());
  // The down dispatched on arrival; sample() has nothing to add, and with a
  // single known position there is no velocity to bridge with.
  assert_eq!(xs(&mut r), vec![]);
  assert_eq!(xs(&mut r), vec![]);
}

#[test]
fn move_without_down_tracks_from_first_sample() {
  let mut r = Resampler::new();
  push(&mut r, KEY, 10.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0)]);
  // Still no velocity (one position known), so a gap holds.
  assert_eq!(xs(&mut r), vec![]);
}

#[test]
fn up_drops_buffered_samples() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  r.remove(KEY);
  assert_eq!(xs(&mut r), vec![]);
}

#[test]
fn pointers_are_independent() {
  let key2 = (PointerType::Touch, 2);
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  r.down(key2, 100.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  push(&mut r, key2, 90.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0), (2, 90.0)]);
  // Pointer 1 goes quiet and bridges; pointer 2 keeps delivering.
  push(&mut r, key2, 80.0);
  assert_eq!(xs(&mut r), vec![(1, 20.0), (2, 80.0)]);
}

#[test]
fn clear_resets_all_histories() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  r.clear();
  assert_eq!(xs(&mut r), vec![]);
}

const MOUSE: (PointerType, u64) = (PointerType::Mouse, 1);

#[test]
fn mouse_dispatches_latest_position_per_slot() {
  let mut r = Resampler::new();
  // Two arrivals in one slot: only the latest dispatches.
  push(&mut r, MOUSE, 10.0);
  push(&mut r, MOUSE, 20.0);
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
  push(&mut r, MOUSE, 30.0);
  assert_eq!(xs(&mut r), vec![(1, 30.0)]);
}

#[test]
fn mouse_never_extrapolates_on_gap() {
  let mut r = Resampler::new();
  push(&mut r, MOUSE, 10.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0)]);
  push(&mut r, MOUSE, 20.0);
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
  // A stop is a stop: no bridged overshoot, no settle-back bounce.
  assert_eq!(xs(&mut r), vec![]);
  assert_eq!(xs(&mut r), vec![]);
}

#[test]
fn down_collapses_buffered_move() {
  let mut r = Resampler::new();
  push(&mut r, MOUSE, 10.0);
  r.down(MOUSE, 15.0, 0.0, Modifiers::default());
  // The down dispatched on arrival with the newest position; the buffered
  // pre-down move must not dispatch stale after it.
  assert_eq!(xs(&mut r), vec![]);
}

#[test]
fn touch_bridges_while_mouse_holds() {
  let mouse = (PointerType::Mouse, 2);
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  push(&mut r, mouse, 90.0);
  assert_eq!(xs(&mut r), vec![(1, 10.0), (2, 90.0)]);
  // Both go quiet: touch bridges one velocity step, mouse just holds.
  assert_eq!(xs(&mut r), vec![(1, 20.0)]);
}

// Movement (dx/dy) contract: hardware deltas sum per slot for pointers that
// report them; everything else diffs against the last dispatched position.

fn push_rel(r: &mut Resampler, key: (PointerType, u64), x: f32, dx: f32) {
  r.push(key, x, 0.0, Some((dx, 0.0)), Modifiers::default());
}

// The (x, dx) dispatched for one frame slot, single-pointer tests.
fn xdx(r: &mut Resampler) -> Vec<(f32, f32)> {
  r.sample().iter().map(|s| (s.x, s.dx)).collect()
}

#[test]
fn hardware_deltas_sum_within_a_slot() {
  let mut r = Resampler::new();
  // Two arrivals in one slot: the position collapses to the latest, the
  // deltas must sum - a fast flick loses distance otherwise.
  push_rel(&mut r, MOUSE, 10.0, 10.0);
  push_rel(&mut r, MOUSE, 25.0, 15.0);
  assert_eq!(xdx(&mut r), vec![(25.0, 25.0)]);
  // Drained: the next slot accumulates from zero.
  push_rel(&mut r, MOUSE, 30.0, 5.0);
  assert_eq!(xdx(&mut r), vec![(30.0, 5.0)]);
}

#[test]
fn hardware_deltas_survive_a_mid_flick_down() {
  let mut r = Resampler::new();
  push_rel(&mut r, MOUSE, 10.0, 10.0);
  // A click mid-flick re-seeds the history; the accumulated motion
  // physically happened and must still dispatch.
  r.down(MOUSE, 12.0, 0.0, Modifiers::default());
  push_rel(&mut r, MOUSE, 15.0, 3.0);
  assert_eq!(xdx(&mut r), vec![(15.0, 13.0)]);
}

#[test]
fn hardware_deltas_report_motion_while_position_freezes() {
  // Relative mouse mode: SDL freezes x/y and reports motion only in rel.
  let mut r = Resampler::new();
  push_rel(&mut r, MOUSE, 50.0, 8.0);
  assert_eq!(xdx(&mut r), vec![(50.0, 8.0)]);
  push_rel(&mut r, MOUSE, 50.0, 12.0);
  assert_eq!(xdx(&mut r), vec![(50.0, 12.0)]);
}

#[test]
fn derived_movement_diffs_dispatched_positions() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  // First move diffs against the down's contact seed.
  push(&mut r, KEY, 10.0);
  assert_eq!(xdx(&mut r), vec![(10.0, 10.0)]);
  push(&mut r, KEY, 25.0);
  assert_eq!(xdx(&mut r), vec![(25.0, 15.0)]);
}

#[test]
fn derived_movement_mirrors_extrapolation_bounce() {
  let mut r = Resampler::new();
  r.down(KEY, 0.0, 0.0, Modifiers::default());
  push(&mut r, KEY, 10.0);
  assert_eq!(xdx(&mut r), vec![(10.0, 10.0)]);
  push(&mut r, KEY, 20.0);
  assert_eq!(xdx(&mut r), vec![(20.0, 10.0)]);
  // Bridged step and settle-back: movement bounces exactly with position.
  assert_eq!(xdx(&mut r), vec![(30.0, 10.0)]);
  assert_eq!(xdx(&mut r), vec![(20.0, -10.0)]);
}

#[test]
fn derived_movement_without_down_starts_at_zero() {
  let mut r = Resampler::new();
  // No down, no baseline: the first dispatch cannot claim movement.
  push(&mut r, KEY, 10.0);
  assert_eq!(xdx(&mut r), vec![(10.0, 0.0)]);
  push(&mut r, KEY, 16.0);
  assert_eq!(xdx(&mut r), vec![(16.0, 6.0)]);
}
