use crate::spatial::{
  transform_point, transform_vector, ChainMethod, ChainSpec, FeetSpec, IkLimit, IkPoint, IkSolverSpec, IkTarget,
  IkTick, LegSpec, LookAtBone, LookAtLimit, LookAtSpec, Mat4f, QueryFilter, SinkWriter, Spatial, SpringJoint,
  TwoBoneSpec,
};

const Q: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f64; 3] = [1.0, 1.0, 1.0];
/// The arm: two bones of this length, straight up at rest.
const BONE: f64 = 0.3;
/// A frame at 60 Hz, ms.
const FRAME: f64 = 1000.0 / 60.0;

struct Sink;

impl SinkWriter for Sink {
  fn write_emitter(
    &mut self,
    _voice: u64,
    _output: &crate::spatial::EmitterOutput,
    _ramp: crate::spatial::EmitterRamp,
  ) -> bool {
    true
  }
  fn write_params(&mut self, _: u64, _: u64, _: &Mat4f, _: Option<&Mat4f>) -> bool {
    true
  }
  fn write_count(&mut self, _: u64, _: u64, _: u32) -> bool {
    true
  }
  fn write_fade(&mut self, _: u64, _: u64, _: [f32; 2]) -> bool {
    true
  }
  fn write_order(&mut self, _: u64, _: &[u64]) -> bool {
    true
  }
  fn shift_params(&mut self, _: u64, _: &[u64], _: [f32; 3]) -> bool {
    true
  }
  fn write_order_direction(&mut self, _: u64, _: u64, _: [f32; 3]) -> bool {
    true
  }
  fn write_shared(&mut self, _: u64, _: &str, _: &[f32]) -> bool {
    true
  }
  fn write_param(&mut self, _: u64, _: Option<u64>, _: &str, _: f32) -> bool {
    true
  }

  fn write_instances(&mut self, _: u64, _: u32, _: u32, _: &[f32]) -> bool {
    true
  }
  fn write_texture(&mut self, _: u64, _: &[f32]) -> bool {
    true
  }
}

fn node(s: &mut Spatial, parent: Option<u64>, position: [f64; 3]) -> u64 {
  let n = s.create(position, Q, ONE, true);
  if let Some(p) = parent {
    s.set_parent(n, Some(p)).expect("parent");
  }
  n
}

fn world_pos(s: &Spatial, n: u64) -> [f64; 3] {
  let m = s.world(n).expect("world");
  [m[12], m[13], m[14]]
}

fn forward_of(s: &Spatial, n: u64) -> [f64; 3] {
  norm(transform_vector(&s.world(n).expect("world"), [0.0, 0.0, 1.0]))
}

fn rotation_of(s: &Spatial, n: u64) -> [f64; 4] {
  s.transform_of(n).expect("read").1
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
  ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn norm(v: [f64; 3]) -> [f64; 3] {
  let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
  [v[0] / l, v[1] / l, v[2] / l]
}

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
  (0..3).all(|i| (a[i] - b[i]).abs() <= tol)
}

fn point(p: [f64; 3]) -> IkTarget {
  IkTarget::Point { position: p, rotation: None, space: None }
}

fn advance_at(s: &mut Spatial, ms: f64) -> IkTick {
  s.set_transition_now(ms);
  s.advance_ik()
}

/// The arm: an anchor, a shoulder on it, an elbow and a wrist BONE apart
/// straight up. Returns (anchor, shoulder, elbow, wrist).
fn arm(s: &mut Spatial) -> (u64, u64, u64, u64) {
  let anchor = node(s, None, [0.0; 3]);
  let shoulder = node(s, Some(anchor), [0.0; 3]);
  let elbow = node(s, Some(shoulder), [0.0, BONE, 0.0]);
  let wrist = node(s, Some(elbow), [0.0, BONE, 0.0]);
  (anchor, shoulder, elbow, wrist)
}

fn two_bone(root: u64, mid: u64, tip: u64, target: [f64; 3], pole: Option<[f64; 3]>) -> TwoBoneSpec {
  TwoBoneSpec {
    root,
    mid,
    tip,
    target: Some(point(target)),
    pole: pole.map(|p| IkPoint::Point { position: p, space: None }),
    pole_angle: 0.0,
    position_weight: 1.0,
    rotation_weight: 0.0,
    soft: 0.0,
    stretch: 0.0,
  }
}

fn look_at(bones: Vec<(u64, f64)>, target: Option<[f64; 3]>, limit: Option<LookAtLimit>, ease_ms: f64) -> LookAtSpec {
  LookAtSpec {
    bones: bones.into_iter().map(|(node, share)| LookAtBone { node, share }).collect(),
    target: target.map(point),
    forward: [0.0, 0.0, 1.0],
    up_axis: [0.0, 1.0, 0.0],
    up: None,
    limit,
    ease_ms,
  }
}

/// The interior angle at `b` of triangle a-b-c.
fn interior_angle(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
  let ba = norm([a[0] - b[0], a[1] - b[1], a[2] - b[2]]);
  let bc = norm([c[0] - b[0], c[1] - b[1], c[2] - b[2]]);
  (ba[0] * bc[0] + ba[1] * bc[1] + ba[2] * bc[2]).clamp(-1.0, 1.0).acos()
}

#[test]
fn two_bone_reaches_a_target_in_range_with_the_law_of_cosines() {
  let mut s = Spatial::new();
  let (_, shoulder, elbow, wrist) = arm(&mut s);
  let target = [0.2, 0.4, 0.1];
  let ik = s
    .create_ik(
      shoulder,
      vec![(IkSolverSpec::TwoBone(two_bone(shoulder, elbow, wrist, target, Some([0.0, 0.3, 1.0]))), 1.0)],
    )
    .expect("ik");
  let tick = advance_at(&mut s, 0.0);
  assert!(tick.wrote);
  let (a, b, c) = (world_pos(&s, shoulder), world_pos(&s, elbow), world_pos(&s, wrist));
  assert!(close(c, target, 1e-4), "tip {c:?} vs {target:?}");
  assert!((dist(a, b) - BONE).abs() < 1e-5 && (dist(b, c) - BONE).abs() < 1e-5, "bone lengths hold");
  // The elbow angle is the analytic one: the brute check of the closed form.
  let d = dist(a, target);
  let want = ((2.0 * BONE * BONE - d * d) / (2.0 * BONE * BONE)).clamp(-1.0, 1.0).acos();
  assert!((interior_angle(a, b, c) - want).abs() < 1e-4, "elbow angle {} vs {want}", interior_angle(a, b, c));
  // The pole decides the bend side.
  assert!(b[2] > 0.0, "the elbow bends toward the +z pole: {b:?}");
  assert!(s.ik_reach(ik, 0).expect("reach") < 1e-4);
  s.set_ik_solver(ik, 0, IkSolverSpec::TwoBone(two_bone(shoulder, elbow, wrist, target, Some([0.0, 0.3, -1.0]))))
    .expect("set");
  advance_at(&mut s, FRAME);
  let b = world_pos(&s, elbow);
  assert!(b[2] < 0.0, "the elbow bends toward the -z pole: {b:?}");
  assert!(close(world_pos(&s, wrist), target, 1e-4));
  // A kind change is refused.
  assert!(s.set_ik_solver(ik, 0, IkSolverSpec::LookAt(look_at(vec![(elbow, 1.0)], None, None, 0.0))).is_err());
}

#[test]
fn two_bone_soft_band_eases_the_reach_and_stretch_lengthens_the_limb() {
  let mut s = Spatial::new();
  let (_, shoulder, elbow, wrist) = arm(&mut s);
  // Within the soft band (the last 20 % of the limb), the tip stops short
  // of a target a hard solve reaches.
  let mut spec = two_bone(shoulder, elbow, wrist, [0.0, 0.55, 0.0], Some([0.0, 0.3, 1.0]));
  spec.soft = 0.2;
  let ik = s.create_ik(shoulder, vec![(IkSolverSpec::TwoBone(spec.clone()), 1.0)]).expect("ik");
  advance_at(&mut s, 0.0);
  let c = world_pos(&s, wrist);
  let band = 0.2 * 2.0 * BONE;
  let want = 2.0 * BONE - band * (-(0.55 - (2.0 * BONE - band)) / band).exp();
  assert!((c[1] - want).abs() < 1e-4, "soft reach {} vs {want}", c[1]);
  assert!(s.ik_reach(ik, 0).expect("reach") > 0.01);
  // Stretch: a target past full reach lengthens the bones.
  spec.soft = 0.0;
  spec.stretch = 0.5;
  spec.target = Some(point([0.0, 0.8, 0.0]));
  s.set_ik_solver(ik, 0, IkSolverSpec::TwoBone(spec)).expect("set");
  advance_at(&mut s, FRAME);
  let c = world_pos(&s, wrist);
  assert!((c[1] - 0.8).abs() < 1e-4, "stretched tip {c:?}");
  let elbow_local = s.transform_of(elbow).expect("read").0;
  assert!((elbow_local[1] - 0.4).abs() < 1e-5, "the elbow offset scaled 4/3: {elbow_local:?}");
}

/// A chain of `n` bones of 0.2 along +y under a root at the origin.
fn chain(s: &mut Spatial, n: usize) -> Vec<u64> {
  let mut nodes = vec![node(s, None, [0.0; 3])];
  for _ in 0..n {
    let parent = *nodes.last().expect("root");
    nodes.push(node(s, Some(parent), [0.0, 0.2, 0.0]));
  }
  nodes
}

fn chain_spec(nodes: &[u64], method: ChainMethod, target: [f64; 3], limits: Vec<(u64, IkLimit)>) -> ChainSpec {
  ChainSpec {
    root: nodes[0],
    tip: *nodes.last().expect("tip"),
    target: Some(point(target)),
    method,
    iterations: 32,
    tolerance: 1e-4,
    magnet: None,
    limits,
    rotation_weight: 0.0,
  }
}

#[test]
fn fabrik_and_ccd_converge_and_keep_the_bone_lengths() {
  for method in [ChainMethod::Fabrik, ChainMethod::Ccd] {
    let mut s = Spatial::new();
    let nodes = chain(&mut s, 5);
    let target = [0.3, 0.5, 0.1];
    let ik =
      s.create_ik(nodes[0], vec![(IkSolverSpec::Chain(chain_spec(&nodes, method, target, vec![])), 1.0)]).expect("ik");
    advance_at(&mut s, 0.0);
    let tip = world_pos(&s, nodes[5]);
    assert!(close(tip, target, 1e-3), "{method:?}: tip {tip:?} vs {target:?}");
    assert!(s.ik_reach(ik, 0).expect("reach") < 1e-3);
    for k in 0..5 {
      let l = dist(world_pos(&s, nodes[k]), world_pos(&s, nodes[k + 1]));
      assert!((l - 0.2).abs() < 1e-5, "{method:?}: bone {k} length {l}");
    }
    // Out of reach: straight toward the target, residual = the gap.
    s.set_ik_target(ik, 0, Some(point([0.0, 2.0, 0.0]))).expect("target");
    advance_at(&mut s, FRAME);
    let tip = world_pos(&s, nodes[5]);
    assert!(close(tip, [0.0, 1.0, 0.0], 1e-3), "{method:?}: straight tip {tip:?}");
    assert!((s.ik_reach(ik, 0).expect("reach") - 1.0).abs() < 1e-3);
  }
}

#[test]
fn fabrik_magnet_picks_the_bend_side() {
  let mut s = Spatial::new();
  let nodes = chain(&mut s, 4);
  let mut spec = chain_spec(&nodes, ChainMethod::Fabrik, [0.0, 0.5, 0.0], vec![]);
  spec.magnet = Some(IkPoint::Point { position: [1.0, 0.25, 0.0], space: None });
  s.create_ik(nodes[0], vec![(IkSolverSpec::Chain(spec), 1.0)]).expect("ik");
  advance_at(&mut s, 0.0);
  assert!(close(world_pos(&s, nodes[4]), [0.0, 0.5, 0.0], 1e-3));
  let mid = world_pos(&s, nodes[2]);
  assert!(mid[0] > 0.05, "the interior joints bow toward the magnet: {mid:?}");
}

#[test]
fn chain_limits_are_honoured_by_both_methods() {
  for method in [ChainMethod::Fabrik, ChainMethod::Ccd] {
    let mut s = Spatial::new();
    let nodes = chain(&mut s, 3);
    // Every joint a hinge about x within +-0.3: the target off to the
    // side wants far more.
    let limits: Vec<(u64, IkLimit)> =
      nodes[..3].iter().map(|&n| (n, IkLimit::Hinge { axis: [1.0, 0.0, 0.0], min: -0.3, max: 0.3 })).collect();
    let ik = s
      .create_ik(nodes[0], vec![(IkSolverSpec::Chain(chain_spec(&nodes, method, [0.0, 0.0, 0.6], limits)), 1.0)])
      .expect("ik");
    advance_at(&mut s, 0.0);
    for &n in &nodes[..3] {
      let q = rotation_of(&s, n);
      assert!(q[1].abs() < 1e-5 && q[2].abs() < 1e-5, "{method:?}: a hinge turns about x alone: {q:?}");
      let angle = 2.0 * q[0].atan2(q[3]);
      assert!(angle.abs() <= 0.3 + 1e-4, "{method:?}: hinge angle {angle}");
    }
    assert!(s.ik_reach(ik, 0).expect("reach") > 0.1, "{method:?}: the limited chain cannot reach");
  }
  // A cone: the swing away from the bone axis is bounded.
  let mut s = Spatial::new();
  let nodes = chain(&mut s, 2);
  let limits =
    vec![(nodes[0], IkLimit::Cone { swing: 0.2, twist: 0.0 }), (nodes[1], IkLimit::Cone { swing: 0.2, twist: 0.0 })];
  s.create_ik(
    nodes[0],
    vec![(IkSolverSpec::Chain(chain_spec(&nodes, ChainMethod::Ccd, [0.4, 0.0, 0.0], limits)), 1.0)],
  )
  .expect("ik");
  advance_at(&mut s, 0.0);
  for &n in &nodes[..2] {
    let q = rotation_of(&s, n);
    let angle = 2.0 * (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt().atan2(q[3].abs());
    assert!(angle <= 0.2 + 1e-4, "cone swing {angle}");
  }
}

#[test]
fn look_at_turns_the_forward_axis_and_distributes_by_shares() {
  let mut s = Spatial::new();
  let spine = node(&mut s, None, [0.0; 3]);
  let head = node(&mut s, Some(spine), [0.0, 0.2, 0.0]);
  let far = [1000.0, 0.2, 0.0];
  s.create_ik(spine, vec![(IkSolverSpec::LookAt(look_at(vec![(spine, 1.0), (head, 1.0)], Some(far), None, 0.0)), 1.0)])
    .expect("ik");
  advance_at(&mut s, 0.0);
  assert!(close(forward_of(&s, head), [1.0, 0.0, 0.0], 1e-3), "head forward {:?}", forward_of(&s, head));
  // Equal shares: the spine took half the quarter turn (yaw +45 deg about y).
  let q = rotation_of(&s, spine);
  assert!(
    (q[1] - (std::f64::consts::FRAC_PI_8).sin()).abs() < 1e-3 && q[0].abs() < 1e-4 && q[2].abs() < 1e-4,
    "spine {q:?}"
  );
}

#[test]
fn look_at_solves_from_the_primary_pose_every_frame() {
  let mut s = Spatial::new();
  let spine = node(&mut s, None, [0.0; 3]);
  let head = node(&mut s, Some(spine), [0.0, 0.2, 0.0]);
  s.create_ik(
    spine,
    vec![(IkSolverSpec::LookAt(look_at(vec![(head, 1.0)], Some([1000.0, 0.2, 0.0]), None, 0.0)), 0.5)],
  )
  .expect("ik");
  advance_at(&mut s, 0.0);
  let first = rotation_of(&s, head);
  // Nobody writes the head: ten frames later it holds the half aim, not
  // an accumulation toward the full one.
  for i in 1..=10 {
    advance_at(&mut s, FRAME * i as f64);
  }
  let later = rotation_of(&s, head);
  assert!((0..4).all(|i| (first[i] - later[i]).abs() < 1e-5), "stable at weight 0.5: {first:?} vs {later:?}");
  assert!((later[1] - (std::f64::consts::FRAC_PI_8).sin()).abs() < 1e-3, "half the quarter turn: {later:?}");
  // A player writes the head every frame (the rest pose here): the same
  // result, the write being the input.
  for i in 11..=20 {
    s.set_transform(head, [0.0, 0.2, 0.0], Q, ONE).expect("write");
    advance_at(&mut s, FRAME * i as f64);
  }
  let written = rotation_of(&s, head);
  assert!((0..4).all(|i| (first[i] - written[i]).abs() < 1e-5), "a primary write is the input: {written:?}");
  // A different primary pose is solved over, not replaced.
  let tilt = [0.3f64.sin(), 0.0, 0.0, 0.3f64.cos()];
  s.set_transform(head, [0.0, 0.2, 0.0], tilt, ONE).expect("write");
  advance_at(&mut s, FRAME * 21.0);
  let tilted = rotation_of(&s, head);
  assert!(tilted[0].abs() > 0.05, "the tilt survives at half weight: {tilted:?}");
}

#[test]
fn look_at_limit_clamps_and_a_flip_eases_through_rest() {
  let mut s = Spatial::new();
  let head = node(&mut s, None, [0.0; 3]);
  let at_yaw = |yaw: f64| [1000.0 * yaw.sin(), 0.0, 1000.0 * yaw.cos()];
  let limit = LookAtLimit { yaw: [-1.0, 1.0], pitch: [-1.0, 1.0], damp: 1.0 };
  let ik = s
    .create_ik(
      head,
      vec![(IkSolverSpec::LookAt(look_at(vec![(head, 1.0)], Some(at_yaw(1.5)), Some(limit), 1000.0)), 1.0)],
    )
    .expect("ik");
  advance_at(&mut s, 0.0);
  let yaw_of = |s: &Spatial| {
    let f = forward_of(s, head);
    f[0].atan2(f[2])
  };
  assert!((yaw_of(&s) - 1.0).abs() < 1e-3, "clamped at the limit: {}", yaw_of(&s));
  // The target goes behind, then flips across the back: no snap to the
  // other limit - the head eases through rest over the ease time.
  s.set_ik_target(ik, 0, Some(point(at_yaw(2.8)))).expect("target");
  advance_at(&mut s, FRAME);
  assert!((yaw_of(&s) - 1.0).abs() < 1e-3, "still at the limit: {}", yaw_of(&s));
  s.set_ik_target(ik, 0, Some(point(at_yaw(-2.8)))).expect("target");
  let t0 = 2.0 * FRAME;
  let tick = advance_at(&mut s, t0);
  assert!(tick.active, "the transition asks for frames");
  assert!(yaw_of(&s) > 0.9, "no snap: {}", yaw_of(&s));
  advance_at(&mut s, t0 + 500.0);
  assert!(yaw_of(&s).abs() < 0.05, "through rest at half time: {}", yaw_of(&s));
  let tick = advance_at(&mut s, t0 + 1000.0);
  assert!((yaw_of(&s) + 1.0).abs() < 1e-3, "at the other limit: {}", yaw_of(&s));
  assert!(!tick.active, "settled");
  // A damped limit eases toward the bound instead of clamping at it.
  let f = crate::spatial::clamp_damped_for_test;
  assert_eq!(f(0.5, [-1.0, 1.0], 1.0), 0.5);
  assert_eq!(f(1.5, [-1.0, 1.0], 1.0), 1.0);
  assert_eq!(f(0.5, [-1.0, 1.0], 0.5), 0.5);
  let eased = f(1.0, [-1.0, 1.0], 0.5);
  assert!(eased > 0.8 && eased < 0.83, "damped {eased}");
  assert!((f(20.0, [-1.0, 1.0], 0.5) - 1.0).abs() < 1e-6);
  assert!((f(-20.0, [-0.5, 1.0], 0.5) + 0.5).abs() < 1e-6);
}

#[test]
fn a_lost_target_eases_back_to_the_primary_pose() {
  let mut s = Spatial::new();
  let head = node(&mut s, None, [0.0; 3]);
  let ik = s
    .create_ik(
      head,
      vec![(IkSolverSpec::LookAt(look_at(vec![(head, 1.0)], Some([1000.0, 0.0, 0.0]), None, 500.0)), 1.0)],
    )
    .expect("ik");
  advance_at(&mut s, 0.0);
  assert!(close(forward_of(&s, head), [1.0, 0.0, 0.0], 1e-3));
  s.set_ik_target(ik, 0, None).expect("target");
  advance_at(&mut s, 250.0);
  let f = forward_of(&s, head);
  assert!(f[0] > 0.5 && f[0] < 0.9, "halfway back: {f:?}");
  let tick = advance_at(&mut s, 500.0);
  assert!(close(forward_of(&s, head), [0.0, 0.0, 1.0], 1e-3));
  assert!(!tick.active);
}

/// A biped over two floors: the model root at the origin, hips at y = 1,
/// legs of two 0.5 bones to the ankles at y = 0, the left floor's top at
/// y = -0.2 and the right's at 0. Returns (root, hips, [left, right] as
/// (hip, knee, ankle)).
fn biped(s: &mut Spatial) -> (u64, u64, [(u64, u64, u64); 2]) {
  let root = node(s, None, [0.0; 3]);
  let hips = node(s, Some(root), [0.0, 1.0, 0.0]);
  let mut legs = [(0, 0, 0); 2];
  for (i, x) in [-0.1f64, 0.1].iter().enumerate() {
    let hip = node(s, Some(hips), [*x, 0.0, 0.0]);
    let knee = node(s, Some(hip), [0.0, -0.5, 0.0]);
    let ankle = node(s, Some(knee), [0.0, -0.5, 0.0]);
    legs[i] = (hip, knee, ankle);
  }
  let left_floor = node(s, None, [-0.1, -0.7, 0.0]);
  let right_floor = node(s, None, [0.1, -0.5, 0.0]);
  for f in [left_floor, right_floor] {
    s.set_bounds(f, Some([-0.09, -0.5, -0.5, 0.09, 0.5, 0.5])).expect("bounds");
  }
  s.flush(&mut Sink);
  (root, hips, legs)
}

fn feet(hips: u64, legs: &[(u64, u64, u64); 2], ease_ms: f64) -> FeetSpec {
  FeetSpec {
    hips,
    legs: legs.iter().map(|&(root, mid, tip)| LegSpec { root, mid, tip, height: None }).collect(),
    up: [0.0, 1.0, 0.0],
    filter: QueryFilter::default(),
    rise: 0.3,
    drop: 0.5,
    plant_height: 0.05,
    plant_band: 0.1,
    hips_ease_ms: ease_ms,
    rotation_weight: 1.0,
  }
}

#[test]
fn feet_plant_on_their_ground_and_the_hips_drop_for_the_lower_foot() {
  let mut s = Spatial::new();
  let (root, hips, legs) = biped(&mut s);
  let ik = s.create_ik(root, vec![(IkSolverSpec::Feet(feet(hips, &legs, 0.0)), 1.0)]).expect("ik");
  advance_at(&mut s, 0.0);
  assert!(
    (world_pos(&s, hips)[1] - 0.8).abs() < 1e-4,
    "hips dropped by the left foot's 0.2: {:?}",
    world_pos(&s, hips)
  );
  let left = world_pos(&s, legs[0].2);
  let right = world_pos(&s, legs[1].2);
  assert!((left[1] + 0.2).abs() < 1e-3, "left ankle on its floor: {left:?}");
  assert!(right[1].abs() < 1e-3, "right ankle on its floor: {right:?}");
  assert!(s.ik_reach(ik, 0).expect("reach") < 1e-3);
  // The right leg, shortened by the drop, bends; the left is straight.
  let knee = world_pos(&s, legs[1].1);
  let line = [world_pos(&s, legs[1].0), right];
  let off = knee[2].abs().max((knee[0] - line[0][0]).abs());
  assert!(off > 0.1, "the right knee bends: {knee:?}");
  // The feet keep their lengths.
  for (hip, knee, ankle) in legs {
    assert!((dist(world_pos(&s, hip), world_pos(&s, knee)) - 0.5).abs() < 1e-4);
    assert!((dist(world_pos(&s, knee), world_pos(&s, ankle)) - 0.5).abs() < 1e-4);
  }
}

#[test]
fn a_lifted_foot_follows_the_clip_and_the_hips_ease() {
  let mut s = Spatial::new();
  let (root, hips, legs) = biped(&mut s);
  s.create_ik(root, vec![(IkSolverSpec::Feet(feet(hips, &legs, 200.0)), 1.0)]).expect("ik");
  // The left foot lifted 0.3 (its knee shortened): past the plant band,
  // so the hips stay for the right foot alone, which is on its floor.
  s.set_transform(legs[0].1, [0.0, -0.2, 0.0], Q, ONE).expect("lift");
  advance_at(&mut s, 0.0);
  let tick = advance_at(&mut s, FRAME);
  assert!((world_pos(&s, hips)[1] - 1.0).abs() < 1e-4, "no drop for a lifted foot: {:?}", world_pos(&s, hips));
  assert!(!tick.active);
  let left = world_pos(&s, legs[0].2);
  assert!((left[1] - 0.3).abs() < 1e-4, "the lifted foot keeps the clip's pose: {left:?}");
  // Planted again: the hips ease down over the time constant.
  s.set_transform(legs[0].1, [0.0, -0.5, 0.0], Q, ONE).expect("plant");
  let tick = advance_at(&mut s, 2.0 * FRAME);
  assert!(tick.active, "easing");
  let y = world_pos(&s, hips)[1];
  assert!(y < 1.0 && y > 0.8, "part way: {y}");
  advance_at(&mut s, 2.0 * FRAME + 2000.0);
  assert!((world_pos(&s, hips)[1] - 0.8).abs() < 1e-3);
}

#[test]
fn the_weight_ramps_and_destroy_puts_the_joints_back() {
  let mut s = Spatial::new();
  let (_, shoulder, elbow, wrist) = arm(&mut s);
  let target = [0.2, 0.4, 0.1];
  let start = dist([0.0, 2.0 * BONE, 0.0], target);
  let ik = s
    .create_ik(shoulder, vec![(IkSolverSpec::TwoBone(two_bone(shoulder, elbow, wrist, target, None)), 0.0)])
    .expect("ik");
  advance_at(&mut s, 0.0);
  assert!(close(world_pos(&s, wrist), [0.0, 2.0 * BONE, 0.0], 1e-6), "weight 0: untouched");
  s.set_ik_weight(ik, 0, 1.0, 1000.0).expect("weight");
  let tick = advance_at(&mut s, 500.0);
  assert!(tick.active);
  assert!((s.ik_weight(ik, 0).expect("weight") - 0.5).abs() < 1e-5);
  let mid = dist(world_pos(&s, wrist), target);
  assert!(mid > 0.02 && mid < start - 0.02, "part way: {mid} of {start}");
  let tick = advance_at(&mut s, 1000.0);
  assert!(!tick.active);
  assert!(close(world_pos(&s, wrist), target, 1e-4));
  assert!(s.destroy_ik(ik), "the joints go back");
  for n in [shoulder, elbow] {
    let q = rotation_of(&s, n);
    assert!((q[3].abs() - 1.0).abs() < 1e-6, "{q:?}");
  }
  assert!(!s.destroy_ik(ik));
}

#[test]
fn a_spring_hanging_from_a_solved_bone_lags_it_in_the_same_frame() {
  let mut s = Spatial::new();
  let head = node(&mut s, None, [0.0, 1.0, 0.0]);
  let b1 = node(&mut s, Some(head), [0.0; 3]);
  let b2 = node(&mut s, Some(b1), [0.0, -0.1, 0.0]);
  let joint = |node| SpringJoint {
    node,
    tail: [0.0, -0.1, 0.0],
    stiffness: 1.0,
    drag: 0.4,
    gravity_power: 0.0,
    gravity_dir: [0.0, -1.0, 0.0],
    hit_radius: 0.0,
  };
  s.set_transition_now(0.0);
  s.create_spring(head, vec![joint(b1), joint(b2)], vec![], None).expect("spring");
  // The head pitches straight up at the target: the chain under it swings.
  s.create_ik(head, vec![(IkSolverSpec::LookAt(look_at(vec![(head, 1.0)], Some([0.0, 1000.0, 0.0]), None, 0.0)), 1.0)])
    .expect("ik");
  s.set_transition_now(FRAME);
  let ik = s.advance_ik();
  let springs = s.advance_springs();
  assert!(ik.wrote && springs.wrote);
  assert!(close(forward_of(&s, head), [0.0, 1.0, 0.0], 1e-3));
  let q = rotation_of(&s, b1);
  assert!((q[3].abs() - 1.0).abs() > 1e-3, "the chain lags the turned head: {q:?}");
}

#[test]
fn a_feet_solver_refuses_a_target_and_dead_nodes_retire_a_solver() {
  let mut s = Spatial::new();
  let (root, hips, legs) = biped(&mut s);
  let ik = s.create_ik(root, vec![(IkSolverSpec::Feet(feet(hips, &legs, 0.0)), 1.0)]).expect("ik");
  assert!(s.set_ik_target(ik, 0, Some(point([0.0; 3]))).is_err());
  assert!(s.set_ik_weight(ik, 0, 1.5, 0.0).is_err());
  assert!(s.set_ik_weight(ik, 1, 1.0, 0.0).is_err());
  advance_at(&mut s, 0.0);
  s.destroy(legs[0].2).expect("destroy");
  let tick = advance_at(&mut s, FRAME);
  assert!(!tick.wrote, "a retired solver writes nothing");
  // A chain whose target node dies retires too; the stack lives on.
  let (_, shoulder, elbow, wrist) = arm(&mut s);
  let t = node(&mut s, None, [0.2, 0.4, 0.1]);
  let ik = s
    .create_ik(
      shoulder,
      vec![(
        IkSolverSpec::TwoBone(TwoBoneSpec {
          target: Some(IkTarget::Node(t)),
          ..two_bone(shoulder, elbow, wrist, [0.0; 3], None)
        }),
        1.0,
      )],
    )
    .expect("ik");
  advance_at(&mut s, 2.0 * FRAME);
  assert!(close(world_pos(&s, wrist), transform_point(&s.world(t).expect("world"), [0.0; 3]), 1e-4));
  s.destroy(t).expect("destroy");
  advance_at(&mut s, 3.0 * FRAME);
  assert!(s.ik_reach(ik, 0).is_ok(), "the stack lives on");
}

/// The probe's tail: four 0.15 bones hanging straight back (-z) from a
/// root, a target orbiting in front of their reach, a magnet far above.
/// Every frame solves from the same rest pose, so the joints must move
/// continuously with the target: no frame may jump.
#[test]
fn a_chain_follows_an_orbiting_target_continuously() {
  for method in [ChainMethod::Fabrik, ChainMethod::Ccd] {
    for magnet in [None, Some(IkPoint::Point { position: [0.0, 1.067, -0.5], space: None })] {
      let mut s = Spatial::new();
      let mut nodes = vec![node(&mut s, None, [0.0; 3])];
      for _ in 0..4 {
        let parent = *nodes.last().expect("root");
        nodes.push(node(&mut s, Some(parent), [0.0, 0.0, -0.15]));
      }
      let mut spec = chain_spec(&nodes, method, [0.0, 0.0, -0.5], vec![]);
      spec.iterations = 10;
      spec.tolerance = 1e-3;
      spec.magnet = magnet;
      let ik = s.create_ik(nodes[0], vec![(IkSolverSpec::Chain(spec), 1.0)]).expect("ik");
      let mut prev: Option<Vec<[f64; 3]>> = None;
      let mut worst = 0.0f64;
      let mut worst_frame = 0;
      for frame in 0..480 {
        let c = frame as f64 / 240.0 * 2.0 * std::f64::consts::PI;
        // The first orbit as the probe's tail sees it under dropped hips,
        // the second under undropped ones.
        let lift = if frame < 240 { -0.083 } else { -0.15 };
        let target = [c.sin() * 0.35, lift + c.cos() * 0.25, -0.45];
        s.set_ik_target(ik, 0, Some(point(target))).expect("target");
        advance_at(&mut s, frame as f64 * FRAME);
        let now: Vec<[f64; 3]> = nodes.iter().map(|&n| world_pos(&s, n)).collect();
        if let (Some(p), true) = (&prev, frame != 240) {
          let jump = now.iter().zip(p).map(|(a, b)| dist(*a, *b)).fold(0.0, f64::max);
          if jump > worst {
            worst = jump;
            worst_frame = frame;
          }
        }
        prev = Some(now);
      }
      // The target moves under 1 cm per frame; a joint may not move more
      // than a few times that.
      assert!(worst < 0.03, "{method:?} magnet {}: a joint jumped {worst} at frame {worst_frame}", magnet.is_some());
    }
  }
}

/// A rig under a mirroring ancestor (scale -1 on x) solves as the mirror
/// image of the plain rig: the limb reaches, the head's forward lands on
/// the target, the chain reaches, and every joint sits at the reflection
/// of its plain twin.
#[test]
fn a_mirrored_rig_solves_like_its_reflection() {
  let mut s = Spatial::new();
  let plain = s.create([0.0; 3], Q, ONE, true);
  let mirror = s.create([0.0; 3], Q, [-1.0, 1.0, 1.0], true);
  let flip = |p: [f64; 3]| [-p[0], p[1], p[2]];
  let build = |s: &mut Spatial, anchor: u64| {
    let shoulder = node(s, Some(anchor), [0.3, 1.0, 0.0]);
    let elbow = node(s, Some(shoulder), [0.0, -BONE, 0.0]);
    let wrist = node(s, Some(elbow), [0.0, -BONE, 0.0]);
    let head = node(s, Some(anchor), [0.0, 1.3, 0.0]);
    let c0 = node(s, Some(anchor), [-0.2, 1.0, 0.0]);
    let c1 = node(s, Some(c0), [0.0, 0.0, -0.2]);
    let c2 = node(s, Some(c1), [0.0, 0.0, -0.2]);
    let c3 = node(s, Some(c2), [0.0, 0.0, -0.2]);
    (shoulder, elbow, wrist, head, [c0, c1, c2, c3])
  };
  let a = build(&mut s, plain);
  let b = build(&mut s, mirror);
  let hand = [0.5, 0.75, 0.15];
  let pole = [0.6, 0.6, 1.0];
  let gaze = [5.0, 1.5, 2.0];
  let tail = [-0.1, 0.7, -0.4];
  let stack = |s: &mut Spatial, r: (u64, u64, u64, u64, [u64; 4]), m: bool| {
    let f = |p: [f64; 3]| if m { flip(p) } else { p };
    let mut arm = two_bone(r.0, r.1, r.2, f(hand), Some(f(pole)));
    arm.rotation_weight = 1.0;
    arm.target = Some(IkTarget::Point { position: f(hand), rotation: Some(Q), space: None });
    let chain_nodes = [r.4[0], r.4[1], r.4[2], r.4[3]];
    let mut ch = chain_spec(&chain_nodes, ChainMethod::Fabrik, f(tail), vec![]);
    ch.magnet = Some(IkPoint::Point { position: f([0.0, 3.0, -0.3]), space: None });
    s.create_ik(
      r.0,
      vec![
        (IkSolverSpec::LookAt(look_at(vec![(r.3, 1.0)], Some(f(gaze)), None, 0.0)), 1.0),
        (IkSolverSpec::TwoBone(arm), 1.0),
        (IkSolverSpec::Chain(ch), 1.0),
      ],
    )
    .expect("ik")
  };
  let ia = stack(&mut s, a, false);
  let ib = stack(&mut s, b, true);
  advance_at(&mut s, 0.0);
  // Reach, on both.
  assert!(close(world_pos(&s, a.2), hand, 1e-4), "plain wrist {:?}", world_pos(&s, a.2));
  assert!(close(world_pos(&s, b.2), flip(hand), 1e-4), "mirrored wrist {:?}", world_pos(&s, b.2));
  assert!(s.ik_reach(ia, 2).expect("reach") < 1e-3 && s.ik_reach(ib, 2).expect("reach") < 1e-3, "chains reach");
  for (p, m) in [(a.3, b.3)] {
    let want = |head: [f64; 3], at: [f64; 3]| norm([at[0] - head[0], at[1] - head[1], at[2] - head[2]]);
    assert!(close(forward_of(&s, p), want(world_pos(&s, p), gaze), 1e-3), "plain gaze {:?}", forward_of(&s, p));
    assert!(
      close(forward_of(&s, m), want(world_pos(&s, m), flip(gaze)), 1e-3),
      "mirrored gaze {:?}",
      forward_of(&s, m)
    );
  }
  // Every joint at the reflection of its twin.
  let pairs = [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3), (a.4[1], b.4[1]), (a.4[2], b.4[2]), (a.4[3], b.4[3])];
  for (p, m) in pairs {
    let (wp, wm) = (world_pos(&s, p), world_pos(&s, m));
    assert!(close(flip(wp), wm, 1e-4), "{wp:?} vs mirrored {wm:?}");
  }
  // The mirrored wrist takes the identity orientation under Three's
  // convention: its world axes are the plain wrist's reflected.
  let (wa, wb) = (s.world(a.2).expect("world"), s.world(b.2).expect("world"));
  for k in [1, 2, 5, 6, 9, 10] {
    assert!((wa[k] - wb[k]).abs() < 1e-3, "axis element {k}: {} vs {}", wa[k], wb[k]);
  }
}
