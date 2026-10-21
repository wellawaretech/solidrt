// Spatial audio on the core (spatial/emitters.rs): the pan, distance,
// doppler and occlusion laws against their closed forms, a listener on a
// view or a node, writes only for what moved past the epsilons (a still
// scene writes nothing), the derived velocity's tail and the teleport
// rule, bindings released with their voice, node or listener, and the
// validation. A recording writer stands in for the mixer.

use crate::audio::OPEN_CUTOFF_HZ;
use crate::spatial::{
  invert_affine, DistanceModel, EmitterOutput, EmitterParams, EmitterRamp, EmitterSource, ListenerSource, Mat4, Mat4f,
  Occluder, OccluderAudio, SinkWriter, Spatial, IDENTITY, OCCLUSION_AUDIO_CUTOFF_HZ, OCCLUSION_AUDIO_GAIN,
  SPEED_OF_SOUND,
};

const Q: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f64; 3] = [1.0, 1.0, 1.0];
const TARGET: u64 = 7;
const VOICE: u64 = 42;
const EPS: f32 = 1e-4;

#[derive(Clone, Debug, PartialEq)]
struct Placed {
  voice: u64,
  output: EmitterOutput,
  ramp: EmitterRamp,
}

/// Records emitter writes; voices listed in `gone` do not land.
#[derive(Default)]
struct Recorder {
  writes: Vec<Placed>,
  gone: Vec<u64>,
}

impl SinkWriter for Recorder {
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
  fn write_order_view(&mut self, _: u64, _: u64, _: [f32; 16]) -> bool {
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
  fn write_emitter(&mut self, voice: u64, output: &EmitterOutput, ramp: EmitterRamp) -> bool {
    self.writes.push(Placed { voice, output: *output, ramp });
    !self.gone.contains(&voice)
  }
}

fn flush(s: &mut Spatial) -> Vec<Placed> {
  let mut out = Recorder::default();
  s.flush(&mut out);
  out.writes
}

/// The one write of a flush, or a panic naming how many came.
fn only(writes: &[Placed]) -> &Placed {
  assert_eq!(writes.len(), 1, "expected one write, got {writes:?}");
  &writes[0]
}

fn near(actual: f32, expected: f32, what: &str) {
  assert!((actual - expected).abs() < EPS, "{what}: {actual}, expected {expected}");
}

/// A camera at `eye` looking down -z (identity rotation): the view is the
/// inverse of that placement.
fn view_from(eye: [f64; 3]) -> Mat4 {
  let mut world = IDENTITY;
  world[12] = eye[0];
  world[13] = eye[1];
  world[14] = eye[2];
  invert_affine(&world)
}

/// A camera at `eye` with the given world basis columns (right, up, back).
fn view_with_basis(eye: [f64; 3], right: [f64; 3], up: [f64; 3], back: [f64; 3]) -> Mat4 {
  let mut world = IDENTITY;
  world[0..3].copy_from_slice(&right);
  world[4..7].copy_from_slice(&up);
  world[8..11].copy_from_slice(&back);
  world[12..15].copy_from_slice(&eye);
  invert_affine(&world)
}

/// A scene with a view listener at the origin looking down -z, an emitter
/// node at `at` bound to `VOICE` under `params`, flushed once.
fn scene(at: [f64; 3], params: EmitterParams) -> (Spatial, u64, crate::spatial::NodeId, EmitterOutput) {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  s.set_view(TARGET, Some((view_from([0.0; 3]), IDENTITY)));
  let listener = s.create_listener(ListenerSource::View(TARGET), SPEED_OF_SOUND).expect("listener");
  let node = s.create(at, Q, ONE, true);
  let initial = s.bind_emitter(VOICE, EmitterSource::Node(node), listener, params).expect("bind");
  let first = flush(&mut s);
  assert!(first.is_empty(), "the bind evaluated already; the first flush writes nothing: {first:?}");
  (s, listener, node, initial)
}

fn params() -> EmitterParams {
  EmitterParams::default()
}

#[test]
fn pan_is_the_lateral_angle_blended_inside_the_reference_distance() {
  let cases: [([f64; 3], f32, &str); 7] = [
    ([5.0, 0.0, 0.0], 1.0, "right"),
    ([-5.0, 0.0, 0.0], -1.0, "left"),
    ([0.0, 0.0, -5.0], 0.0, "ahead"),
    ([0.0, 0.0, 5.0], 0.0, "behind"),
    ([0.0, 5.0, 0.0], 0.0, "overhead"),
    ([3.0, 0.0, -3.0], 0.5, "45 degrees right"),
    ([0.5, 0.0, 0.0], 0.5, "hard right, but inside refDistance: half"),
  ];
  for (at, pan, what) in cases {
    let (_, _, _, out) = scene(at, params());
    near(out.pan, pan, what);
  }
  // Behind and to the right folds the same as ahead and to the right.
  let (_, _, _, behind) = scene([3.0, 0.0, 3.0], params());
  near(behind.pan, 0.5, "45 degrees behind right");
}

#[test]
fn distance_models_follow_web_audio_s_definitions() {
  let at = |d: f64| [0.0, 0.0, -d];
  let inverse = EmitterParams { ref_distance: 1.0, max_distance: 11.0, rolloff: 1.0, ..params() };
  near(scene(at(5.0), inverse).3.gain, 0.2, "inverse at 5");
  near(scene(at(0.5), inverse).3.gain, 1.0, "inside the reference distance: unity");
  near(scene(at(100.0), inverse).3.gain, 1.0 / 11.0, "clamped at maxDistance");
  let linear = EmitterParams { model: DistanceModel::Linear, ..inverse };
  near(scene(at(5.0), linear).3.gain, 0.6, "linear at 5");
  near(scene(at(100.0), linear).3.gain, 0.0, "linear past max: silent");
  let exponential = EmitterParams { model: DistanceModel::Exponential, rolloff: 2.0, ..inverse };
  near(scene(at(5.0), exponential).3.gain, 1.0 / 25.0, "exponential at 5 with rolloff 2");
  let flat = EmitterParams { rolloff: 0.0, ..inverse };
  near(scene(at(5.0), flat).3.gain, 1.0, "rolloff 0: no attenuation");
}

#[test]
fn a_still_scene_writes_nothing_and_a_move_writes_once() {
  let (mut s, _, node, _) = scene([0.0, 0.0, -2.0], params());
  assert!(flush(&mut s).is_empty());
  assert!(flush(&mut s).is_empty());
  s.set_transform(node, [2.0, 0.0, -2.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  assert_eq!(w.voice, VOICE);
  assert_eq!(w.ramp, EmitterRamp::Frame);
  near(w.output.pan, 0.5, "moved to 45 degrees right");
  near(w.output.gain, 1.0 / (8.0f32).sqrt(), "inverse at sqrt 8");
  // Nothing moved since: nothing written, and no velocity tail (the clock
  // never advanced, so no velocity was derived).
  assert!(flush(&mut s).is_empty());
  assert!(!s.emitters_active());
  assert_eq!(s.emitter_stats().writes, 2, "the bind and the move");
}

#[test]
fn a_sub_epsilon_move_is_not_written() {
  let (mut s, _, node, _) = scene([0.0, 0.0, -100.0], params());
  // A millimetre at a hundred metres: pan and gain change in the fifth
  // decimal, below every epsilon.
  s.set_transform(node, [0.001, 0.0, -100.0], Q, ONE).expect("move");
  assert!(flush(&mut s).is_empty(), "float noise stays out of the mixer");
}

#[test]
fn doppler_follows_the_approach_speed_and_its_tail_decays_to_exactly_zero() {
  let (mut s, _, node, _) = scene([0.0, 0.0, -100.0], params());
  // One second on, ten units nearer: 10 m/s toward the listener.
  s.set_transition_now(1000.0);
  s.set_transform(node, [0.0, 0.0, -90.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  // The smoothing over a whole second has converged on the raw speed.
  let expected = (SPEED_OF_SOUND / (SPEED_OF_SOUND - 10.0)) as f32;
  assert!((w.output.rate - expected).abs() < 1e-3, "approaching: {} vs {expected}", w.output.rate);
  assert!(s.emitters_active(), "a derived velocity is a tail to write out");
  // Standing still for a second: the velocity decays below the floor,
  // snaps to zero, and the rate returns to 1 in one write.
  s.set_transition_now(2000.0);
  let w = only(&flush(&mut s)).clone();
  near(w.output.rate, 1.0, "stopped");
  assert!(!s.emitters_active(), "the tail ended");
  s.set_transition_now(3000.0);
  assert!(flush(&mut s).is_empty(), "nothing moves, nothing is written");
  // Receding: the rate falls below 1.
  s.set_transition_now(4000.0);
  s.set_transform(node, [0.0, 0.0, -100.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  assert!(w.output.rate < 1.0, "receding: {}", w.output.rate);
  // Doppler off: the same move leaves the rate at 1.
  let (mut s, _, node, _) = scene([0.0, 0.0, -100.0], EmitterParams { doppler: 0.0, ..params() });
  s.set_transition_now(1000.0);
  s.set_transform(node, [0.0, 0.0, -90.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  near(w.output.rate, 1.0, "doppler off");
}

#[test]
fn a_supersonic_jump_is_a_teleport_not_motion() {
  let (mut s, _, node, _) = scene([0.0, 0.0, -100.0], params());
  s.set_transition_now(1000.0);
  // A kilometre in a second: faster than sound, so no velocity is derived.
  s.set_transform(node, [0.0, 0.0, -1100.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  near(w.output.rate, 1.0, "teleported: no doppler");
  assert!(!s.emitters_active());
}

#[test]
fn the_doppler_ratio_is_clamped() {
  let (mut s, _, node, _) = scene([0.0, 0.0, -1000.0], params());
  s.set_transition_now(1000.0);
  // Just under the speed of sound toward the listener: the ratio would be
  // enormous; it is held at the clamp.
  s.set_transform(node, [0.0, 0.0, -1000.0 + 340.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  near(w.output.rate, 2.0, "clamped to the top of the range");
}

#[test]
fn a_view_listener_hears_from_the_camera_frame() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  // A camera at (10, 0, 0) looking along +x: right is +z.
  let view = view_with_basis([10.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]);
  s.set_view(TARGET, Some((view, IDENTITY)));
  let listener = s.create_listener(ListenerSource::View(TARGET), SPEED_OF_SOUND).expect("listener");
  let ahead = s.create([15.0, 0.0, 0.0], Q, ONE, true);
  let out = s.bind_emitter(VOICE, EmitterSource::Node(ahead), listener, params()).expect("bind");
  near(out.pan, 0.0, "ahead of the camera");
  near(out.gain, 0.2, "five units off");
  let right = s.create([10.0, 0.0, 5.0], Q, ONE, true);
  let out = s.bind_emitter(VOICE + 1, EmitterSource::Node(right), listener, params()).expect("bind");
  near(out.pan, 1.0, "to the camera's right");
  flush(&mut s);
  // The camera turns to look along +z: now `right` is ahead and `ahead`
  // is to the left; one write each.
  let turned = view_with_basis([10.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]);
  s.set_view(TARGET, Some((turned, IDENTITY)));
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 2);
  let of = |voice: u64| writes.iter().find(|w| w.voice == voice).expect("write").output;
  near(of(VOICE).pan, -1.0, "ahead became left");
  near(of(VOICE + 1).pan, 0.0, "right became ahead");
  // The same view again: nothing.
  s.set_view(TARGET, Some((turned, IDENTITY)));
  assert!(flush(&mut s).is_empty());
}

#[test]
fn a_node_listener_hears_from_its_frame_and_moves_with_it() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let ear = s.create([0.0, 0.0, 0.0], Q, ONE, true);
  let listener = s.create_listener(ListenerSource::Node(ear), SPEED_OF_SOUND).expect("listener");
  let node = s.create([5.0, 0.0, 0.0], Q, ONE, true);
  let out = s.bind_emitter(VOICE, EmitterSource::Node(node), listener, params()).expect("bind");
  near(out.pan, 1.0, "to the ear's right (+x)");
  assert!(flush(&mut s).is_empty());
  // The ear walks under the emitter: center, loud.
  s.set_transform(ear, [5.0, 0.0, 0.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  near(w.output.pan, 0.0, "coincident: no direction");
  near(w.output.gain, 1.0, "coincident: unity");
  // The ear turns a quarter turn about y (its -z forward now points to
  // -x, its right to -z): an emitter at +x is behind, center.
  s.set_transform(
    ear,
    [0.0, 0.0, 0.0],
    [0.0, std::f64::consts::FRAC_1_SQRT_2, 0.0, std::f64::consts::FRAC_1_SQRT_2],
    ONE,
  )
  .expect("turn");
  let w = only(&flush(&mut s)).clone();
  near(w.output.pan, 0.0, "behind the turned ear");
}

#[test]
fn a_fixed_emitter_follows_the_listener_only() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  s.set_view(TARGET, Some((view_from([0.0; 3]), IDENTITY)));
  let listener = s.create_listener(ListenerSource::View(TARGET), SPEED_OF_SOUND).expect("listener");
  let out = s.bind_emitter(VOICE, EmitterSource::Fixed([4.0, 0.0, 0.0]), listener, params()).expect("bind");
  near(out.pan, 1.0, "right");
  near(out.gain, 0.25, "four off");
  assert!(flush(&mut s).is_empty());
  s.set_view(TARGET, Some((view_from([4.0, 0.0, -4.0]), IDENTITY)));
  let w = only(&flush(&mut s)).clone();
  near(w.output.pan, 0.0, "now straight ahead");
  near(w.output.gain, 0.25, "still four off");
  assert_eq!(s.emitter_stats().emitters, 1);
}

#[test]
fn a_listener_without_a_view_yet_hears_plainly_until_it_arrives() {
  let mut s = Spatial::new();
  s.set_transition_now(0.0);
  let listener = s.create_listener(ListenerSource::View(TARGET), SPEED_OF_SOUND).expect("listener");
  let node = s.create([5.0, 0.0, 0.0], Q, ONE, true);
  let out = s.bind_emitter(VOICE, EmitterSource::Node(node), listener, params()).expect("bind");
  assert_eq!(out, EmitterOutput::NEUTRAL);
  assert!(flush(&mut s).is_empty());
  s.set_view(TARGET, Some((view_from([0.0; 3]), IDENTITY)));
  let w = only(&flush(&mut s)).clone();
  near(w.output.pan, 1.0, "placed once the view exists");
}

#[test]
fn occluders_with_audio_dim_and_dull_the_sound_and_release_it_when_hidden() {
  let (mut s, _, node, _) = scene([0.0, 0.0, -10.0], params());
  // A wall across the line of sight, a solid box.
  let wall = s.create([0.0, 0.0, -5.0], Q, ONE, true);
  s.set_occluder(wall, Some(Occluder::box3([-2.0, -2.0, -0.1, 2.0, 2.0, 0.1]))).expect("occluder");
  let w = only(&flush(&mut s)).clone();
  assert_eq!(w.ramp, EmitterRamp::Occlusion, "an occlusion change glides over the occlusion time");
  near(w.output.gain, 0.1 * OCCLUSION_AUDIO_GAIN, "inverse at 10, through the wall");
  near(w.output.cutoff, OCCLUSION_AUDIO_CUTOFF_HZ, "the wall's cutoff");
  assert!(flush(&mut s).is_empty());
  // The wall slides aside: clear again.
  s.set_transform(wall, [10.0, 0.0, -5.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  near(w.output.gain, 0.1, "clear");
  near(w.output.cutoff, OPEN_CUTOFF_HZ, "open");
  // Back, then hidden: a hidden occluder blocks nothing.
  s.set_transform(wall, [0.0, 0.0, -5.0], Q, ONE).expect("move");
  near(only(&flush(&mut s)).output.gain, 0.1 * OCCLUSION_AUDIO_GAIN, "blocked again");
  s.set_visible(wall, false).expect("hide");
  near(only(&flush(&mut s)).output.gain, 0.1, "hidden: clear");
  // The emitter steps out from behind the shown wall: the ray clears.
  s.set_visible(wall, true).expect("show");
  near(only(&flush(&mut s)).output.gain, 0.1 * OCCLUSION_AUDIO_GAIN, "shown: blocked");
  s.set_transform(node, [10.0, 0.0, -10.0], Q, ONE).expect("move");
  let w = only(&flush(&mut s)).clone();
  near(w.output.cutoff, OPEN_CUTOFF_HZ, "beside the wall: clear");
}

#[test]
fn occluder_audio_composes_and_can_be_declined() {
  let (mut s, _, _, _) = scene([0.0, 0.0, -10.0], params());
  let glass = s.create([0.0, 0.0, -3.0], Q, ONE, true);
  s.set_occluder(glass, Some(Occluder::quad(4.0, 4.0).with_audio(None))).expect("occluder");
  assert!(flush(&mut s).is_empty(), "a sight-only occluder changes no sound");
  let curtain = s.create([0.0, 0.0, -6.0], Q, ONE, true);
  let soft = OccluderAudio { gain: 0.5, cutoff: 8000.0 };
  s.set_occluder(curtain, Some(Occluder::quad(4.0, 4.0).with_audio(Some(soft)))).expect("occluder");
  let w = only(&flush(&mut s)).clone();
  near(w.output.gain, 0.1 * 0.5, "the curtain's gain");
  near(w.output.cutoff, 8000.0, "the curtain's cutoff");
  let wall = s.create([0.0, 0.0, -8.0], Q, ONE, true);
  s.set_occluder(wall, Some(Occluder::quad(4.0, 4.0))).expect("occluder");
  let w = only(&flush(&mut s)).clone();
  near(w.output.gain, 0.1 * 0.5 * OCCLUSION_AUDIO_GAIN, "gains multiply");
  near(w.output.cutoff, OCCLUSION_AUDIO_CUTOFF_HZ, "the lowest cutoff wins");
  // A box the ray passes through twice counts once.
  let (mut s, _, _, _) = scene([0.0, 0.0, -10.0], params());
  let block = s.create([0.0, 0.0, -5.0], Q, ONE, true);
  s.set_occluder(block, Some(Occluder::box3([-1.0, -1.0, -1.0, 1.0, 1.0, 1.0]))).expect("occluder");
  near(only(&flush(&mut s)).output.gain, 0.1 * OCCLUSION_AUDIO_GAIN, "one occluder, one attenuation");
}

#[test]
fn bindings_release_with_their_voice_node_or_listener() {
  // The voice ends: the host says so before a flush.
  let (mut s, _, node, _) = scene([0.0, 0.0, -2.0], params());
  s.release_emitters(|voice| voice == VOICE);
  assert_eq!(s.emitter_stats().emitters, 0);
  s.set_transform(node, [5.0, 0.0, -2.0], Q, ONE).expect("move");
  assert!(flush(&mut s).is_empty());
  // A write that does not land releases.
  let (mut s, _, node, _) = scene([0.0, 0.0, -2.0], params());
  s.set_transform(node, [5.0, 0.0, -2.0], Q, ONE).expect("move");
  let mut out = Recorder { gone: vec![VOICE], ..Recorder::default() };
  s.flush(&mut out);
  assert_eq!(out.writes.len(), 1);
  assert_eq!(s.emitter_stats().emitters, 0);
  // The node dies.
  let (mut s, listener, node, _) = scene([0.0, 0.0, -2.0], params());
  s.destroy(node).expect("destroy");
  assert_eq!(s.emitter_stats().emitters, 0);
  // Unbind by hand, and a rebind replaces.
  let node = s.create([0.0, 0.0, -2.0], Q, ONE, true);
  s.bind_emitter(VOICE, EmitterSource::Node(node), listener, params()).expect("bind");
  s.bind_emitter(VOICE, EmitterSource::Fixed([1.0, 0.0, 0.0]), listener, params()).expect("rebind");
  assert_eq!(s.emitter_stats().emitters, 1);
  assert!(s.unbind_emitter(VOICE));
  assert!(!s.unbind_emitter(VOICE));
  // The listener dies: its emitters go with it.
  s.bind_emitter(VOICE, EmitterSource::Node(node), listener, params()).expect("bind");
  s.destroy_listener(listener).expect("destroy");
  assert_eq!(s.emitter_stats().emitters, 0);
  assert_eq!(s.emitter_stats().listeners, 0);
  assert!(s.destroy_listener(listener).is_err());
}

#[test]
fn re_pointing_a_listener_rewrites_its_emitters() {
  let (mut s, listener, _, _) = scene([5.0, 0.0, 0.0], params());
  let ear = s.create([5.0, 0.0, -5.0], Q, ONE, true);
  s.set_listener(listener, ListenerSource::Node(ear), SPEED_OF_SOUND).expect("re-point");
  let w = only(&flush(&mut s)).clone();
  near(w.output.pan, 0.0, "straight ahead of the ear");
  near(w.output.gain, 0.2, "five off");
  // Back to the view, with a slower speed of sound: the same geometry, a
  // stronger doppler per unit of speed.
  s.set_listener(listener, ListenerSource::View(TARGET), 100.0).expect("re-point");
  near(only(&flush(&mut s)).output.pan, 1.0, "right of the camera again");
}

#[test]
fn validation_refuses_what_cannot_be_heard() {
  let mut s = Spatial::new();
  let node = s.create([0.0; 3], Q, ONE, true);
  assert!(s.create_listener(ListenerSource::View(TARGET), 0.0).is_err(), "speed of sound 0");
  assert!(s.create_listener(ListenerSource::Node(node + (1 << 40)), SPEED_OF_SOUND).is_err(), "a dead node");
  let listener = s.create_listener(ListenerSource::View(TARGET), SPEED_OF_SOUND).expect("listener");
  let mut bad = |p: EmitterParams| s.bind_emitter(VOICE, EmitterSource::Node(node), listener, p).is_err();
  assert!(bad(EmitterParams { ref_distance: 0.0, ..params() }), "refDistance 0");
  assert!(bad(EmitterParams { max_distance: 0.5, ..params() }), "maxDistance under refDistance");
  assert!(bad(EmitterParams { rolloff: -1.0, ..params() }), "negative rolloff");
  assert!(bad(EmitterParams { doppler: f64::NAN, ..params() }), "NaN doppler");
  assert!(s.bind_emitter(VOICE, EmitterSource::Node(node), listener + 99, params()).is_err(), "unknown listener");
  assert!(s.bind_emitter(VOICE, EmitterSource::Fixed([f64::INFINITY, 0.0, 0.0]), listener, params()).is_err());
  assert_eq!(s.emitter_stats().emitters, 0, "nothing bound by a refused call");
}
