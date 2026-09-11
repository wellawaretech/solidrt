// Native transitions on node transforms (okf/done/spatial-node-
// transitions.md): a transition declaration on a node makes
// `write_transform` calls animate on the Rust side - JS writes only
// targets, the arena owns time. The first producer of the producer model:
// each advance writes local TRS through the ordinary snap path (queue,
// dirty propagation, sinks and the BVH just see moved nodes; flush is
// untouched). Position and scale are 3-lane tracks on the shared motion
// math; rotation is the one new piece - a quaternion track whose tween
// slerps the geodesic and whose spring is an angular-velocity spring in
// the exponential map at the target, the retargeting-safe rotational
// primitive. Spec vocabulary and semantics match the element transitions
// (spring default, retarget keeps spring state, settles land exactly), and
// so does the lifecycle vocabulary, whole (okf/done/
// spatial-node-exit-transitions.md): a component's `from` is where a node
// starts at creation, animating to the transform it holds at the first
// advance after `create`; its `exit` is where it animates to when the
// consumer lets go of it (mod.rs `exit`), the node kept in the arena as a
// LEAVING one - painted, invisible to every query - until the last exit
// track settles; `delay` holds a write (or an endpoint's start) on the
// animation clock, and a held write starting late runs as if started on
// time; `stagger` on an ancestor spaces the enters and exits beginning
// under it in one frame, the children order of the arena's hierarchy
// standing in for the element tree's.

use std::collections::HashMap;

use super::NodeId;
use crate::motion::{spring_step, TransitionSpec};

/// Which node register a track animates: a local-TRS component, or the
/// weights register (morph target weights, one lane per target).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Component {
  Position,
  Rotation,
  Scale,
  Weights,
}

/// The motion one entry or endpoint plays: the spec, and the hold (ms on
/// the animation clock) before a write to it applies.
#[derive(Clone, Copy, Debug)]
pub struct NodeMotion {
  pub spec: TransitionSpec,
  pub delay_ms: f32,
}

impl From<TransitionSpec> for NodeMotion {
  fn from(spec: TransitionSpec) -> Self {
    NodeMotion { spec, delay_ms: 0.0 }
  }
}

/// A lifecycle endpoint of one component (`from` at creation, `exit` at
/// removal): the lanes the component animates from or to, with the motion
/// that direction plays - resolved by the decoder, so an endpoint without
/// its own curve, duration, bounce or delay carries its entry's and the
/// runtime never falls back. `V` is the component's lane carrier: `[f32;
/// 3]` for position and scale, `[f32; 4]` for a rotation quaternion,
/// `Vec<f32>` for weights (one lane per morph target).
#[derive(Clone, Debug)]
pub struct NodeEndpoint<V> {
  pub value: V,
  pub motion: NodeMotion,
}

/// One component's declaration: the motion its writes play plus the
/// lifecycle endpoints (the element TransitionEntry, per component).
#[derive(Clone, Debug)]
pub struct NodeTransitionEntry<V> {
  pub motion: NodeMotion,
  pub from: Option<NodeEndpoint<V>>,
  pub exit: Option<NodeEndpoint<V>>,
}

impl<V> From<TransitionSpec> for NodeTransitionEntry<V> {
  fn from(spec: TransitionSpec) -> Self {
    NodeTransitionEntry { motion: spec.into(), from: None, exit: None }
  }
}

/// The transition declaration a node carries: an entry per component plus
/// an `all` catch-all (the element TransitionConfig shape; `all` carries
/// motion only, an endpoint needs its component named). Applies to
/// `write_transform` calls from the moment it is set; it does not
/// retroactively affect running tracks. An entry's `from` plays at the
/// first advance after `create` (mod.rs start_enter_transitions), so the
/// declaration must be set before that advance; its `exit` plays from
/// `exit` (mod.rs), whenever that comes.
#[derive(Clone, Debug, Default)]
pub struct NodeTransitionConfig {
  pub position: Option<NodeTransitionEntry<[f32; 3]>>,
  pub rotation: Option<NodeTransitionEntry<[f32; 4]>>,
  pub scale: Option<NodeTransitionEntry<[f32; 3]>>,
  /// The weights register's entry: its endpoints carry one lane per
  /// morph target (an endpoint shorter than the register pads with
  /// zeros, like a write).
  pub weights: Option<NodeTransitionEntry<Vec<f32>>>,
  pub all: Option<NodeMotion>,
  /// Makes the node a stagger group: every descendant enter (`from`) or
  /// exit that begins in the same frame under it gets `index * stagger_ms`
  /// of extra delay, indexed in occurrence order (enters and exits count
  /// separately). The nearest declaring ancestor wins; nested groups never
  /// compound. It orchestrates descendants only - the node's own lifecycle
  /// is staggered by ITS ancestors, and ordinary writes never stagger.
  pub stagger_ms: Option<f32>,
}

impl NodeTransitionConfig {
  /// The motion a write to `component` plays: its entry's, else `all`'s.
  pub fn motion_for(&self, component: Component) -> Option<NodeMotion> {
    match component {
      Component::Position => self.position.as_ref().map(|e| e.motion),
      Component::Rotation => self.rotation.as_ref().map(|e| e.motion),
      Component::Scale => self.scale.as_ref().map(|e| e.motion),
      Component::Weights => self.weights.as_ref().map(|e| e.motion),
    }
    .or(self.all)
  }

  /// Whether `component` declares an exit: the set whose tracks gate a
  /// leaving node's free.
  pub fn has_exit(&self, component: Component) -> bool {
    match component {
      Component::Position => self.position.as_ref().is_some_and(|e| e.exit.is_some()),
      Component::Rotation => self.rotation.as_ref().is_some_and(|e| e.exit.is_some()),
      Component::Scale => self.scale.as_ref().is_some_and(|e| e.exit.is_some()),
      Component::Weights => self.weights.as_ref().is_some_and(|e| e.exit.is_some()),
    }
  }
}

/// Every component a declaration can carry, the order lifecycle passes
/// walk them in.
pub const COMPONENTS: [Component; 4] = [Component::Position, Component::Rotation, Component::Scale, Component::Weights];

/// A track's or a held write's target as lanes: a position or scale uses
/// the first three, a rotation all four (its quaternion).
pub type Lanes = [f32; 4];

pub(super) fn lanes3(v: [f32; 3]) -> Lanes {
  [v[0], v[1], v[2], 0.0]
}

/// A write held by `delay`: it applies (starts or retargets the pair's
/// track) when the animation clock reaches `at_ms`, exactly as if written
/// then, and the track runs as if started at `at_ms` - a frame landing late
/// past the slot finds the motion already that far along.
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingWrite {
  pub node: NodeId,
  pub component: Component,
  pub to: Lanes,
  pub spec: TransitionSpec,
  pub at_ms: f64,
}

/// A weights write held by `delay` (the `PendingWrite` of the weights
/// lane, which has as many lanes as the register has targets).
#[derive(Clone, Debug)]
pub(super) struct PendingWeights {
  pub node: NodeId,
  pub to: Vec<f32>,
  pub spec: TransitionSpec,
  pub at_ms: f64,
}

/// The motion in force on one component, for the node dump: the target
/// lanes (three for position and scale, four for rotation, one per target
/// for weights), and the clock a held write applies at when it is still
/// waiting.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionState {
  pub component: Component,
  pub to: Vec<f32>,
  pub held_until_ms: Option<f64>,
}

// Settle threshold, scaled to the animated distance so world units and
// radians both settle promptly (the element transitions' rule).
fn eps_for(d: f32) -> f32 {
  d.abs().max(1.0) * 1e-3
}

/// Interpolation state of a position/scale track: three independent lanes.
#[derive(Clone, Copy, Debug)]
pub(super) enum LinearState {
  Tween {
    from: [f32; 3],
    start_ms: f64,
  },
  /// Position and velocity (units/s), integrated each frame.
  Spring {
    pos: [f32; 3],
    vel: [f32; 3],
  },
}

pub(super) struct LinearTrack {
  pub node: NodeId,
  pub component: Component,
  spec: TransitionSpec,
  state: LinearState,
  // The clock `state` is valid at: the scheduled time of a held write when
  // it starts, the previous advance otherwise. A spring integrates from
  // here to the advance clock, so a late activating frame catches up.
  since_ms: f64,
  to: [f32; 3],
  eps: f32,
}

impl LinearTrack {
  /// Advance to `now_ms`. Returns the value to write and whether the track
  /// settled; a settled track reports the target exactly.
  pub(super) fn advance(&mut self, now_ms: f64) -> ([f32; 3], bool) {
    let dt_ms = (now_ms - self.since_ms).max(0.0);
    self.since_ms = now_ms;
    match (&mut self.state, self.spec) {
      (LinearState::Tween { from, start_ms }, TransitionSpec::Tween { duration_ms, curve }) => {
        let p = ((now_ms - *start_ms) / duration_ms as f64).clamp(0.0, 1.0) as f32;
        if p >= 1.0 {
          return (self.to, true);
        }
        let e = curve.eval(p);
        let mut out = *from;
        for i in 0..3 {
          out[i] += (self.to[i] - out[i]) * e;
        }
        (out, false)
      }
      (LinearState::Spring { pos, vel }, TransitionSpec::Spring { omega, zeta }) => {
        let dt = (dt_ms / 1000.0) as f32;
        let mut settled = true;
        for i in 0..3 {
          let (x, v) = spring_step(pos[i] - self.to[i], vel[i], omega, zeta, dt);
          pos[i] = self.to[i] + x;
          vel[i] = v;
          if x.abs() >= self.eps || v.abs() >= self.eps * omega {
            settled = false;
          }
        }
        if settled {
          return (self.to, true);
        }
        (*pos, false)
      }
      // Spec kind and state kind are paired at creation and on retarget;
      // a mismatch cannot arise, but settle instantly rather than panic.
      _ => (self.to, true),
    }
  }
}

/// Interpolation state of a rotation track. The spring carries the current
/// orientation and the angular velocity (rad/s) as a rotation vector in
/// the tangent space at the target: each step maps the offset from the
/// target through the log map, springs the three lanes with the shared
/// oscillator (the ODE is linear, so lanes decouple), and maps back
/// through the exp map. Retargeting moves the equilibrium and keeps the
/// velocity vector - momentum survives, like the linear spring.
#[derive(Clone, Copy, Debug)]
pub(super) enum RotationState {
  Tween { from: [f32; 4], start_ms: f64 },
  Spring { q: [f32; 4], vel: [f32; 3] },
}

pub(super) struct RotationTrack {
  pub node: NodeId,
  spec: TransitionSpec,
  state: RotationState,
  since_ms: f64,
  /// Kept on the hemisphere of the track's current orientation, so the
  /// path is the short arc.
  to: [f32; 4],
  eps: f32,
}

impl RotationTrack {
  pub(super) fn advance(&mut self, now_ms: f64) -> ([f32; 4], bool) {
    let dt_ms = (now_ms - self.since_ms).max(0.0);
    self.since_ms = now_ms;
    match (&mut self.state, self.spec) {
      (RotationState::Tween { from, start_ms }, TransitionSpec::Tween { duration_ms, curve }) => {
        let p = ((now_ms - *start_ms) / duration_ms as f64).clamp(0.0, 1.0) as f32;
        if p >= 1.0 {
          return (self.to, true);
        }
        (slerp(*from, self.to, curve.eval(p)), false)
      }
      (RotationState::Spring { q, vel }, TransitionSpec::Spring { omega, zeta }) => {
        let dt = (dt_ms / 1000.0) as f32;
        let mut x = quat_log(quat_mul(quat_conjugate(self.to), *q));
        let mut settled = true;
        for i in 0..3 {
          let (nx, nv) = spring_step(x[i], vel[i], omega, zeta, dt);
          x[i] = nx;
          vel[i] = nv;
          if nx.abs() >= self.eps || nv.abs() >= self.eps * omega {
            settled = false;
          }
        }
        if settled {
          return (self.to, true);
        }
        *q = quat_normalize(quat_mul(self.to, quat_exp(x)));
        (*q, false)
      }
      _ => (self.to, true),
    }
  }
}

/// Interpolation state of a weights track: one independent lane per
/// morph target, the linear track's math over a register-sized vector.
#[derive(Clone, Debug)]
pub(super) enum WeightsState {
  Tween { from: Vec<f32>, start_ms: f64 },
  Spring { pos: Vec<f32>, vel: Vec<f32> },
}

pub(super) struct WeightsTrack {
  pub node: NodeId,
  spec: TransitionSpec,
  state: WeightsState,
  since_ms: f64,
  to: Vec<f32>,
  eps: f32,
}

impl WeightsTrack {
  /// Advance to `now_ms`, writing the value into `out` (resized to the
  /// track's lanes). Returns whether the track settled; a settled track
  /// reports the target exactly.
  pub(super) fn advance(&mut self, now_ms: f64, out: &mut Vec<f32>) -> bool {
    let dt_ms = (now_ms - self.since_ms).max(0.0);
    self.since_ms = now_ms;
    out.clear();
    match (&mut self.state, self.spec) {
      (WeightsState::Tween { from, start_ms }, TransitionSpec::Tween { duration_ms, curve }) => {
        let p = ((now_ms - *start_ms) / duration_ms as f64).clamp(0.0, 1.0) as f32;
        if p >= 1.0 {
          out.extend_from_slice(&self.to);
          return true;
        }
        let e = curve.eval(p);
        out.extend(from.iter().zip(&self.to).map(|(&a, &b)| a + (b - a) * e));
        false
      }
      (WeightsState::Spring { pos, vel }, TransitionSpec::Spring { omega, zeta }) => {
        let dt = (dt_ms / 1000.0) as f32;
        let mut settled = true;
        for i in 0..self.to.len() {
          let (x, v) = spring_step(pos[i] - self.to[i], vel[i], omega, zeta, dt);
          pos[i] = self.to[i] + x;
          vel[i] = v;
          if x.abs() >= self.eps || v.abs() >= self.eps * omega {
            settled = false;
          }
        }
        if settled {
          out.extend_from_slice(&self.to);
          return true;
        }
        out.extend_from_slice(pos);
        false
      }
      _ => {
        out.extend_from_slice(&self.to);
        true
      }
    }
  }
}

/// `v` brought to `len` lanes: cut, or padded with zeros (a weights
/// endpoint or write shorter than the register weighs the rest at zero,
/// exactly as the register's rows publish).
fn fit_lanes(v: &[f32], len: usize) -> Vec<f32> {
  let mut out = v[..v.len().min(len)].to_vec();
  out.resize(len, 0.0);
  out
}

/// Arena-level transition state: the per-node declarations, the running
/// tracks, the held writes and the animation clock, stamped once per frame
/// from the app timeline before the frame's JS runs, so writes and the
/// advance agree on time. Owned by `Spatial`; the write/advance plumbing
/// lives in mod.rs, where the nodes are.
#[derive(Default)]
pub(super) struct NodeTransitions {
  pub now_ms: f64,
  pub configs: HashMap<NodeId, NodeTransitionConfig>,
  pub linear: Vec<LinearTrack>,
  pub rotation: Vec<RotationTrack>,
  pub weights: Vec<WeightsTrack>,
  // Writes held by `delay`, applied by the advance that finds them due.
  pub pending: Vec<PendingWrite>,
  pub pending_weights: Vec<PendingWeights>,
  // (node, component) pairs whose track settled, awaiting the embedder's
  // drain. Cancelled tracks, and the tracks of a leaving node, never land
  // here.
  pub settled: Vec<(NodeId, Component)>,
  // Nodes created since the last advance, owed their enter animation
  // (mod.rs start_enter_transitions drains it; a node without enter
  // values, or freed again already, costs one lookup).
  pub entering: Vec<NodeId>,
  // Leaving nodes whose free gate wants a look after the advance: an exit
  // track settled, a due exit write started nothing, a declaration was
  // cleared (mod.rs check_exit).
  pub exit_checks: Vec<NodeId>,
  // Leaving nodes freed since the last drain, however the free came (the
  // gate emptying, a cascade from the parent's, a `destroy`).
  pub freed: Vec<NodeId>,
  // Per-frame stagger counters, keyed by (group ancestor, is_exit): how
  // many descendant enters/exits the group has seen this frame. Cleared
  // at every clock stamp (mod.rs set_transition_now), so a batch created
  // or let go of in one tick cascades and later frames start at zero.
  pub stagger_counts: HashMap<(NodeId, bool), u32>,
  // Exits let go of under a stagger group since the last advance, with
  // the clock they were let go of at: the advance orders them by their
  // place in the tree and starts them (mod.rs start_staggered_exits), so
  // the cascade never depends on the order the consumer tore down in.
  pub staggered_exits: Vec<(NodeId, f64)>,
}

impl NodeTransitions {
  /// The next stagger index for a lifecycle event under `group` this
  /// frame (post-incremented). Enters and exits count separately, so a
  /// swap that frees and creates in one tick runs two clean cascades.
  pub fn stagger_index(&mut self, group: NodeId, exit: bool) -> u32 {
    let count = self.stagger_counts.entry((group, exit)).or_insert(0);
    let index = *count;
    *count += 1;
    index
  }

  /// Nothing to advance: no track runs, no write is held and no exit
  /// waits for its place in a cascade. A held write keeps the advance
  /// live so its activation frame comes.
  pub fn is_empty(&self) -> bool {
    self.linear.is_empty()
      && self.rotation.is_empty()
      && self.weights.is_empty()
      && self.pending.is_empty()
      && self.pending_weights.is_empty()
      && self.staggered_exits.is_empty()
  }

  /// Start or retarget the position/scale track for (node, component), as
  /// of `at_ms`: the current clock for a write landing now, the scheduled
  /// time for a held write coming due. `current` is the node's present
  /// value (the from-value for a fresh or restarted tween); a running
  /// spring keeps its position and velocity and only moves its
  /// equilibrium. A write matching a running track's target is a no-op -
  /// the full-TRS write shape re-sends unchanged components on every call,
  /// and re-anchoring a tween on them would restart it. Returns whether a
  /// track now runs for the pair.
  pub fn retarget_linear(
    &mut self,
    node: NodeId,
    component: Component,
    current: [f32; 3],
    to: [f32; 3],
    spec: TransitionSpec,
    at_ms: f64,
  ) -> bool {
    let mut d = 0.0f32;
    for i in 0..3 {
      d = d.max((to[i] - current[i]).abs());
    }
    if let Some(t) = self.linear.iter_mut().find(|t| t.node == node && t.component == component) {
      if t.to == to {
        return true;
      }
      t.to = to;
      t.eps = eps_for(d);
      let keep_spring_state = matches!((&t.state, spec), (LinearState::Spring { .. }, TransitionSpec::Spring { .. }));
      t.spec = spec;
      if !keep_spring_state {
        t.state = match spec {
          TransitionSpec::Tween { .. } => LinearState::Tween { from: current, start_ms: at_ms },
          TransitionSpec::Spring { .. } => LinearState::Spring { pos: current, vel: [0.0; 3] },
        };
        t.since_ms = at_ms;
      }
      return true;
    }
    if to == current {
      return false;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => LinearState::Tween { from: current, start_ms: at_ms },
      TransitionSpec::Spring { .. } => LinearState::Spring { pos: current, vel: [0.0; 3] },
    };
    self.linear.push(LinearTrack { node, component, spec, state, since_ms: at_ms, to, eps: eps_for(d) });
    true
  }

  /// The rotation counterpart. Quaternion sign is normalized away: q and
  /// -q are the same rotation, so target comparison accepts either and
  /// the stored target is flipped to the hemisphere of the track's
  /// current orientation (the short arc).
  pub fn retarget_rotation(
    &mut self,
    node: NodeId,
    current: [f32; 4],
    to: [f32; 4],
    spec: TransitionSpec,
    at_ms: f64,
  ) -> bool {
    if let Some(t) = self.rotation.iter_mut().find(|t| t.node == node) {
      if same_quat(t.to, to) {
        return true;
      }
      let anchor = match t.state {
        RotationState::Spring { q, .. } => q,
        RotationState::Tween { .. } => current,
      };
      t.to = near_hemisphere(to, anchor);
      t.eps = eps_for(angle_between(anchor, to));
      let keep_spring_state = matches!((&t.state, spec), (RotationState::Spring { .. }, TransitionSpec::Spring { .. }));
      t.spec = spec;
      if !keep_spring_state {
        t.state = match spec {
          TransitionSpec::Tween { .. } => RotationState::Tween { from: current, start_ms: at_ms },
          TransitionSpec::Spring { .. } => RotationState::Spring { q: current, vel: [0.0; 3] },
        };
        t.since_ms = at_ms;
      }
      return true;
    }
    if same_quat(to, current) {
      return false;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => RotationState::Tween { from: current, start_ms: at_ms },
      TransitionSpec::Spring { .. } => RotationState::Spring { q: current, vel: [0.0; 3] },
    };
    self.rotation.push(RotationTrack {
      node,
      spec,
      state,
      since_ms: at_ms,
      to: near_hemisphere(to, current),
      eps: eps_for(angle_between(current, to)),
    });
    true
  }

  /// The weights counterpart of `retarget_linear`: the lanes are the
  /// register's targets, as many as the longer of `current` and `to`
  /// (the shorter padded with zeros). A running track keeps its lane
  /// count unless the target is wider.
  pub fn retarget_weights(
    &mut self,
    node: NodeId,
    current: &[f32],
    to: &[f32],
    spec: TransitionSpec,
    at_ms: f64,
  ) -> bool {
    let len = current.len().max(to.len());
    let current = fit_lanes(current, len);
    let to = fit_lanes(to, len);
    let mut d = 0.0f32;
    for i in 0..len {
      d = d.max((to[i] - current[i]).abs());
    }
    if let Some(t) = self.weights.iter_mut().find(|t| t.node == node) {
      if t.to == to {
        return true;
      }
      let keep_spring_state =
        matches!((&t.state, spec), (WeightsState::Spring { .. }, TransitionSpec::Spring { .. }));
      if keep_spring_state {
        if let WeightsState::Spring { pos, vel } = &mut t.state {
          // A wider target grows the lanes; the new ones start at rest
          // where the register holds them.
          for i in pos.len()..len {
            pos.push(current[i]);
            vel.push(0.0);
          }
        }
      } else {
        t.state = match spec {
          TransitionSpec::Tween { .. } => WeightsState::Tween { from: current, start_ms: at_ms },
          TransitionSpec::Spring { .. } => WeightsState::Spring { pos: current, vel: vec![0.0; len] },
        };
        t.since_ms = at_ms;
      }
      t.to = to;
      t.eps = eps_for(d);
      t.spec = spec;
      return true;
    }
    if to == current {
      return false;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => WeightsState::Tween { from: current, start_ms: at_ms },
      TransitionSpec::Spring { .. } => WeightsState::Spring { pos: current, vel: vec![0.0; len] },
    };
    self.weights.push(WeightsTrack { node, spec, state, since_ms: at_ms, to, eps: eps_for(d) });
    true
  }

  /// The weights counterpart of `write`: a write to the node's weights
  /// register through `motion` - held when it carries a delay, started or
  /// retargeted now otherwise. Returns whether the register now animates
  /// or waits.
  pub fn write_weights(&mut self, node: NodeId, current: &[f32], to: &[f32], motion: NodeMotion, unchanged: bool) -> bool {
    let now = self.now_ms;
    if motion.delay_ms > 0.0 {
      if let Some(w) = self.pending_weights.iter().find(|w| w.node == node) {
        if w.to == to {
          return true;
        }
      } else if self.weights_target_of(node).is_some_and(|t| t == to) {
        return true;
      } else if unchanged {
        return false;
      }
      self.schedule_weights(PendingWeights { node, to: to.to_vec(), spec: motion.spec, at_ms: now + motion.delay_ms as f64 });
      return true;
    }
    self.unschedule_weights(node);
    self.retarget_weights(node, current, to, motion.spec, now)
  }

  /// The target a running weights track for the node heads for.
  fn weights_target_of(&self, node: NodeId) -> Option<&[f32]> {
    self.weights.iter().find(|t| t.node == node).map(|t| t.to.as_slice())
  }

  /// Hold a delayed weights write (one per node; a newer one replaces it,
  /// delay restarted).
  pub fn schedule_weights(&mut self, write: PendingWeights) {
    self.unschedule_weights(write.node);
    self.pending_weights.push(write);
  }

  pub fn unschedule_weights(&mut self, node: NodeId) {
    self.pending_weights.retain(|w| w.node != node);
  }

  /// Drain the held weights writes whose activation time has arrived.
  pub fn take_due_weights(&mut self, now_ms: f64) -> Vec<PendingWeights> {
    let mut due = Vec::new();
    self.pending_weights.retain(|w| {
      if w.at_ms <= now_ms {
        due.push(w.clone());
        false
      } else {
        true
      }
    });
    due
  }

  /// Drop the node's weights track or held write and hand back its
  /// target, if one was in force (the enter animation's retarget).
  pub fn take_weights_target(&mut self, node: NodeId) -> Option<Vec<f32>> {
    if let Some(i) = self.pending_weights.iter().position(|w| w.node == node) {
      return Some(self.pending_weights.swap_remove(i).to);
    }
    let i = self.weights.iter().position(|t| t.node == node)?;
    Some(self.weights.swap_remove(i).to)
  }

  /// Apply a write to (node, component) through its motion: held when the
  /// motion carries a delay, started or retargeted now otherwise.
  /// `current` and `to` are lanes (see `Lanes`); `unchanged` says the
  /// write matches what the pair already holds or heads for (no track, no
  /// hold, `to` == `current`), which is a no-op either way. Returns whether
  /// the pair now animates or waits.
  pub fn write(
    &mut self,
    node: NodeId,
    component: Component,
    current: Lanes,
    to: Lanes,
    motion: NodeMotion,
    unchanged: bool,
  ) -> bool {
    let now = self.now_ms;
    if motion.delay_ms > 0.0 {
      if let Some(w) = self.pending.iter().find(|w| w.node == node && w.component == component) {
        if targets_match(component, w.to, to) {
          return true;
        }
      } else if self.target_of(node, component).is_some_and(|t| targets_match(component, t, to)) {
        return true;
      } else if unchanged {
        return false;
      }
      self.schedule(PendingWrite { node, component, to, spec: motion.spec, at_ms: now + motion.delay_ms as f64 });
      return true;
    }
    // Last write wins: an immediate write supersedes a held one.
    self.unschedule(node, component);
    self.apply(node, component, current, to, motion.spec, now)
  }

  /// Start or retarget the pair's track from `current` toward `to` as of
  /// `at_ms`, whichever list it lives in.
  pub fn apply(
    &mut self,
    node: NodeId,
    component: Component,
    current: Lanes,
    to: Lanes,
    spec: TransitionSpec,
    at_ms: f64,
  ) -> bool {
    match component {
      Component::Rotation => self.retarget_rotation(node, current, to, spec, at_ms),
      Component::Weights => false,
      _ => {
        self.retarget_linear(node, component, [current[0], current[1], current[2]], [to[0], to[1], to[2]], spec, at_ms)
      }
    }
  }

  /// The target a running TRS track for the pair heads for, if one runs.
  fn target_of(&self, node: NodeId, component: Component) -> Option<Lanes> {
    match component {
      Component::Rotation => self.rotation.iter().find(|t| t.node == node).map(|t| t.to),
      Component::Weights => None,
      _ => self.linear.iter().find(|t| t.node == node && t.component == component).map(|t| lanes3(t.to)),
    }
  }

  /// Hold a delayed write until its activation time. One hold per
  /// (node, component): a newer write replaces it, delay restarted.
  pub fn schedule(&mut self, write: PendingWrite) {
    self.unschedule(write.node, write.component);
    self.pending.push(write);
  }

  pub fn unschedule(&mut self, node: NodeId, component: Component) {
    self.pending.retain(|w| !(w.node == node && w.component == component));
  }

  /// Drain the held writes whose activation time has arrived.
  pub fn take_due(&mut self, now_ms: f64) -> Vec<PendingWrite> {
    let mut due = Vec::new();
    self.pending.retain(|w| {
      if w.at_ms <= now_ms {
        due.push(*w);
        false
      } else {
        true
      }
    });
    due
  }

  /// Whether the pair animates or waits: a track runs or a write is held.
  pub fn any_running(&self, node: NodeId, component: Component) -> bool {
    if component == Component::Weights {
      return self.weights_target_of(node).is_some() || self.pending_weights.iter().any(|w| w.node == node);
    }
    self.target_of(node, component).is_some() || self.pending.iter().any(|w| w.node == node && w.component == component)
  }

  /// Drop every track and held write of a node (config cleared, or the
  /// node freed). The node keeps whatever mid-flight value the last
  /// advance wrote; no settled event fires.
  pub fn cancel_node(&mut self, node: NodeId) {
    self.linear.retain(|t| t.node != node);
    self.rotation.retain(|t| t.node != node);
    self.weights.retain(|t| t.node != node);
    self.pending.retain(|w| w.node != node);
    self.pending_weights.retain(|w| w.node != node);
    self.staggered_exits.retain(|(n, _)| *n != node);
  }

  /// Drop the pair's track or held write and hand back its target, if one
  /// was in force: the enter animation restarts the component from `from`
  /// toward it. TRS components only (`take_weights_target` for weights).
  pub fn take_target(&mut self, node: NodeId, component: Component) -> Option<Lanes> {
    if let Some(i) = self.pending.iter().position(|w| w.node == node && w.component == component) {
      return Some(self.pending.swap_remove(i).to);
    }
    match component {
      Component::Weights => None,
      Component::Rotation => {
        let i = self.rotation.iter().position(|t| t.node == node)?;
        Some(self.rotation.swap_remove(i).to)
      }
      _ => {
        let i = self.linear.iter().position(|t| t.node == node && t.component == component)?;
        Some(lanes3(self.linear.swap_remove(i).to))
      }
    }
  }

  /// The motion in force on a node, per component (the node dump).
  pub fn motion_of(&self, node: NodeId) -> Vec<MotionState> {
    let mut out = Vec::new();
    for t in self.linear.iter().filter(|t| t.node == node) {
      out.push(MotionState { component: t.component, to: t.to.to_vec(), held_until_ms: None });
    }
    for t in self.rotation.iter().filter(|t| t.node == node) {
      out.push(MotionState { component: Component::Rotation, to: t.to.to_vec(), held_until_ms: None });
    }
    for t in self.weights.iter().filter(|t| t.node == node) {
      out.push(MotionState { component: Component::Weights, to: t.to.clone(), held_until_ms: None });
    }
    for w in self.pending.iter().filter(|w| w.node == node) {
      let lanes = if w.component == Component::Rotation { 4 } else { 3 };
      out.push(MotionState { component: w.component, to: w.to[..lanes].to_vec(), held_until_ms: Some(w.at_ms) });
    }
    for w in self.pending_weights.iter().filter(|w| w.node == node) {
      out.push(MotionState { component: Component::Weights, to: w.to.clone(), held_until_ms: Some(w.at_ms) });
    }
    out
  }
}

/// Two targets of a component name the same place: exact lanes for
/// position and scale (writes re-send the same floats), either quaternion
/// sign for a rotation.
pub(super) fn targets_match(component: Component, a: Lanes, b: Lanes) -> bool {
  match component {
    Component::Rotation => same_quat(a, b),
    Component::Weights => a == b,
    _ => a[0..3] == b[0..3],
  }
}

// Quaternion helpers, xyzw (the arena's rotation layout). Local to the
// track math on purpose: math.rs consumes quaternions whole (compose);
// only the geodesic tracks decompose them.

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
  [
    a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
    a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
    a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
    a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
  ]
}

fn quat_conjugate(q: [f32; 4]) -> [f32; 4] {
  [-q[0], -q[1], -q[2], q[3]]
}

fn quat_dot(a: [f32; 4], b: [f32; 4]) -> f32 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

pub(super) fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
  let l = quat_dot(q, q).sqrt();
  if l > 0.0 {
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
  } else {
    [0.0, 0.0, 0.0, 1.0]
  }
}

/// The rotation vector (axis * angle, radians) of the SHORT arc from
/// identity to `q`: the sign of q is normalized first, so the result's
/// angle is at most pi.
fn quat_log(q: [f32; 4]) -> [f32; 3] {
  let q = if q[3] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
  let sin_half = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
  if sin_half < 1e-6 {
    // Small angle: angle/sin(angle/2) -> 2, first order.
    return [2.0 * q[0], 2.0 * q[1], 2.0 * q[2]];
  }
  let angle = 2.0 * sin_half.atan2(q[3]);
  let s = angle / sin_half;
  [q[0] * s, q[1] * s, q[2] * s]
}

fn quat_exp(r: [f32; 3]) -> [f32; 4] {
  let angle = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
  if angle < 1e-6 {
    return quat_normalize([r[0] / 2.0, r[1] / 2.0, r[2] / 2.0, 1.0]);
  }
  let (sin, cos) = (angle / 2.0).sin_cos();
  let s = sin / angle;
  [r[0] * s, r[1] * s, r[2] * s, cos]
}

/// `b` (or its negation) on the hemisphere of `a`, so interpolation takes
/// the short arc.
fn near_hemisphere(b: [f32; 4], a: [f32; 4]) -> [f32; 4] {
  if quat_dot(a, b) < 0.0 {
    [-b[0], -b[1], -b[2], -b[3]]
  } else {
    b
  }
}

/// The same rotation, either sign (the target-unchanged check; writes
/// re-send the same floats, so exact comparison is the contract).
fn same_quat(a: [f32; 4], b: [f32; 4]) -> bool {
  a == b || a == [-b[0], -b[1], -b[2], -b[3]]
}

/// Angle (radians) between two unit quaternions, short arc.
fn angle_between(a: [f32; 4], b: [f32; 4]) -> f32 {
  2.0 * quat_dot(a, b).abs().clamp(0.0, 1.0).acos()
}

pub(super) fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
  let b = near_hemisphere(b, a);
  let dot = quat_dot(a, b).clamp(-1.0, 1.0);
  if dot > 0.9995 {
    // Nearly parallel: lerp and renormalize.
    let mut out = [0.0f32; 4];
    for i in 0..4 {
      out[i] = a[i] + (b[i] - a[i]) * t;
    }
    return quat_normalize(out);
  }
  let theta = dot.acos();
  let sin_theta = theta.sin();
  let wa = ((1.0 - t) * theta).sin() / sin_theta;
  let wb = (t * theta).sin() / sin_theta;
  [a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb, a[2] * wa + b[2] * wb, a[3] * wa + b[3] * wb]
}
