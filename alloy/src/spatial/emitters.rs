// Spatial audio on the spatial core: a voice of the mixer bound to a node
// (or a fixed world position) and heard by a LISTENER (a node, or a
// target's view: the camera), the core writing the voice's pan, distance
// gain, doppler rate and occlusion low-pass from the flushed world
// matrices. Three's PositionalAudio over an AudioListener, Unity's
// AudioSource and AudioListener, Godot's AudioStreamPlayer3D and
// AudioListener3D, in the Web Audio PannerNode vocabulary (distance
// model, refDistance, maxDistance, rolloff) plus a doppler factor and a
// per-listener speed of sound. JS sends intent, O(changes): bind, set the
// listener, set the parameters once; the pass runs after the walk and
// writes only what moved past an epsilon, so a still scene costs
// nothing and a scene of twenty engine voices costs twenty writes a
// frame while the karts move.
//
// The laws, all in the listener's frame with d the offset listener ->
// emitter, dist = |d|:
//
// - pan = (2 / pi) asin(dot(d, right) / dist), the LATERAL angle: the
//   interaural cue itself, one law for a 3d camera and a top-down 2d
//   view (Web Audio's horizontal azimuth hard-pans a source overhead and
//   every sprite on a map; the lateral angle sends both to center).
//   Scaled by min(dist / refDistance, 1): a source inside the reference
//   distance surrounds the listener instead of flipping left to right
//   as it passes through (Unity's spread, Godot's panning_strength,
//   folded into the one distance every emitter already names).
// - gain: Web Audio's three models over dc = clamp(dist, ref, max):
//   inverse ref / (ref + rolloff (dc - ref)); linear 1 - rolloff (dc -
//   ref) / (max - ref); exponential (dc / ref)^-rolloff; clamped to
//   [0, 1], times the occlusion gain. A multiplier UNDER the voice's own
//   gain (the mixer composes them), so setGain on a bound voice keeps
//   meaning "how loud is this thing".
// - rate = (c + f a_l) / (c - f a_e), with a_l the listener's approach
//   speed along d, a_e the emitter's, f the emitter's doppler factor, c
//   the listener's speed of sound (world units per second, so the one
//   knob fixes the world's unit), each product capped below c and the
//   ratio clamped: the physical formula Web Audio shipped before it
//   dropped doppler, Unity's dopplerLevel as f.
// - occlusion: the segment listener -> emitter against the shown
//   occluders that carry audio properties (`Occluder::audio`): gains
//   multiply, the lowest cutoff wins (`occlusion.rs`). One authoring for
//   sight and sound; a hidden occluder blocks neither.
//
// Velocities are derived, as Unity derives them: the position delta over
// the app-time frame delta, smoothed with a short time constant (frame
// jitter would otherwise warble the pitch) and snapped to exactly zero
// under a floor, so a stopped kart writes a few decaying frames and then
// nothing. A frame's displacement faster than sound is a teleport, not
// motion: the velocity resets instead of chirping.
//
// Engine independent: native types in and out (rendertree rules). Audio
// enters only as the four floats of `EmitterOutput` handed to the sink
// writer; the mixer applies them.

use std::collections::HashMap;

use super::cull::world_box;
use super::math::{invert_affine, transform_point, transform_vector};
use super::{math, Mat4, NodeId, SinkWriter, Spatial};
use crate::audio::OPEN_CUTOFF_HZ;

pub type ListenerId = u64;

/// What a listener hears from: a node (its world position; its local -z
/// is forward, +x right, Three's AudioListener convention), or a target's
/// view as `set_view` last wrote it (the camera, with no node to keep in
/// step). A view key (`alloc_view_key`) is a view like any target's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListenerSource {
  Node(NodeId),
  View(u64),
}

/// Where a voice sounds from: a node (its world position, moving with
/// it), or a fixed world position (a one-shot at a point, Unity's
/// PlayClipAtPoint, with no node to create and destroy around it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmitterSource {
  Node(NodeId),
  Fixed([f64; 3]),
}

/// Web Audio's distance models (PannerNode.distanceModel).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DistanceModel {
  #[default]
  Inverse,
  Linear,
  Exponential,
}

/// An emitter's parameters, Web Audio's PannerNode defaults: inverse law,
/// refDistance 1, maxDistance 10000, rolloff 1; doppler 1 (the physical
/// shift; 0 switches it off, Godot's and Three's default).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitterParams {
  pub ref_distance: f64,
  pub max_distance: f64,
  pub rolloff: f64,
  pub model: DistanceModel,
  pub doppler: f64,
}

impl Default for EmitterParams {
  fn default() -> Self {
    EmitterParams {
      ref_distance: 1.0,
      max_distance: 10_000.0,
      rolloff: 1.0,
      model: DistanceModel::Inverse,
      doppler: 1.0,
    }
  }
}

/// What the pass writes for a voice: its pan in [-1, 1], the gain and
/// rate multipliers under the voice's own, and the low-pass cutoff in Hz
/// (`OPEN_CUTOFF_HZ` and above: open).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitterOutput {
  pub pan: f32,
  pub gain: f32,
  pub rate: f32,
  pub cutoff: f32,
}

impl EmitterOutput {
  /// A voice heard plainly: center, unity, open.
  pub const NEUTRAL: EmitterOutput = EmitterOutput { pan: 0.0, gain: 1.0, rate: 1.0, cutoff: OPEN_CUTOFF_HZ };
}

/// How a write should glide to its values: over the frame interval (the
/// emitter or listener moved: a piecewise-linear trajectory that reaches
/// the value as the next frame arrives), or over the occlusion smoothing
/// time (the ray's answer flipped: a wall does not click).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitterRamp {
  Frame,
  Occlusion,
}

/// The pass's figures (`emitter_stats`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EmitterStats {
  pub listeners: usize,
  pub emitters: usize,
  /// Writes handed to the sink writer so far.
  pub writes: u64,
  /// Occlusion segments tested so far.
  pub rays: u64,
}

/// The speed of sound in air, metres per second: the listener default,
/// right for a world in metres (glTF's unit).
pub const SPEED_OF_SOUND: f64 = 343.0;
/// The time constant (seconds) the derived velocities are smoothed with:
/// a frame-time jitter of a millisecond at 60 Hz would otherwise swing a
/// derived speed by a tenth and warble the pitch; 80 ms follows a kart's
/// acceleration within a few frames.
const VELOCITY_SMOOTHING_S: f64 = 0.08;
/// Below this speed (world units per second) a derived velocity snaps to
/// exactly zero, so a stopped emitter's tail ends and writes stop.
const VELOCITY_FLOOR: f64 = 1e-3;
/// Each doppler product (factor times approach speed) is capped at this
/// fraction of the speed of sound, keeping the ratio's denominator
/// positive whatever the factor.
const DOPPLER_SPEED_CAP: f64 = 0.9;
/// The doppler ratio's range: an octave either way is every shift a game
/// means; past it is a teleport or a bug, not motion.
const DOPPLER_MIN_RATIO: f64 = 0.5;
const DOPPLER_MAX_RATIO: f64 = 2.0;
/// Below these changes a value is not rewritten: a still scene costs the
/// mixer nothing, and float noise never reaches the command queue.
const PAN_EPSILON: f32 = 1e-3;
const GAIN_EPSILON: f32 = 1e-3;
const RATE_EPSILON: f32 = 1e-4;
/// Relative: a cutoff within a tenth of a percent is the same cutoff.
const CUTOFF_EPSILON: f32 = 1e-3;
/// Under this distance the direction to an emitter is undefined: center
/// pan, no doppler.
const NEAR_EPSILON: f64 = 1e-6;

/// The camera frame a view implies: the eye and the unit right and
/// forward axes in world space, from the inverse view (exact for any
/// invertible view, a 2d camera's zoom scale included; a singular view
/// takes the rigid reading, as `derive_lod_view` does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewFrame {
  pub(super) eye: [f64; 3],
  pub(super) right: [f64; 3],
  pub(super) forward: [f64; 3],
}

impl ViewFrame {
  pub(super) fn from_view(view: &Mat4) -> ViewFrame {
    match math::invert(view) {
      Some(inv) => ViewFrame {
        eye: [inv[12], inv[13], inv[14]],
        right: normalize_or([inv[0], inv[1], inv[2]], [1.0, 0.0, 0.0]),
        forward: normalize_or([-inv[8], -inv[9], -inv[10]], [0.0, 0.0, -1.0]),
      },
      None => {
        let v = view;
        ViewFrame {
          eye: [
            -(v[0] * v[12] + v[1] * v[13] + v[2] * v[14]),
            -(v[4] * v[12] + v[5] * v[13] + v[6] * v[14]),
            -(v[8] * v[12] + v[9] * v[13] + v[10] * v[14]),
          ],
          right: normalize_or([v[0], v[4], v[8]], [1.0, 0.0, 0.0]),
          forward: normalize_or([-v[2], -v[6], -v[10]], [0.0, 0.0, -1.0]),
        }
      }
    }
  }

  /// A node's frame: its world position, its local +x and -z axes.
  fn from_world(world: &Mat4) -> ViewFrame {
    ViewFrame {
      eye: [world[12], world[13], world[14]],
      right: normalize_or([world[0], world[1], world[2]], [1.0, 0.0, 0.0]),
      forward: normalize_or([-world[8], -world[9], -world[10]], [0.0, 0.0, -1.0]),
    }
  }
}

/// A position followed over frames and the velocity derived from it.
#[derive(Clone, Copy, Debug, Default)]
struct Tracked {
  position: Option<[f64; 3]>,
  velocity: [f64; 3],
}

impl Tracked {
  /// The entity stands at `p` this frame, `dt` seconds after the last:
  /// derive the velocity, smoothed, a supersonic jump read as a teleport.
  fn update(&mut self, p: [f64; 3], dt: f64, speed_of_sound: f64) {
    if let Some(q) = self.position {
      if dt > 0.0 {
        let inv = 1.0 / dt;
        let raw = [(p[0] - q[0]) * inv, (p[1] - q[1]) * inv, (p[2] - q[2]) * inv];
        if length(raw) > speed_of_sound {
          self.velocity = [0.0; 3];
        } else {
          let alpha = 1.0 - (-dt / VELOCITY_SMOOTHING_S).exp();
          for k in 0..3 {
            self.velocity[k] += (raw[k] - self.velocity[k]) * alpha;
          }
        }
      }
    } else {
      self.velocity = [0.0; 3];
    }
    self.position = Some(p);
    self.floor();
  }

  /// The entity did not move this frame: the smoothed velocity decays
  /// toward zero.
  fn decay(&mut self, dt: f64) {
    if dt > 0.0 && self.moving() {
      let keep = (-dt / VELOCITY_SMOOTHING_S).exp();
      for v in &mut self.velocity {
        *v *= keep;
      }
      self.floor();
    }
  }

  fn floor(&mut self) {
    if length(self.velocity) < VELOCITY_FLOOR {
      self.velocity = [0.0; 3];
    }
  }

  fn moving(&self) -> bool {
    self.velocity != [0.0; 3]
  }
}

struct Listener {
  source: ListenerSource,
  speed_of_sound: f64,
  track: Tracked,
  /// The source changed or its view was rewritten since the last pass.
  moved: bool,
  /// Per pass: the frame as it stands (None: a view not yet set, a dead
  /// node), whether it moved this pass, and whether its velocity was live
  /// before or after this pass's step (a tail still to write out).
  frame: Option<ViewFrame>,
  moved_now: bool,
  tail: bool,
}

struct Emitter {
  voice: u64,
  source: EmitterSource,
  listener: ListenerId,
  params: EmitterParams,
  track: Tracked,
  /// What was last written; None before the first.
  last: Option<EmitterOutput>,
  /// Owes a write at the next pass whatever moved (bound, re-pointed).
  fresh: bool,
  /// The occlusion in force: (gain, cutoff).
  occlusion: (f32, f32),
  /// The occlusion flipped since the last write: the next write glides
  /// over the occlusion time.
  occlusion_changed: bool,
}

/// The emitters, listeners and view frames of the arena (`Spatial`).
#[derive(Default)]
pub(super) struct EmitterSet {
  listeners: HashMap<ListenerId, Listener>,
  next_listener: u64,
  emitters: Vec<Emitter>,
  /// The app-time stamp of the last pass (`set_transition_now` starts it).
  pub(super) last_ms: f64,
  /// Per target with a view, its camera frame.
  pub(super) view_frames: HashMap<u64, ViewFrame>,
  /// An occluder moved, changed, was shown or hidden since the last pass:
  /// every emitter re-tests its segment.
  pub(super) occluders_changed: bool,
  /// A node with audio bound to it moved this flush (`audio_moved` set).
  pub(super) any_moved: bool,
  writes: u64,
  rays: u64,
}

impl EmitterSet {
  /// A target's view was rewritten: the listeners on it moved.
  pub(super) fn view_moved(&mut self, target: u64) {
    for l in self.listeners.values_mut() {
      if l.source == ListenerSource::View(target) {
        l.moved = true;
      }
    }
  }
}

fn length(v: [f64; 3]) -> f64 {
  (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize_or(v: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
  let l = length(v);
  if l > 0.0 && l.is_finite() {
    [v[0] / l, v[1] / l, v[2] / l]
  } else {
    fallback
  }
}

fn check_speed(speed_of_sound: f64) -> Result<(), String> {
  if !(speed_of_sound.is_finite() && speed_of_sound > 0.0) {
    return Err("speed of sound must be a finite number > 0".into());
  }
  Ok(())
}

fn check_params(p: &EmitterParams) -> Result<(), String> {
  if !(p.ref_distance.is_finite() && p.ref_distance > 0.0) {
    return Err("refDistance must be a finite number > 0".into());
  }
  if !(p.max_distance.is_finite() && p.max_distance >= p.ref_distance) {
    return Err("maxDistance must be a finite number >= refDistance".into());
  }
  if !(p.rolloff.is_finite() && p.rolloff >= 0.0) {
    return Err("rolloff must be a finite number >= 0".into());
  }
  if !(p.doppler.is_finite() && p.doppler >= 0.0) {
    return Err("doppler must be a finite number >= 0".into());
  }
  Ok(())
}

/// The laws of the header for one emitter at `p` heard from `frame`.
pub(crate) fn evaluate(
  params: &EmitterParams,
  frame: &ViewFrame,
  p: [f64; 3],
  listener_velocity: [f64; 3],
  emitter_velocity: [f64; 3],
  speed_of_sound: f64,
  occlusion: (f32, f32),
) -> EmitterOutput {
  let d = [p[0] - frame.eye[0], p[1] - frame.eye[1], p[2] - frame.eye[2]];
  let dist = length(d);
  let near = dist <= NEAR_EPSILON;
  let pan = if near {
    0.0
  } else {
    let lateral = (dot(d, frame.right) / dist).clamp(-1.0, 1.0);
    lateral.asin() * std::f64::consts::FRAC_2_PI * (dist / params.ref_distance).min(1.0)
  };
  let (r, m) = (params.ref_distance, params.max_distance);
  let dc = dist.clamp(r, m);
  let law = match params.model {
    DistanceModel::Inverse => r / (r + params.rolloff * (dc - r)),
    DistanceModel::Linear => {
      if m > r {
        1.0 - params.rolloff * (dc - r) / (m - r)
      } else {
        1.0
      }
    }
    DistanceModel::Exponential => (dc / r).powf(-params.rolloff),
  };
  let gain = law.clamp(0.0, 1.0) as f32 * occlusion.0;
  let rate = if params.doppler > 0.0 && !near {
    let u = [d[0] / dist, d[1] / dist, d[2] / dist];
    let cap = speed_of_sound * DOPPLER_SPEED_CAP;
    let listener = (params.doppler * dot(listener_velocity, u)).clamp(-cap, cap);
    let emitter = (-params.doppler * dot(emitter_velocity, u)).clamp(-cap, cap);
    ((speed_of_sound + listener) / (speed_of_sound - emitter)).clamp(DOPPLER_MIN_RATIO, DOPPLER_MAX_RATIO)
  } else {
    1.0
  };
  EmitterOutput { pan: pan as f32, gain, rate: rate as f32, cutoff: occlusion.1 }
}

/// Whether two outputs differ past the epsilons: what decides a write.
fn differs(a: &EmitterOutput, b: &EmitterOutput) -> bool {
  let open = |c: f32| c >= OPEN_CUTOFF_HZ;
  let cutoff_differs = match (open(a.cutoff), open(b.cutoff)) {
    (true, true) => false,
    (false, false) => (a.cutoff / b.cutoff - 1.0).abs() > CUTOFF_EPSILON,
    _ => true,
  };
  (a.pan - b.pan).abs() > PAN_EPSILON
    || (a.gain - b.gain).abs() > GAIN_EPSILON
    || (a.rate - b.rate).abs() > RATE_EPSILON
    || cutoff_differs
}

impl Spatial {
  fn add_audio_ref(&mut self, id: NodeId) {
    if let Ok(i) = self.resolve(id) {
      self.nodes[i as usize].audio_refs += 1;
    }
  }

  fn drop_audio_ref(&mut self, id: NodeId) {
    if let Ok(i) = self.resolve(id) {
      let n = &mut self.nodes[i as usize];
      n.audio_refs = n.audio_refs.saturating_sub(1);
      if n.audio_refs == 0 {
        n.audio_moved = false;
      }
    }
  }

  /// A listener hearing from `source` at `speed_of_sound` world units per
  /// second (`SPEED_OF_SOUND` for a world in metres). Several may exist
  /// (one per scene, so split-screen works); an emitter names the one it
  /// is heard by. A node source must be alive; a view source may be set
  /// before its view is (its emitters play plainly until then).
  pub fn create_listener(&mut self, source: ListenerSource, speed_of_sound: f64) -> Result<ListenerId, String> {
    check_speed(speed_of_sound)?;
    if let ListenerSource::Node(id) = source {
      self.resolve(id)?;
      self.add_audio_ref(id);
    }
    let id = self.emitters.next_listener + 1;
    self.emitters.next_listener = id;
    self.emitters.listeners.insert(
      id,
      Listener {
        source,
        speed_of_sound,
        track: Tracked::default(),
        moved: true,
        frame: None,
        moved_now: false,
        tail: false,
      },
    );
    Ok(id)
  }

  /// Re-point a listener (a camera switch, a node taking over from the
  /// view). A changed source forgets the velocity history, so the switch
  /// itself is no motion; the same source keeps it.
  pub fn set_listener(&mut self, id: ListenerId, source: ListenerSource, speed_of_sound: f64) -> Result<(), String> {
    check_speed(speed_of_sound)?;
    if let ListenerSource::Node(node) = source {
      self.resolve(node)?;
    }
    let previous = {
      let listener = self.emitters.listeners.get(&id).ok_or_else(|| format!("audio listener {id} not found"))?;
      listener.source
    };
    if previous != source {
      if let ListenerSource::Node(old) = previous {
        self.drop_audio_ref(old);
      }
      if let ListenerSource::Node(new) = source {
        self.add_audio_ref(new);
      }
    }
    let listener = self.emitters.listeners.get_mut(&id).expect("listener checked above");
    if previous != source {
      listener.track = Tracked::default();
    }
    listener.source = source;
    listener.speed_of_sound = speed_of_sound;
    listener.moved = true;
    for e in &mut self.emitters.emitters {
      if e.listener == id {
        e.fresh = true;
      }
    }
    Ok(())
  }

  /// Forget a listener. The emitters heard by it are released (their
  /// voices keep the last values written).
  pub fn destroy_listener(&mut self, id: ListenerId) -> Result<(), String> {
    let listener = self.emitters.listeners.remove(&id).ok_or_else(|| format!("audio listener {id} not found"))?;
    if let ListenerSource::Node(node) = listener.source {
      self.drop_audio_ref(node);
    }
    let orphans: Vec<EmitterSource> =
      self.emitters.emitters.iter().filter(|e| e.listener == id).map(|e| e.source).collect();
    self.emitters.emitters.retain(|e| e.listener != id);
    for source in orphans {
      if let EmitterSource::Node(node) = source {
        self.drop_audio_ref(node);
      }
    }
    Ok(())
  }

  /// Bind voice `voice` to `source`, heard by `listener` under `params`
  /// (a second bind of the same voice replaces its binding; the same
  /// source keeps the velocity history). Evaluated NOW against the tree as
  /// it stands, pending writes included, and returned: the caller writes
  /// it at once (with no ramp), so a one-shot at a position is placed from
  /// its first sample. Later changes come through the flush.
  pub fn bind_emitter(
    &mut self,
    voice: u64,
    source: EmitterSource,
    listener: ListenerId,
    params: EmitterParams,
  ) -> Result<EmitterOutput, String> {
    check_params(&params)?;
    if let EmitterSource::Node(id) = source {
      self.resolve(id)?;
    }
    if !self.emitters.listeners.contains_key(&listener) {
      return Err(format!("audio listener {listener} not found"));
    }
    if let EmitterSource::Fixed(p) = source {
      if p.iter().any(|v| !v.is_finite()) {
        return Err("position must be finite".into());
      }
    }
    let existing = self.emitters.emitters.iter().position(|e| e.voice == voice);
    let index = match existing {
      Some(k) => {
        let previous = self.emitters.emitters[k].source;
        if previous != source {
          if let EmitterSource::Node(old) = previous {
            self.drop_audio_ref(old);
          }
          if let EmitterSource::Node(new) = source {
            self.add_audio_ref(new);
          }
          let e = &mut self.emitters.emitters[k];
          e.track = Tracked::default();
        }
        let e = &mut self.emitters.emitters[k];
        e.source = source;
        e.listener = listener;
        e.params = params;
        k
      }
      None => {
        if let EmitterSource::Node(id) = source {
          self.add_audio_ref(id);
        }
        self.emitters.emitters.push(Emitter {
          voice,
          source,
          listener,
          params,
          track: Tracked::default(),
          last: None,
          fresh: true,
          occlusion: (1.0, OPEN_CUTOFF_HZ),
          occlusion_changed: false,
        });
        self.emitters.emitters.len() - 1
      }
    };
    // Evaluate now, pending writes included (`world`), the listener's
    // frame as it stands.
    let position = match source {
      EmitterSource::Node(id) => {
        let w = self.world(id)?;
        [w[12], w[13], w[14]]
      }
      EmitterSource::Fixed(p) => p,
    };
    let (frame, speed) = {
      let l = &self.emitters.listeners[&listener];
      (self.listener_frame(l.source, &self.emitters.view_frames), l.speed_of_sound)
    };
    let output = match frame {
      Some(frame) => {
        let (occlusion, rays) = self.occlusion_between(frame.eye, position);
        self.emitters.rays += rays;
        let e = &mut self.emitters.emitters[index];
        e.occlusion = occlusion;
        e.track.update(position, 0.0, speed);
        evaluate(&e.params, &frame, position, [0.0; 3], e.track.velocity, speed, occlusion)
      }
      None => EmitterOutput::NEUTRAL,
    };
    let e = &mut self.emitters.emitters[index];
    e.last = Some(output);
    e.fresh = false;
    e.occlusion_changed = false;
    self.emitters.writes += 1;
    Ok(output)
  }

  /// Release a voice's binding; whether it had one. The voice keeps the
  /// last values written (the caller clears them if it wants to).
  pub fn unbind_emitter(&mut self, voice: u64) -> bool {
    let Some(k) = self.emitters.emitters.iter().position(|e| e.voice == voice) else {
      return false;
    };
    let e = self.emitters.emitters.remove(k);
    if let EmitterSource::Node(id) = e.source {
      self.drop_audio_ref(id);
    }
    true
  }

  /// Release the bindings of voices that have ended, by the host's word
  /// (the core never learns a voice's state otherwise). Called before a
  /// flush, so a voice that ended in a still scene frees its binding
  /// without waiting for motion.
  pub fn release_emitters(&mut self, mut ended: impl FnMut(u64) -> bool) {
    let mut gone: Vec<EmitterSource> = Vec::new();
    self.emitters.emitters.retain(|e| {
      let keep = !ended(e.voice);
      if !keep {
        gone.push(e.source);
      }
      keep
    });
    for source in gone {
      if let EmitterSource::Node(id) = source {
        self.drop_audio_ref(id);
      }
    }
  }

  /// A derived velocity is still decaying: the pass wants the next frame
  /// to write the tail out.
  pub fn emitters_active(&self) -> bool {
    self.emitters.emitters.iter().any(|e| e.track.moving())
      || self.emitters.listeners.values().any(|l| l.track.moving())
  }

  pub fn emitter_stats(&self) -> EmitterStats {
    EmitterStats {
      listeners: self.emitters.listeners.len(),
      emitters: self.emitters.emitters.len(),
      writes: self.emitters.writes,
      rays: self.emitters.rays,
    }
  }

  /// A node is freed: the emitters on it go (their voices keep their last
  /// values; the consumer stops them if it wants silence), a listener on
  /// it loses its frame until re-pointed.
  pub(super) fn node_audio_freed(&mut self, id: NodeId) {
    if self.nodes[super::index(id)].audio_refs == 0 {
      return;
    }
    self.emitters.emitters.retain(|e| e.source != EmitterSource::Node(id));
    let i = super::index(id);
    self.nodes[i].audio_refs = 0;
    self.nodes[i].audio_moved = false;
  }

  /// The frame a listener source stands at, from the flushed worlds and
  /// the view frames; None for a view not set or a node gone.
  fn listener_frame(&self, source: ListenerSource, frames: &HashMap<u64, ViewFrame>) -> Option<ViewFrame> {
    match source {
      ListenerSource::Node(id) => self.world(id).ok().map(|w| ViewFrame::from_world(&w)),
      ListenerSource::View(target) => frames.get(&target).copied(),
    }
  }

  /// What the shown occluders with audio do to a sound travelling the
  /// segment `from` -> `to`: (gain, cutoff), and the segments tested.
  fn occlusion_between(&self, from: [f64; 3], to: [f64; 3]) -> ((f32, f32), u64) {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let mut gain = 1.0f32;
    let mut cutoff = OPEN_CUTOFF_HZ;
    let mut rays = 0;
    for &i in &self.occluders {
      let n = &self.nodes[i as usize];
      if !n.alive || !n.shown {
        continue;
      }
      let Some(o) = &n.occluder else { continue };
      let Some(audio) = o.audio() else { continue };
      rays += 1;
      let boxed = world_box(o.bounds(), &n.world);
      if !matches!(super::bvh::ray_box_distance(from, d, &boxed), Some(t) if t <= 1.0) {
        continue;
      }
      let inv = invert_affine(&n.world);
      if o.crossed(transform_point(&inv, from), transform_vector(&inv, d)) {
        gain *= audio.gain;
        cutoff = cutoff.min(audio.cutoff);
      }
    }
    ((gain, cutoff), rays)
  }

  /// The pass, at the end of a flush (worlds fresh): derive what moved,
  /// evaluate the emitters that need it and hand the writer what changed.
  pub(super) fn emitter_pass(&mut self, out: &mut dyn SinkWriter) {
    let now = self.emitters.last_ms.max(self.transitions.now_ms);
    let dt = (now - self.emitters.last_ms) / 1000.0;
    self.emitters.last_ms = now;
    if self.emitters.emitters.is_empty() {
      self.emitters.any_moved = false;
      self.emitters.occluders_changed = false;
      return;
    }
    let any_moved = std::mem::take(&mut self.emitters.any_moved);
    let occluders_changed = std::mem::take(&mut self.emitters.occluders_changed);
    let listeners_moved = self.emitters.listeners.values().any(|l| l.moved);
    let fresh = self.emitters.emitters.iter().any(|e| e.fresh);
    if !(any_moved || occluders_changed || listeners_moved || fresh || self.emitters_active()) {
      return;
    }
    let mut set = std::mem::take(&mut self.emitters);
    // The listeners' frames and velocities for this pass.
    for l in set.listeners.values_mut() {
      let node_moved = match l.source {
        ListenerSource::Node(id) => self.resolve(id).map(|i| self.nodes[i as usize].audio_moved).unwrap_or(false),
        ListenerSource::View(_) => false,
      };
      l.frame = self.listener_frame(l.source, &set.view_frames);
      l.moved_now = node_moved || l.moved;
      l.moved = false;
      let was_moving = l.track.moving();
      match l.frame {
        Some(f) if l.moved_now => l.track.update(f.eye, dt, l.speed_of_sound),
        Some(_) => l.track.decay(dt),
        None => l.track = Tracked::default(),
      }
      l.tail = was_moving || l.track.moving();
    }
    let EmitterSet { listeners, emitters, writes, rays, .. } = &mut set;
    let mut released: Vec<EmitterSource> = Vec::new();
    emitters.retain_mut(|e| {
      let Some(l) = listeners.get(&e.listener) else {
        released.push(e.source);
        return false;
      };
      let (position, node_moved) = match e.source {
        EmitterSource::Node(id) => match self.resolve(id) {
          Ok(i) => {
            let n = &self.nodes[i as usize];
            ([n.world[12], n.world[13], n.world[14]], n.audio_moved)
          }
          Err(_) => {
            released.push(e.source);
            return false;
          }
        },
        EmitterSource::Fixed(p) => (p, false),
      };
      // A velocity live before or after the step is a tail to write: the
      // one that just decayed to zero still owes the write that puts the
      // rate back to 1.
      let was_moving = e.track.moving();
      if node_moved {
        e.track.update(position, dt, l.speed_of_sound);
      } else {
        e.track.decay(dt);
      }
      let tail = was_moving || e.track.moving() || l.tail;
      let Some(frame) = l.frame else { return true };
      let placed = node_moved || l.moved_now || occluders_changed || e.fresh;
      if !(placed || tail) {
        return true;
      }
      if placed {
        let (occlusion, tested) = self.occlusion_between(frame.eye, position);
        *rays += tested;
        if occlusion != e.occlusion {
          e.occlusion = occlusion;
          e.occlusion_changed = true;
        }
      }
      let output =
        evaluate(&e.params, &frame, position, l.track.velocity, e.track.velocity, l.speed_of_sound, e.occlusion);
      let write = e.fresh || e.last.map_or(true, |last| differs(&last, &output));
      if write {
        let ramp = if e.occlusion_changed { EmitterRamp::Occlusion } else { EmitterRamp::Frame };
        if !out.write_emitter(e.voice, &output, ramp) {
          released.push(e.source);
          return false;
        }
        *writes += 1;
        e.last = Some(output);
        e.fresh = false;
        e.occlusion_changed = false;
      }
      true
    });
    // The flags this pass consumed.
    for e in emitters.iter() {
      if let EmitterSource::Node(id) = e.source {
        if let Ok(i) = self.resolve(id) {
          self.nodes[i as usize].audio_moved = false;
        }
      }
    }
    for l in listeners.values() {
      if let ListenerSource::Node(id) = l.source {
        if let Ok(i) = self.resolve(id) {
          self.nodes[i as usize].audio_moved = false;
        }
      }
    }
    self.emitters = set;
    for source in released {
      if let EmitterSource::Node(id) = source {
        self.drop_audio_ref(id);
      }
    }
  }
}
