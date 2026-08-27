// Native transitions on node transforms (okf/backlog/spatial-node-
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
// (spring default, retarget keeps spring state, settles land exactly);
// the element lifecycle conveniences (delay, from, exit, stagger) do not
// apply to nodes and are deliberately absent.

use std::collections::HashMap;

use super::NodeId;
use crate::motion::{spring_step, TransitionSpec};

/// Which local-TRS component a node track animates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Component {
  Position,
  Rotation,
  Scale,
}

/// The transition declaration a node carries: a spec per component plus an
/// `all` catch-all (the element TransitionConfig shape). Applies to
/// `write_transform` calls from the moment it is set; it does not
/// retroactively affect running tracks.
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeTransitionConfig {
  pub position: Option<TransitionSpec>,
  pub rotation: Option<TransitionSpec>,
  pub scale: Option<TransitionSpec>,
  pub all: Option<TransitionSpec>,
}

impl NodeTransitionConfig {
  pub fn entry_for(&self, component: Component) -> Option<TransitionSpec> {
    match component {
      Component::Position => self.position,
      Component::Rotation => self.rotation,
      Component::Scale => self.scale,
    }
    .or(self.all)
  }
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
  to: [f32; 3],
  eps: f32,
}

impl LinearTrack {
  /// Advance to `now_ms` (`dt_ms` since the previous advance, for the
  /// spring). Returns the value to write and whether the track settled; a
  /// settled track reports the target exactly.
  pub(super) fn advance(&mut self, now_ms: f64, dt_ms: f64) -> ([f32; 3], bool) {
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
  /// Kept on the hemisphere of the track's current orientation, so the
  /// path is the short arc.
  to: [f32; 4],
  eps: f32,
}

impl RotationTrack {
  pub(super) fn advance(&mut self, now_ms: f64, dt_ms: f64) -> ([f32; 4], bool) {
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

/// Arena-level transition state: the per-node declarations, the running
/// tracks and the animation clock, stamped once per frame from the app
/// timeline before the frame's JS runs, so writes and the advance agree
/// on time. Owned by `Spatial`; the write/advance plumbing lives in
/// mod.rs, where the nodes are.
#[derive(Default)]
pub(super) struct NodeTransitions {
  pub now_ms: f64,
  // Time of the previous advance, the spring dt reference. Reset when the
  // track list becomes non-empty so an idle gap never enters a spring.
  pub last_ms: f64,
  pub configs: HashMap<NodeId, NodeTransitionConfig>,
  pub linear: Vec<LinearTrack>,
  pub rotation: Vec<RotationTrack>,
  // (node, component) pairs whose track settled, awaiting the embedder's
  // drain. Cancelled tracks never land here.
  pub settled: Vec<(NodeId, Component)>,
}

impl NodeTransitions {
  pub fn is_empty(&self) -> bool {
    self.linear.is_empty() && self.rotation.is_empty()
  }

  /// Start or retarget the position/scale track for (node, component).
  /// `current` is the node's present value (the from-value for a fresh or
  /// restarted tween); a running spring keeps its position and velocity
  /// and only moves its equilibrium. A write matching a running track's
  /// target is a no-op - the full-TRS write shape re-sends unchanged
  /// components on every call, and re-anchoring a tween on them would
  /// restart it. Returns whether a track now runs for the pair.
  pub fn retarget_linear(
    &mut self,
    node: NodeId,
    component: Component,
    current: [f32; 3],
    to: [f32; 3],
    spec: TransitionSpec,
  ) -> bool {
    let now = self.now_ms;
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
          TransitionSpec::Tween { .. } => LinearState::Tween { from: current, start_ms: now },
          TransitionSpec::Spring { .. } => LinearState::Spring { pos: current, vel: [0.0; 3] },
        };
      }
      return true;
    }
    if to == current {
      return false;
    }
    if self.is_empty() {
      self.last_ms = now;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => LinearState::Tween { from: current, start_ms: now },
      TransitionSpec::Spring { .. } => LinearState::Spring { pos: current, vel: [0.0; 3] },
    };
    self.linear.push(LinearTrack { node, component, spec, state, to, eps: eps_for(d) });
    true
  }

  /// The rotation counterpart. Quaternion sign is normalized away: q and
  /// -q are the same rotation, so target comparison accepts either and
  /// the stored target is flipped to the hemisphere of the track's
  /// current orientation (the short arc).
  pub fn retarget_rotation(&mut self, node: NodeId, current: [f32; 4], to: [f32; 4], spec: TransitionSpec) -> bool {
    let now = self.now_ms;
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
          TransitionSpec::Tween { .. } => RotationState::Tween { from: current, start_ms: now },
          TransitionSpec::Spring { .. } => RotationState::Spring { q: current, vel: [0.0; 3] },
        };
      }
      return true;
    }
    if same_quat(to, current) {
      return false;
    }
    if self.is_empty() {
      self.last_ms = now;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => RotationState::Tween { from: current, start_ms: now },
      TransitionSpec::Spring { .. } => RotationState::Spring { q: current, vel: [0.0; 3] },
    };
    self.rotation.push(RotationTrack {
      node,
      spec,
      state,
      to: near_hemisphere(to, current),
      eps: eps_for(angle_between(current, to)),
    });
    true
  }

  /// Drop every track of a node (config cleared, or the node freed). The
  /// node keeps whatever mid-flight value the last advance wrote; no
  /// settled event fires.
  pub fn cancel_node(&mut self, node: NodeId) {
    self.linear.retain(|t| t.node != node);
    self.rotation.retain(|t| t.node != node);
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

fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
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

fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
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
