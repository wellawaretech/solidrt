// Rigs: a model's animation state in the core - Unity's Animator, Godot's
// AnimationMixer, Unreal's AnimInstance, ozz's local pose buffer. A rig
// owns a joint table (one arena node per slot), the rest pose per joint,
// a persistent pose buffer (slot x path, plus the morph weights and the
// named curves), ordered layers, the players on them, and the sync groups
// that share a phase. Once per frame BEFORE the frame's JS every rig
// advances its players, composes its layers over the rest pose and writes
// the result through the ordinary snap path, so app code reads and may
// overwrite freshly posed joints (last write wins) and the draw path's
// single flush publishes everything. JS keeps policy at O(changes): a
// crossfade is a player create and a couple of fade writes, a mask swap
// is one write, a blend space is a few weight writes.
//
// The blend, per (joint, path) and per layer in order: an `Override`
// layer normalizes its players' weighted contributions (incremental
// weighted average, rotations slerped) and composes the result OVER what
// is under it by the layer's weight times its mask's share for the joint;
// an `Additive` layer adds its players' weighted deltas. The layer floor
// is the REST pose (Godot's deterministic mode, ozz's blending job,
// Unreal): when a layer's players weigh less than one at a slot the
// remainder is what is under the layer (rest for the base), and a player
// lacking a channel another player on its layer animates contributes
// that same "under" for it - so a stop-with-fade fades to rest and a
// crossfade between clips of different coverage has no snap. A slot no
// player on any layer animates is left alone.
//
// Transitions blend two ways: a crossfade (two players fading past each
// other, both sampled) or an inertial transition (Unreal's
// inertialization): the new pose plays at full weight at once and the
// rig decays the offset from the pose it last wrote - with the velocity
// from the two previous outputs - on a quintic that lands at zero with
// zero velocity and acceleration, its duration capped so the curve never
// overshoots. Sync groups (Unreal's) keep a walk and a run in phase: the
// heaviest member leads, followers take its normalized time or, with
// markers common to every member, its position between markers.
//
// Engine independent: native types in and out (rendertree rules).

use std::collections::HashMap;

use super::clips::{
  additive_channel, check_channel, crossings, key_layout, quat_inverse, quat_multiply, sample, ChannelPath, Clip,
  ClipChannel, ClipId, ClipMarker, MarkerPhase,
};
use super::math::rotate_vector;
use super::transitions::{quat_normalize, slerp};
use super::{NodeId, SinkWriter, Spatial};

pub type RigId = u64;
pub type PlayerId = u64;
pub type MaskId = u64;
pub type GroupId = u64;

/// A weight within this of its target lands on it (layer fades).
const WEIGHT_SETTLE: f64 = 1e-6;
/// An inertial offset below this (world units, radians, a weight) is not
/// worth a transition: the switch is taken as continuous.
const INERTIAL_MIN_OFFSET: f64 = 1e-5;
/// The overshoot cap of the inertialization quintic: with the offset
/// velocity pointing at zero the duration is cut to this multiple of
/// offset over speed, past which the curve would cross zero and come
/// back (the Gears of War rule).
const INERTIAL_OVERSHOOT_FACTOR: f64 = 5.0;
/// A frame shorter than this (seconds) yields no usable velocity: the
/// history keeps the previous frame's.
const MIN_HISTORY_DT: f64 = 1e-6;
/// A normalized layer total at or above this counts as full: nothing
/// under the layer shows through.
const FULL_WEIGHT: f64 = 1.0;

const IDENTITY_QUAT: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerMode {
  Override,
  Additive,
}

/// A layer's declaration: `weight` 0..1, its mode, and the mask whose
/// per-joint shares scale it (None = every joint).
#[derive(Clone, Copy, Debug)]
pub struct LayerSpec {
  pub weight: f64,
  pub mode: LayerMode,
  pub mask: Option<MaskId>,
}

/// A partial layer write: `weight` ramps over `fade_ms` of the rig's
/// time (0 snaps); `mask` Some(None) clears the mask.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayerUpdate {
  pub weight: Option<f64>,
  pub fade_ms: f64,
  pub mask: Option<Option<MaskId>>,
}

/// A joint's rest TRS: what the layers compose over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestPose {
  pub position: [f64; 3],
  pub rotation: [f64; 4],
  pub scale: [f64; 3],
}

/// A player's declaration. `fade` is the weight change per second
/// (positive in, negative out, 0 steady); `window` is the subclip in
/// seconds (None = the whole clip); `fade_out` (seconds, 0 = none) starts
/// a fade-out that many seconds before a non-looping player's end so it
/// ends on the pose under it (Godot's OneShot fadeout, Unreal's montage
/// blend-out); `time` is the start time.
#[derive(Clone, Copy, Debug)]
pub struct PlayerSpec {
  pub layer: usize,
  pub speed: f64,
  pub looped: bool,
  pub weight: f64,
  pub fade: f64,
  pub mask: Option<MaskId>,
  pub window: Option<(f64, f64)>,
  pub fade_out: f64,
  pub time: f64,
}

impl Default for PlayerSpec {
  fn default() -> Self {
    PlayerSpec {
      layer: 0,
      speed: 1.0,
      looped: true,
      weight: 1.0,
      fade: 0.0,
      mask: None,
      window: None,
      fade_out: 0.0,
      time: 0.0,
    }
  }
}

/// A partial player write: the O(changes) control channel. `time` also
/// clears `finished` (a scrub back re-arms the end report) and never
/// fires the markers it jumps over; `window` Some(None) clears the
/// window; `mask` Some(None) clears the mask.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerUpdate {
  pub weight: Option<f64>,
  pub fade: Option<f64>,
  pub speed: Option<f64>,
  pub time: Option<f64>,
  pub window: Option<Option<(f64, f64)>>,
  pub mask: Option<Option<MaskId>>,
}

/// How a sync group aligns its members: by normalized time within their
/// windows, or by the markers common to every member (Unreal's
/// marker-based sync), falling back to time where the marker sequences
/// do not match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMode {
  Time,
  Markers,
}

/// What an additive clip's deltas are taken against: the clip's own
/// first frame (Three's referenceFrame 0, Unity's default), another clip
/// at a time, or a rig's rest pose (Godot's Add nodes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AdditiveReference {
  FirstFrame,
  Clip { clip: ClipId, time: f64 },
  Rest { rig: RigId },
}

/// A root-motion binding (Godot's root_motion_track, Unity's
/// applyRootMotion): `channel` of `clip` is the ROOT's position track -
/// usually of the authored clip, while the player plays an in-place
/// variant that holds the root still - and `rotation` its rotation
/// track, when the turn should travel too. Each advance samples them at
/// the player's time and takes the difference from the previous sample
/// (the clip's net drift added across a loop wrap), weighted by the
/// player's EFFECTIVE weight (its own times its layer's times the root
/// joint's mask shares): a translation, un-turned by the clip's own yaw
/// so far into the root's CURRENT facing (the character's local frame),
/// and a yaw, the twist about `up`. Both are reported per player; with
/// an `anchor` the translation, rotated by the anchor's rotation, is
/// added to the anchor's position and the yaw turns the anchor about
/// `up` in its own frame - the character walks and turns where its clip
/// says. `vertical` false drops the component along `up` (the height
/// stays in the pose, a controller's gravity owns it). A player on an
/// additive layer cannot bind one: a delta clip has no travel.
#[derive(Clone, Copy, Debug)]
pub struct RootMotion {
  pub clip: ClipId,
  pub channel: u32,
  pub rotation: Option<u32>,
  pub anchor: Option<NodeId>,
  /// Unit up axis of the root's parent space; the turn axis.
  pub up: [f64; 3],
  pub vertical: bool,
}

struct RootState {
  binding: RootMotion,
  /// Last key value minus first: what a loop wrap adds.
  drift: [f64; 3],
  last: [f64; 3],
  cursor: u32,
  /// Net yaw over the clip (unwrapped key by key), the wrap's yaw step.
  yaw_drift: f64,
  last_yaw: f64,
  yaw_cursor: u32,
  /// The clip's own yaw since the bind, unwrapped and unweighted: how far
  /// the clip's frame has turned against the anchor's.
  clip_yaw: f64,
}

/// What an advance reports (drained by the embedder after each advance
/// and delivered to JS as one engine event per entry). `Finished` fires
/// once when a non-looping player reaches its end - the player STAYS,
/// holding the pose, unless an end fade removes it (then `Finished` is
/// followed by `Dropped`). `Dropped` fires when a player is removed
/// without finishing: faded out, or its clip or rig died. `Loop` fires
/// on every wrap of a looping player. `Marker` fires per marker
/// crossing with the player's effective weight (its own times its
/// layer's) and layer. `RigDropped` fires when a rig's clock or a joint
/// died, after the `Dropped` of each of its players.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipEvent {
  Finished(PlayerId),
  Dropped(PlayerId),
  Loop(PlayerId),
  Marker { player: PlayerId, marker: usize, phase: MarkerPhase, weight: f64, layer: usize },
  RigDropped(RigId),
}

/// What an advance produced: `active` is standing frame demand (a player
/// that can still progress, a layer fading, an inertial transition
/// decaying), `wrote` means node state changed this advance (this frame
/// must flush and paint).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayersTick {
  pub active: bool,
  pub wrote: bool,
}

/// A bone mask (Unity's AvatarMask, Godot's filters, Unreal's blend
/// masks): a share per node, `default` for the unlisted. Shared by any
/// number of layers and players; a change bumps the version the
/// bindings' share caches are keyed on.
struct Mask {
  default: f64,
  shares: HashMap<NodeId, f64>,
  version: u64,
}

impl Mask {
  fn share(&self, node: NodeId) -> f64 {
    self.shares.get(&node).copied().unwrap_or(self.default)
  }
}

/// A mask reference as last resolved: the id and the version seen.
type MaskSeen = Option<(MaskId, u64)>;

struct Layer {
  weight: f64,
  weight_target: f64,
  /// Per second of the rig's time; 0 when at the target.
  weight_rate: f64,
  mode: LayerMode,
  mask: Option<MaskId>,
  /// The mask's share per joint (empty = 1 everywhere).
  shares: Vec<f64>,
  seen: MaskSeen,
}

impl Layer {
  fn new(spec: LayerSpec) -> Self {
    Layer {
      weight: spec.weight,
      weight_target: spec.weight,
      weight_rate: 0.0,
      mode: spec.mode,
      mask: spec.mask,
      shares: Vec::new(),
      seen: None,
    }
  }

  fn share(&self, joint: usize) -> f64 {
    self.shares.get(joint).copied().unwrap_or(1.0)
  }
}

struct Player {
  id: PlayerId,
  layer: usize,
  clip: ClipId,
  time: f64,
  speed: f64,
  weight: f64,
  fade: f64,
  looped: bool,
  finished: bool,
  window: Option<(f64, f64)>,
  fade_out: f64,
  /// The end fade has started: reaching weight 0 finishes AND drops.
  end_fading: bool,
  /// Faded out this advance: blends once more at weight 0, so the slots
  /// it fed land on what is under it (rest, for the base), then drops.
  dying: bool,
  mask: Option<MaskId>,
  /// The player's own mask's share per joint (empty = 1 everywhere).
  shares: Vec<f64>,
  seen: MaskSeen,
  /// Last key-pair lo index per channel (the sequential-playback cache).
  cursors: Vec<u32>,
  /// Per channel: the rig's curve index for a Curve channel.
  curve_slots: Vec<usize>,
  /// Per marker: inside its window.
  inside: Vec<bool>,
  /// The next crossing walk fires a marker sitting exactly at the
  /// current time (the first advance, the one after a seek).
  inclusive: bool,
  group: Option<GroupId>,
  /// The step the time pass took: (previous time, wrapped).
  step: (f64, bool),
  root: Option<RootState>,
}

impl Player {
  fn share(&self, joint: usize) -> f64 {
    self.shares.get(joint).copied().unwrap_or(1.0)
  }

  /// The window the time runs in: the clip's extent without one.
  fn window(&self, duration: f64) -> (f64, f64) {
    self.window.unwrap_or((0.0, duration))
  }
}

struct Group {
  rig: RigId,
  members: Vec<PlayerId>,
  mode: SyncMode,
  leader: Option<PlayerId>,
}

/// An inertial transition on one slot: the offset from the previous
/// output decays along `dir` (a unit vector; a rotation's axis) on the
/// quintic x(t) = A t^5 + B t^4 + C t^3 + a0/2 t^2 + v0 t + x0, which
/// matches the offset and its velocity at t = 0 and reaches zero offset,
/// velocity and acceleration at t1.
#[derive(Clone, Copy, Debug)]
struct Inertial {
  dir: [f64; 4],
  x0: f64,
  v0: f64,
  a0: f64,
  aa: f64,
  bb: f64,
  cc: f64,
  t1: f64,
  t: f64,
}

impl Inertial {
  /// Fit the quintic to an offset `x0` with velocity `v0` over `t1`
  /// seconds, the duration cut so the curve never crosses zero. None for
  /// an offset not worth a transition.
  fn fit(dir: [f64; 4], x0: f64, v0: f64, t1: f64) -> Option<Self> {
    if !(x0 > INERTIAL_MIN_OFFSET) || !(t1 > 0.0) {
      return None;
    }
    let t1 = if v0 < 0.0 { t1.min(-INERTIAL_OVERSHOOT_FACTOR * x0 / v0) } else { t1 };
    if !(t1 > 0.0) {
      return None;
    }
    let t2 = t1 * t1;
    let t3 = t2 * t1;
    let t4 = t3 * t1;
    let t5 = t4 * t1;
    let a0 = (-8.0 * v0 * t1 - 20.0 * x0) / t2;
    let aa = -(a0 * t2 + 6.0 * v0 * t1 + 12.0 * x0) / (2.0 * t5);
    let bb = (3.0 * a0 * t2 + 16.0 * v0 * t1 + 30.0 * x0) / (2.0 * t4);
    let cc = -(3.0 * a0 * t2 + 12.0 * v0 * t1 + 20.0 * x0) / (2.0 * t3);
    Some(Inertial { dir, x0, v0, a0, aa, bb, cc, t1, t: 0.0 })
  }

  /// The offset at the current time.
  fn offset(&self) -> f64 {
    let t = self.t;
    ((((self.aa * t + self.bb) * t + self.cc) * t + self.a0 * 0.5) * t + self.v0) * t + self.x0
  }
}

/// The inertialization quintic, for tests: the offset at `t` of a fit.
#[cfg(test)]
pub(crate) fn inertial_offset_for_test(x0: f64, v0: f64, t1: f64, t: f64) -> Option<(f64, f64)> {
  let mut fit = Inertial::fit([1.0, 0.0, 0.0, 0.0], x0, v0, t1)?;
  fit.t = t;
  Some((fit.offset(), fit.t1))
}

/// One blended value of the pose buffer: a path of a joint, one morph
/// weight, or a curve. `fed` stamps the frame a layer last wrote it; the
/// history is what the inertial transition decays from.
#[derive(Clone, Copy)]
struct Slot {
  value: [f64; 4],
  fed: u64,
  /// Valid history entries, 0..2.
  hist: u8,
  prev: [f64; 4],
  prev2: [f64; 4],
  prev_dt: f64,
  inertial: Option<Inertial>,
}

impl Slot {
  const EMPTY: Slot =
    Slot { value: [0.0; 4], fed: 0, hist: 0, prev: [0.0; 4], prev2: [0.0; 4], prev_dt: 0.0, inertial: None };
}

/// A layer's accumulator for one slot: an override layer's normalized
/// average and the weight that covered it, an additive layer's summed
/// delta (positions and weights add, rotations compose, scales
/// multiply). `fed` stamps the layer pass.
#[derive(Clone, Copy)]
struct Acc {
  value: [f64; 4],
  covered: f64,
  fed: u64,
}

impl Acc {
  const EMPTY: Acc = Acc { value: [0.0; 4], covered: 0.0, fed: 0 };
}

/// A curve routed into a scalar shader param at the flush: of one draw
/// entry on a target, or of the target itself (a scene-wide uniform).
struct CurveSink {
  curve: usize,
  target: u64,
  draw: Option<u64>,
  name: String,
  /// The value last written; a repeat writes nothing.
  last: Option<f64>,
}

struct Rig {
  id: RigId,
  clock: NodeId,
  joints: Vec<NodeId>,
  rest: Vec<RestPose>,
  rest_weights: Vec<Vec<f64>>,
  layers: Vec<Layer>,
  players: Vec<Player>,
  curves: Vec<String>,
  curve_index: HashMap<String, usize>,
  /// The pose buffer: per joint the three TRS paths.
  trs: Vec<[Slot; 3]>,
  /// Per joint its morph weights (empty until a weights channel feeds
  /// it).
  morph: Vec<Vec<Slot>>,
  /// Per curve.
  curve_slots: Vec<Slot>,
  acc_trs: Vec<[Acc; 3]>,
  acc_morph: Vec<Vec<Acc>>,
  acc_curves: Vec<Acc>,
  /// Per joint, the layer pass's total player weight (the normalization
  /// base of the override fill).
  total: Vec<f64>,
  /// Advances since creation: the `fed` stamp.
  frame: u64,
  /// Layer passes since creation: the accumulators' stamp.
  pass: u64,
  /// An inertial transition requested for the next advance: its
  /// duration in seconds.
  pending_inertial: Option<f64>,
  sinks: Vec<CurveSink>,
}

/// The rig set (a Spatial field; the public API is the Spatial methods
/// below).
#[derive(Default)]
pub(super) struct RigSet {
  rigs: Vec<Rig>,
  masks: HashMap<MaskId, Mask>,
  groups: HashMap<GroupId, Group>,
  /// Which rig a player lives on.
  player_rig: HashMap<PlayerId, RigId>,
  next_rig: RigId,
  next_player: PlayerId,
  next_mask: MaskId,
  next_group: GroupId,
  // The clock of the last advance; started with the transition clock by
  // the arena (mod.rs set_transition_now).
  pub(super) last_ms: f64,
  events: Vec<ClipEvent>,
  /// Root-motion deltas of the last advance, per bound player:
  /// translation and yaw (radians).
  root_deltas: Vec<(PlayerId, [f64; 3], f64)>,
  /// Sampling scratch (as many floats as the widest channel).
  scratch: Vec<f64>,
  /// Marker crossing scratch.
  crossed: Vec<(usize, MarkerPhase)>,
  /// Anchor moves of the advance: (anchor, delta, yaw, up).
  anchor_moves: Vec<(NodeId, [f64; 3], f64, [f64; 3])>,
}

/// The twist of a unit quaternion ([x, y, z, w]) about the unit axis
/// `up`, in radians, -pi..pi: the swing-twist split, exact under any
/// lean (an Euler yaw is not).
fn twist_of(q: [f64; 4], up: [f64; 3]) -> f64 {
  let along = q[0] * up[0] + q[1] * up[1] + q[2] * up[2];
  wrap_angle(2.0 * along.atan2(q[3]))
}

/// The rotation of `angle` radians about the unit axis `up`.
fn axis_quat(up: [f64; 3], angle: f64) -> [f64; 4] {
  let half = angle * 0.5;
  let s = half.sin();
  [up[0] * s, up[1] * s, up[2] * s, half.cos()]
}

/// An angle folded into -pi..pi.
fn wrap_angle(a: f64) -> f64 {
  let mut r = a % std::f64::consts::TAU;
  if r > std::f64::consts::PI {
    r -= std::f64::consts::TAU;
  } else if r < -std::f64::consts::PI {
    r += std::f64::consts::TAU;
  }
  r
}

/// A unit quaternion as (axis, angle), the angle in 0..pi (the sign
/// folded into the axis). An identity has angle 0 and no axis.
fn axis_angle(q: [f64; 4]) -> ([f64; 3], f64) {
  let q = if q[3] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
  let q = quat_normalize(q);
  let s = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
  if s <= 0.0 {
    return ([0.0; 3], 0.0);
  }
  let angle = 2.0 * s.atan2(q[3]);
  ([q[0] / s, q[1] / s, q[2] / s], angle)
}

/// The path index of a TRS path.
fn path_index(path: ChannelPath) -> usize {
  match path {
    ChannelPath::Position => 0,
    ChannelPath::Rotation => 1,
    ChannelPath::Scale => 2,
    _ => 0,
  }
}

/// A rest pose's value along a path, widened to four floats.
fn rest_value(rest: &RestPose, path: usize) -> [f64; 4] {
  match path {
    0 => [rest.position[0], rest.position[1], rest.position[2], 0.0],
    1 => rest.rotation,
    _ => [rest.scale[0], rest.scale[1], rest.scale[2], 0.0],
  }
}

/// `a` toward `b` by `t`: a slerp for a rotation, a lerp otherwise.
fn mix(a: [f64; 4], b: [f64; 4], t: f64, rotation: bool) -> [f64; 4] {
  if rotation {
    slerp(a, b, t)
  } else {
    let mut out = a;
    for e in 0..4 {
      out[e] += (b[e] - a[e]) * t;
    }
    out
  }
}

/// Where a player's time sits in a looping window after a step of `dt`
/// seconds: the new time, whether it wrapped, and whether it reached the
/// window's end (a non-looping player holds there).
fn step_time(time: f64, dt: f64, speed: f64, looped: bool, window: (f64, f64)) -> (f64, bool, bool) {
  let (start, end) = window;
  let len = end - start;
  let mut t = time + dt * speed as f64;
  if len <= 0.0 {
    return (start, false, !looped);
  }
  if looped {
    let local = t - start;
    let wrapped = local < 0.0 || local >= len;
    t = start + ((local % len) + len) % len;
    (t, wrapped, false)
  } else if t >= end {
    (end, false, true)
  } else if t <= start {
    (start, false, speed < 0.0)
  } else {
    (t, false, false)
  }
}

/// The marker times of a clip that `common` names, in time order and
/// within `window` (what a marker-synced group aligns on), with their
/// names.
fn sync_sequence(clip: &Clip, common: &[String], window: (f64, f64)) -> Vec<(f64, usize)> {
  clip
    .markers
    .iter()
    .filter(|m| m.time >= window.0 && m.time <= window.1)
    .filter_map(|m| common.iter().position(|n| n == &m.name).map(|k| (m.time, k)))
    .collect()
}

/// A position within a marker sequence: the segment the time is in and
/// the fraction along it. Segments are between consecutive markers; in a
/// looping window the last marker wraps to the first, in a non-looping
/// one the window edges bound the first and last segments (segment
/// index = marker count means "after the last").
fn sequence_position(times: &[f64], t: f64, looped: bool, window: (f64, f64)) -> (usize, f64) {
  let n = times.len();
  let last_at_or_before = times.iter().rposition(|&m| (m as f64) <= t);
  let len = window.1 - window.0;
  match last_at_or_before {
    None => {
      if looped && n > 0 {
        // Before the first marker: in the wrap segment from the last.
        let from = times[n - 1] as f64 - len;
        let to = times[0] as f64;
        let span = to - from;
        (n - 1, if span > 0.0 { (t - from) / span } else { 0.0 })
      } else {
        let from = window.0;
        let to = if n > 0 { times[0] as f64 } else { window.1 };
        let span = to - from;
        (usize::MAX, if span > 0.0 { (t - from) / span } else { 0.0 })
      }
    }
    Some(i) => {
      let from = times[i] as f64;
      let to = if i + 1 < n {
        times[i + 1] as f64
      } else if looped {
        times[0] as f64 + len
      } else {
        window.1
      };
      let span = to - from;
      (i, if span > 0.0 { (t - from) / span } else { 0.0 })
    }
  }
}

/// The time at a sequence position (the inverse of `sequence_position`
/// for a follower's own markers).
fn sequence_time(times: &[f64], segment: usize, fraction: f64, looped: bool, window: (f64, f64)) -> f64 {
  let n = times.len();
  let len = window.1 - window.0;
  let (from, to) = if segment == usize::MAX {
    (window.0, if n > 0 { times[0] as f64 } else { window.1 })
  } else {
    let from = times[segment] as f64;
    let to = if segment + 1 < n {
      times[segment + 1] as f64
    } else if looped {
      times[0] as f64 + len
    } else {
      window.1
    };
    (from, to)
  };
  let t = from + (to - from) * fraction;
  if looped && len > 0.0 {
    window.0 + (((t - window.0) % len) + len) % len
  } else {
    t.clamp(window.0, window.1)
  }
}

impl Spatial {
  // Clips -------------------------------------------------------------

  /// Register a baked clip: its channels (checked here, the one indexing
  /// safety boundary) and the names its Curve channels index. Key-time
  /// ordering is the baker's contract.
  pub fn create_clip(
    &mut self,
    duration: f64,
    channels: Vec<ClipChannel>,
    curves: Vec<String>,
  ) -> Result<ClipId, String> {
    for (i, c) in channels.iter().enumerate() {
      check_channel(i, c)?;
      if c.path == ChannelPath::Curve && c.target_slot as usize >= curves.len() {
        return Err(format!("clip channel {i} names curve {}, only {} curve names given", c.target_slot, curves.len()));
      }
    }
    if !(duration >= 0.0) || !duration.is_finite() {
      return Err("clip duration must be a finite, non-negative number".to_string());
    }
    let id = self.clips.next;
    self.clips.next += 1;
    self.clips.clips.insert(id, Clip { duration, channels, curves, markers: Vec::new() });
    Ok(id)
  }

  /// Free a clip. Players still on it drop at their next advance with a
  /// Dropped event.
  pub fn destroy_clip(&mut self, id: ClipId) -> Result<(), String> {
    self.clips.clips.remove(&id).map(|_| ()).ok_or_else(|| format!("clip {id} not found"))
  }

  /// A clip's duration in seconds.
  pub fn clip_duration(&self, id: ClipId) -> Result<f64, String> {
    self.clips.clips.get(&id).map(|c| c.duration).ok_or_else(|| format!("clip {id} not found"))
  }

  /// Replace a clip's markers (sorted here by time). A player already on
  /// the clip takes them from its next advance, its window states reset.
  pub fn set_clip_markers(&mut self, id: ClipId, mut markers: Vec<ClipMarker>) -> Result<(), String> {
    let clip = self.clips.clips.get_mut(&id).ok_or_else(|| format!("clip {id} not found"))?;
    for (i, m) in markers.iter().enumerate() {
      if !m.time.is_finite() || !(m.duration >= 0.0) || !m.duration.is_finite() {
        return Err(format!("marker {i} needs a finite time and a non-negative finite duration"));
      }
    }
    markers.sort_by(|a, b| a.time.total_cmp(&b.time));
    let count = markers.len();
    clip.markers = markers;
    for rig in &mut self.rigs.rigs {
      for p in rig.players.iter_mut().filter(|p| p.clip == id) {
        p.inside = vec![false; count];
      }
    }
    Ok(())
  }

  /// A clip's markers, for the facade's bookkeeping.
  pub fn clip_markers(&self, id: ClipId) -> Result<&[ClipMarker], String> {
    self.clips.clips.get(&id).map(|c| c.markers.as_slice()).ok_or_else(|| format!("clip {id} not found"))
  }

  /// The additive form of a clip: a second clip whose keys are deltas
  /// against `reference` (see `AdditiveReference`), so that playing it on
  /// an additive layer over the reference pose reproduces the authored
  /// clip. A channel the reference clip lacks, or a joint the rest
  /// reference has no slot for, is taken against the clip's own first
  /// key. Markers and curve names carry over (a curve's delta is against
  /// its reference value too). Registered as a new clip; free it like one.
  pub fn derive_additive(&mut self, clip: ClipId, reference: AdditiveReference) -> Result<ClipId, String> {
    let source = self.clips.clips.get(&clip).ok_or_else(|| format!("clip {clip} not found"))?;
    let mut channels = Vec::with_capacity(source.channels.len());
    let mut value = vec![0.0f64; source.channels.iter().map(|c| c.elements).max().unwrap_or(1)];
    for c in &source.channels {
      let elements = c.elements;
      let mut from_first = true;
      match reference {
        AdditiveReference::FirstFrame => {}
        AdditiveReference::Clip { clip: ref_id, time } => {
          let ref_clip = self.clips.clips.get(&ref_id).ok_or_else(|| format!("reference clip {ref_id} not found"))?;
          let found = if c.path == ChannelPath::Curve {
            let name = &source.curves[c.target_slot as usize];
            ref_clip
              .curves
              .iter()
              .position(|n| n == name)
              .and_then(|k| ref_clip.channel_for(k as u32, ChannelPath::Curve))
          } else {
            ref_clip.channel_for(c.target_slot, c.path)
          };
          if let Some(rc) = found.filter(|rc| rc.elements == elements) {
            let mut cursor = 0;
            sample(rc, time, &mut cursor, &mut value);
            from_first = false;
          }
        }
        AdditiveReference::Rest { rig } => {
          let r = self.rigs.rigs.iter().find(|r| r.id == rig).ok_or_else(|| format!("rig {rig} not found"))?;
          let slot = c.target_slot as usize;
          match c.path {
            ChannelPath::Position | ChannelPath::Rotation | ChannelPath::Scale if slot < r.rest.len() => {
              value[..elements].copy_from_slice(&rest_value(&r.rest[slot], path_index(c.path))[..elements]);
              from_first = false;
            }
            ChannelPath::Weights if slot < r.rest_weights.len() && r.rest_weights[slot].len() == elements => {
              value[..elements].copy_from_slice(&r.rest_weights[slot]);
              from_first = false;
            }
            ChannelPath::Curve => {
              value[..elements].fill(0.0);
              from_first = false;
            }
            _ => {}
          }
        }
      }
      if from_first {
        let (_, mid) = key_layout(c);
        for (v, x) in value[..elements].iter_mut().zip(&c.values[mid..mid + elements]) {
          *v = *x as f64;
        }
      }
      channels.push(additive_channel(c, &value));
    }
    let derived =
      Clip { duration: source.duration, channels, curves: source.curves.clone(), markers: source.markers.clone() };
    let id = self.clips.next;
    self.clips.next += 1;
    self.clips.clips.insert(id, derived);
    Ok(id)
  }

  // Masks -------------------------------------------------------------

  /// Register a mask: `default` for every node not listed, the listed
  /// shares otherwise (0..1 each).
  pub fn create_mask(&mut self, default: f64, shares: Vec<(NodeId, f64)>) -> Result<MaskId, String> {
    Self::check_mask(default, &shares)?;
    let id = self.rigs.next_mask;
    self.rigs.next_mask += 1;
    self.rigs.masks.insert(id, Mask { default, shares: shares.into_iter().collect(), version: 0 });
    Ok(id)
  }

  /// Replace a mask's shares; every layer and player on it re-resolves at
  /// the next advance.
  pub fn set_mask(&mut self, id: MaskId, default: f64, shares: Vec<(NodeId, f64)>) -> Result<(), String> {
    Self::check_mask(default, &shares)?;
    let mask = self.rigs.masks.get_mut(&id).ok_or_else(|| format!("mask {id} not found"))?;
    mask.default = default;
    mask.shares = shares.into_iter().collect();
    mask.version += 1;
    Ok(())
  }

  /// Free a mask. A layer or player still naming it blends unmasked from
  /// the next advance on.
  pub fn destroy_mask(&mut self, id: MaskId) -> Result<(), String> {
    self.rigs.masks.remove(&id).map(|_| ()).ok_or_else(|| format!("mask {id} not found"))
  }

  fn check_mask(default: f64, shares: &[(NodeId, f64)]) -> Result<(), String> {
    if !(0.0..=1.0).contains(&default) {
      return Err(format!("mask default must be within 0..1, got {default}"));
    }
    if let Some((node, share)) = shares.iter().find(|(_, s)| !(0.0..=1.0).contains(s)) {
      return Err(format!("mask share for node {node} must be within 0..1, got {share}"));
    }
    Ok(())
  }

  // Rigs --------------------------------------------------------------

  /// Create a rig over `joints` (the slots clip channels target), clocked
  /// on `clock` (the node whose rate, `set_time_scale`, the rig's time
  /// runs at: the model's root for a mixer). The layers compose over each
  /// joint's rest pose (`set_rest`, else the pose as it stands now); the
  /// joints' morph weights as they stand are the rest weights. Every node
  /// must be live NOW; a later death drops the rig (a `RigDropped` event
  /// after its players' `Dropped`). The base layer (override, weight 1,
  /// unmasked) exists from the start.
  pub fn create_rig(&mut self, clock: NodeId, joints: Vec<NodeId>) -> Result<RigId, String> {
    self.resolve(clock).map_err(|e| format!("clock: {e}"))?;
    let mut rest = Vec::with_capacity(joints.len());
    let mut rest_weights = Vec::with_capacity(joints.len());
    for (i, &j) in joints.iter().enumerate() {
      let idx = self.resolve(j).map_err(|e| format!("joint {i}: {e}"))?;
      rest.push(self.rest_of(j)?);
      rest_weights.push(self.nodes[idx as usize].weights.clone());
    }
    let id = self.rigs.next_rig;
    self.rigs.next_rig += 1;
    let count = joints.len();
    self.rigs.rigs.push(Rig {
      id,
      clock,
      joints,
      rest,
      rest_weights,
      layers: vec![Layer::new(LayerSpec { weight: 1.0, mode: LayerMode::Override, mask: None })],
      players: Vec::new(),
      curves: Vec::new(),
      curve_index: HashMap::new(),
      trs: vec![[Slot::EMPTY; 3]; count],
      morph: vec![Vec::new(); count],
      curve_slots: Vec::new(),
      acc_trs: vec![[Acc::EMPTY; 3]; count],
      acc_morph: vec![Vec::new(); count],
      acc_curves: Vec::new(),
      total: vec![0.0; count],
      frame: 0,
      pass: 0,
      pending_inertial: None,
      sinks: Vec::new(),
    });
    Ok(id)
  }

  /// Remove a rig and its players at once (no events), the joints
  /// holding whatever was last written.
  pub fn destroy_rig(&mut self, id: RigId) -> Result<(), String> {
    let at = self.rig_index(id)?;
    let rig = self.rigs.rigs.remove(at);
    for p in &rig.players {
      self.rigs.player_rig.remove(&p.id);
    }
    self.rigs.groups.retain(|_, g| g.rig != id);
    Ok(())
  }

  pub(super) fn rig_index(&self, id: RigId) -> Result<usize, String> {
    self.rigs.rigs.iter().position(|r| r.id == id).ok_or_else(|| format!("rig {id} not found"))
  }

  fn rig_mut(&mut self, id: RigId) -> Result<&mut Rig, String> {
    self.rigs.rigs.iter_mut().find(|r| r.id == id).ok_or_else(|| format!("rig {id} not found"))
  }

  /// Where a player lives: its rig and index.
  fn locate(&self, id: PlayerId) -> Result<(usize, usize), String> {
    let rig = *self.rigs.player_rig.get(&id).ok_or_else(|| format!("player {id} not found"))?;
    let at = self.rig_index(rig)?;
    let pi =
      self.rigs.rigs[at].players.iter().position(|p| p.id == id).ok_or_else(|| format!("player {id} not found"))?;
    Ok((at, pi))
  }

  // Layers ------------------------------------------------------------

  /// Append a layer above the existing ones; returns its index.
  pub fn create_layer(&mut self, rig: RigId, spec: LayerSpec) -> Result<usize, String> {
    if !(0.0..=1.0).contains(&spec.weight) {
      return Err(format!("layer weight must be within 0..1, got {}", spec.weight));
    }
    if let Some(m) = spec.mask {
      if !self.rigs.masks.contains_key(&m) {
        return Err(format!("mask {m} not found"));
      }
    }
    let r = self.rig_mut(rig)?;
    r.layers.push(Layer::new(spec));
    Ok(r.layers.len() - 1)
  }

  /// Write a layer's weight (ramped over `fade_ms` of the rig's time) and
  /// mask.
  pub fn set_layer(&mut self, rig: RigId, index: usize, update: LayerUpdate) -> Result<(), String> {
    if let Some(w) = update.weight {
      if !(0.0..=1.0).contains(&w) {
        return Err(format!("layer weight must be within 0..1, got {w}"));
      }
    }
    if !(update.fade_ms >= 0.0) {
      return Err("the fade must be >= 0 ms".to_string());
    }
    if let Some(Some(m)) = update.mask {
      if !self.rigs.masks.contains_key(&m) {
        return Err(format!("mask {m} not found"));
      }
    }
    let r = self.rig_mut(rig)?;
    let layer = r.layers.get_mut(index).ok_or_else(|| format!("rig {rig} has no layer {index}"))?;
    if let Some(w) = update.weight {
      layer.weight_target = w;
      if update.fade_ms <= 0.0 || (w - layer.weight).abs() <= WEIGHT_SETTLE {
        layer.weight = w;
        layer.weight_rate = 0.0;
      } else {
        layer.weight_rate = (w - layer.weight) / (update.fade_ms / 1000.0);
      }
    }
    if let Some(m) = update.mask {
      layer.mask = m;
    }
    Ok(())
  }

  /// Remove a layer above the base and its players (no events); the
  /// layers above it move down one index.
  pub fn destroy_layer(&mut self, rig: RigId, index: usize) -> Result<(), String> {
    if index == 0 {
      return Err("the base layer cannot be destroyed".to_string());
    }
    let at = self.rig_index(rig)?;
    let r = &mut self.rigs.rigs[at];
    if index >= r.layers.len() {
      return Err(format!("rig {rig} has no layer {index}"));
    }
    r.layers.remove(index);
    let mut gone = Vec::new();
    r.players.retain(|p| {
      if p.layer == index {
        gone.push(p.id);
        false
      } else {
        true
      }
    });
    for p in &mut r.players {
      if p.layer > index {
        p.layer -= 1;
      }
    }
    for id in gone {
      self.rigs.player_rig.remove(&id);
      for g in self.rigs.groups.values_mut() {
        g.members.retain(|&m| m != id);
      }
    }
    Ok(())
  }

  /// A layer's current weight (its ramp included), for tests and
  /// bookkeeping.
  pub fn layer_weight(&self, rig: RigId, index: usize) -> Result<f64, String> {
    let r = &self.rigs.rigs[self.rig_index(rig)?];
    r.layers.get(index).map(|l| l.weight).ok_or_else(|| format!("rig {rig} has no layer {index}"))
  }

  // Players -----------------------------------------------------------

  /// Start a player on `clip` on the rig's layer `spec.layer`. Every
  /// joint channel must target a slot the rig has; curve channels bind
  /// their names to the rig's curves (new names are added).
  pub fn create_player(&mut self, rig: RigId, clip: ClipId, spec: PlayerSpec) -> Result<PlayerId, String> {
    let clip_ref = self.clips.clips.get(&clip).ok_or_else(|| format!("clip {clip} not found"))?;
    let at = self.rig_index(rig)?;
    let r = &self.rigs.rigs[at];
    if spec.layer >= r.layers.len() {
      return Err(format!("rig {rig} has no layer {}", spec.layer));
    }
    for (i, c) in clip_ref.channels.iter().enumerate() {
      if c.path.on_joint() && c.target_slot as usize >= r.joints.len() {
        return Err(format!("clip channel {i} targets slot {}, the rig has {} joints", c.target_slot, r.joints.len()));
      }
    }
    if let Some(m) = spec.mask {
      if !self.rigs.masks.contains_key(&m) {
        return Err(format!("mask {m} not found"));
      }
    }
    if !(0.0..=1.0).contains(&spec.weight) {
      return Err(format!("weight must be within 0..1, got {}", spec.weight));
    }
    if !(spec.fade_out >= 0.0) || !spec.speed.is_finite() || !spec.fade.is_finite() || !spec.time.is_finite() {
      return Err("speed, fade, time and fade-out must be finite (fade-out >= 0)".to_string());
    }
    let window = Self::check_window(spec.window, clip_ref.duration)?;
    let channels = clip_ref.channels.len();
    let markers = clip_ref.markers.len();
    let curve_names: Vec<Option<String>> = clip_ref
      .channels
      .iter()
      .map(|c| (c.path == ChannelPath::Curve).then(|| clip_ref.curves[c.target_slot as usize].clone()))
      .collect();
    let r = &mut self.rigs.rigs[at];
    let curve_slots = curve_names
      .into_iter()
      .map(|name| match name {
        None => usize::MAX,
        Some(name) => r.curve_slot(name),
      })
      .collect();
    let id = self.rigs.next_player;
    self.rigs.next_player += 1;
    let (start, end) = window.unwrap_or((0.0, clip_ref.duration));
    r.players.push(Player {
      id,
      layer: spec.layer,
      clip,
      time: spec.time.clamp(start, end),
      speed: spec.speed,
      weight: spec.weight,
      fade: spec.fade,
      looped: spec.looped,
      finished: false,
      window,
      fade_out: spec.fade_out,
      end_fading: false,
      dying: false,
      mask: spec.mask,
      shares: Vec::new(),
      seen: None,
      cursors: vec![0; channels],
      curve_slots,
      inside: vec![false; markers],
      inclusive: true,
      group: None,
      step: (spec.time, false),
      root: None,
    });
    self.rigs.player_rig.insert(id, rig);
    Ok(id)
  }

  fn check_window(window: Option<(f64, f64)>, duration: f64) -> Result<Option<(f64, f64)>, String> {
    match window {
      None => Ok(None),
      Some((start, end)) => {
        if !(start >= 0.0) || !(end > start) || !(end <= duration) {
          return Err(format!("window {start}..{end} must satisfy 0 <= start < end <= clip duration {duration}"));
        }
        Ok(Some((start, end)))
      }
    }
  }

  /// Write the given fields of a player. Unknown ids err - a Dropped
  /// player is gone. Returns the window changes a seek made, in marker
  /// order: a seek fires none of the instants it jumps over, but a
  /// window is a state, so the ones it leaves end and the ones it lands
  /// in begin (every Begin keeps its End), reported here to the caller
  /// at once rather than as frame events.
  pub fn set_player(&mut self, id: PlayerId, update: PlayerUpdate) -> Result<Vec<(usize, MarkerPhase)>, String> {
    if let Some(w) = update.weight {
      if !(0.0..=1.0).contains(&w) {
        return Err(format!("weight must be within 0..1, got {w}"));
      }
    }
    if let Some(Some(m)) = update.mask {
      if !self.rigs.masks.contains_key(&m) {
        return Err(format!("mask {m} not found"));
      }
    }
    let (at, pi) = self.locate(id)?;
    let duration = self.clips.clips.get(&self.rigs.rigs[at].players[pi].clip).map(|c| c.duration).unwrap_or(0.0);
    let window = match update.window {
      Some(w) => Some(Self::check_window(w, duration)?),
      None => None,
    };
    let p = &mut self.rigs.rigs[at].players[pi];
    if let Some(w) = update.weight {
      p.weight = w;
    }
    if let Some(f) = update.fade {
      p.fade = f;
      if f >= 0.0 {
        p.end_fading = false;
      }
    }
    if let Some(s) = update.speed {
      p.speed = s;
    }
    if let Some(w) = window {
      p.window = w;
    }
    let seek = update.time.is_some();
    if let Some(t) = update.time {
      p.time = t;
      p.finished = false;
      p.inclusive = true;
    }
    let (start, end) = p.window(duration);
    p.time = p.time.clamp(start, end);
    if let Some(m) = update.mask {
      p.mask = m;
    }
    let mut changes = Vec::new();
    if seek {
      let p = &self.rigs.rigs[at].players[pi];
      let Some(clip) = self.clips.clips.get(&p.clip) else { return Ok(changes) };
      let time = p.time as f64;
      for (m, marker) in clip.markers.iter().enumerate() {
        if marker.duration <= 0.0 || (marker.time as f64) < start || (marker.time as f64) > end {
          continue;
        }
        let close = (marker.time + marker.duration).min(end as f64);
        let now_inside = time >= marker.time && time <= close;
        if now_inside != p.inside[m] {
          changes.push((m, if now_inside { MarkerPhase::Begin } else { MarkerPhase::End }));
        }
      }
      let p = &mut self.rigs.rigs[at].players[pi];
      for &(m, phase) in &changes {
        p.inside[m] = phase == MarkerPhase::Begin;
      }
    }
    Ok(changes)
  }

  /// Remove a player at once (no end event; stop-with-fade is a
  /// `set_player` fade write instead). A missing id is fine - it may
  /// have dropped. Returns the markers whose windows the player was
  /// inside, which end with it: the caller's to report, at once.
  pub fn destroy_player(&mut self, id: PlayerId) -> Vec<usize> {
    let Ok((at, pi)) = self.locate(id) else { return Vec::new() };
    let p = self.rigs.rigs[at].players.remove(pi);
    self.rigs.player_rig.remove(&id);
    for g in self.rigs.groups.values_mut() {
      g.members.retain(|&m| m != id);
    }
    p.inside.iter().enumerate().filter(|(_, inside)| **inside).map(|(m, _)| m).collect()
  }

  /// A player's current time in seconds.
  pub fn player_time(&self, id: PlayerId) -> Result<f64, String> {
    let (at, pi) = self.locate(id)?;
    Ok(self.rigs.rigs[at].players[pi].time)
  }

  /// A player's current weight (its fade included).
  pub fn player_weight(&self, id: PlayerId) -> Result<f64, String> {
    let (at, pi) = self.locate(id)?;
    Ok(self.rigs.rigs[at].players[pi].weight)
  }

  /// Whether the player is inside marker `marker`'s window as of the
  /// last advance.
  pub fn marker_inside(&self, id: PlayerId, marker: usize) -> Result<bool, String> {
    let (at, pi) = self.locate(id)?;
    let p = &self.rigs.rigs[at].players[pi];
    p.inside.get(marker).copied().ok_or_else(|| format!("player {id} has no marker {marker}"))
  }

  /// Bind root motion to a player (see `RootMotion`); a second call
  /// rebinds. The channel must be a position channel of a registered
  /// clip; an anchor must be a live node; the player's layer must not be
  /// additive.
  pub fn bind_root_motion(&mut self, id: PlayerId, binding: RootMotion) -> Result<(), String> {
    let len = (binding.up[0] * binding.up[0] + binding.up[1] * binding.up[1] + binding.up[2] * binding.up[2]).sqrt();
    if !(len > 0.0) {
      return Err("root motion up axis is zero".to_string());
    }
    let up = [binding.up[0] / len, binding.up[1] / len, binding.up[2] / len];
    let binding = RootMotion { up, ..binding };
    let clip = self.clips.clips.get(&binding.clip).ok_or_else(|| format!("clip {} not found", binding.clip))?;
    let channel = clip
      .channels
      .get(binding.channel as usize)
      .ok_or_else(|| format!("clip {} has no channel {}", binding.clip, binding.channel))?;
    if channel.path != ChannelPath::Position {
      return Err(format!("clip {} channel {} is not a position channel", binding.clip, binding.channel));
    }
    let (stride, mid) = key_layout(channel);
    let v = &channel.values;
    let first = mid;
    let last = v.len() - stride + mid;
    let drift = [(v[last] - v[first]) as f64, (v[last + 1] - v[first + 1]) as f64, (v[last + 2] - v[first + 2]) as f64];
    let mut yaw_drift = 0.0;
    if let Some(ri) = binding.rotation {
      let rc = clip.channels.get(ri as usize).ok_or_else(|| format!("clip {} has no channel {}", binding.clip, ri))?;
      if rc.path != ChannelPath::Rotation {
        return Err(format!("clip {} channel {} is not a rotation channel", binding.clip, ri));
      }
      // Unwrapped key by key, so a full turn counts as one, not zero.
      let (stride, mid) = key_layout(rc);
      let key = |k: usize| {
        [
          rc.values[k + mid] as f64,
          rc.values[k + mid + 1] as f64,
          rc.values[k + mid + 2] as f64,
          rc.values[k + mid + 3] as f64,
        ]
      };
      let mut prev = twist_of(key(0), up);
      let mut k = stride;
      while k + mid + 3 < rc.values.len() {
        let y = twist_of(key(k), up);
        yaw_drift += wrap_angle(y - prev);
        prev = y;
        k += stride;
      }
    }
    if let Some(anchor) = binding.anchor {
      self.resolve(anchor)?;
    }
    let (at, pi) = self.locate(id)?;
    let rig = &self.rigs.rigs[at];
    let p = &rig.players[pi];
    if rig.layers[p.layer].mode == LayerMode::Additive {
      return Err("a player on an additive layer carries no root motion".to_string());
    }
    // Primed at the player's current time, so the very next advance
    // already delivers travel (a play() loses no frame).
    let mut cursor = 0;
    let mut now = [0.0f64; 4];
    sample(&clip.channels[binding.channel as usize], p.time as f64, &mut cursor, &mut now);
    let mut yaw_cursor = 0;
    let last_yaw = match binding.rotation {
      Some(ri) => {
        let mut q = [0.0f64; 4];
        sample(&clip.channels[ri as usize], p.time as f64, &mut yaw_cursor, &mut q);
        twist_of(q, up)
      }
      None => 0.0,
    };
    self.rigs.rigs[at].players[pi].root = Some(RootState {
      binding,
      drift,
      last: [now[0], now[1], now[2]],
      cursor,
      yaw_drift,
      last_yaw,
      yaw_cursor,
      clip_yaw: 0.0,
    });
    Ok(())
  }

  // Groups ------------------------------------------------------------

  /// Make a sync group of players on `rig`: the heaviest member leads,
  /// the others follow its phase (see `SyncMode`). A player leaves its
  /// previous group.
  pub fn create_group(&mut self, rig: RigId, members: Vec<PlayerId>, mode: SyncMode) -> Result<GroupId, String> {
    self.rig_index(rig)?;
    let id = self.rigs.next_group;
    self.rigs.next_group += 1;
    self.rigs.groups.insert(id, Group { rig, members: Vec::new(), mode, leader: None });
    if let Err(e) = self.set_group(id, members) {
      self.rigs.groups.remove(&id);
      return Err(e);
    }
    Ok(id)
  }

  /// Replace a group's members.
  pub fn set_group(&mut self, id: GroupId, members: Vec<PlayerId>) -> Result<(), String> {
    let rig = self.rigs.groups.get(&id).map(|g| g.rig).ok_or_else(|| format!("group {id} not found"))?;
    let at = self.rig_index(rig)?;
    for &m in &members {
      if self.rigs.player_rig.get(&m) != Some(&rig) {
        return Err(format!("player {m} is not on rig {rig}"));
      }
    }
    let r = &mut self.rigs.rigs[at];
    for p in &mut r.players {
      if p.group == Some(id) {
        p.group = None;
      }
      if members.contains(&p.id) {
        p.group = Some(id);
      }
    }
    for g in self.rigs.groups.values_mut() {
      if g.rig == rig {
        g.members.retain(|m| !members.contains(m));
      }
    }
    let g = self.rigs.groups.get_mut(&id).ok_or_else(|| format!("group {id} not found"))?;
    g.members = members;
    if g.leader.is_some_and(|l| !g.members.contains(&l)) {
      g.leader = None;
    }
    Ok(())
  }

  /// Dissolve a group; its members keep their own time from here.
  pub fn destroy_group(&mut self, id: GroupId) -> Result<(), String> {
    let g = self.rigs.groups.remove(&id).ok_or_else(|| format!("group {id} not found"))?;
    if let Ok(at) = self.rig_index(g.rig) {
      for p in &mut self.rigs.rigs[at].players {
        if p.group == Some(id) {
          p.group = None;
        }
      }
    }
    Ok(())
  }

  // Transitions and reads ---------------------------------------------

  /// Request an inertial transition: at the next advance every slot the
  /// layers feed keeps the offset from the pose it last wrote and decays
  /// it over `ms` of the rig's time. Call it in the same frame as the
  /// player change it smooths (a play, a stop, a layer switched on, a
  /// seek); a request during a running transition re-captures from the
  /// pose as it stands.
  pub fn inertialize(&mut self, rig: RigId, ms: f64) -> Result<(), String> {
    if !(ms > 0.0) || !ms.is_finite() {
      return Err("the duration must be a positive number of milliseconds".to_string());
    }
    let r = self.rig_mut(rig)?;
    r.pending_inertial = Some(ms / 1000.0);
    Ok(())
  }

  /// A curve's value as of the last advance (None for a name no clip on
  /// the rig has, or one nothing fed this frame).
  pub fn curve_value(&self, rig: RigId, name: &str) -> Result<Option<f64>, String> {
    let r = &self.rigs.rigs[self.rig_index(rig)?];
    Ok(r.curve_index.get(name).and_then(|&k| {
      let slot = &r.curve_slots[k];
      (slot.fed == r.frame && r.frame > 0).then_some(slot.value[0])
    }))
  }

  /// Route a rig's curve into the scalar param `name` of draw entry
  /// `draw` on `target` (None: a target-level param): every flush after
  /// an advance that fed the curve writes its value when it changed,
  /// through `SinkWriter::write_param`. A name no clip on the rig
  /// carries yet binds all the same and writes once a clip feeds it. A
  /// second bind on the same (curve, target, draw) replaces the name.
  /// Dropped when the write stops landing (the entry or target died).
  pub fn bind_curve_param(
    &mut self,
    rig: RigId,
    curve: &str,
    target: u64,
    draw: Option<u64>,
    name: String,
  ) -> Result<(), String> {
    let r = self.rig_mut(rig)?;
    let k = r.curve_slot(curve.to_string());
    r.sinks.retain(|s| !(s.curve == k && s.target == target && s.draw == draw));
    r.sinks.push(CurveSink { curve: k, target, draw, name, last: None });
    Ok(())
  }

  /// Remove the curve's binding on (target, draw); a missing one is fine.
  pub fn unbind_curve_param(&mut self, rig: RigId, curve: &str, target: u64, draw: Option<u64>) -> Result<(), String> {
    let r = self.rig_mut(rig)?;
    let Some(&k) = r.curve_index.get(curve) else { return Ok(()) };
    r.sinks.retain(|s| !(s.curve == k && s.target == target && s.draw == draw));
    Ok(())
  }

  /// The flush's curve publish: every bound curve fed by the last
  /// advance whose value changed since its last write.
  pub(super) fn flush_curve_sinks(&mut self, out: &mut dyn SinkWriter) {
    for rig in &mut self.rigs.rigs {
      let frame = rig.frame;
      let slots = &rig.curve_slots;
      rig.sinks.retain_mut(|sink| {
        let slot = &slots[sink.curve];
        if frame == 0 || slot.fed != frame || sink.last == Some(slot.value[0]) {
          return true;
        }
        let landed = out.write_param(sink.target, sink.draw, &sink.name, slot.value[0] as f32);
        sink.last = Some(slot.value[0]);
        landed
      });
    }
  }

  /// The node's current local TRS (what the rigs last wrote, or any
  /// later snap): position, quaternion, scale. The pose read for
  /// root-motion strips and skeleton copies.
  pub fn transform_of(&self, id: NodeId) -> Result<([f64; 3], [f64; 4], [f64; 3]), String> {
    let n = &self.nodes[self.resolve(id)? as usize];
    Ok((n.position, n.rotation, n.scale))
  }

  /// The events since the last drain, in order.
  pub fn take_clip_events(&mut self) -> Vec<ClipEvent> {
    std::mem::take(&mut self.rigs.events)
  }

  /// The root-motion deltas of the last advance, one per bound player
  /// (zero while a player is primed or frozen).
  pub fn take_root_motion(&mut self) -> Vec<(PlayerId, [f64; 3], f64)> {
    std::mem::take(&mut self.rigs.root_deltas)
  }

  // The advance -------------------------------------------------------

  /// Advance every rig by the app time since the last advance
  /// (`set_transition_now` stamps it before any frame work), each at its
  /// clock node's rate, and write the composed poses through the snap
  /// path. Called once per frame BEFORE the frame's JS; the embedder
  /// drains `take_clip_events` right after and keeps requesting frames
  /// while `active`.
  pub fn advance_players(&mut self) -> PlayersTick {
    let now = self.transitions.now_ms;
    let frame_dt = ((now - self.rigs.last_ms).max(0.0)) / 1000.0;
    self.rigs.last_ms = now;
    if self.rigs.rigs.is_empty() {
      return PlayersTick::default();
    }
    self.rigs.root_deltas.clear();
    self.rigs.anchor_moves.clear();
    let mut rigs = std::mem::take(&mut self.rigs.rigs);
    let mut tick = PlayersTick::default();
    rigs.retain_mut(|rig| {
      let live = |t: NodeId| {
        let idx = super::index(t);
        self.nodes.get(idx).filter(|n| n.alive && n.generation == super::generation(t)).is_some()
      };
      let rate = super::rate_in(&self.nodes, rig.clock);
      let alive = rate.is_some() && rig.joints.iter().all(|&j| live(j));
      if !alive {
        self.drop_rig(rig);
        return false;
      }
      let dt = frame_dt * rate.unwrap_or(1.0) as f64;
      let (active, wrote) = self.step_rig(rig, dt, rate.unwrap_or(1.0) > 0.0);
      tick.active |= active;
      tick.wrote |= wrote;
      true
    });
    self.rigs.rigs = rigs;
    // Root motion onto the anchors: the delta is in the root's parent
    // space, which is the anchor's own local frame, so it turns with the
    // anchor's rotation before it moves the anchor.
    let moves = std::mem::take(&mut self.rigs.anchor_moves);
    for (anchor, delta, yaw, up) in moves {
      let Ok(idx) = self.resolve(anchor) else { continue };
      let n = &mut self.nodes[idx as usize];
      let step = rotate_vector(n.rotation, delta);
      for e in 0..3 {
        n.position[e] += step[e];
      }
      if yaw != 0.0 {
        // A turn about the anchor's own up: post-multiplied, as the
        // translation is taken in the anchor's local frame.
        n.rotation = quat_normalize(quat_multiply(n.rotation, axis_quat(up, yaw)));
      }
      n.local_dirty = true;
      self.enqueue(idx);
      tick.wrote = true;
    }
    tick
  }

  /// A rig whose clock or joint died: its players drop (windows closed),
  /// then the rig.
  fn drop_rig(&mut self, rig: &mut Rig) {
    for p in rig.players.drain(..) {
      self.rigs.player_rig.remove(&p.id);
      let weight = p.weight * rig.layers[p.layer].weight;
      for (m, inside) in p.inside.iter().enumerate() {
        if *inside {
          self.rigs.events.push(ClipEvent::Marker {
            player: p.id,
            marker: m,
            phase: MarkerPhase::End,
            weight,
            layer: p.layer,
          });
        }
      }
      self.rigs.events.push(ClipEvent::Dropped(p.id));
    }
    self.rigs.groups.retain(|_, g| g.rig != rig.id);
    self.rigs.events.push(ClipEvent::RigDropped(rig.id));
  }

  /// One rig's advance: the lifecycle and time pass, the layered blend,
  /// the inertial stage, the write. `running` is whether the clock
  /// moves at all (a frozen rig is neither demand nor a write).
  fn step_rig(&mut self, rig: &mut Rig, dt: f64, running: bool) -> (bool, bool) {
    rig.frame += 1;
    let mut active = false;

    // The mask shares, current before anything reads them (the root
    // motion's effective weight, the blend).
    for li in 0..rig.layers.len() {
      self.refresh_layer_shares(rig, li);
    }
    for pi in 0..rig.players.len() {
      self.refresh_player_shares(rig, pi);
    }

    // Layer fades.
    for layer in &mut rig.layers {
      if layer.weight_rate != 0.0 {
        layer.weight += layer.weight_rate * dt as f64;
        let overshot = (layer.weight_rate > 0.0 && layer.weight >= layer.weight_target)
          || (layer.weight_rate < 0.0 && layer.weight <= layer.weight_target)
          || (layer.weight - layer.weight_target).abs() <= WEIGHT_SETTLE;
        if overshot {
          layer.weight = layer.weight_target;
          layer.weight_rate = 0.0;
        } else {
          active = true;
        }
      }
    }

    // Lifecycle: a player whose clip died drops; fades step, an end fade
    // starts; a player fading out past zero drops (finishing first when
    // it was an end fade).
    let mut i = 0;
    while i < rig.players.len() {
      let p = &mut rig.players[i];
      let Some(clip) = self.clips.clips.get(&p.clip) else {
        let p = rig.players.remove(i);
        self.remove_player(rig, p, false);
        continue;
      };
      let window = p.window(clip.duration);
      if p.fade_out > 0.0 && !p.looped && !p.end_fading && p.fade >= 0.0 && p.weight > 0.0 {
        let near_end = if p.speed > 0.0 {
          p.time >= window.1 - p.fade_out as f64
        } else if p.speed < 0.0 {
          p.time <= window.0 + p.fade_out as f64
        } else {
          false
        };
        if near_end {
          p.fade = -1.0 / p.fade_out;
          p.end_fading = true;
        }
      }
      if p.fade != 0.0 {
        p.weight += p.fade * dt as f64;
        if p.weight >= 1.0 {
          p.weight = 1.0;
          p.fade = 0.0;
        } else if p.weight <= 0.0 && p.fade < 0.0 {
          // Fading OUT past zero removes, after one more blend at weight
          // 0 (below); a fade-IN sitting at zero (just created, or a
          // dt-0 advance under a frozen clock) stays.
          p.weight = 0.0;
          p.fade = 0.0;
          p.dying = true;
        }
      }
      i += 1;
    }

    // Time: singles and leaders advance on their own, followers take
    // their group's phase.
    let groups: Vec<GroupId> = self.rigs.groups.iter().filter(|(_, g)| g.rig == rig.id).map(|(&id, _)| id).collect();
    for gid in &groups {
      let Some(g) = self.rigs.groups.get_mut(gid) else { continue };
      g.members.retain(|m| rig.players.iter().any(|p| p.id == *m));
      let mut best: Option<(PlayerId, f64)> = None;
      for &m in &g.members {
        let w = rig.players.iter().find(|p| p.id == m).map(|p| p.weight).unwrap_or(0.0);
        let keep_current = g.leader == Some(m);
        if best.map_or(true, |(_, bw)| w > bw || (w == bw && keep_current)) {
          best = Some((m, w));
        }
      }
      g.leader = best.map(|(m, _)| m);
    }
    for p in &mut rig.players {
      let Some(clip) = self.clips.clips.get(&p.clip) else { continue };
      let follower = p.group.is_some_and(|g| self.rigs.groups.get(&g).is_some_and(|g| g.leader != Some(p.id)));
      let prev = p.time;
      if follower || p.dying {
        // A dying player's last frame is a blend, not a step: no wrap,
        // no marker, no travel from it.
        p.step = (prev, false);
        continue;
      }
      let (t, wrapped, reached) = step_time(p.time, dt, p.speed, p.looped, p.window(clip.duration));
      p.time = t;
      p.step = (prev, wrapped);
      if reached && !p.finished {
        p.finished = true;
        self.rigs.events.push(ClipEvent::Finished(p.id));
      }
      if running && (!p.finished || p.fade != 0.0) {
        active = true;
      }
    }
    for gid in &groups {
      let g = &self.rigs.groups[gid];
      let Some(leader_id) = g.leader else { continue };
      let Some(leader) = rig.players.iter().find(|p| p.id == leader_id) else { continue };
      let Some(lclip) = self.clips.clips.get(&leader.clip) else { continue };
      let lwindow = leader.window(lclip.duration);
      let llen = lwindow.1 - lwindow.0;
      let fraction = if llen > 0.0 { (leader.time - lwindow.0) / llen } else { 0.0 };
      let finished = leader.finished;
      let looped = leader.looped;
      // Marker sync: the names every member's clip carries, and the
      // leader's position among them.
      let common: Vec<String> = if g.mode == SyncMode::Markers {
        let mut names: Vec<String> = lclip.markers.iter().map(|m| m.name.clone()).collect();
        names.dedup();
        names.retain(|n| {
          g.members.iter().all(|&m| {
            rig
              .players
              .iter()
              .find(|p| p.id == m)
              .and_then(|p| self.clips.clips.get(&p.clip))
              .is_some_and(|c| c.markers.iter().any(|mk| &mk.name == n))
          })
        });
        names
      } else {
        Vec::new()
      };
      let leader_seq = sync_sequence(lclip, &common, lwindow);
      let leader_times: Vec<f64> = leader_seq.iter().map(|&(t, _)| t).collect();
      let leader_pos = (!leader_seq.is_empty()).then(|| sequence_position(&leader_times, leader.time, looped, lwindow));
      for &m in &g.members {
        if m == leader_id {
          continue;
        }
        let Some(p) = rig.players.iter_mut().find(|p| p.id == m) else { continue };
        let Some(clip) = self.clips.clips.get(&p.clip) else { continue };
        let window = p.window(clip.duration);
        let prev = p.time;
        let mut t = None;
        if let Some((segment, frac)) = leader_pos {
          let seq = sync_sequence(clip, &common, window);
          let same = seq.len() == leader_seq.len() && seq.iter().zip(&leader_seq).all(|(a, b)| a.1 == b.1);
          if same {
            let times: Vec<f64> = seq.iter().map(|&(t, _)| t).collect();
            t = Some(sequence_time(&times, segment, frac, looped, window));
          }
        }
        let t = t.unwrap_or(window.0 + (window.1 - window.0) * fraction);
        let forward = p.speed >= 0.0;
        let wrapped = p.looped && if forward { t < prev } else { t > prev } && dt > 0.0;
        p.time = t;
        p.step = (prev, wrapped);
        if finished && !p.looped && !p.finished {
          p.finished = true;
          self.rigs.events.push(ClipEvent::Finished(p.id));
        }
      }
    }

    // Events of the step: loops, markers, root motion.
    let mut crossed = std::mem::take(&mut self.rigs.crossed);
    for p in &mut rig.players {
      if p.dying {
        continue;
      }
      let Some(clip) = self.clips.clips.get(&p.clip) else { continue };
      let (prev, wrapped) = p.step;
      let window = p.window(clip.duration);
      let weight = p.weight * rig.layers[p.layer].weight;
      if wrapped {
        self.rigs.events.push(ClipEvent::Loop(p.id));
      }
      if !clip.markers.is_empty() && (prev != p.time || p.inclusive) {
        let forward = p.speed >= 0.0;
        crossed.clear();
        // A player starting inside a window (created at a time, or
        // seeked there by a write that already reported it) is in it.
        if p.inclusive {
          let at = prev as f64;
          for (m, marker) in clip.markers.iter().enumerate() {
            if marker.duration <= 0.0 || marker.time < window.0 || marker.time > window.1 {
              continue;
            }
            let close = (marker.time + marker.duration).min(window.1 as f64);
            if at > marker.time && at < close && !p.inside[m] {
              p.inside[m] = true;
              crossed.push((m, MarkerPhase::Begin));
            }
          }
        }
        let wrap = wrapped.then_some((window.0 as f64, window.1 as f64));
        crossings(
          &clip.markers,
          prev as f64,
          p.time as f64,
          forward,
          wrap,
          (window.0 as f64, window.1 as f64),
          p.inclusive,
          &mut p.inside,
          &mut crossed,
        );
        for &(marker, phase) in &crossed {
          self.rigs.events.push(ClipEvent::Marker { player: p.id, marker, phase, weight, layer: p.layer });
        }
        if p.finished && !p.looped {
          for (m, inside) in p.inside.iter_mut().enumerate() {
            if *inside {
              *inside = false;
              self.rigs.events.push(ClipEvent::Marker {
                player: p.id,
                marker: m,
                phase: MarkerPhase::End,
                weight,
                layer: p.layer,
              });
            }
          }
        }
      }
      p.inclusive = false;
      // Root motion: the bound channels' travel since the last advance,
      // continuous across a loop wrap (the wrap direction follows the
      // sign of the speed: the wrap adds the clip's net drift, the yaw
      // folded back into a half turn), weighted by the effective weight
      // at the root joint.
      if let Some(binding) = p.root.as_ref().map(|r| r.binding) {
        let Some(rclip) = self.clips.clips.get(&binding.clip) else { continue };
        let Some(channel) = rclip.channels.get(binding.channel as usize) else { continue };
        let joint = channel.target_slot as usize;
        let share = rig.layers[p.layer].share(joint) * p.share(joint);
        let effective = weight * share;
        let Some(root) = p.root.as_mut() else { continue };
        let up = binding.up;
        let mut now = [0.0f64; 4];
        sample(channel, p.time as f64, &mut root.cursor, &mut now);
        let yaw_now = match root.binding.rotation.and_then(|ri| rclip.channels.get(ri as usize)) {
          Some(rc) => {
            let mut q = [0.0f64; 4];
            sample(rc, p.time as f64, &mut root.yaw_cursor, &mut q);
            twist_of(q, up)
          }
          None => 0.0,
        };
        let sign = if !wrapped {
          0.0
        } else if p.speed > 0.0 {
          1.0
        } else {
          -1.0
        };
        let yaw_step = wrap_angle(wrap_angle(yaw_now - root.last_yaw) + sign * root.yaw_drift);
        // The clip authors its travel in ITS frame, which has turned by
        // the clip's own yaw so far; un-turned by that yaw (the pre-step
        // one: the anchor applies the step with its pre-step rotation, so
        // the two cancel exactly) it is the step in the root's current
        // facing - the anchor's local frame.
        let raw = [
          now[0] - root.last[0] + sign * root.drift[0],
          now[1] - root.last[1] + sign * root.drift[1],
          now[2] - root.last[2] + sign * root.drift[2],
        ];
        let mut local = rotate_vector(axis_quat(up, -root.clip_yaw), raw);
        if !root.binding.vertical {
          let rise = local[0] * up[0] + local[1] * up[1] + local[2] * up[2];
          for e in 0..3 {
            local[e] -= rise * up[e];
          }
        }
        let delta = [local[0] * effective, local[1] * effective, local[2] * effective];
        let yaw = yaw_step * effective;
        root.last = [now[0], now[1], now[2]];
        root.last_yaw = yaw_now;
        root.clip_yaw += yaw_step;
        self.rigs.root_deltas.push((p.id, delta, yaw));
        if let Some(anchor) = root.binding.anchor {
          if delta != [0.0; 3] || yaw != 0.0 {
            self.rigs.anchor_moves.push((anchor, delta, yaw, up));
          }
        }
      }
    }
    self.rigs.crossed = crossed;

    // The blend: the pose buffer starts at rest, every layer composes
    // over it in order.
    for (j, slots) in rig.trs.iter_mut().enumerate() {
      for (path, slot) in slots.iter_mut().enumerate() {
        slot.value = rest_value(&rig.rest[j], path);
      }
    }
    for (j, slots) in rig.morph.iter_mut().enumerate() {
      for (e, slot) in slots.iter_mut().enumerate() {
        slot.value[0] = rig.rest_weights[j].get(e).copied().unwrap_or(0.0);
      }
    }
    for slot in &mut rig.curve_slots {
      slot.value[0] = 0.0;
    }
    let mut scratch = std::mem::take(&mut self.rigs.scratch);
    for li in 0..rig.layers.len() {
      let lw = rig.layers[li].weight;
      if lw <= 0.0 {
        continue;
      }
      let on_layer: Vec<usize> = (0..rig.players.len())
        .filter(|&pi| rig.players[pi].layer == li && (rig.players[pi].weight > 0.0 || rig.players[pi].dying))
        .collect();
      if on_layer.is_empty() {
        continue;
      }
      rig.pass += 1;
      let pass = rig.pass;
      let additive = rig.layers[li].mode == LayerMode::Additive;
      // The layer's total weight per joint: the normalization base.
      rig.total.fill(0.0);
      let mut total_curves = 0.0f64;
      for &pi in &on_layer {
        let p = &rig.players[pi];
        total_curves += p.weight;
        if p.shares.is_empty() {
          for t in rig.total.iter_mut() {
            *t += p.weight;
          }
        } else {
          for (t, s) in rig.total.iter_mut().zip(&p.shares) {
            *t += p.weight * s;
          }
        }
      }
      // Sample and accumulate.
      for &pi in &on_layer {
        let p = &mut rig.players[pi];
        let Some(clip) = self.clips.clips.get(&p.clip) else { continue };
        let time = p.time as f64;
        let pw = p.weight;
        for (ci, channel) in clip.channels.iter().enumerate() {
          let elements = channel.elements;
          let (acc, w_eff, rotation) = match channel.path {
            ChannelPath::Curve => {
              let k = p.curve_slots[ci];
              scratch.clear();
              scratch.resize(1, 0.0);
              sample(channel, time, &mut p.cursors[ci], &mut scratch);
              (&mut rig.acc_curves[k], pw, Kind::Linear)
            }
            ChannelPath::Weights => {
              let j = channel.target_slot as usize;
              let w_eff = pw * p.share(j);
              scratch.clear();
              scratch.resize(elements, 0.0);
              sample(channel, time, &mut p.cursors[ci], &mut scratch);
              if rig.acc_morph[j].len() < elements {
                let have = rig.morph[j].len();
                rig.acc_morph[j].resize(elements, Acc::EMPTY);
                rig.morph[j].resize(elements, Slot::EMPTY);
                // A slot made mid-frame starts at rest like the others.
                for e in have..elements {
                  rig.morph[j][e].value[0] = rig.rest_weights[j].get(e).copied().unwrap_or(0.0);
                }
              }
              // Element by element: each target is its own slot.
              for e in 0..elements {
                let acc = &mut rig.acc_morph[j][e];
                accumulate(acc, [scratch[e], 0.0, 0.0, 0.0], w_eff, Kind::Linear, additive, pass, 1);
              }
              continue;
            }
            _ => {
              let j = channel.target_slot as usize;
              let w_eff = pw * p.share(j);
              scratch.clear();
              scratch.resize(4, 0.0);
              sample(channel, time, &mut p.cursors[ci], &mut scratch);
              let path = path_index(channel.path);
              (&mut rig.acc_trs[j][path], w_eff, Kind::of_path(path))
            }
          };
          let mut v = [0.0f64; 4];
          v[..elements].copy_from_slice(&scratch[..elements]);
          accumulate(acc, v, w_eff, rotation, additive, pass, elements);
        }
      }
      // Apply the layer over the pose so far.
      let frame = rig.frame;
      let layer = &rig.layers[li];
      for j in 0..rig.joints.len() {
        let ls = layer.share(j);
        let k = lw * ls;
        let total = rig.total[j];
        for path in 0..3 {
          let acc = &rig.acc_trs[j][path];
          if acc.fed != pass || k <= 0.0 {
            continue;
          }
          let slot = &mut rig.trs[j][path];
          apply_layer(slot, acc, k, total, Kind::of_path(path), additive, frame);
        }
        for e in 0..rig.acc_morph[j].len() {
          let acc = &rig.acc_morph[j][e];
          if acc.fed != pass || k <= 0.0 {
            continue;
          }
          let slot = &mut rig.morph[j][e];
          apply_layer(slot, acc, k, total, Kind::Linear, additive, frame);
        }
      }
      for k in 0..rig.acc_curves.len() {
        let acc = &rig.acc_curves[k];
        if acc.fed != pass {
          continue;
        }
        let slot = &mut rig.curve_slots[k];
        apply_layer(slot, acc, lw, total_curves, Kind::Linear, additive, frame);
      }
    }
    self.rigs.scratch = scratch;

    // The players that faded out this advance have had their last blend:
    // they drop now, reporting.
    let mut i = 0;
    while i < rig.players.len() {
      if rig.players[i].dying {
        let p = rig.players.remove(i);
        let finished = p.end_fading;
        self.remove_player(rig, p, finished);
      } else {
        i += 1;
      }
    }

    // The inertial stage: capture a requested transition on every fed
    // slot with a history, step the running ones.
    let frame = rig.frame;
    let pending = rig.pending_inertial.take();
    let dt32 = dt as f64;
    for slots in rig.trs.iter_mut() {
      for (path, slot) in slots.iter_mut().enumerate() {
        if slot.fed == frame {
          active |= inertial_step(slot, path == 1, if path == 1 { 4 } else { 3 }, pending, dt32, running);
        }
      }
    }
    for slots in rig.morph.iter_mut() {
      for slot in slots.iter_mut() {
        if slot.fed == frame {
          active |= inertial_step(slot, false, 1, pending, dt32, running);
        }
      }
    }
    for slot in rig.curve_slots.iter_mut() {
      if slot.fed == frame {
        active |= inertial_step(slot, false, 1, pending, dt32, running);
      }
    }

    // Write the fed slots through the snap path; unchanged values write
    // nothing, so a frozen clock settles to zero work.
    let mut wrote = false;
    for (j, slots) in rig.trs.iter().enumerate() {
      let Ok(idx) = self.resolve(rig.joints[j]) else { continue };
      let mut changed = false;
      let n = &mut self.nodes[idx as usize];
      if slots[0].fed == frame {
        let v = [slots[0].value[0], slots[0].value[1], slots[0].value[2]];
        if n.position != v {
          n.position = v;
          changed = true;
        }
      }
      if slots[1].fed == frame && n.rotation != slots[1].value {
        n.rotation = slots[1].value;
        changed = true;
      }
      if slots[2].fed == frame {
        let v = [slots[2].value[0], slots[2].value[1], slots[2].value[2]];
        if n.scale != v {
          n.scale = v;
          changed = true;
        }
      }
      if changed {
        n.local_dirty = true;
        self.enqueue(idx);
        wrote = true;
      }
    }
    for (j, slots) in rig.morph.iter().enumerate() {
      if slots.is_empty() || !slots.iter().any(|s| s.fed == frame) {
        continue;
      }
      let Ok(idx) = self.resolve(rig.joints[j]) else { continue };
      let n = &mut self.nodes[idx as usize];
      let same = n.weights.len() == slots.len() && n.weights.iter().zip(slots).all(|(w, s)| *w == s.value[0]);
      if !same {
        n.weights.clear();
        n.weights.extend(slots.iter().map(|s| s.value[0]));
        self.mark_weights(idx);
        wrote = true;
      }
    }

    // History, for the next inertial capture: the outputs of this frame
    // and the last, with the step between them.
    if dt32 > MIN_HISTORY_DT {
      for slots in rig.trs.iter_mut() {
        for slot in slots.iter_mut() {
          record_history(slot, frame, dt32);
        }
      }
      for slots in rig.morph.iter_mut() {
        for slot in slots.iter_mut() {
          record_history(slot, frame, dt32);
        }
      }
      for slot in rig.curve_slots.iter_mut() {
        record_history(slot, frame, dt32);
      }
    }
    (active, wrote)
  }

  /// A player leaving the set: its windows close, it reports.
  fn remove_player(&mut self, rig: &Rig, p: Player, finished: bool) {
    self.rigs.player_rig.remove(&p.id);
    for g in self.rigs.groups.values_mut() {
      g.members.retain(|&m| m != p.id);
    }
    let weight = p.weight * rig.layers[p.layer].weight;
    for (m, inside) in p.inside.iter().enumerate() {
      if *inside {
        self.rigs.events.push(ClipEvent::Marker {
          player: p.id,
          marker: m,
          phase: MarkerPhase::End,
          weight,
          layer: p.layer,
        });
      }
    }
    if finished && !p.finished {
      self.rigs.events.push(ClipEvent::Finished(p.id));
    }
    self.rigs.events.push(ClipEvent::Dropped(p.id));
  }

  /// A layer's per-joint shares, recomputed when its mask changed.
  fn refresh_layer_shares(&self, rig: &mut Rig, li: usize) {
    let layer = &mut rig.layers[li];
    let current = layer.mask.and_then(|m| self.rigs.masks.get(&m).map(|mask| (m, mask.version)));
    if layer.seen == current && (current.is_some() || layer.shares.is_empty()) {
      return;
    }
    layer.seen = current;
    match current.and_then(|(m, _)| self.rigs.masks.get(&m)) {
      None => layer.shares.clear(),
      Some(mask) => layer.shares = rig.joints.iter().map(|&j| mask.share(j)).collect(),
    }
  }

  fn refresh_player_shares(&self, rig: &mut Rig, pi: usize) {
    let joints = &rig.joints;
    let p = &mut rig.players[pi];
    let current = p.mask.and_then(|m| self.rigs.masks.get(&m).map(|mask| (m, mask.version)));
    if p.seen == current && (current.is_some() || p.shares.is_empty()) {
      return;
    }
    p.seen = current;
    match current.and_then(|(m, _)| self.rigs.masks.get(&m)) {
      None => p.shares.clear(),
      Some(mask) => p.shares = joints.iter().map(|&j| mask.share(j)).collect(),
    }
  }
}

impl Rig {
  /// The rig's curve index for a name, added when new.
  fn curve_slot(&mut self, name: String) -> usize {
    if let Some(&k) = self.curve_index.get(&name) {
      return k;
    }
    let k = self.curves.len();
    self.curves.push(name.clone());
    self.curve_index.insert(name, k);
    self.curve_slots.push(Slot::EMPTY);
    self.acc_curves.push(Acc::EMPTY);
    k
  }
}

/// What a slot holds, for the blend algebra: a rotation composes, a
/// scale's additive delta multiplies, everything else adds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
  Linear,
  Rotation,
  Scale,
}

impl Kind {
  fn of_path(path: usize) -> Kind {
    match path {
      1 => Kind::Rotation,
      2 => Kind::Scale,
      _ => Kind::Linear,
    }
  }
}

/// Fold one sampled value into a layer accumulator: an override layer's
/// incremental weighted average (the first contributor sets the value),
/// an additive layer's weighted delta sum (positions and weights add,
/// rotations compose, a scale accumulates its weighted excess over one).
fn accumulate(acc: &mut Acc, v: [f64; 4], w: f64, kind: Kind, additive: bool, pass: u64, elements: usize) {
  let rotation = kind == Kind::Rotation;
  if acc.fed != pass {
    acc.fed = pass;
    acc.covered = 0.0;
    acc.value = if additive && rotation { IDENTITY_QUAT } else { [0.0; 4] };
  }
  // A zero weight still marks the slot fed (a dying player's last frame
  // lands it on what is under the layer) and adds nothing.
  if w <= 0.0 {
    return;
  }
  if additive {
    match kind {
      Kind::Rotation => acc.value = quat_normalize(quat_multiply(acc.value, slerp(IDENTITY_QUAT, v, w))),
      Kind::Scale => {
        for e in 0..elements {
          acc.value[e] += (v[e] - 1.0) * w;
        }
      }
      Kind::Linear => {
        for e in 0..elements {
          acc.value[e] += v[e] * w;
        }
      }
    }
    acc.covered += w;
    return;
  }
  let total = acc.covered + w;
  let share = if acc.covered <= 0.0 { 1.0 } else { w / total };
  acc.covered = total;
  acc.value = mix(acc.value, v, share, rotation);
}

/// Compose a layer's accumulated slot over the pose so far by `k` (the
/// layer weight times its mask share); `total` is the layer's player
/// weight at the joint, whose shortfall under one lets the pose under
/// the layer through (rest, for the base).
fn apply_layer(slot: &mut Slot, acc: &Acc, k: f64, total: f64, kind: Kind, additive: bool, frame: u64) {
  let under = slot.value;
  let rotation = kind == Kind::Rotation;
  if additive {
    match kind {
      Kind::Rotation => {
        let delta = slerp(IDENTITY_QUAT, acc.value, k);
        slot.value = quat_normalize(quat_multiply(under, delta));
      }
      // The accumulated delta is a sum of weighted (scale - 1) terms: a
      // multiplicative delta linearized per term.
      Kind::Scale => {
        for e in 0..3 {
          slot.value[e] = under[e] * (1.0 + k * acc.value[e]);
        }
      }
      Kind::Linear => {
        for e in 0..4 {
          slot.value[e] = under[e] + k * acc.value[e];
        }
      }
    }
  } else {
    let base = total.max(FULL_WEIGHT);
    let under_share = ((base - acc.covered) / base).clamp(0.0, 1.0);
    let layer_value = mix(acc.value, under, under_share, rotation);
    slot.value = mix(under, layer_value, k, rotation);
  }
  slot.fed = frame;
}

/// The inertial transition on one slot: capture a pending request
/// (offset and velocity from the history), then step and apply the
/// running one. Returns whether the slot still decays.
fn inertial_step(
  slot: &mut Slot,
  rotation: bool,
  elements: usize,
  pending: Option<f64>,
  dt: f64,
  running: bool,
) -> bool {
  if let Some(t1) = pending {
    slot.inertial = None;
    if slot.hist >= 1 {
      if rotation {
        let q = quat_normalize(quat_multiply(slot.prev, quat_inverse(quat_normalize(slot.value))));
        let (axis, angle) = axis_angle(q);
        let mut v0 = 0.0;
        if slot.hist >= 2 && slot.prev_dt > MIN_HISTORY_DT {
          let d = quat_normalize(quat_multiply(slot.prev, quat_inverse(slot.prev2)));
          let (daxis, dangle) = axis_angle(d);
          v0 = dangle * (daxis[0] * axis[0] + daxis[1] * axis[1] + daxis[2] * axis[2]) / slot.prev_dt;
        }
        slot.inertial = Inertial::fit([axis[0], axis[1], axis[2], 0.0], angle, v0, t1);
      } else {
        let mut diff = [0.0f64; 4];
        let mut len2 = 0.0;
        for e in 0..elements {
          diff[e] = slot.prev[e] - slot.value[e];
          len2 += diff[e] * diff[e];
        }
        let x0 = len2.sqrt();
        if x0 > 0.0 {
          let dir = [diff[0] / x0, diff[1] / x0, diff[2] / x0, diff[3] / x0];
          let mut v0 = 0.0;
          if slot.hist >= 2 && slot.prev_dt > MIN_HISTORY_DT {
            for e in 0..elements {
              v0 += (slot.prev[e] - slot.prev2[e]) / slot.prev_dt * dir[e];
            }
          }
          slot.inertial = Inertial::fit(dir, x0, v0, t1);
        }
      }
    }
  }
  let Some(mut inertial) = slot.inertial else {
    return false;
  };
  inertial.t += dt;
  if inertial.t >= inertial.t1 {
    slot.inertial = None;
    return false;
  }
  let x = inertial.offset();
  if rotation {
    let axis = [inertial.dir[0], inertial.dir[1], inertial.dir[2]];
    slot.value = quat_normalize(quat_multiply(axis_quat(axis, x), slot.value));
  } else {
    for e in 0..elements {
      slot.value[e] += inertial.dir[e] * x;
    }
  }
  slot.inertial = Some(inertial);
  running
}

/// Push a slot's output into its two-deep history; a slot not fed this
/// frame forgets (its next feed starts a fresh history).
fn record_history(slot: &mut Slot, frame: u64, dt: f64) {
  if slot.fed != frame {
    slot.hist = 0;
    return;
  }
  slot.prev2 = slot.prev;
  slot.prev = slot.value;
  slot.prev_dt = dt;
  slot.hist = (slot.hist + 1).min(2);
}

#[cfg(test)]
pub(crate) fn step_time_for_test(
  time: f64,
  dt: f64,
  speed: f64,
  looped: bool,
  window: (f64, f64),
) -> (f64, bool, bool) {
  step_time(time, dt, speed, looped, window)
}

#[cfg(test)]
pub(crate) fn sequence_for_test(times: &[f64], t: f64, looped: bool, window: (f64, f64), follower: &[f64]) -> f64 {
  let (segment, frac) = sequence_position(times, t, looped, window);
  sequence_time(follower, segment, frac, looped, window)
}
