use crate::motion::{Curve, TransitionSpec};
use crate::spatial::{Component, NodeTransitionConfig, Spatial};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];
const LINEAR_100: TransitionSpec = TransitionSpec::Tween { duration_ms: 100.0, curve: Curve::Linear };

fn all(spec: TransitionSpec) -> Option<NodeTransitionConfig> {
  Some(NodeTransitionConfig { all: Some(spec), ..Default::default() })
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
  let config = NodeTransitionConfig { position: Some(LINEAR_100), ..Default::default() };
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
