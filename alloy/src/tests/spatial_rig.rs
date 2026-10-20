use crate::spatial::SinkWriter;
use crate::spatial::{
  inertial_offset_for_test, marker_crossings, sample_channel, sequence_for_test, step_time_for_test, AdditiveReference,
  ChannelInterpolation, ChannelPath, ClipChannel, ClipEvent, ClipMarker, LayerMode, LayerSpec, LayerUpdate,
  MarkerPhase, PlayerSpec, PlayerUpdate, RestPose, RootMotion, Spatial, SyncMode,
};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];
const EPS: f32 = 1e-4;

/// A channel whose element count is read off the data (the JS baker's
/// rule): values per key over the cubic triple.
fn channel(path: ChannelPath, interpolation: ChannelInterpolation, times: &[f32], values: &[f32]) -> ClipChannel {
  let per_key = if interpolation == ChannelInterpolation::Cubic { 3 } else { 1 };
  let elements = values.len() / (times.len() * per_key);
  ClipChannel { target_slot: 0, path, interpolation, elements, times: times.to_vec(), values: values.to_vec() }
}

fn on(slot: u32, c: ClipChannel) -> ClipChannel {
  ClipChannel { target_slot: slot, ..c }
}

fn sampled(c: &ClipChannel, time: f32, cursor: &mut u32) -> [f32; 4] {
  let mut out = [0.0f32; 4];
  sample_channel(c, time, cursor, &mut out);
  out
}

/// A linear position channel from `from` to `to` on x over `seconds`.
fn slide(from: f32, to: f32, seconds: f32) -> ClipChannel {
  channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0, seconds], &[from, 0.0, 0.0, to, 0.0, 0.0])
}

/// A constant position at x.
fn at_x(x: f32) -> ClipChannel {
  channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0], &[x, 0.0, 0.0])
}

fn z_rotation(angle: f32) -> [f32; 4] {
  [0.0, 0.0, (angle / 2.0).sin(), (angle / 2.0).cos()]
}

fn z_angle(q: [f32; 4]) -> f32 {
  2.0 * q[2].atan2(q[3])
}

/// A rig over `count` fresh nodes clocked on the first, all at rest.
fn rig_of(s: &mut Spatial, count: usize) -> (u64, Vec<u64>) {
  let joints: Vec<u64> = (0..count).map(|_| s.create([0.0; 3], Q, ONE, true)).collect();
  let rig = s.create_rig(joints[0], joints.clone()).expect("rig");
  (rig, joints)
}

fn play(s: &mut Spatial, rig: u64, clip: u64, spec: PlayerSpec) -> u64 {
  s.create_player(rig, clip, spec).expect("player")
}

fn advance_at(s: &mut Spatial, ms: f64) -> crate::spatial::PlayersTick {
  s.set_transition_now(ms);
  s.advance_players()
}

fn x_of(s: &Spatial, n: u64) -> f32 {
  s.transform_of(n).expect("read").0[0]
}

#[test]
fn sampling_matches_the_gltf_contract() {
  // Linear position: clamped ends, midpoint lerp.
  let lin = channel(
    ChannelPath::Position,
    ChannelInterpolation::Linear,
    &[0.0, 1.0, 2.0],
    &[0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 10.0, 20.0, 0.0],
  );
  let mut cur = 0;
  assert_eq!(sampled(&lin, -1.0, &mut cur)[..3], [0.0, 0.0, 0.0]);
  assert_eq!(sampled(&lin, 0.5, &mut cur)[..3], [5.0, 0.0, 0.0]);
  assert_eq!(sampled(&lin, 1.5, &mut cur)[..3], [10.0, 10.0, 0.0]);
  assert_eq!(sampled(&lin, 9.0, &mut cur)[..3], [10.0, 20.0, 0.0]);
  // A cursor left at the last pair still answers an earlier time (the
  // loop-wrap seek).
  assert_eq!(sampled(&lin, 0.25, &mut cur)[..3], [2.5, 0.0, 0.0]);
  assert_eq!(cur, 0);

  // Step holds the earlier key.
  let step = channel(ChannelPath::Scale, ChannelInterpolation::Step, &[0.0, 1.0], &[1.0, 1.0, 1.0, 3.0, 3.0, 3.0]);
  let mut cur = 0;
  assert_eq!(sampled(&step, 0.99, &mut cur)[..3], [1.0, 1.0, 1.0]);
  assert_eq!(sampled(&step, 1.0, &mut cur)[..3], [3.0, 3.0, 3.0]);

  // Rotation slerps the short arc: identity to 180 degrees about z at
  // t = 0.5 is 90 degrees (x = 0, y = 0, z = sin 45, w = cos 45).
  let rot = channel(
    ChannelPath::Rotation,
    ChannelInterpolation::Linear,
    &[0.0, 1.0],
    &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0],
  );
  let mut cur = 0;
  let q = sampled(&rot, 0.5, &mut cur);
  let h = (0.5f32).sqrt();
  for (got, want) in q.iter().zip([0.0, 0.0, h, h]) {
    assert!((got - want).abs() < 1e-5, "{q:?}");
  }

  // Cubic: zero tangents at both keys give the smoothstep of the values;
  // at s = 0.5 that is the midpoint.
  let cubic = channel(
    ChannelPath::Position,
    ChannelInterpolation::Cubic,
    &[0.0, 2.0],
    &[
      // key 0: in-tangent, value, out-tangent
      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // key 1
      0.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ],
  );
  let mut cur = 0;
  assert_eq!(sampled(&cubic, 1.0, &mut cur)[..3], [4.0, 0.0, 0.0]);
  // Quarter point of the smoothstep h01(0.25) = -2(1/64) + 3(1/16) = 5/32.
  let q1 = sampled(&cubic, 0.5, &mut cur);
  assert!((q1[0] - 8.0 * (5.0 / 32.0)).abs() < 1e-5, "{q1:?}");
}

#[test]
fn a_player_writes_the_pose_and_settles_when_frozen() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, clip, PlayerSpec::default());
  let tick = advance_at(&mut s, 500.0);
  assert!(tick.active && tick.wrote);
  assert!((x_of(&s, n[0]) - 5.0).abs() < EPS);
  // A frozen clock (same stamp) resamples the same pose: no writes.
  let tick = advance_at(&mut s, 500.0);
  assert!(tick.active && !tick.wrote, "frozen clock must write nothing");
}

#[test]
fn two_players_crossfade_by_weight() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let a = s.create_clip(1.0, vec![at_x(0.0)], vec![]).expect("clip");
  let b = s.create_clip(1.0, vec![at_x(10.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, a, PlayerSpec::default());
  let pb = play(&mut s, rig, b, PlayerSpec { weight: 0.0, ..Default::default() });
  // Equal weights: the average.
  s.set_player(pb, PlayerUpdate { weight: Some(1.0), ..Default::default() }).expect("weight");
  advance_at(&mut s, 16.0);
  assert!((x_of(&s, n[0]) - 5.0).abs() < EPS);
  // Fade b out over 100 ms: at +50 ms its weight is 0.5 against a's 1.0,
  // a total above one, so nothing of the rest shows through.
  s.set_player(pb, PlayerUpdate { fade: Some(-10.0), ..Default::default() }).expect("fade");
  advance_at(&mut s, 66.0);
  let expected = 10.0 * (0.5 / 1.5);
  assert!((x_of(&s, n[0]) - expected).abs() < EPS, "{} vs {expected}", x_of(&s, n[0]));
  // Past zero the fader drops with a Dropped event; a alone remains.
  advance_at(&mut s, 300.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(pb)]);
  assert!(x_of(&s, n[0]).abs() < EPS);
}

#[test]
fn a_base_player_fading_out_alone_fades_to_rest() {
  // The layer floor is the rest pose: a stop-with-fade lands there
  // instead of popping when the player drops.
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![at_x(10.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  advance_at(&mut s, 16.0);
  assert!((x_of(&s, n[0]) - 10.0).abs() < EPS);
  s.set_player(p, PlayerUpdate { fade: Some(-2.0), ..Default::default() }).expect("fade");
  advance_at(&mut s, 266.0);
  assert!((x_of(&s, n[0]) - 5.0).abs() < EPS, "half weight is halfway to rest: {}", x_of(&s, n[0]));
  advance_at(&mut s, 600.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p)]);
  // Gone: the joint holds the last written pose (not rest - nothing
  // feeds it any more).
  let last = x_of(&s, n[0]);
  advance_at(&mut s, 700.0);
  assert_eq!(x_of(&s, n[0]), last);
}

#[test]
fn a_joint_one_clip_lacks_blends_from_rest_in_a_crossfade() {
  // Clip a animates both joints, clip b only the first: on the second
  // joint b contributes rest for its share instead of letting a hold
  // the joint at full weight (no snap when a finally drops).
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 2);
  let a = s.create_clip(1.0, vec![on(0, at_x(10.0)), on(1, at_x(10.0))], vec![]).expect("clip");
  let b = s.create_clip(1.0, vec![on(0, at_x(10.0))], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, a, PlayerSpec { weight: 0.5, ..Default::default() });
  play(&mut s, rig, b, PlayerSpec { weight: 0.5, ..Default::default() });
  advance_at(&mut s, 16.0);
  assert!((x_of(&s, n[0]) - 10.0).abs() < EPS, "both cover joint 0");
  assert!((x_of(&s, n[1]) - 5.0).abs() < EPS, "half of joint 1 is rest: {}", x_of(&s, n[1]));
}

#[test]
fn an_override_layer_replaces_masked_joints_only() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 2);
  let walk = s.create_clip(1.0, vec![on(0, at_x(1.0)), on(1, at_x(1.0))], vec![]).expect("clip");
  let aim = s.create_clip(1.0, vec![on(0, at_x(9.0)), on(1, at_x(9.0))], vec![]).expect("clip");
  let mask = s.create_mask(0.0, vec![(n[1], 1.0)]).expect("mask");
  let upper =
    s.create_layer(rig, LayerSpec { weight: 1.0, mode: LayerMode::Override, mask: Some(mask) }).expect("layer");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, walk, PlayerSpec::default());
  play(&mut s, rig, aim, PlayerSpec { layer: upper, ..Default::default() });
  advance_at(&mut s, 16.0);
  assert!((x_of(&s, n[0]) - 1.0).abs() < EPS, "masked out: the base shows");
  assert!((x_of(&s, n[1]) - 9.0).abs() < EPS, "masked in: the layer replaces");
  // A soft share lerps.
  s.set_mask(mask, 0.0, vec![(n[1], 0.5)]).expect("mask");
  advance_at(&mut s, 32.0);
  assert!((x_of(&s, n[1]) - 5.0).abs() < EPS, "{}", x_of(&s, n[1]));
  // The layer weight ramps in core: half way through a 1 s fade to 0 the
  // layer is at 0.5, so the joint sits at a quarter of the way.
  s.set_layer(rig, upper, LayerUpdate { weight: Some(0.0), fade_ms: 1000.0, mask: None }).expect("layer");
  let tick = advance_at(&mut s, 532.0);
  assert!(tick.active);
  assert!((s.layer_weight(rig, upper).expect("weight") - 0.5).abs() < EPS);
  assert!((x_of(&s, n[1]) - 3.0).abs() < EPS, "{}", x_of(&s, n[1]));
  advance_at(&mut s, 1100.0);
  assert!((x_of(&s, n[1]) - 1.0).abs() < EPS, "a layer at 0 shows the base");
  // A layer at weight 0 is no demand on its own.
  let tick = advance_at(&mut s, 1200.0);
  assert!(tick.active, "the base's looping player still runs");
}

#[test]
fn an_additive_layer_adds_the_authored_clip_minus_its_reference() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let h = z_rotation(30f32.to_radians());
  let base = s
    .create_clip(1.0, vec![at_x(5.0), channel(ChannelPath::Rotation, ChannelInterpolation::Linear, &[0.0], &h)], vec![])
    .expect("base");
  // The flinch as authored: x 1 -> 3, 90 -> 180 degrees about z over 1 s.
  let q90 = z_rotation(90f32.to_radians());
  let q180 = z_rotation(180f32.to_radians());
  let flinch = s
    .create_clip(
      1.0,
      vec![
        slide(1.0, 3.0, 1.0),
        channel(
          ChannelPath::Rotation,
          ChannelInterpolation::Linear,
          &[0.0, 1.0],
          &[q90[0], q90[1], q90[2], q90[3], q180[0], q180[1], q180[2], q180[3]],
        ),
      ],
      vec![],
    )
    .expect("flinch");
  let delta = s.derive_additive(flinch, AdditiveReference::FirstFrame).expect("additive");
  let layer = s.create_layer(rig, LayerSpec { weight: 1.0, mode: LayerMode::Additive, mask: None }).expect("layer");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, base, PlayerSpec::default());
  let pf = play(&mut s, rig, delta, PlayerSpec { layer, looped: false, ..Default::default() });
  // At t = 0 the delta is zero: the base alone.
  advance_at(&mut s, 0.0);
  assert!((x_of(&s, n[0]) - 5.0).abs() < EPS);
  advance_at(&mut s, 500.0);
  let (p, q, _) = s.transform_of(n[0]).expect("read");
  assert!((p[0] - 6.0).abs() < EPS, "{p:?}");
  assert!((z_angle(q) - 75f32.to_radians()).abs() < 1e-4, "30 + 45 degrees, got {}", z_angle(q).to_degrees());
  // Half weight halves the delta: slerped from identity.
  s.set_player(pf, PlayerUpdate { weight: Some(0.5), ..Default::default() }).expect("weight");
  advance_at(&mut s, 1000.0);
  let (p, q, _) = s.transform_of(n[0]).expect("read");
  assert!((p[0] - 6.0).abs() < EPS, "{p:?}");
  assert!((z_angle(q) - 75f32.to_radians()).abs() < 1e-4, "30 + 90 / 2 degrees, got {}", z_angle(q).to_degrees());
  // Against the rig's rest: a constant x = 3 is a delta of 3.
  let three = s.create_clip(1.0, vec![at_x(3.0)], vec![]).expect("three");
  let delta_rest = s.derive_additive(three, AdditiveReference::Rest { rig }).expect("additive");
  s.destroy_player(pf);
  play(&mut s, rig, delta_rest, PlayerSpec { layer, ..Default::default() });
  advance_at(&mut s, 1016.0);
  assert!((x_of(&s, n[0]) - 8.0).abs() < EPS, "{}", x_of(&s, n[0]));
  // A delta clip binds no root motion.
  let pr = play(&mut s, rig, delta, PlayerSpec { layer, ..Default::default() });
  let err = s
    .bind_root_motion(
      pr,
      RootMotion { clip: flinch, channel: 0, rotation: None, anchor: None, up: [0.0, 1.0, 0.0], vertical: true },
    )
    .unwrap_err();
  assert!(err.contains("additive"), "{err}");
}

#[test]
fn the_inertial_quintic_lands_at_zero_and_never_overshoots() {
  let (x, t1) = inertial_offset_for_test(1.0, 0.0, 1.0, 0.0).expect("fit");
  assert!((x - 1.0).abs() < 1e-6 && t1 == 1.0);
  // The curve front-loads the decay (its initial acceleration is
  // -20 x0 / t1^2): past the half way mark less than a fifth is left.
  let (x, _) = inertial_offset_for_test(1.0, 0.0, 1.0, 0.5).expect("fit");
  assert!((x - 0.1875).abs() < 1e-6, "{x}");
  // Monotone: x'(t) = 20 t (t - 1)^3 on 0..1 never rises.
  let mut last = 1.0f32;
  for k in 1..=100 {
    let (x, _) = inertial_offset_for_test(1.0, 0.0, 1.0, k as f32 / 100.0).expect("fit");
    assert!(x <= last + 1e-6, "{k}: {x} after {last}");
    last = x;
  }
  let (x, _) = inertial_offset_for_test(1.0, 0.0, 1.0, 1.0).expect("fit");
  assert!(x.abs() < 1e-6, "{x}");
  // Monotone: never below zero on the way.
  for k in 0..=100 {
    let (x, _) = inertial_offset_for_test(1.0, 0.0, 1.0, k as f32 / 100.0).expect("fit");
    assert!(x >= -1e-6, "{k}: {x}");
  }
  // A velocity toward zero caps the duration at 5 x0 / |v0|.
  let (_, t1) = inertial_offset_for_test(1.0, -1.0, 10.0, 0.0).expect("fit");
  assert!((t1 - 5.0).abs() < 1e-6, "{t1}");
  assert!(inertial_offset_for_test(0.0, 0.0, 1.0, 0.0).is_none(), "no offset, no transition");
}

#[test]
fn an_inertial_switch_is_continuous_and_ends_on_the_new_clip() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let moving = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  let still = s.create_clip(1.0, vec![at_x(1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let pm = play(&mut s, rig, moving, PlayerSpec::default());
  // Two frames of history: the source moves at 10 units per second.
  advance_at(&mut s, 16.0);
  advance_at(&mut s, 32.0);
  let before = x_of(&s, n[0]);
  // The switch: the new clip at full weight, the offset decays.
  s.destroy_player(pm);
  play(&mut s, rig, still, PlayerSpec::default());
  s.inertialize(rig, 1000.0).expect("inertialize");
  let tick = advance_at(&mut s, 48.0);
  assert!(tick.active && tick.wrote);
  let after = x_of(&s, n[0]);
  // Position and velocity carry over: one more 16 ms step at 10 u/s.
  assert!((after - (before + 0.16)).abs() < 0.02, "before {before}, after {after}");
  // The decay ends on the new pose (the cap shortens 1 s to 5 x0 / v0).
  advance_at(&mut s, 2000.0);
  assert!((x_of(&s, n[0]) - 1.0).abs() < EPS, "{}", x_of(&s, n[0]));
  let tick = advance_at(&mut s, 2016.0);
  assert!(!tick.wrote, "settled");
}

#[test]
fn an_inertial_transition_smooths_a_jump_in_the_same_clip() {
  // A seek is a discontinuity like any other: requested with it, the
  // transition hides the jump and lands on the clip.
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  advance_at(&mut s, 100.0);
  advance_at(&mut s, 200.0);
  s.set_player(p, PlayerUpdate { time: Some(0.9), ..Default::default() }).expect("seek");
  s.inertialize(rig, 400.0).expect("inertialize");
  advance_at(&mut s, 216.0);
  let x = x_of(&s, n[0]);
  assert!(x > 2.0 && x < 3.0, "continuous from 2.0 at 10 u/s, got {x}");
  advance_at(&mut s, 700.0);
  // Time 0.9 + 0.5 s wraps to 0.4: the clip's own pose, offset gone.
  assert!((x_of(&s, n[0]) - 4.0).abs() < 1e-3, "{}", x_of(&s, n[0]));
}

#[test]
fn a_fade_in_at_weight_zero_survives_a_frozen_clock() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  // Fading in from 0: a dt-0 advance (frozen clock, or the same-frame
  // create) must NOT cull it as faded out.
  let p = play(&mut s, rig, clip, PlayerSpec { weight: 0.0, fade: 2.5, ..Default::default() });
  advance_at(&mut s, 0.0);
  assert!(s.take_clip_events().is_empty(), "fade-in dropped at weight 0");
  advance_at(&mut s, 400.0);
  assert!(x_of(&s, n[0]) > 0.0, "fade-in never took");
  // The same player fading OUT does drop past zero.
  s.set_player(p, PlayerUpdate { fade: Some(-2.5), ..Default::default() }).expect("fade");
  advance_at(&mut s, 1000.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p)]);
}

#[test]
fn once_clips_finish_hold_and_report_once() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec { looped: false, ..Default::default() });
  advance_at(&mut s, 1500.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Finished(p)]);
  assert!((x_of(&s, n[0]) - 10.0).abs() < EPS);
  // Held: no more events, no more demand, pose stays.
  let tick = advance_at(&mut s, 2000.0);
  assert!(s.take_clip_events().is_empty());
  assert!(!tick.active && !tick.wrote);
  // A time write re-arms it.
  s.set_player(p, PlayerUpdate { time: Some(0.0), ..Default::default() }).expect("rewind");
  let tick = advance_at(&mut s, 2016.0);
  assert!(tick.active && tick.wrote);
  assert!((s.player_time(p).expect("time") - 0.016).abs() < 1e-6);
}

#[test]
fn looping_wraps_continuously_and_reports_the_loop() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  advance_at(&mut s, 900.0);
  assert!(s.take_clip_events().is_empty());
  // 1.15 s wraps to 0.15 s - the cursor seeks backwards correctly.
  advance_at(&mut s, 1150.0);
  assert!((x_of(&s, n[0]) - 1.5).abs() < EPS);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Loop(p)]);
}

#[test]
fn a_subclip_loops_within_its_window_and_finishes_at_its_end() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec { window: Some((0.2, 0.6)), ..Default::default() });
  assert!((s.player_time(p).expect("time") - 0.2).abs() < 1e-9, "starts at the window");
  advance_at(&mut s, 300.0);
  assert!((x_of(&s, n[0]) - 5.0).abs() < EPS);
  advance_at(&mut s, 500.0);
  assert!((x_of(&s, n[0]) - 3.0).abs() < EPS, "0.7 wraps to 0.3: {}", x_of(&s, n[0]));
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Loop(p)]);
  // A seek clamps to the window.
  s.set_player(p, PlayerUpdate { time: Some(0.9), ..Default::default() }).expect("seek");
  assert!((s.player_time(p).expect("time") - 0.6).abs() < 1e-9);
  // Non-looping: finishes at the window's end.
  s.destroy_player(p);
  let p = play(&mut s, rig, clip, PlayerSpec { window: Some((0.2, 0.6)), looped: false, ..Default::default() });
  advance_at(&mut s, 1500.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Finished(p)]);
  assert!((x_of(&s, n[0]) - 6.0).abs() < EPS);
  // An invalid window is refused.
  assert!(s.create_player(rig, clip, PlayerSpec { window: Some((0.5, 0.2)), ..Default::default() }).is_err());
  assert!(s.create_player(rig, clip, PlayerSpec { window: Some((0.0, 2.0)), ..Default::default() }).is_err());
}

#[test]
fn a_one_shot_fades_out_before_its_end_then_finishes_and_drops() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec { looped: false, fade_out: 0.2, ..Default::default() });
  advance_at(&mut s, 800.0);
  assert!((x_of(&s, n[0]) - 8.0).abs() < EPS, "full until the fade starts");
  advance_at(&mut s, 900.0);
  assert!((s.player_weight(p).expect("weight") - 0.5).abs() < 1e-3);
  assert!((x_of(&s, n[0]) - 4.5).abs() < 1e-3, "halfway to rest: {}", x_of(&s, n[0]));
  advance_at(&mut s, 1000.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Finished(p), ClipEvent::Dropped(p)]);
}

#[test]
fn markers_fire_per_crossing_over_wraps_and_in_reverse() {
  let mut s = Spatial::new();
  let (rig, _) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  s.set_clip_markers(
    clip,
    vec![
      ClipMarker { time: 0.5, duration: 0.0, name: "b".into() },
      ClipMarker { time: 0.0, duration: 0.0, name: "a".into() },
      ClipMarker { time: 0.3, duration: 0.4, name: "w".into() },
    ],
  )
  .expect("markers");
  // Sorted: a (0), w (1), b (2).
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  let marker =
    |marker: usize, phase: MarkerPhase| ClipEvent::Marker { player: p, marker, phase, weight: 1.0, layer: 0 };
  advance_at(&mut s, 100.0);
  assert_eq!(
    s.take_clip_events(),
    vec![marker(0, MarkerPhase::Instant)],
    "a marker at the start fires on the first advance"
  );
  advance_at(&mut s, 600.0);
  assert_eq!(s.take_clip_events(), vec![marker(1, MarkerPhase::Begin), marker(2, MarkerPhase::Instant)]);
  assert!(s.marker_inside(p, 1).expect("inside"));
  // A wrap: the window closes at 0.7, then the start marker fires again.
  advance_at(&mut s, 1200.0);
  assert_eq!(
    s.take_clip_events(),
    vec![ClipEvent::Loop(p), marker(1, MarkerPhase::End), marker(0, MarkerPhase::Instant)]
  );
  assert!(!s.marker_inside(p, 1).expect("inside"));
  // Reverse: 0.2 back to 0.9 crosses the start marker on the way out.
  s.set_player(p, PlayerUpdate { speed: Some(-1.0), ..Default::default() }).expect("speed");
  advance_at(&mut s, 1500.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Loop(p), marker(0, MarkerPhase::Instant)]);
  // Backwards into the window from its far edge.
  advance_at(&mut s, 1800.0);
  assert_eq!(s.take_clip_events(), vec![marker(1, MarkerPhase::Begin)]);
  // A seek fires nothing it jumps over, but a window is a state: the
  // one it leaves ends at once (and one it lands in would begin), told
  // to the caller by the write itself. A destroy tells the same.
  let changes =
    s.set_player(p, PlayerUpdate { time: Some(0.1), speed: Some(1.0), ..Default::default() }).expect("seek");
  assert_eq!(changes, vec![(1, MarkerPhase::End)], "a seek out of a window ends it, reported at once");
  assert!(s.take_clip_events().is_empty());
  assert!(!s.marker_inside(p, 1).expect("inside"));
  advance_at(&mut s, 1816.0);
  assert_eq!(s.take_clip_events(), vec![]);
  // Inside again, then faded out mid-window: the window ends with the
  // drop, before it.
  advance_at(&mut s, 2116.0);
  assert_eq!(s.take_clip_events(), vec![marker(1, MarkerPhase::Begin)]);
  s.set_player(p, PlayerUpdate { fade: Some(-100.0), ..Default::default() }).expect("fade");
  advance_at(&mut s, 2132.0);
  let events = s.take_clip_events();
  assert_eq!(events.len(), 2);
  assert!(matches!(events[0], ClipEvent::Marker { marker: 1, phase: MarkerPhase::End, .. }), "{events:?}");
  assert_eq!(events[1], ClipEvent::Dropped(p));
  // Destroyed inside a window: the destroy names the windows it closed.
  let p = play(&mut s, rig, clip, PlayerSpec { time: 0.4, ..Default::default() });
  advance_at(&mut s, 2148.0);
  assert_eq!(s.destroy_player(p), vec![1]);
  assert!(s.take_clip_events().iter().all(|e| !matches!(e, ClipEvent::Dropped(_))), "a destroy is silent");
}

#[test]
fn marker_events_carry_the_effective_weight_and_layer() {
  let mut s = Spatial::new();
  let (rig, _) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  s.set_clip_markers(clip, vec![ClipMarker { time: 0.5, duration: 0.0, name: "hit".into() }]).expect("markers");
  let layer = s.create_layer(rig, LayerSpec { weight: 0.5, mode: LayerMode::Override, mask: None }).expect("layer");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec { layer, weight: 0.5, ..Default::default() });
  advance_at(&mut s, 600.0);
  assert_eq!(
    s.take_clip_events(),
    vec![ClipEvent::Marker { player: p, marker: 0, phase: MarkerPhase::Instant, weight: 0.25, layer }]
  );
}

#[test]
fn the_marker_walk_handles_long_steps_and_windows_across_the_wrap() {
  let markers = vec![
    ClipMarker { time: 0.1, duration: 0.0, name: "a".into() },
    ClipMarker { time: 0.8, duration: 0.5, name: "w".into() },
  ];
  let mut inside = vec![false; 2];
  let mut out = Vec::new();
  // One long step over two loops' worth: a fires once per segment, the
  // window opens at 0.8 and closes at the clip end (clamped), each
  // once.
  marker_crossings(&markers, 0.5, 0.5, true, Some((0.0, 1.0)), (0.0, 1.0), false, &mut inside, &mut out);
  assert_eq!(out, vec![(1, MarkerPhase::Begin), (1, MarkerPhase::End), (0, MarkerPhase::Instant)], "{out:?}");
  // Outside the window of a subclip nothing fires.
  out.clear();
  marker_crossings(&markers, 0.2, 0.9, true, None, (0.2, 0.7), false, &mut inside, &mut out);
  assert!(out.is_empty(), "{out:?}");
}

#[test]
fn a_sync_group_follows_its_heaviest_member() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let walk = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("walk");
  let run = s.create_clip(0.5, vec![slide(0.0, 10.0, 0.5)], vec![]).expect("run");
  advance_at(&mut s, 0.0);
  let pw = play(&mut s, rig, walk, PlayerSpec::default());
  let pr = play(&mut s, rig, run, PlayerSpec { weight: 0.2, ..Default::default() });
  let group = s.create_group(rig, vec![pw, pr], SyncMode::Time).expect("group");
  advance_at(&mut s, 250.0);
  assert!((s.player_time(pw).expect("t") - 0.25).abs() < 1e-6, "the walk leads");
  assert!((s.player_time(pr).expect("t") - 0.125).abs() < 1e-6, "the run follows at the same phase");
  // Both clips are at the same phase, so the blend has no foot-slide:
  // the two samples agree.
  assert!((x_of(&s, n[0]) - 2.5).abs() < EPS);
  // The lead changes with the weights; the new leader goes on from its
  // own time.
  s.set_player(pw, PlayerUpdate { weight: Some(0.2), ..Default::default() }).expect("w");
  s.set_player(pr, PlayerUpdate { weight: Some(1.0), ..Default::default() }).expect("w");
  advance_at(&mut s, 375.0);
  assert!((s.player_time(pr).expect("t") - 0.25).abs() < 1e-6);
  assert!((s.player_time(pw).expect("t") - 0.5).abs() < 1e-6);
  // A wrap of the leader wraps the follower, both reporting a loop.
  advance_at(&mut s, 675.0);
  assert!((s.player_time(pr).expect("t") - 0.05).abs() < 1e-6, "{}", s.player_time(pr).expect("t"));
  assert!((s.player_time(pw).expect("t") - 0.1).abs() < 1e-6);
  let events = s.take_clip_events();
  assert!(events.contains(&ClipEvent::Loop(pr)) && events.contains(&ClipEvent::Loop(pw)), "{events:?}");
  // Dissolved: each keeps its own time again.
  s.destroy_group(group).expect("destroy");
  advance_at(&mut s, 775.0);
  assert!((s.player_time(pr).expect("t") - 0.15).abs() < 1e-6);
  assert!((s.player_time(pw).expect("t") - 0.2).abs() < 1e-6);
}

#[test]
fn a_marker_synced_group_aligns_common_markers() {
  let mut s = Spatial::new();
  let (rig, _) = rig_of(&mut s, 1);
  let walk = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("walk");
  let run = s.create_clip(0.5, vec![slide(0.0, 10.0, 0.5)], vec![]).expect("run");
  s.set_clip_markers(
    walk,
    vec![
      ClipMarker { time: 0.2, duration: 0.0, name: "L".into() },
      ClipMarker { time: 0.7, duration: 0.0, name: "R".into() },
    ],
  )
  .expect("markers");
  s.set_clip_markers(
    run,
    vec![
      ClipMarker { time: 0.05, duration: 0.0, name: "L".into() },
      ClipMarker { time: 0.3, duration: 0.0, name: "R".into() },
      ClipMarker { time: 0.4, duration: 0.0, name: "extra".into() },
    ],
  )
  .expect("markers");
  advance_at(&mut s, 0.0);
  let pw = play(&mut s, rig, walk, PlayerSpec::default());
  let pr = play(&mut s, rig, run, PlayerSpec { weight: 0.5, ..Default::default() });
  s.create_group(rig, vec![pw, pr], SyncMode::Markers).expect("group");
  // The walk halfway between L and R: the run halfway between ITS L and
  // R (0.175), not at the walk's normalized time (0.225).
  s.set_player(pw, PlayerUpdate { time: Some(0.45), ..Default::default() }).expect("seek");
  advance_at(&mut s, 0.0);
  assert!((s.player_time(pr).expect("t") - 0.175).abs() < 1e-6, "{}", s.player_time(pr).expect("t"));
  // Past the last marker the segment wraps to the first: 0.85 is half
  // way from R (0.7) to L + 1 (1.2); the run: half way from 0.3 to 0.55,
  // i.e. 0.425.
  s.set_player(pw, PlayerUpdate { time: Some(0.95), ..Default::default() }).expect("seek");
  advance_at(&mut s, 0.0);
  assert!((s.player_time(pr).expect("t") - 0.425).abs() < 1e-6, "{}", s.player_time(pr).expect("t"));
  // The pure mapping, non-looping: the edges bound the outer segments.
  let t = sequence_for_test(&[0.2, 0.7], 0.1, false, (0.0, 1.0), &[0.05, 0.3]);
  assert!((t - 0.025).abs() < 1e-6, "{t}");
}

#[test]
fn the_time_step_honours_windows_and_direction() {
  let (t, wrapped, reached) = step_time_for_test(0.5, 0.7, 1.0, true, (0.0, 1.0));
  assert!((t - 0.2).abs() < 1e-9 && wrapped && !reached);
  assert_eq!(step_time_for_test(0.5, 0.7, 1.0, false, (0.0, 1.0)), (1.0, false, true));
  let (t, wrapped, reached) = step_time_for_test(0.3, 0.2, -1.0, false, (0.0, 1.0));
  assert!((t - 0.1).abs() < 1e-9 && !wrapped && !reached);
  assert_eq!(step_time_for_test(0.1, 0.2, -1.0, false, (0.0, 1.0)), (0.0, false, true));
  let (t, wrapped, _) = step_time_for_test(0.25, 0.1, -1.0, true, (0.2, 0.6));
  assert!((t - 0.55).abs() < 1e-9 && wrapped);
}

#[test]
fn dead_joints_and_dead_clips_drop() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 2);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  // A destroyed clip drops its player, the rig stays.
  s.destroy_clip(clip).expect("destroy clip");
  advance_at(&mut s, 16.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p)]);
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  // A dead joint drops the rig and everything on it.
  s.destroy(n[1]).expect("destroy");
  let tick = advance_at(&mut s, 32.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p), ClipEvent::RigDropped(rig)]);
  assert!(!tick.active);
  assert!(s.create_player(rig, clip, PlayerSpec::default()).is_err(), "the rig is gone");
  // Creation-time validation: a channel past the joint table, a dead
  // joint, a dead clock, a mismatched rest table.
  let (rig2, n2) = rig_of(&mut s, 1);
  let wide = s.create_clip(1.0, vec![on(3, slide(0.0, 1.0, 1.0))], vec![]).expect("clip");
  assert!(s.create_player(rig2, wide, PlayerSpec::default()).is_err());
  assert!(s.create_rig(n2[0], vec![n[1]]).is_err(), "dead joint must fail at create");
  assert!(s.create_rig(n[1], vec![n2[0]]).is_err(), "dead clock must fail at create");
}

/// A root that walks +x 10 units while turning 90 degrees about +y over
/// 1 s: channel 0 its position, channel 1 its rotation.
fn walk_and_turn_clip(s: &mut Spatial) -> u64 {
  let h = (0.5f32).sqrt();
  let rot =
    channel(ChannelPath::Rotation, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 0.0, 1.0, 0.0, h, 0.0, h]);
  s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0), rot], vec![]).expect("clip")
}

fn yaw_about_y(q: [f32; 4]) -> f32 {
  2.0 * q[1].atan2(q[3])
}

#[test]
fn root_motion_moves_and_turns_the_anchor_continuously() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let anchor = s.create([0.0; 3], Q, ONE, true);
  let clip = walk_and_turn_clip(&mut s);
  advance_at(&mut s, 0.0);
  let player = play(&mut s, rig, clip, PlayerSpec::default());
  s.bind_root_motion(
    player,
    RootMotion { clip, channel: 0, rotation: Some(1), anchor: Some(anchor), up: [0.0, 1.0, 0.0], vertical: true },
  )
  .expect("bind");

  // Binding primes at the player's time: the first advance already
  // delivers the travel, in the clip's frame (+x) since the anchor
  // started aligned with it.
  advance_at(&mut s, 500.0);
  let (p, q, _) = s.transform_of(anchor).expect("read");
  assert!((p[0] - 5.0).abs() < 0.05 && p[2].abs() < 0.2, "{p:?}");
  assert!((yaw_about_y(q) - std::f32::consts::FRAC_PI_4).abs() < 1e-3, "{q:?}");
  let reports = s.take_root_motion();
  assert_eq!(reports.len(), 1);
  assert!((reports[0].2 - std::f32::consts::FRAC_PI_4).abs() < 1e-3, "{reports:?}");

  // Across the loop wrap the walk and the turn both continue: 1.5 s in,
  // 15 units along the clip's +x and 135 degrees.
  for ms in [1000.0, 1250.0, 1500.0] {
    advance_at(&mut s, ms);
  }
  let (p, q, _) = s.transform_of(anchor).expect("read");
  assert!((p[0] - 15.0).abs() < 0.3 && p[2].abs() < 0.5, "{p:?}");
  assert!((yaw_about_y(q) - 3.0 * std::f32::consts::FRAC_PI_4).abs() < 1e-2, "{q:?}");
  let _ = n;
}

#[test]
fn root_motion_without_vertical_keeps_the_rise_out_of_the_delta() {
  let mut s = Spatial::new();
  let (rig, _) = rig_of(&mut s, 1);
  let clip = s
    .create_clip(
      1.0,
      vec![channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 0.0, 4.0, 3.0, 0.0])],
      vec![],
    )
    .expect("clip");
  advance_at(&mut s, 0.0);
  let player = play(&mut s, rig, clip, PlayerSpec { looped: false, ..Default::default() });
  s.bind_root_motion(
    player,
    RootMotion { clip, channel: 0, rotation: None, anchor: None, up: [0.0, 1.0, 0.0], vertical: false },
  )
  .expect("bind");
  advance_at(&mut s, 500.0);
  let reports = s.take_root_motion();
  assert_eq!(reports.len(), 1);
  let d = reports[0].1;
  assert!((d[0] - 2.0).abs() < 1e-5 && d[1].abs() < 1e-6, "{d:?}");
}

#[test]
fn root_travel_follows_the_effective_weight_through_layer_and_mask() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 2);
  let clip =
    s.create_clip(1.0, vec![on(0, slide(0.0, 10.0, 1.0)), on(1, slide(0.0, 10.0, 1.0))], vec![]).expect("clip");
  // An upper-body layer whose mask leaves the root (joint 0) out.
  let mask = s.create_mask(1.0, vec![(n[0], 0.0)]).expect("mask");
  let upper =
    s.create_layer(rig, LayerSpec { weight: 0.5, mode: LayerMode::Override, mask: Some(mask) }).expect("layer");
  advance_at(&mut s, 0.0);
  let pb = play(&mut s, rig, clip, PlayerSpec::default());
  let pu = play(&mut s, rig, clip, PlayerSpec { layer: upper, ..Default::default() });
  let bind = |s: &mut Spatial, p: u64| {
    s.bind_root_motion(
      p,
      RootMotion { clip, channel: 0, rotation: None, anchor: None, up: [0.0, 1.0, 0.0], vertical: true },
    )
    .expect("bind")
  };
  bind(&mut s, pb);
  bind(&mut s, pu);
  advance_at(&mut s, 500.0);
  let reports = s.take_root_motion();
  assert_eq!(reports.len(), 2);
  let of = |p: u64| reports.iter().find(|r| r.0 == p).expect("report").1[0];
  assert!((of(pb) - 5.0).abs() < 1e-4, "the base keeps the travel: {}", of(pb));
  assert!(of(pu).abs() < 1e-6, "the masked-out root contributes none: {}", of(pu));
  // The layer weight scales it once the mask lets the root through.
  s.set_mask(mask, 1.0, vec![]).expect("mask");
  advance_at(&mut s, 1000.0);
  let reports = s.take_root_motion();
  let of = |p: u64| reports.iter().find(|r| r.0 == p).expect("report").1[0];
  assert!((of(pu) - 2.5).abs() < 1e-4, "half the layer, half the travel: {}", of(pu));
}

#[test]
fn weights_channels_blend_into_the_register_over_the_rest_weights() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  s.set_weights(n, &[0.0, 0.0]).expect("weights");
  let rig = s.create_rig(n, vec![n]).expect("rig");
  // Two targets over 1 s: [0, 0] -> [1, 0]; a second, constant clip at
  // [0, 1]; a third, one target wide, at [4].
  let a = s
    .create_clip(
      1.0,
      vec![channel(ChannelPath::Weights, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 1.0, 0.0])],
      vec![],
    )
    .expect("clip a");
  let b = s
    .create_clip(1.0, vec![channel(ChannelPath::Weights, ChannelInterpolation::Linear, &[0.0], &[0.0, 1.0])], vec![])
    .expect("clip b");
  let c = s
    .create_clip(1.0, vec![channel(ChannelPath::Weights, ChannelInterpolation::Step, &[0.0], &[4.0])], vec![])
    .expect("clip c");
  // The path fixes nothing about the width, but a TRS path does.
  let bad = ClipChannel {
    target_slot: 0,
    path: ChannelPath::Position,
    interpolation: ChannelInterpolation::Linear,
    elements: 2,
    times: vec![0.0],
    values: vec![0.0, 0.0],
  };
  assert!(s.create_clip(1.0, vec![bad], vec![]).unwrap_err().contains("elements per key"));
  advance_at(&mut s, 0.0);
  play(&mut s, rig, a, PlayerSpec::default());
  let tick = advance_at(&mut s, 500.0);
  assert!(tick.active && tick.wrote);
  assert_eq!(s.weights_of(n).expect("read"), &[0.5, 0.0][..]);
  // A frozen clock rewrites nothing.
  assert!(!advance_at(&mut s, 500.0).wrote, "frozen clock must write nothing");
  // A second player at equal weight: the average, target by target.
  let pb = play(&mut s, rig, b, PlayerSpec::default());
  advance_at(&mut s, 500.0);
  assert_eq!(s.weights_of(n).expect("read"), &[0.25, 0.5][..]);
  // A narrower clip leaves the rest weight (0) for the target it lacks,
  // at its share; the register keeps the widest width.
  s.destroy_player(pb);
  let pc = play(&mut s, rig, c, PlayerSpec::default());
  advance_at(&mut s, 500.0);
  assert_eq!(s.weights_of(n).expect("read"), &[2.25, 0.0][..]);
  // Gone, the register keeps the last blend until something writes it.
  s.destroy_player(pc);
  advance_at(&mut s, 500.0);
  assert_eq!(s.weights_of(n).expect("read"), &[0.5, 0.0][..]);
}

#[test]
fn curves_feed_the_register_and_add_on_additive_layers() {
  let mut s = Spatial::new();
  let (rig, _) = rig_of(&mut s, 1);
  let curve = |t: &[f32], v: &[f32]| ClipChannel {
    target_slot: 0,
    path: ChannelPath::Curve,
    interpolation: ChannelInterpolation::Linear,
    elements: 1,
    times: t.to_vec(),
    values: v.to_vec(),
  };
  let a = s.create_clip(1.0, vec![curve(&[0.0, 1.0], &[0.0, 1.0])], vec!["ikWeight".into()]).expect("clip");
  assert!(s.create_clip(1.0, vec![curve(&[0.0], &[0.0])], vec![]).is_err(), "a curve needs its name");
  assert_eq!(s.curve_value(rig, "ikWeight").expect("read"), None);
  advance_at(&mut s, 0.0);
  play(&mut s, rig, a, PlayerSpec::default());
  advance_at(&mut s, 500.0);
  assert_eq!(s.curve_value(rig, "ikWeight").expect("read"), Some(0.5));
  assert_eq!(s.curve_value(rig, "other").expect("read"), None);
  // An additive curve adds its delta against its first key.
  let b = s.create_clip(1.0, vec![curve(&[0.0, 1.0], &[0.0, 0.5])], vec!["ikWeight".into()]).expect("clip");
  let delta = s.derive_additive(b, AdditiveReference::FirstFrame).expect("additive");
  let layer = s.create_layer(rig, LayerSpec { weight: 1.0, mode: LayerMode::Additive, mask: None }).expect("layer");
  play(&mut s, rig, delta, PlayerSpec { layer, time: 0.5, looped: false, ..Default::default() });
  advance_at(&mut s, 500.0);
  assert!((s.curve_value(rig, "ikWeight").expect("read").expect("fed") - 0.75).abs() < 1e-6);
}

#[test]
fn a_rig_runs_at_its_clock_nodes_rate() {
  // The rig's one time follows the node it is clocked on (the model's
  // root for a mixer), not any of its joints.
  let mut s = Spatial::new();
  let model = s.create([0.0; 3], Q, ONE, true);
  let n = s.create([0.0; 3], Q, ONE, true);
  s.set_parent(n, Some(model)).expect("parent");
  let rig = s.create_rig(model, vec![n]).expect("rig");
  let clip = s.create_clip(1.0, vec![slide(0.0, 10.0, 1.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  advance_at(&mut s, 500.0);
  s.set_time_scale(model, Some(0.0)).expect("scale");
  let tick = advance_at(&mut s, 1000.0);
  assert!(!tick.active && !tick.wrote, "a frozen rig is neither demand nor a write");
  assert!((x_of(&s, n) - 5.0).abs() < EPS, "held");
  // Half rate: 200 ms of app time move the clip 100 ms.
  s.set_time_scale(model, Some(0.5)).expect("scale");
  let tick = advance_at(&mut s, 1200.0);
  assert!(tick.active && tick.wrote);
  assert!((x_of(&s, n) - 6.0).abs() < EPS);
  // A fade counts on the same time: 0.5 s of app time at half rate.
  s.set_player(p, PlayerUpdate { fade: Some(-1.0), ..Default::default() }).expect("fade");
  advance_at(&mut s, 1700.0);
  assert!(s.take_clip_events().is_empty(), "weight 0.75: still fading");
  advance_at(&mut s, 3300.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p)], "faded out on the node's time");
  // A dead clock drops the rig like a dead joint.
  let p2 = play(&mut s, rig, clip, PlayerSpec::default());
  s.set_time_scale(model, None).expect("scale");
  s.set_parent(n, None).expect("parent");
  s.destroy(model).expect("destroy");
  advance_at(&mut s, 3400.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p2), ClipEvent::RigDropped(rig)]);
}

#[test]
fn destroying_a_layer_drops_its_players_and_reindexes_the_rest() {
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 1);
  let clip = s.create_clip(1.0, vec![at_x(10.0)], vec![]).expect("clip");
  let l1 = s.create_layer(rig, LayerSpec { weight: 1.0, mode: LayerMode::Override, mask: None }).expect("layer");
  let l2 = s.create_layer(rig, LayerSpec { weight: 1.0, mode: LayerMode::Additive, mask: None }).expect("layer");
  advance_at(&mut s, 0.0);
  let p1 = play(&mut s, rig, clip, PlayerSpec { layer: l1, ..Default::default() });
  let p2 = play(&mut s, rig, clip, PlayerSpec { layer: l2, ..Default::default() });
  advance_at(&mut s, 16.0);
  assert!((x_of(&s, n[0]) - 20.0).abs() < EPS, "override 10 plus additive 10 (a raw clip adds as is)");
  assert!(s.destroy_layer(rig, 0).is_err(), "the base stays");
  s.destroy_layer(rig, l1).expect("destroy");
  assert!(s.set_player(p1, PlayerUpdate::default()).is_err(), "gone with its layer");
  // p2 moved down to index 1 and still plays additively over rest.
  s.set_player(p2, PlayerUpdate::default()).expect("still there");
  advance_at(&mut s, 32.0);
  assert!((x_of(&s, n[0]) - 10.0).abs() < EPS, "{}", x_of(&s, n[0]));
}

#[test]
fn a_finished_additive_one_shot_holds_its_delta_over_a_finished_base() {
  // The mixer's app test shape: a one-key base (duration 0) and a short
  // additive clip, stepped at frame rate past both ends.
  let mut s = Spatial::new();
  let (rig, n) = rig_of(&mut s, 2);
  let base = s.create_clip(0.0, vec![on(1, at_x(4.0))], vec![]).expect("base");
  let flinch = s.create_clip(0.2, vec![on(1, slide(1.0, 3.0, 0.2))], vec![]).expect("flinch");
  let delta = s.derive_additive(flinch, AdditiveReference::FirstFrame).expect("additive");
  let hit = s.create_layer(rig, LayerSpec { weight: 1.0, mode: LayerMode::Additive, mask: None }).expect("layer");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, base, PlayerSpec { looped: false, ..Default::default() });
  advance_at(&mut s, 16.0);
  assert!((x_of(&s, n[1]) - 4.0).abs() < EPS);
  play(&mut s, rig, delta, PlayerSpec { layer: hit, looped: false, ..Default::default() });
  let mut ms = 16.0;
  while ms < 400.0 {
    ms += 16.0;
    advance_at(&mut s, ms);
  }
  assert!((x_of(&s, n[1]) - 6.0).abs() < 1e-3, "{}", x_of(&s, n[1]));
}

#[test]
fn the_layers_compose_over_the_nodes_declared_rest() {
  // A joint declared at rest x = 2 while standing at 0: a fade-out lands
  // on the declaration, not on where the node stood.
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  s.set_rest(n, Some(RestPose { position: [2.0, 0.0, 0.0], rotation: Q, scale: ONE })).expect("rest");
  assert!(
    s.set_rest(n, Some(RestPose { position: [0.0; 3], rotation: [0.0; 4], scale: ONE })).is_err(),
    "a zero rotation is no rest"
  );
  let rig = s.create_rig(n, vec![n]).expect("rig");
  let clip = s.create_clip(1.0, vec![at_x(10.0)], vec![]).expect("clip");
  advance_at(&mut s, 0.0);
  let p = play(&mut s, rig, clip, PlayerSpec::default());
  s.set_player(p, PlayerUpdate { fade: Some(-2.0), ..Default::default() }).expect("fade");
  advance_at(&mut s, 250.0);
  assert!((x_of(&s, n) - 6.0).abs() < EPS, "halfway between the clip and the declared rest: {}", x_of(&s, n));
  advance_at(&mut s, 600.0);
  assert!((x_of(&s, n) - 2.0).abs() < EPS, "{}", x_of(&s, n));
}

/// Records curve param writes; a target in `dead` does not land.
#[derive(Default)]
struct ParamRecorder {
  writes: Vec<(u64, Option<u64>, String, f32)>,
  dead: Vec<u64>,
}

impl SinkWriter for ParamRecorder {
  fn write_params(&mut self, _: u64, _: u64, _: &crate::spatial::Mat4, _: Option<&crate::spatial::Mat4>) -> bool {
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
  fn write_order_direction(&mut self, _: u64, _: u64, _: [f32; 3]) -> bool {
    true
  }
  fn write_shared(&mut self, _: u64, _: &str, _: &[f32]) -> bool {
    true
  }
  fn write_param(&mut self, target: u64, draw: Option<u64>, name: &str, value: f32) -> bool {
    self.writes.push((target, draw, name.to_string(), value));
    !self.dead.contains(&target)
  }
  fn write_instances(&mut self, _: u64, _: u32, _: u32, _: &[f32]) -> bool {
    true
  }
  fn write_texture(&mut self, _: u64, _: &[f32]) -> bool {
    true
  }
}

#[test]
fn a_bound_curve_writes_its_param_at_the_flush_when_it_changes() {
  let mut s = Spatial::new();
  let (rig, _) = rig_of(&mut s, 1);
  let curve = ClipChannel {
    target_slot: 0,
    path: ChannelPath::Curve,
    interpolation: ChannelInterpolation::Linear,
    elements: 1,
    times: vec![0.0, 1.0],
    values: vec![0.0, 1.0],
  };
  let clip = s.create_clip(1.0, vec![curve], vec!["glow".into()]).expect("clip");
  // Bound before any clip carries the curve: writes once one does.
  s.bind_curve_param(rig, "glow", 7, Some(3), "uGlow".into()).expect("bind");
  s.bind_curve_param(rig, "glow", 9, None, "uSceneGlow".into()).expect("bind");
  let mut out = ParamRecorder::default();
  s.flush(&mut out);
  assert!(out.writes.is_empty(), "nothing fed yet");
  advance_at(&mut s, 0.0);
  play(&mut s, rig, clip, PlayerSpec { looped: false, ..Default::default() });
  advance_at(&mut s, 500.0);
  s.flush(&mut out);
  assert_eq!(out.writes, vec![(7, Some(3), "uGlow".to_string(), 0.5), (9, None, "uSceneGlow".to_string(), 0.5)]);
  // A frozen value writes nothing more; a change writes again.
  out.writes.clear();
  s.flush(&mut out);
  assert!(out.writes.is_empty());
  advance_at(&mut s, 750.0);
  s.flush(&mut out);
  assert_eq!(out.writes.len(), 2);
  assert!((out.writes[0].3 - 0.75).abs() < 1e-6);
  // A dead target drops its binding; an unbind drops the other.
  out.dead.push(7);
  out.writes.clear();
  advance_at(&mut s, 1000.0);
  s.flush(&mut out);
  assert_eq!(out.writes.len(), 2);
  s.unbind_curve_param(rig, "glow", 9, None).expect("unbind");
  out.writes.clear();
  advance_at(&mut s, 1000.0);
  let finished = s
    .take_clip_events()
    .iter()
    .find_map(|e| if let ClipEvent::Finished(p) = e { Some(*p) } else { None })
    .expect("finished");
  s.set_player(finished, PlayerUpdate { time: Some(0.25), ..Default::default() }).expect("seek");
  advance_at(&mut s, 1016.0);
  s.flush(&mut out);
  assert!(out.writes.is_empty(), "both bindings gone: {:?}", out.writes);
}
