use crate::spatial::{
  sample_channel, ChannelInterpolation, ChannelPath, ClipChannel, ClipEvent, PlayerUpdate, RootMotion, Spatial,
};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];

fn channel(path: ChannelPath, interpolation: ChannelInterpolation, times: &[f32], values: &[f32]) -> ClipChannel {
  ClipChannel { target_slot: 0, path, interpolation, times: times.to_vec(), values: values.to_vec() }
}

fn sampled(c: &ClipChannel, time: f32, cursor: &mut u32) -> [f32; 4] {
  let mut out = [0.0f32; 4];
  sample_channel(c, time, cursor, &mut out);
  out
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

/// A one-channel linear position clip from x = 0 to x = 10 over 1 s.
fn slide_clip(s: &mut Spatial) -> u64 {
  s.create_clip(
    1.0,
    vec![channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 0.0, 10.0, 0.0, 0.0])],
  )
  .expect("clip")
}

fn advance_at(s: &mut Spatial, ms: f64) -> crate::spatial::PlayersTick {
  s.set_transition_now(ms);
  s.advance_players()
}

#[test]
fn a_player_writes_the_pose_and_settles_when_frozen() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let clip = slide_clip(&mut s);
  advance_at(&mut s, 0.0);
  s.create_player(clip, vec![n], 1.0, true, 1.0, 0.0).expect("player");
  let tick = advance_at(&mut s, 500.0);
  assert!(tick.active && tick.wrote);
  let (p, _, _) = s.transform_of(n).expect("read");
  assert!((p[0] - 5.0).abs() < 1e-5, "{p:?}");
  // A frozen clock (same stamp) resamples the same pose: no writes.
  let tick = advance_at(&mut s, 500.0);
  assert!(tick.active && !tick.wrote, "frozen clock must write nothing");
}

#[test]
fn two_players_crossfade_by_weight() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  // Two constant single-key clips at x = 0 and x = 10.
  let a = s
    .create_clip(1.0, vec![channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0], &[0.0, 0.0, 0.0])])
    .expect("clip");
  let b = s
    .create_clip(1.0, vec![channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0], &[10.0, 0.0, 0.0])])
    .expect("clip");
  advance_at(&mut s, 0.0);
  s.create_player(a, vec![n], 1.0, true, 1.0, 0.0).expect("player");
  let pb = s.create_player(b, vec![n], 1.0, true, 0.0, 0.0).expect("player");
  // Equal weights: the average.
  s.set_player(pb, PlayerUpdate { weight: Some(1.0), ..Default::default() }).expect("weight");
  advance_at(&mut s, 16.0);
  let (p, _, _) = s.transform_of(n).expect("read");
  assert!((p[0] - 5.0).abs() < 1e-5, "{p:?}");
  // Fade b out over 100 ms: at +50 ms its weight is 0.5 against a's 1.0.
  s.set_player(pb, PlayerUpdate { fade: Some(-10.0), ..Default::default() }).expect("fade");
  advance_at(&mut s, 66.0);
  let (p, _, _) = s.transform_of(n).expect("read");
  let expected = 10.0 * (0.5 / 1.5);
  assert!((p[0] - expected).abs() < 1e-4, "{} vs {expected}", p[0]);
  // Past zero the fader drops with a Dropped event; a alone remains.
  advance_at(&mut s, 300.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(pb)]);
  let (p, _, _) = s.transform_of(n).expect("read");
  assert!(p[0].abs() < 1e-5, "{p:?}");
}

#[test]
fn a_fade_in_at_weight_zero_survives_a_frozen_clock() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let clip = slide_clip(&mut s);
  advance_at(&mut s, 0.0);
  // Fading in from 0: a dt-0 advance (frozen clock, or the same-frame
  // create) must NOT cull it as faded out.
  let p = s.create_player(clip, vec![n], 1.0, true, 0.0, 2.5).expect("player");
  advance_at(&mut s, 0.0);
  assert!(s.take_clip_events().is_empty(), "fade-in dropped at weight 0");
  advance_at(&mut s, 400.0);
  let (pos, _, _) = s.transform_of(n).expect("read");
  assert!(pos[0] > 0.0, "fade-in never took: {pos:?}");
  // The same player fading OUT does drop past zero.
  s.set_player(p, PlayerUpdate { fade: Some(-2.5), ..Default::default() }).expect("fade");
  advance_at(&mut s, 1000.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p)]);
}

#[test]
fn once_clips_finish_hold_and_report_once() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let clip = slide_clip(&mut s);
  advance_at(&mut s, 0.0);
  let p = s.create_player(clip, vec![n], 1.0, false, 1.0, 0.0).expect("player");
  advance_at(&mut s, 1500.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Finished(p)]);
  let (pos, _, _) = s.transform_of(n).expect("read");
  assert!((pos[0] - 10.0).abs() < 1e-5);
  // Held: no more events, no more demand, pose stays.
  let tick = advance_at(&mut s, 2000.0);
  assert!(s.take_clip_events().is_empty());
  assert!(!tick.active && !tick.wrote);
  // A time write re-arms it.
  s.set_player(p, PlayerUpdate { time: Some(0.0), ..Default::default() }).expect("rewind");
  let tick = advance_at(&mut s, 2016.0);
  assert!(tick.active && tick.wrote);
}

#[test]
fn looping_wraps_continuously() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let clip = slide_clip(&mut s);
  advance_at(&mut s, 0.0);
  s.create_player(clip, vec![n], 1.0, true, 1.0, 0.0).expect("player");
  advance_at(&mut s, 900.0);
  // 1.15 s wraps to 0.15 s - the cursor seeks backwards correctly.
  advance_at(&mut s, 1150.0);
  let (p, _, _) = s.transform_of(n).expect("read");
  assert!((p[0] - 1.5).abs() < 1e-4, "{p:?}");
}

#[test]
fn dead_targets_and_dead_clips_drop_players() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let clip = slide_clip(&mut s);
  advance_at(&mut s, 0.0);
  let p = s.create_player(clip, vec![n], 1.0, true, 1.0, 0.0).expect("player");
  s.destroy(n).expect("destroy");
  let tick = advance_at(&mut s, 16.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p)]);
  assert!(!tick.active);
  // Same for a destroyed clip.
  let n2 = s.create([0.0; 3], Q, ONE, true);
  let p2 = s.create_player(clip, vec![n2], 1.0, true, 1.0, 0.0).expect("player");
  s.destroy_clip(clip).expect("destroy clip");
  advance_at(&mut s, 32.0);
  assert_eq!(s.take_clip_events(), vec![ClipEvent::Dropped(p2)]);
  // Creation-time validation: missing clip, short target table, dead node.
  assert!(s.create_player(clip, vec![n2], 1.0, true, 1.0, 0.0).is_err());
  let clip2 = slide_clip(&mut s);
  assert!(s.create_player(clip2, vec![], 1.0, true, 1.0, 0.0).is_err());
  assert!(s.create_player(clip2, vec![n], 1.0, true, 1.0, 0.0).is_err(), "dead target must fail at create");
}

/// A root that walks +x 10 units while turning 90 degrees about +y over
/// 1 s: channel 0 its position, channel 1 its rotation.
fn walk_and_turn_clip(s: &mut Spatial) -> u64 {
  let h = (0.5f32).sqrt();
  let mut rot =
    channel(ChannelPath::Rotation, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 0.0, 1.0, 0.0, h, 0.0, h]);
  rot.target_slot = 0;
  s.create_clip(
    1.0,
    vec![
      channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 0.0, 10.0, 0.0, 0.0]),
      rot,
    ],
  )
  .expect("clip")
}

fn yaw_about_y(q: [f32; 4]) -> f32 {
  2.0 * q[1].atan2(q[3])
}

#[test]
fn root_motion_moves_and_turns_the_anchor_continuously() {
  let mut s = Spatial::new();
  let root = s.create([0.0; 3], Q, ONE, true);
  let anchor = s.create([0.0; 3], Q, ONE, true);
  let clip = walk_and_turn_clip(&mut s);
  advance_at(&mut s, 0.0);
  let player = s.create_player(clip, vec![root], 1.0, true, 1.0, 0.0).expect("player");
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
}

#[test]
fn root_motion_without_vertical_keeps_the_rise_out_of_the_delta() {
  let mut s = Spatial::new();
  let root = s.create([0.0; 3], Q, ONE, true);
  let clip = s
    .create_clip(
      1.0,
      vec![channel(ChannelPath::Position, ChannelInterpolation::Linear, &[0.0, 1.0], &[0.0, 0.0, 0.0, 4.0, 3.0, 0.0])],
    )
    .expect("clip");
  advance_at(&mut s, 0.0);
  let player = s.create_player(clip, vec![root], 1.0, false, 1.0, 0.0).expect("player");
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
