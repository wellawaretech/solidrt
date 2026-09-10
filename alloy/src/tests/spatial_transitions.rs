use crate::motion::{Curve, TransitionSpec};
use crate::spatial::{
  Component, Mat4, MotionState, NodeEndpoint, NodeMotion, NodeTransitionConfig, NodeTransitionEntry, QueryFilter,
  SinkWriter, Spatial,
};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];
const LINEAR_100: TransitionSpec = TransitionSpec::Tween { duration_ms: 100.0, curve: Curve::Linear };

fn all(spec: TransitionSpec) -> Option<NodeTransitionConfig> {
  Some(NodeTransitionConfig { all: Some(spec.into()), ..Default::default() })
}

/// A position entry on `spec` with the given lifecycle endpoints, each on
/// the entry's own motion (the decoder's default when an endpoint names
/// no motion of its own).
fn position_entry(spec: TransitionSpec, from: Option<[f32; 3]>, exit: Option<[f32; 3]>) -> NodeTransitionEntry<3> {
  NodeTransitionEntry {
    motion: spec.into(),
    from: from.map(|value| NodeEndpoint { value, motion: spec.into() }),
    exit: exit.map(|value| NodeEndpoint { value, motion: spec.into() }),
  }
}

fn exit_to_x(spec: TransitionSpec, x: f32) -> Option<NodeTransitionConfig> {
  Some(NodeTransitionConfig { position: Some(position_entry(spec, None, Some([x, 0.0, 0.0]))), ..Default::default() })
}

/// Sinks are not under test here: a writer that lands everything, so a
/// flush computes `shown` and the index for the query tests.
struct Sink;

impl SinkWriter for Sink {
  fn write_params(&mut self, _: u64, _: u64, _: &Mat4, _: Option<&Mat4>) -> bool {
    true
  }
  fn write_count(&mut self, _: u64, _: u64, _: u32) -> bool {
    true
  }
  fn write_shared(&mut self, _: u64, _: &str, _: &[f32]) -> bool {
    true
  }
  fn write_instances(&mut self, _: u64, _: u32, _: u32, _: &[f32]) -> bool {
    true
  }
  fn write_texture(&mut self, _: u64, _: &[f32]) -> bool {
    true
  }
}

/// Run the clock to `ms` in 16 ms steps; returns whether anything still
/// runs at the end.
fn run_to(s: &mut Spatial, from_ms: f64, ms: f64) -> bool {
  let mut running = true;
  let mut t = from_ms;
  while t < ms {
    t = (t + 16.0).min(ms);
    s.set_transition_now(t);
    running = s.advance_transitions();
  }
  running
}

/// Quaternion for a rotation of `rad` about z (xyzw).
fn qz(rad: f32) -> [f32; 4] {
  [0.0, 0.0, (rad / 2.0).sin(), (rad / 2.0).cos()]
}

/// The node's world rotation about z, from the matrix: atan2(m[1], m[0]).
fn angle_z(s: &Spatial, id: u64) -> f32 {
  let m = s.world(id).expect("world");
  m[1].atan2(m[0])
}

fn pos_x(s: &Spatial, id: u64) -> f32 {
  s.world(id).expect("world")[12]
}

#[test]
fn spring_position_settles_exactly_and_reports() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  assert!(s.write_transform(id, [100.0, 20.0, 0.0], Q, ONE).expect("write"));

  s.set_transition_now(16.0);
  assert!(s.advance_transitions());
  let mid = pos_x(&s, id);
  assert!(mid > 0.0 && mid < 100.0, "mid-flight, got {mid}");

  let mut running = true;
  for k in 2..200 {
    s.set_transition_now(k as f64 * 16.0);
    running = s.advance_transitions();
    if !running {
      break;
    }
  }
  assert!(!running, "spring never settled");
  let m = s.world(id).expect("world");
  assert_eq!((m[12], m[13]), (100.0, 20.0), "settle is exact");
  assert_eq!(s.take_settled_transitions(), vec![(id, Component::Position)]);
  assert!(s.take_settled_transitions().is_empty(), "drain empties");
}

#[test]
fn full_write_leaves_unchanged_components_alone() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(LINEAR_100)).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [10.0, 0.0, 0.0], Q, ONE).expect("write");

  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 5.0).abs() < 1e-4, "tween midway");
  // The full-TRS re-send: position target unchanged, scale is new. The
  // position tween must NOT restart from 5 - it settles at t=100.
  s.write_transform(id, [10.0, 0.0, 0.0], Q, [2.0, 2.0, 2.0]).expect("write");

  s.set_transition_now(100.0);
  s.advance_transitions();
  let m = s.world(id).expect("world");
  assert_eq!(m[12], 10.0, "position settled on schedule");
  assert!((m[0] - 1.5).abs() < 1e-4, "scale midway (started at 50)");
  assert_eq!(s.take_settled_transitions(), vec![(id, Component::Position)]);

  s.set_transition_now(150.0);
  assert!(!s.advance_transitions());
  assert_eq!(s.world(id).expect("world")[0], 2.0, "scale settled exactly");
  assert_eq!(s.take_settled_transitions(), vec![(id, Component::Scale)]);
}

#[test]
fn undeclared_components_snap() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  let config = NodeTransitionConfig { position: Some(LINEAR_100.into()), ..Default::default() };
  s.set_node_transition(id, Some(config)).expect("config");
  s.set_transition_now(0.0);
  let quarter = qz(std::f32::consts::FRAC_PI_2);
  assert!(s.write_transform(id, [10.0, 0.0, 0.0], quarter, ONE).expect("write"));
  // Rotation snapped with the write; position has not moved yet.
  let m = s.world(id).expect("world");
  assert!(m[1] > 0.999, "rotation snapped");
  assert_eq!(m[12], 0.0, "position waits for the advance");
}

#[test]
fn rotation_tween_follows_geodesic() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(LINEAR_100)).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [0.0; 3], qz(std::f32::consts::FRAC_PI_2), ONE).expect("write");

  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!((angle_z(&s, id) - std::f32::consts::FRAC_PI_4).abs() < 1e-3, "slerp midpoint is half the angle");

  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  let m = s.world(id).expect("world");
  assert!(m[0].abs() < 1e-6 && (m[1] - 1.0).abs() < 1e-6, "lands the target exactly");
  assert_eq!(s.take_settled_transitions(), vec![(id, Component::Rotation)]);
}

#[test]
fn rotation_spring_retarget_keeps_momentum() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [0.0; 3], qz(std::f32::consts::FRAC_PI_2), ONE).expect("write");
  for k in 1..=5 {
    s.set_transition_now(k as f64 * 16.0);
    s.advance_transitions();
  }
  let before = angle_z(&s, id);
  assert!(before > 0.1 && before < std::f32::consts::FRAC_PI_2, "mid-flight");
  // Retarget back to identity: the angular velocity carries the node PAST
  // the retarget point before the spring pulls it back.
  s.write_transform(id, [0.0; 3], Q, ONE).expect("write");
  s.set_transition_now(6.0 * 16.0);
  s.advance_transitions();
  assert!(angle_z(&s, id) > before, "momentum survives the retarget");
  // And it still settles on the new target.
  let mut running = true;
  for k in 7..300 {
    s.set_transition_now(k as f64 * 16.0);
    running = s.advance_transitions();
    if !running {
      break;
    }
  }
  assert!(!running, "never settled after retarget");
  assert!(angle_z(&s, id).abs() < 1e-6, "settled on the retargeted value");
}

#[test]
fn near_antipodal_target_takes_the_short_arc() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(LINEAR_100)).expect("config");
  s.set_transition_now(0.0);
  // 181 degrees about z: the quaternion's w is negative, so the short arc
  // from identity runs BACKWARD (through -179), not forward through +90.
  let rad = 181.0f32.to_radians();
  s.write_transform(id, [0.0; 3], qz(rad), ONE).expect("write");

  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!(angle_z(&s, id) < 0.0, "midpoint on the short (negative) arc");

  s.set_transition_now(100.0);
  s.advance_transitions();
  let m = s.world(id).expect("world");
  assert!((m[0] - rad.cos()).abs() < 1e-5 && (m[1] - rad.sin()).abs() < 1e-5, "same rotation as written");
}

#[test]
fn config_clear_cancels_in_place() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(32.0);
  s.advance_transitions();
  let mid = pos_x(&s, id);
  assert!(mid > 0.0 && mid < 100.0);

  s.set_node_transition(id, None).expect("clear");
  s.set_transition_now(200.0);
  assert!(!s.advance_transitions());
  assert_eq!(pos_x(&s, id), mid, "keeps the mid-flight value");
  assert!(s.take_settled_transitions().is_empty(), "cancel is not a settle");

  // Later writes snap.
  assert!(s.write_transform(id, [7.0, 0.0, 0.0], Q, ONE).expect("write"));
  assert_eq!(pos_x(&s, id), 7.0);
}

#[test]
fn destroy_drops_tracks() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write");
  s.destroy(id).expect("destroy");
  s.set_transition_now(16.0);
  assert!(!s.advance_transitions());
  assert!(s.take_settled_transitions().is_empty());
}

#[test]
fn write_without_config_snaps() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  assert!(s.write_transform(id, [5.0, 0.0, 0.0], Q, ONE).expect("write"));
  assert_eq!(pos_x(&s, id), 5.0);
  assert!(!s.write_transform(id, [5.0, 0.0, 0.0], Q, ONE).expect("write"), "unchanged write reports false");
}

#[test]
fn noop_write_starts_no_track() {
  let mut s = Spatial::new();
  let id = s.create([3.0, 0.0, 0.0], Q, ONE, true);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  assert!(!s.write_transform(id, [3.0, 0.0, 0.0], Q, ONE).expect("write"));
  s.set_transition_now(16.0);
  assert!(!s.advance_transitions());
}

#[test]
fn paused_clock_holds_values() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(16.0);
  assert!(s.advance_transitions());
  let held = pos_x(&s, id);
  // Same stamp again (the paused path): still running, nothing moves.
  assert!(s.advance_transitions());
  assert_eq!(pos_x(&s, id), held);
}

#[test]
fn hidden_nodes_still_animate() {
  let mut s = Spatial::new();
  let id = s.create([0.0; 3], Q, ONE, false);
  s.set_node_transition(id, all(TransitionSpec::spring(300.0, 0.0))).expect("config");
  s.set_transition_now(0.0);
  s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(16.0);
  assert!(s.advance_transitions());
  assert!(pos_x(&s, id) > 0.0, "visibility gates sinks, not motion");
}

// Enter animations: a component's `from` in the declaration is where a
// node starts, played at the first advance after `create` toward the
// transform it holds then. See spatial/mod.rs start_enter_transitions.

fn enter_from_x(x: f32) -> Option<NodeTransitionConfig> {
  Some(NodeTransitionConfig {
    position: Some(position_entry(LINEAR_100, Some([x, 0.0, 0.0]), None)),
    ..Default::default()
  })
}

#[test]
fn enter_from_plays_at_first_advance() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([100.0, 0.0, 0.0], Q, ONE, true);
  s.set_node_transition(id, enter_from_x(0.0)).expect("config");
  assert_eq!(pos_x(&s, id), 100.0, "creation holds the created pose");
  assert!(s.advance_transitions(), "the enter starts at the advance");
  assert_eq!(pos_x(&s, id), 0.0, "the first advance snaps to from");
  s.set_transition_now(50.0);
  assert!(s.advance_transitions());
  assert!((pos_x(&s, id) - 50.0).abs() < 0.01, "halfway to the created pose, got {}", pos_x(&s, id));
  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  assert_eq!(pos_x(&s, id), 100.0, "settles on the created pose");
  assert_eq!(s.take_settled_transitions(), vec![(id, Component::Position)]);
}

#[test]
fn enter_from_rotation_slerps_to_created_pose() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], qz(1.0), ONE, true);
  let config = NodeTransitionConfig {
    rotation: Some(NodeTransitionEntry {
      motion: LINEAR_100.into(),
      from: Some(NodeEndpoint { value: Q, motion: LINEAR_100.into() }),
      exit: None,
    }),
    ..Default::default()
  };
  s.set_node_transition(id, Some(config)).expect("config");
  assert!(s.advance_transitions());
  assert!(angle_z(&s, id).abs() < 1e-5, "snapped to the from rotation");
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!((angle_z(&s, id) - 0.5).abs() < 1e-3, "halfway along the arc, got {}", angle_z(&s, id));
  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  assert!((angle_z(&s, id) - 1.0).abs() < 1e-5);
}

#[test]
fn enter_from_targets_a_write_made_in_the_creating_tick() {
  // The declaration and the pose may land in any order before the advance:
  // a write that already started a track becomes the enter's target.
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, enter_from_x(-100.0)).expect("config");
  assert!(s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write"));
  assert!(s.advance_transitions());
  assert_eq!(pos_x(&s, id), -100.0, "from wins over the written target for the first frame");
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 0.0).abs() < 0.01, "halfway from -100 to the written 100, got {}", pos_x(&s, id));
  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  assert_eq!(pos_x(&s, id), 100.0);
}

#[test]
fn enter_runs_once_and_skips_nodes_without_from_or_gone() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let plain = s.create([5.0, 0.0, 0.0], Q, ONE, true);
  s.set_node_transition(plain, all(LINEAR_100)).expect("config");
  let same = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(same, enter_from_x(0.0)).expect("config");
  let gone = s.create([9.0, 0.0, 0.0], Q, ONE, true);
  s.set_node_transition(gone, enter_from_x(0.0)).expect("config");
  s.destroy(gone).expect("destroy");
  assert!(!s.advance_transitions(), "no from, from == pose, or freed: nothing to animate");
  assert_eq!(pos_x(&s, plain), 5.0);
  // A later declaration with from does not replay: creation is the one
  // way onto the enter queue.
  s.set_node_transition(plain, enter_from_x(0.0)).expect("config");
  s.set_transition_now(16.0);
  assert!(!s.advance_transitions());
  assert_eq!(pos_x(&s, plain), 5.0);
}

// Exit animations and the leaving state (mod.rs `exit`): a node let go of
// animates each declared component to its exit value and is a ghost
// meanwhile - painted, skipped by every query - until its exit tracks
// settle and its leaving children are gone, when it frees and reports
// through take_freed. See okf/done/spatial-node-exit-transitions.md.

#[test]
fn exit_animates_out_and_frees_at_settle() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  s.set_node_transition(id, exit_to_x(LINEAR_100, 0.0)).expect("config");
  assert!(s.exit(id).expect("exit"), "an exit with somewhere to go keeps the node");
  assert!(s.leaving(id).expect("leaving"));
  s.set_transition_now(50.0);
  assert!(s.advance_transitions());
  assert!((pos_x(&s, id) - 5.0).abs() < 1e-4, "halfway out, got {}", pos_x(&s, id));
  assert!(s.take_freed().is_empty(), "not freed mid-flight");
  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  assert!(s.world(id).is_err(), "freed at the settle");
  assert_eq!(s.take_freed(), vec![id]);
  assert!(s.take_freed().is_empty(), "drain empties");
  assert!(s.take_settled_transitions().is_empty(), "an exit settle is not an event");
}

#[test]
fn exit_with_nothing_to_animate_frees_now() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let plain = s.create([0.0; 3], Q, ONE, true);
  assert!(!s.exit(plain).expect("exit"), "no declaration: freed at once");
  assert!(s.world(plain).is_err());
  let motion_only = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(motion_only, all(LINEAR_100)).expect("config");
  assert!(!s.exit(motion_only).expect("exit"), "no exit endpoint: freed at once");
  let held = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(held, exit_to_x(LINEAR_100, 0.0)).expect("config");
  assert!(!s.exit(held).expect("exit"), "exit value already holds: freed at once");
  assert!(s.world(held).is_err());
  assert!(s.take_freed().is_empty(), "a synchronous free is the caller's to see");
  assert!(!s.advance_transitions());
}

#[test]
fn exit_picks_up_from_mid_flight_with_momentum() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  let spring = TransitionSpec::spring(300.0, 0.0);
  s.set_node_transition(id, exit_to_x(spring, 0.0)).expect("config");
  s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write");
  run_to(&mut s, 0.0, 80.0);
  let before = pos_x(&s, id);
  assert!(before > 0.0 && before < 100.0, "mid-flight");
  assert!(s.exit(id).expect("exit"));
  s.set_transition_now(96.0);
  s.advance_transitions();
  assert!(pos_x(&s, id) > before, "the spring keeps its velocity through the exit retarget");
  assert!(!run_to(&mut s, 96.0, 3000.0), "settles on the exit value");
  assert_eq!(s.take_freed(), vec![id]);
}

#[test]
fn leaving_node_is_a_ghost_to_queries() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0, 0.0, -2.0], Q, ONE, true);
  s.set_bounds(id, Some([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5])).expect("bounds");
  s.set_node_transition(id, exit_to_x(LINEAR_100, 5.0)).expect("config");
  s.flush(&mut Sink);
  let ray = |s: &mut Spatial| s.raycast([0.0; 3], [0.0, 0.0, -1.0], &QueryFilter::default()).expect("raycast").len();
  assert_eq!(ray(&mut s), 1, "live: struck");
  assert!(s.exit(id).expect("exit"));
  s.flush(&mut Sink);
  assert_eq!(ray(&mut s), 0, "leaving: skipped, though still shown and flushed");
  assert!(s.shown(id).expect("shown"), "a ghost is still painted");
}

#[test]
fn parent_frees_after_its_leaving_children() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let parent = s.create([0.0; 3], Q, ONE, true);
  let child = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  s.set_parent(child, Some(parent)).expect("parent");
  s.set_node_transition(child, exit_to_x(LINEAR_100, 0.0)).expect("config");
  // Children first, as both trees tear down: the child leaves, then the
  // parent (no exit of its own) waits for it instead of freeing.
  assert!(s.exit(child).expect("exit child"));
  assert!(s.exit(parent).expect("exit parent"), "a parent with a leaving child waits");
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!(s.world(parent).is_ok() && s.world(child).is_ok());
  // The child's world position composes through the parent to the end.
  s.set_transform(parent, [100.0, 0.0, 0.0], Q, ONE).expect("move parent");
  assert!((pos_x(&s, child) - 105.0).abs() < 1e-4, "the corpse stays in its parent's frame");
  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  assert_eq!(s.take_freed(), vec![child, parent], "child settles, then the parent's gate empties");
}

#[test]
fn parent_with_own_exit_waits_for_both() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let parent = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  let child = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  s.set_parent(child, Some(parent)).expect("parent");
  let slow = TransitionSpec::Tween { duration_ms: 200.0, curve: Curve::Linear };
  s.set_node_transition(child, exit_to_x(slow, 0.0)).expect("config");
  s.set_node_transition(parent, exit_to_x(LINEAR_100, 0.0)).expect("config");
  assert!(s.exit(child).expect("exit"));
  assert!(s.exit(parent).expect("exit"));
  s.set_transition_now(100.0);
  s.advance_transitions();
  assert!(s.take_freed().is_empty(), "the parent's own exit settled; its child still runs");
  s.set_transition_now(200.0);
  assert!(!s.advance_transitions());
  assert_eq!(s.take_freed(), vec![child, parent]);
}

#[test]
fn destroy_frees_a_leaving_node_and_its_corpses_now() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let parent = s.create([0.0; 3], Q, ONE, true);
  let child = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  let live = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  s.set_parent(child, Some(parent)).expect("parent");
  s.set_parent(live, Some(parent)).expect("parent");
  s.set_node_transition(child, exit_to_x(LINEAR_100, 0.0)).expect("config");
  s.set_node_transition(parent, exit_to_x(LINEAR_100, 5.0)).expect("config");
  assert!(s.exit(child).expect("exit"));
  assert!(s.exit(parent).expect("exit"));
  s.destroy(parent).expect("destroy");
  assert!(s.world(parent).is_err() && s.world(child).is_err(), "the corpse and its corpses go together");
  assert!(s.world(live).is_ok(), "a live child becomes a root, as ever");
  let mut freed = s.take_freed();
  freed.sort_unstable();
  let mut expected = vec![parent, child];
  expected.sort_unstable();
  assert_eq!(freed, expected, "every free of a leaving node reports, whoever caused it");
  assert!(!s.advance_transitions(), "their tracks went with them");
}

#[test]
fn destroying_the_last_corpse_frees_its_waiting_parent() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let parent = s.create([0.0; 3], Q, ONE, true);
  let child = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  s.set_parent(child, Some(parent)).expect("parent");
  s.set_node_transition(child, exit_to_x(LINEAR_100, 0.0)).expect("config");
  assert!(s.exit(child).expect("exit"));
  assert!(s.exit(parent).expect("exit"));
  s.destroy(child).expect("destroy");
  assert!(s.world(parent).is_err(), "nothing left to wait for");
  assert_eq!(s.take_freed(), vec![child, parent]);
}

#[test]
fn leaving_nodes_are_refused_as_parent_and_child() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let ghost = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  let live = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(ghost, exit_to_x(LINEAR_100, 0.0)).expect("config");
  assert!(s.exit(ghost).expect("exit"));
  assert!(s.set_parent(live, Some(ghost)).is_err(), "nothing hangs under a corpse");
  assert!(s.set_parent(ghost, Some(live)).is_err(), "a corpse stays in its frame");
  assert!(s.exit(ghost).expect("second exit"), "a second exit changes nothing");
}

#[test]
fn exit_keeps_undeclared_components_running_without_gating_on_them() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  let config = NodeTransitionConfig {
    position: Some(TransitionSpec::spring(2000.0, 0.0).into()),
    scale: Some(NodeTransitionEntry {
      motion: LINEAR_100.into(),
      from: None,
      exit: Some(NodeEndpoint { value: [0.0; 3], motion: LINEAR_100.into() }),
    }),
    ..Default::default()
  };
  s.set_node_transition(id, Some(config)).expect("config");
  s.write_transform(id, [100.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(16.0);
  s.advance_transitions();
  let before = pos_x(&s, id);
  assert!(s.exit(id).expect("exit"));
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!(pos_x(&s, id) > before, "the slow position spring keeps running under the exit");
  // The scale exit started at 16 and settles at 116, the spring runs on
  // for seconds: the free rides on the exit alone.
  s.set_transition_now(116.0);
  assert!(!s.advance_transitions(), "freed at the scale exit's settle, the spring notwithstanding");
  assert_eq!(s.take_freed(), vec![id]);
  assert!(s.take_settled_transitions().is_empty());
}

#[test]
fn clearing_the_declaration_of_a_leaving_node_frees_it() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  s.set_node_transition(id, exit_to_x(LINEAR_100, 0.0)).expect("config");
  assert!(s.exit(id).expect("exit"));
  s.set_node_transition(id, None).expect("clear");
  s.set_transition_now(16.0);
  assert!(!s.advance_transitions());
  assert_eq!(s.take_freed(), vec![id], "no exit set left to gate on");
}

#[test]
fn exit_in_the_creating_tick_skips_the_enter() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([100.0, 0.0, 0.0], Q, ONE, true);
  let config = NodeTransitionConfig {
    position: Some(position_entry(LINEAR_100, Some([0.0; 3]), Some([200.0, 0.0, 0.0]))),
    ..Default::default()
  };
  s.set_node_transition(id, Some(config)).expect("config");
  assert!(s.exit(id).expect("exit"));
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!(
    (pos_x(&s, id) - 150.0).abs() < 1e-3,
    "leaves from the created pose, never from `from`; got {}",
    pos_x(&s, id)
  );
}

// Delay: an entry's or an endpoint's `delay_ms` holds the write on the
// animation clock, and a held write starting late runs as if started on
// time (the element tracks' catch-up rule).

fn delayed_position(delay_ms: f32) -> Option<NodeTransitionConfig> {
  let motion = NodeMotion { spec: LINEAR_100, delay_ms };
  Some(NodeTransitionConfig {
    position: Some(NodeTransitionEntry { motion, from: None, exit: None }),
    ..Default::default()
  })
}

#[test]
fn delayed_write_holds_then_runs_on_schedule() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, delayed_position(100.0)).expect("config");
  assert!(s.write_transform(id, [10.0, 0.0, 0.0], Q, ONE).expect("write"), "a held write is a change");
  s.set_transition_now(50.0);
  assert!(s.advance_transitions(), "a hold keeps the advance live");
  assert_eq!(pos_x(&s, id), 0.0, "nothing moves during the hold");
  // The full-TRS re-send of the held target keeps the hold: the delay does
  // not restart from 50.
  s.write_transform(id, [10.0, 0.0, 0.0], Q, ONE).expect("write");
  assert_eq!(
    s.motion_of(id).expect("motion"),
    vec![MotionState { component: Component::Position, to: [10.0, 0.0, 0.0, 0.0], held_until_ms: Some(100.0) }]
  );
  s.set_transition_now(150.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 5.0).abs() < 1e-4, "halfway, 50 ms into the 100 ms tween");
  s.set_transition_now(200.0);
  assert!(!s.advance_transitions());
  assert_eq!(pos_x(&s, id), 10.0);
  assert_eq!(s.take_settled_transitions(), vec![(id, Component::Position)]);
}

#[test]
fn late_frame_after_a_hold_catches_up() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, delayed_position(100.0)).expect("config");
  s.write_transform(id, [10.0, 0.0, 0.0], Q, ONE).expect("write");
  // The first advance after the slot lands 75 ms late: the tween is
  // already three quarters through, not starting.
  s.set_transition_now(175.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 7.5).abs() < 1e-4, "got {}", pos_x(&s, id));
}

#[test]
fn newer_write_restarts_the_hold_and_an_immediate_one_supersedes_it() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  s.set_node_transition(id, delayed_position(100.0)).expect("config");
  s.write_transform(id, [10.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(60.0);
  s.write_transform(id, [20.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(120.0);
  s.advance_transitions();
  assert_eq!(pos_x(&s, id), 0.0, "the newer write restarted the delay (due at 160)");
  // A declaration without delay takes over: its write applies now.
  s.set_node_transition(id, all(LINEAR_100)).expect("config");
  s.write_transform(id, [30.0, 0.0, 0.0], Q, ONE).expect("write");
  s.set_transition_now(170.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 15.0).abs() < 1e-4, "the held write is gone; the immediate one runs from 120");
}

#[test]
fn enter_delay_holds_at_from() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([100.0, 0.0, 0.0], Q, ONE, true);
  let config = NodeTransitionConfig {
    position: Some(NodeTransitionEntry {
      motion: LINEAR_100.into(),
      from: Some(NodeEndpoint { value: [0.0; 3], motion: NodeMotion { spec: LINEAR_100, delay_ms: 100.0 } }),
      exit: None,
    }),
    ..Default::default()
  };
  s.set_node_transition(id, Some(config)).expect("config");
  assert!(s.advance_transitions());
  assert_eq!(pos_x(&s, id), 0.0, "snapped to from at the first advance");
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert_eq!(pos_x(&s, id), 0.0, "held there");
  s.set_transition_now(150.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 50.0).abs() < 1e-3, "halfway to the created pose, got {}", pos_x(&s, id));
}

#[test]
fn exit_delay_holds_then_leaves() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  let config = NodeTransitionConfig {
    position: Some(NodeTransitionEntry {
      motion: LINEAR_100.into(),
      from: None,
      exit: Some(NodeEndpoint { value: [0.0; 3], motion: NodeMotion { spec: LINEAR_100, delay_ms: 100.0 } }),
    }),
    ..Default::default()
  };
  s.set_node_transition(id, Some(config)).expect("config");
  assert!(s.exit(id).expect("exit"), "a held exit keeps the node");
  s.set_transition_now(50.0);
  assert!(s.advance_transitions());
  assert_eq!(pos_x(&s, id), 10.0, "holds its pose during the delay");
  assert!(s.leaving(id).expect("leaving"));
  s.set_transition_now(150.0);
  s.advance_transitions();
  assert!((pos_x(&s, id) - 5.0).abs() < 1e-4);
  s.set_transition_now(200.0);
  assert!(!s.advance_transitions());
  assert_eq!(s.take_freed(), vec![id]);
}

#[test]
fn held_exit_to_the_current_value_frees_when_due() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let id = s.create([0.0; 3], Q, ONE, true);
  let config = NodeTransitionConfig {
    position: Some(NodeTransitionEntry {
      motion: LINEAR_100.into(),
      from: None,
      exit: Some(NodeEndpoint { value: [0.0; 3], motion: NodeMotion { spec: LINEAR_100, delay_ms: 100.0 } }),
    }),
    ..Default::default()
  };
  s.set_node_transition(id, Some(config)).expect("config");
  assert!(s.exit(id).expect("exit"), "the hold itself keeps the node");
  s.set_transition_now(50.0);
  s.advance_transitions();
  assert!(s.take_freed().is_empty());
  s.set_transition_now(100.0);
  assert!(!s.advance_transitions());
  assert_eq!(s.take_freed(), vec![id], "due, nothing to move: freed");
}
