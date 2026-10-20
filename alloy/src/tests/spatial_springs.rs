use crate::spatial::{
  flow_step, transform_point, Spatial, SpringCollider, SpringJoint, SpringJointUpdate, SpringShape, SpringsTick,
  SPRING_REFERENCE_STEP,
};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];
const S: f64 = SPRING_REFERENCE_STEP;
/// The port's bone: 10 cm hanging down.
const BONE: f32 = 0.1;

fn axis_quat(axis: [f32; 3], angle: f32) -> [f32; 4] {
  let h = angle * 0.5;
  let s = h.sin();
  [axis[0] * s, axis[1] * s, axis[2] * s, h.cos()]
}

fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
  (0..3).all(|i| (a[i] - b[i]).abs() <= tol)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
  ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn joint(node: u64, drag: f32, gravity_power: f32, hit_radius: f32) -> SpringJoint {
  SpringJoint {
    node,
    tail: [0.0, -BONE, 0.0],
    rest_rotation: Q,
    stiffness: 1.0,
    drag,
    gravity_power,
    gravity_dir: [0.0, -1.0, 0.0],
    hit_radius,
  }
}

/// The port's rig: an anchor at y = 1, two bones hanging from it 10 cm
/// each. Returns (anchor, bone 1, bone 2).
fn rig(s: &mut Spatial) -> (u64, u64, u64) {
  let anchor = s.create([0.0, 1.0, 0.0], Q, ONE, true);
  let b1 = s.create([0.0; 3], Q, ONE, true);
  let b2 = s.create([0.0, -BONE, 0.0], Q, ONE, true);
  s.set_parent(b1, Some(anchor)).expect("parent");
  s.set_parent(b2, Some(b1)).expect("parent");
  (anchor, b1, b2)
}

fn chain(s: &mut Spatial, b1: u64, b2: u64, gravity_power: f32, colliders: Vec<SpringCollider>) -> u64 {
  let anchor_clock = b1;
  s.create_spring(
    anchor_clock,
    vec![joint(b1, 0.4, gravity_power, 0.01), joint(b2, 0.4, gravity_power, 0.01)],
    colliders,
    None,
  )
  .expect("spring")
}

fn advance_at(s: &mut Spatial, ms: f64) -> SpringsTick {
  s.set_transition_now(ms);
  s.advance_springs()
}

/// The end of bone 2 (its tail) in world space.
fn chain_end(s: &Spatial, b2: u64) -> [f32; 3] {
  transform_point(&s.world(b2).expect("world"), [0.0, -BONE, 0.0])
}

fn rotation_of(s: &Spatial, n: u64) -> [f32; 4] {
  s.transform_of(n).expect("read").1
}

fn is_identity(q: [f32; 4]) -> bool {
  (q[3].abs() - 1.0).abs() < 1e-6
}

#[test]
fn the_flow_is_three_vrm_at_the_reference_step() {
  let x = [0.3, -0.2, 0.5];
  let u = [1.0, 0.5, -2.0];
  let f = [0.3, -1.0, 0.2];
  for drag in [0.0f32, 0.1, 0.4, 0.9, 0.999, 1.0] {
    let keep = (1.0 - drag).max(1e-6);
    let (nx, nu, kappa) = flow_step(drag, x, u, f, S);
    for i in 0..3 {
      let want_u = keep * u[i] + f[i];
      let want_x = x[i] + S as f32 * want_u;
      assert!((nu[i] - want_u).abs() < 1e-5, "drag {drag}: u {nu:?} vs {want_u}");
      assert!((nx[i] - want_x).abs() < 1e-6, "drag {drag}: x {nx:?} vs {want_x}");
    }
    // The correction factor is position verlet's at the reference step.
    assert!((kappa - 1.0 / S as f32).abs() < 1e-2, "drag {drag}: kappa {kappa}");
  }
}

#[test]
fn the_flow_composes_exactly() {
  let x = [0.3, -0.2, 0.5];
  let u = [1.0, 0.5, -2.0];
  let f = [0.3, -1.0, 0.2];
  for drag in [0.0f32, 0.05, 0.4, 0.9] {
    let (x1, u1, _) = flow_step(drag, x, u, f, S);
    // Two halves, three thirds, and a quarter plus three quarters.
    for parts in [[0.5, 0.5, 0.0], [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], [0.25, 0.75, 0.0]] {
      let (mut px, mut pu) = (x, u);
      for p in parts {
        if p == 0.0 {
          continue;
        }
        let (nx, nu, _) = flow_step(drag, px, pu, f, S * p);
        px = nx;
        pu = nu;
      }
      assert!(close(px, x1, 1e-6), "drag {drag} parts {parts:?}: {px:?} vs {x1:?}");
      assert!(close(pu, u1, 1e-5), "drag {drag} parts {parts:?}: {pu:?} vs {u1:?}");
    }
    // A triple step is three steps.
    let (x3, u3, _) = flow_step(drag, x, u, f, 3.0 * S);
    let (xa, ua, _) = flow_step(drag, x1, u1, f, S);
    let (xb, ub, _) = flow_step(drag, xa, ua, f, S);
    assert!(close(x3, xb, 1e-6) && close(u3, ub, 1e-5), "drag {drag}: {x3:?} vs {xb:?}");
  }
}

#[test]
fn a_chain_under_a_still_bone_rests_and_asks_for_nothing() {
  let mut s = Spatial::new();
  let (_, b1, b2) = rig(&mut s);
  chain(&mut s, b1, b2, 0.0, vec![]);
  let tick = advance_at(&mut s, 0.0);
  assert!(!tick.active && !tick.wrote, "priming at rest writes nothing: {tick:?}");
  for i in 1..=120 {
    let tick = advance_at(&mut s, i as f64 * S * 1000.0);
    assert!(!tick.active && !tick.wrote, "frame {i}: {tick:?}");
  }
  assert!(is_identity(rotation_of(&s, b1)) && is_identity(rotation_of(&s, b2)));
}

#[test]
fn a_turned_bone_drags_the_chain_which_settles_along_the_new_rest() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  chain(&mut s, b1, b2, 0.0, vec![]);
  advance_at(&mut s, 0.0);
  // A quarter turn about z: the rest direction swings from -y to +x.
  s.set_transform(anchor, [0.0, 1.0, 0.0], axis_quat([0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2), ONE).expect("turn");
  let tick = advance_at(&mut s, S * 1000.0);
  assert!(tick.active && tick.wrote, "{tick:?}");
  // The first step lags: the bone has not followed its parent all the way.
  let q = rotation_of(&s, b1);
  assert!(q[3].abs() < 0.9999, "{q:?}");
  let mut settled_at = None;
  for i in 2..=600 {
    let tick = advance_at(&mut s, i as f64 * S * 1000.0);
    if !tick.active && settled_at.is_none() {
      settled_at = Some(i);
    }
  }
  let end = chain_end(&s, b2);
  assert!(close(end, [2.0 * BONE, 1.0, 0.0], 1e-3), "{end:?}");
  let at = settled_at.expect("the chain settles");
  assert!(at < 600, "settled at frame {at}");
  // Settled: a further frame asks for nothing.
  let tick = advance_at(&mut s, 601.0 * S * 1000.0);
  assert!(!tick.active && !tick.wrote, "{tick:?}");
}

#[test]
fn bones_keep_their_length_while_swinging() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  chain(&mut s, b1, b2, 0.5, vec![]);
  advance_at(&mut s, 0.0);
  for i in 1..=240 {
    let q = axis_quat([1.0, 0.0, 0.0], (i as f32 / 10.0).sin() * 0.8);
    s.set_transform(anchor, [0.0, 1.0, 0.0], q, ONE).expect("swing");
    advance_at(&mut s, i as f64 * S * 1000.0);
    let p1 = transform_point(&s.world(b1).expect("w"), [0.0; 3]);
    let t1 = transform_point(&s.world(b1).expect("w"), [0.0, -BONE, 0.0]);
    let p2 = transform_point(&s.world(b2).expect("w"), [0.0; 3]);
    let t2 = chain_end(&s, b2);
    assert!((dist(p1, t1) - BONE).abs() < 1e-5 && (dist(p2, t2) - BONE).abs() < 1e-5);
    assert!(dist(p1, [0.0, 1.0, 0.0]) < 1e-5 && (dist(t1, p2)) < 1e-5, "bone 2 hangs from bone 1's tail");
  }
}

#[test]
fn the_motion_does_not_depend_on_the_frame_rate() {
  // The same step response (a quarter turn under gravity) sampled at 60,
  // 120 and 50 Hz: the chain ends agree at every common instant.
  let run = |hz: f64| -> Vec<[f32; 3]> {
    let mut s = Spatial::new();
    let (anchor, b1, b2) = rig(&mut s);
    chain(&mut s, b1, b2, 0.5, vec![]);
    advance_at(&mut s, 0.0);
    s.set_transform(anchor, [0.0, 1.0, 0.0], axis_quat([0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2), ONE)
      .expect("turn");
    let mut samples = Vec::new();
    let frames = (hz * 2.0) as u32;
    for i in 1..=frames {
      let ms = i as f64 * 1000.0 / hz;
      advance_at(&mut s, ms);
      // Every 100 ms, on every rate's grid.
      if (i as f64 * 10.0 / hz).fract().abs() < 1e-9 {
        samples.push(chain_end(&s, b2));
      }
    }
    samples
  };
  let at60 = run(60.0);
  let at120 = run(120.0);
  let at50 = run(50.0);
  assert_eq!(at60.len(), 20);
  assert_eq!(at120.len(), 20);
  assert_eq!(at50.len(), 20);
  // Half a millimetre on a 20 cm chain through the transient (whole
  // frames without substeps: 4 mm).
  for i in 0..20 {
    assert!(close(at60[i], at120[i], 5e-4), "sample {i}: 60 Hz {:?} vs 120 Hz {:?}", at60[i], at120[i]);
    assert!(close(at60[i], at50[i], 5e-4), "sample {i}: 60 Hz {:?} vs 50 Hz {:?}", at60[i], at50[i]);
  }
}

#[test]
fn a_sphere_pushes_the_tail_out() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  // A sphere sitting where the first bone's tail hangs at rest.
  let sphere = SpringCollider {
    node: anchor,
    shape: SpringShape::Sphere { offset: [0.0, -BONE, 0.02], radius: 0.04, inside: false },
  };
  chain(&mut s, b1, b2, 0.0, vec![sphere]);
  advance_at(&mut s, 0.0);
  for i in 1..=60 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  let tail = transform_point(&s.world(b1).expect("w"), [0.0, -BONE, 0.0]);
  let d = dist(tail, [0.0, 1.0 - BONE, 0.02]);
  assert!(d > 0.045, "tail {tail:?} is {d} from the sphere's center");
}

#[test]
fn an_inside_capsule_and_a_plane_hold_the_tail() {
  // A chain pulled sideways by gravity, kept within a capsule around its
  // rest line, then by a plane.
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  let capsule = SpringCollider {
    node: anchor,
    shape: SpringShape::Capsule { a: [0.0, 0.0, 0.0], b: [0.0, -0.3, 0.0], radius: 0.05, inside: true },
  };
  let id = s
    .create_spring(
      b1,
      vec![
        SpringJoint { gravity_dir: [1.0, 0.0, 0.0], ..joint(b1, 0.4, 3.0, 0.01) },
        SpringJoint { gravity_dir: [1.0, 0.0, 0.0], ..joint(b2, 0.4, 3.0, 0.01) },
      ],
      vec![capsule],
      None,
    )
    .expect("spring");
  advance_at(&mut s, 0.0);
  for i in 1..=120 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  let end = chain_end(&s, b2);
  assert!(end[0] > 0.02 && end[0] <= 0.04 + 1e-4, "held within the capsule: {end:?}");
  s.destroy_spring(id);

  let plane =
    SpringCollider { node: anchor, shape: SpringShape::Plane { offset: [0.03, 0.0, 0.0], normal: [-1.0, 0.0, 0.0] } };
  s.create_spring(
    b1,
    vec![
      SpringJoint { gravity_dir: [1.0, 0.0, 0.0], ..joint(b1, 0.4, 3.0, 0.01) },
      SpringJoint { gravity_dir: [1.0, 0.0, 0.0], ..joint(b2, 0.4, 3.0, 0.01) },
    ],
    vec![plane],
    None,
  )
  .expect("spring");
  for i in 121..=240 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  let end = chain_end(&s, b2);
  assert!(end[0] > 0.015 && end[0] <= 0.02 + 1e-4, "held on the plane's side: {end:?}");
}

#[test]
fn a_frozen_clock_holds_and_a_dead_node_drops() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  let id = chain(&mut s, b1, b2, 0.0, vec![]);
  advance_at(&mut s, 0.0);
  s.set_transform(anchor, [0.0, 1.0, 0.0], axis_quat([0.0, 0.0, 1.0], 1.0), ONE).expect("turn");
  s.set_time_scale(b1, Some(0.0)).expect("freeze");
  let tick = advance_at(&mut s, S * 1000.0);
  assert!(!tick.wrote && is_identity(rotation_of(&s, b1)), "frozen: {tick:?}");
  s.set_time_scale(b1, None).expect("thaw");
  let tick = advance_at(&mut s, 2.0 * S * 1000.0);
  assert!(tick.wrote && !is_identity(rotation_of(&s, b1)), "thawed: {tick:?}");
  s.destroy(b2).expect("destroy");
  advance_at(&mut s, 3.0 * S * 1000.0);
  assert!(s.reset_spring(id).is_err(), "the chain dropped with its node");
}

#[test]
fn reset_places_the_tails_back_at_rest() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  let id = chain(&mut s, b1, b2, 0.0, vec![]);
  advance_at(&mut s, 0.0);
  s.set_transform(anchor, [0.0, 1.0, 0.0], axis_quat([0.0, 0.0, 1.0], 1.0), ONE).expect("turn");
  for i in 1..=5 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  assert!(!is_identity(rotation_of(&s, b1)));
  // Reset writes the rest pose on the spot; at rest under the turned
  // anchor, the next advance has nothing to move.
  s.reset_spring(id).expect("reset");
  assert!(is_identity(rotation_of(&s, b1)) && is_identity(rotation_of(&s, b2)));
  let tick = advance_at(&mut s, 6.0 * S * 1000.0);
  assert!(!tick.wrote && !tick.active && is_identity(rotation_of(&s, b1)), "{tick:?}");
}

#[test]
fn destroy_puts_the_joints_back_at_rest() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  let id = chain(&mut s, b1, b2, 0.0, vec![]);
  advance_at(&mut s, 0.0);
  s.set_transform(anchor, [0.0, 1.0, 0.0], axis_quat([0.0, 0.0, 1.0], 1.0), ONE).expect("turn");
  for i in 1..=5 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  assert!(!is_identity(rotation_of(&s, b1)) && !is_identity(rotation_of(&s, b2)));
  // Mid-swing, the chain goes: the bones hang as authored, not frozen
  // where the last step put them, and nothing steps them any more.
  assert!(s.destroy_spring(id), "the rest rotations were written");
  assert!(is_identity(rotation_of(&s, b1)) && is_identity(rotation_of(&s, b2)));
  let tick = advance_at(&mut s, 6.0 * S * 1000.0);
  assert!(!tick.wrote && !tick.active, "{tick:?}");
  assert!(s.reset_spring(id).is_err(), "the chain is gone");
  // A chain already at rest writes nothing; a dropped id is fine.
  let id = chain(&mut s, b1, b2, 0.0, vec![]);
  assert!(!s.destroy_spring(id));
  assert!(!s.destroy_spring(id));
}

#[test]
fn a_center_keeps_its_own_motion_out_of_the_chain() {
  // The anchor slides along x every frame. In world space the chain
  // trails; in the anchor's space it rests.
  let run = |centered: bool| -> bool {
    let mut s = Spatial::new();
    let (anchor, b1, b2) = rig(&mut s);
    s.create_spring(b1, vec![joint(b1, 0.4, 0.0, 0.0), joint(b2, 0.4, 0.0, 0.0)], vec![], centered.then_some(anchor))
      .expect("spring");
    advance_at(&mut s, 0.0);
    for i in 1..=30 {
      s.set_transform(anchor, [i as f32 * 0.05, 1.0, 0.0], Q, ONE).expect("slide");
      advance_at(&mut s, i as f64 * S * 1000.0);
    }
    is_identity(rotation_of(&s, b1))
  };
  assert!(!run(false), "a world-space chain trails the anchor");
  assert!(run(true), "a centered chain rests in the anchor's space");
}

#[test]
fn creation_rejects_bad_input() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  assert!(s.create_spring(b1, vec![], vec![], None).is_err());
  assert!(s.create_spring(b1, vec![SpringJoint { tail: [0.0; 3], ..joint(b1, 0.4, 0.0, 0.0) }], vec![], None).is_err());
  assert!(s.create_spring(b1, vec![joint(b1, 1.5, 0.0, 0.0)], vec![], None).is_err());
  // A child listed before its parent joint, a node listed twice.
  assert!(s.create_spring(b1, vec![joint(b2, 0.4, 0.0, 0.0), joint(b1, 0.4, 0.0, 0.0)], vec![], None).is_err());
  assert!(s.create_spring(b1, vec![joint(b1, 0.4, 0.0, 0.0), joint(b1, 0.4, 0.0, 0.0)], vec![], None).is_err());
  s.destroy(anchor).expect("destroy");
  assert!(s.create_spring(anchor, vec![joint(b1, 0.4, 0.0, 0.0)], vec![], None).is_err(), "dead clock");
}

#[test]
fn a_force_deflects_the_chain_and_a_constant_one_settles() {
  let mut s = Spatial::new();
  let (_, b1, b2) = rig(&mut s);
  let id = chain(&mut s, b1, b2, 0.0, vec![]);
  advance_at(&mut s, 0.0);
  // A wind along +x: the chain leans that way and comes to rest there.
  s.set_spring_force(id, [1.0, 0.0, 0.0]).expect("force");
  let tick = advance_at(&mut s, S * 1000.0);
  assert!(tick.active && tick.wrote, "{tick:?}");
  let mut settled = false;
  for i in 2..=600 {
    if !advance_at(&mut s, i as f64 * S * 1000.0).active {
      settled = true;
      break;
    }
  }
  assert!(settled, "a constant force settles");
  let end = chain_end(&s, b2);
  assert!(end[0] > 0.05 && end[1] > 1.0 - 2.0 * BONE, "leaning along the wind, its end lifted: {end:?}");
  // The force off: it hangs again.
  s.set_spring_force(id, [0.0; 3]).expect("force");
  for i in 601..=1200 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  assert!(close(chain_end(&s, b2), [0.0, 1.0 - 2.0 * BONE, 0.0], 1e-3));
  assert!(s.set_spring_force(id, [f32::NAN, 0.0, 0.0]).is_err());
  assert!(s.set_spring_force(id + 1, [0.0; 3]).is_err());
}

#[test]
fn a_joint_update_changes_the_motion_from_here_on() {
  let mut s = Spatial::new();
  let (anchor, b1, b2) = rig(&mut s);
  let id = chain(&mut s, b1, b2, 0.0, vec![]);
  advance_at(&mut s, 0.0);
  s.set_transform(anchor, [0.0, 1.0, 0.0], axis_quat([0.0, 0.0, 1.0], 1.0), ONE).expect("turn");
  advance_at(&mut s, S * 1000.0);
  // Total drag on both joints: the swing stops dead, and stiffness 0 holds
  // it where it is (no pull back) - the chain rests off its rest line.
  for j in 0..2 {
    s.set_spring_joint(id, j, SpringJointUpdate { drag: Some(1.0), stiffness: Some(0.0), ..Default::default() })
      .expect("update");
  }
  let before = rotation_of(&s, b1);
  for i in 2..=30 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  let after = rotation_of(&s, b1);
  assert!(!is_identity(before) && (0..4).all(|k| (before[k] - after[k]).abs() < 1e-4), "{before:?} vs {after:?}");
  // Stiffness back: it returns to rest.
  for j in 0..2 {
    s.set_spring_joint(id, j, SpringJointUpdate { stiffness: Some(1.0), drag: Some(0.4), ..Default::default() })
      .expect("update");
  }
  for i in 31..=600 {
    advance_at(&mut s, i as f64 * S * 1000.0);
  }
  assert!(is_identity(rotation_of(&s, b1)), "{:?}", rotation_of(&s, b1));
  assert!(s.set_spring_joint(id, 2, SpringJointUpdate::default()).is_err(), "out of range");
  assert!(s.set_spring_joint(id, 0, SpringJointUpdate { drag: Some(1.5), ..Default::default() }).is_err());
  assert!(s.set_spring_joint(id + 1, 0, SpringJointUpdate::default()).is_err());
}
