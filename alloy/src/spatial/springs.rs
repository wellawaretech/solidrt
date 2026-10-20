// Spring bones: secondary motion solved in core. A chain of joints (hair,
// a skirt, a tail, a cable) whose tails lag behind the animated bones they
// hang from, pulled back toward the rest direction, pulled down by
// gravity, kept at bone length and pushed out of colliders - Unity's
// SpringBone, Godot's SpringBoneSimulator3D, three-vrm's
// VRMSpringBoneJoint, with the VRM vocabulary (stiffness, drag, gravity
// power and direction, hit radius) so authored values mean what they mean
// there. Declared once per model, stepped once per frame AFTER the frame's
// JS and its flush (the pose post-pass in the draw tick, ahead of the
// spatial flush): secondary motion is a function of the FINAL primary
// pose, and the frame's JS is a primary producer (a look-at, an IK, a
// controller's move), so this is the one mover that steps after it.
//
// The step is three-vrm's: per joint, in the chain's simulation space (the
// center node's space, else world space), with the bone's position `p`
// and its rest tail direction `d` at bone length `L` taken from the
// parent's matrix as it stands this frame,
//
//   tail' = tail + (tail - prev) * keep
//         + (d * stiffness + g * power + force) * S
//   tail' = p + normalize(tail' - p) * L, then out of each collider, then
//   back to L again; the bone turns from its rest tail to tail'
//
// where keep = 1 - drag, `force` is the chain's runtime force (the wind,
// beside the joint's authored gravity, as UniVRM's ExternalForce and
// Godot's external_force) and S is the reference step, 1/60 s. three-vrm,
// UniVRM and Godot apply that map once per FRAME with S = the frame's
// delta, so the drag damps twice as much per second at 120 Hz as at 60 Hz
// and the same hair swings differently on every display. A fixed 60 Hz
// substep would need interpolation on 50/90/120 Hz displays, and
// interpolation costs a substep of latency - the lag the placement above
// exists to avoid. Instead the step here is the continuous flow that
// agrees with three-vrm's map on every whole S and is defined for any dt:
//
//   per step S, with velocity u = (tail - prev) / S and kick f = d *
//   stiffness + g * power + force (all in length per second):
//     u' = keep * u + f
//     x' = x + S * u'
//   is affine in (x, u) with eigenvalues 1 and keep, so it has a unique
//   real logarithm: the system
//     du/dt = -lambda * u + mu * f
//     dx/dt = a * u + c * f
//   whose flow over S is that map. Matching gives, with r = (1 - keep) /
//   -ln keep,
//     lambda = -ln keep / S,  mu = 1 / (S * r),  a = keep / r,
//     c = 1 - a * (1 - r) / (1 - keep)
//   and the flow over any t, with s = lambda * t, E = exp(-s),
//   g1 = (1 - E) / s and g2 = (s - 1 + E) / s^2:
//     u' = E * u + mu * f * t * g1
//     x' = x + a * (u * t * g1 + mu * f * t^2 * g2) + c * f * t.
//   Two half steps compose to one exactly; t = S is three-vrm's step to
//   the float. The projections (length, colliders) are the nonlinear
//   part and stay per step, as in every engine, which is where a step
//   size still shows: a frame is split into substeps of S / 4 (240 Hz,
//   finer than any display), so every rate resolves the same model to
//   within a third of a millimetre on a 20 cm chain in a quarter-turn
//   step response (whole frames: 4 mm between 60 and 120 Hz; the test
//   pins it), and a long frame cannot tunnel a collider. The projection's
//   correction enters the velocity the way position verlet implies it
//   (at S: the corrected displacement over S), scaled so a resting tail
//   is a fixed point of every step size.
//
// Engine independent: native types in and out (rendertree rules).

use std::collections::HashMap;

use super::math::{invert_affine, rotate_vector, transform_point, transform_vector};
use super::players::quat_multiply;
use super::transitions::quat_normalize;
use super::{compose, multiply, Mat4, NodeId, Spatial, IDENTITY};

pub type SpringId = u64;

/// The reference step: the frame three-vrm, UniVRM and Godot are tuned
/// at. The flow agrees with their map on every whole step of this, and a
/// longer frame is split into steps of at most this for the projections.
pub const SPRING_REFERENCE_STEP: f64 = 1.0 / 60.0;
/// Substeps per reference step: the projections resolve at 240 Hz, so
/// every display rate solves the same model (see the header).
const SUBSTEPS_PER_REFERENCE_STEP: u32 = 4;
/// Most reference steps one advance covers; a longer gap (a stall, a
/// resume) is stepped as this many, so a chain is never flung.
const MAX_REFERENCE_STEPS_PER_ADVANCE: u32 = 8;
/// A frame within this fraction of a whole number of substeps takes that
/// many (a 60 Hz frame is four, not five): the float noise of a clock
/// stamp must not double the projection rate.
const SUBSTEP_COUNT_SLACK: f64 = 1e-3;
/// Below this keep (1 - drag) the per-step damping is total for any step:
/// the clamp keeps ln keep finite.
const KEEP_MIN: f64 = 1e-6;
/// Below this 1 - keep the flow constants take their series form (the
/// closed forms cancel catastrophically as keep -> 1).
const KEEP_SERIES_BELOW: f64 = 1e-4;
/// Below this s = lambda * t the step's g1/g2 take their series form.
const STEP_SERIES_BELOW: f64 = 1e-3;
/// A tail moving slower than this (simulation units per second) counts
/// as settled: a chain whose every tail settled asks for no frame. 0.1 mm
/// per second on a model in metres; the residual drift after the gate
/// closes is a few micrometres.
const SETTLE_SPEED: f32 = 1e-4;
/// A solved rotation within this of the node's (per quaternion component)
/// is not written: the float noise of a settled chain stays out of the
/// flush.
const ROTATION_EPSILON: f32 = 1e-6;
/// Two unit vectors closer than this to antiparallel turn about a
/// perpendicular instead of the degenerate cross product.
const ANTIPARALLEL_EPSILON: f32 = 1e-6;

/// One simulated bone. `tail` is where the chain continues, in the bone's
/// own space (the next joint's rest position; a leaf's virtual tail);
/// `rest_rotation` the bone's rest local rotation, which the solve turns
/// from. Settings are VRM's: `stiffness` pulls the tail back toward the
/// rest direction, `drag` damps the motion (0 none, 1 total), gravity is
/// `gravity_dir` (world space, any length) at `gravity_power`, `hit_radius`
/// the tail's radius against colliders. Lengths are in the bone's own
/// units and follow its matrices' scale.
#[derive(Clone, Copy, Debug)]
pub struct SpringJoint {
  pub node: NodeId,
  pub tail: [f32; 3],
  pub rest_rotation: [f32; 4],
  pub stiffness: f32,
  pub drag: f32,
  pub gravity_power: f32,
  pub gravity_dir: [f32; 3],
  pub hit_radius: f32,
}

/// A change to a joint's authored settings (`set_spring_joint`): the
/// fields given replace the joint's, the others stay. The tail and the
/// rest rotation are the chain's structure and stay as created.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpringJointUpdate {
  pub stiffness: Option<f32>,
  pub drag: Option<f32>,
  pub gravity_power: Option<f32>,
  pub gravity_dir: Option<[f32; 3]>,
  pub hit_radius: Option<f32>,
}

/// A collider's shape, in its node's space. `inside` keeps the tails
/// WITHIN the sphere or capsule instead of out of it (a skirt around the
/// legs); a plane keeps them on its normal's side.
#[derive(Clone, Copy, Debug)]
pub enum SpringShape {
  Sphere { offset: [f32; 3], radius: f32, inside: bool },
  Capsule { a: [f32; 3], b: [f32; 3], radius: f32, inside: bool },
  Plane { offset: [f32; 3], normal: [f32; 3] },
}

#[derive(Clone, Copy, Debug)]
pub struct SpringCollider {
  pub node: NodeId,
  pub shape: SpringShape,
}

/// What `advance_springs` produced.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpringsTick {
  /// A tail is still moving: keep requesting frames.
  pub active: bool,
  /// A joint's rotation changed: this frame must flush and paint.
  pub wrote: bool,
}

/// The flow's per-joint constants (see the header), in f64: they are
/// evaluated per step and the closed forms cancel near keep = 1.
#[derive(Clone, Copy, Debug)]
struct Flow {
  lambda: f64,
  mu: f64,
  a: f64,
  c: f64,
}

impl Flow {
  fn new(drag: f32) -> Self {
    let keep = (1.0 - drag as f64).clamp(KEEP_MIN, 1.0);
    let d = 1.0 - keep;
    let s = SPRING_REFERENCE_STEP;
    if d < KEEP_SERIES_BELOW {
      // r = 1 - d/2 - d^2/12, (1 - r)/d = 1/2 + d/12, -ln keep = d + d^2/2.
      let r = 1.0 - d / 2.0 - d * d / 12.0;
      let a = keep / r;
      return Flow { lambda: (d + d * d / 2.0) / s, mu: 1.0 / (s * r), a, c: 1.0 - a * (0.5 + d / 12.0) };
    }
    let neg_ln = -(-d).ln_1p();
    let r = d / neg_ln;
    let a = keep / r;
    Flow { lambda: neg_ln / s, mu: 1.0 / (s * r), a, c: 1.0 - a * (1.0 - r) / d }
  }

  /// One step of `t` seconds: the new (x, u) for the kick `f`, and the
  /// factor a projection's correction of x enters u by.
  fn step(&self, x: [f32; 3], u: [f32; 3], f: [f32; 3], t: f64) -> ([f32; 3], [f32; 3], f32) {
    let s = self.lambda * t;
    let (e, g1, g2) = if s < STEP_SERIES_BELOW {
      (1.0 - s + s * s / 2.0, 1.0 - s / 2.0 + s * s / 6.0, 0.5 - s / 6.0 + s * s / 24.0)
    } else {
      let e = (-s).exp();
      (e, (1.0 - e) / s, (s - 1.0 + e) / (s * s))
    };
    let ku = self.mu * t * g1;
    let xu = self.a * t * g1;
    let xf = self.a * self.mu * t * t * g2 + self.c * t;
    let mut nx = [0.0f32; 3];
    let mut nu = [0.0f32; 3];
    for i in 0..3 {
      let (xi, ui, fi) = (x[i] as f64, u[i] as f64, f[i] as f64);
      nu[i] = (e * ui + ku * fi) as f32;
      nx[i] = (xi + xu * ui + xf * fi) as f32;
    }
    // The correction factor: the velocity a resting tail's kick leaves,
    // per unit of the displacement the projection takes back (1/S at S,
    // position verlet's rule).
    let kappa = self.mu * g1 / (self.a * self.mu * t * g2 + self.c);
    (nx, nu, kappa as f32)
  }
}

struct Joint {
  spec: SpringJoint,
  flow: Flow,
  /// The tail's unit direction in the bone's own space, after its scale.
  axis: [f32; 3],
  /// |tail| in the bone's own units: what the sim-space length is
  /// divided by for the bone's scale factor.
  rest_length: f32,
  /// The joint listed earlier whose node is this bone's parent, else the
  /// parent's matrix is read from the arena.
  parent_joint: Option<usize>,
  /// Tail position and velocity in simulation space.
  x: [f32; 3],
  u: [f32; 3],
  /// The rotation last written (what the node's holds unless JS wrote).
  written: [f32; 4],
}

struct Chain {
  id: SpringId,
  clock: NodeId,
  center: Option<NodeId>,
  joints: Vec<Joint>,
  colliders: Vec<SpringCollider>,
  /// By arena index of a joint's node: its position in `joints`.
  by_node: HashMap<u32, usize>,
  /// The runtime force on every tail (`set_spring_force`), world space,
  /// beside the joints' authored gravity: the wind. Zero until set.
  force: [f32; 3],
  /// `force` in simulation space, per frame.
  force_sim: [f32; 3],
  /// Per frame: the parent matrix of every joint without a parent joint
  /// (unused slots stay identity), the solved matrix of every joint, the
  /// collider shapes in simulation space.
  parent_sims: Vec<Mat4>,
  solved: Vec<Mat4>,
  sim_shapes: Vec<SimShape>,
}

/// A collider resolved into simulation space for one frame.
#[derive(Clone, Copy)]
enum SimShape {
  Sphere { center: [f32; 3], radius: f32, inside: bool },
  Capsule { a: [f32; 3], b: [f32; 3], radius: f32, inside: bool },
  Plane { point: [f32; 3], normal: [f32; 3] },
}

#[derive(Default)]
pub(super) struct SpringSet {
  chains: Vec<Chain>,
  next: SpringId,
  /// The clock of the last advance; started with the transition clock by
  /// the arena (mod.rs set_transition_now).
  pub(super) last_ms: f64,
}

impl Spatial {
  /// Declare a chain: `joints` parents first (a joint whose parent node is
  /// another joint's must come after it), stepped at `clock`'s rate
  /// (`set_time_scale`) in `center`'s space when given (the tails live
  /// there, so the center's own motion does not swing them - VRM's
  /// center), else in world space. Every node must be live. The tails
  /// start at rest under the pose as the tree stands now (pending writes
  /// included), the rest rotations written.
  pub fn create_spring(
    &mut self,
    clock: NodeId,
    joints: Vec<SpringJoint>,
    colliders: Vec<SpringCollider>,
    center: Option<NodeId>,
  ) -> Result<SpringId, String> {
    if joints.is_empty() {
      return Err("a spring needs at least one joint".to_string());
    }
    self.resolve(clock).map_err(|e| format!("clock: {e}"))?;
    if let Some(c) = center {
      self.resolve(c).map_err(|e| format!("center: {e}"))?;
    }
    for (i, c) in colliders.iter().enumerate() {
      self.resolve(c.node).map_err(|e| format!("collider {i}: {e}"))?;
      let finite = match c.shape {
        SpringShape::Sphere { offset, radius, .. } => offset.iter().all(|v| v.is_finite()) && radius >= 0.0,
        SpringShape::Capsule { a, b, radius, .. } => a.iter().chain(b.iter()).all(|v| v.is_finite()) && radius >= 0.0,
        SpringShape::Plane { offset, normal } => {
          offset.iter().chain(normal.iter()).all(|v| v.is_finite()) && length(normal) > 0.0
        }
      };
      if !finite {
        return Err(format!("collider {i}: degenerate shape"));
      }
    }
    let mut by_node = HashMap::new();
    let mut built = Vec::with_capacity(joints.len());
    for (i, spec) in joints.into_iter().enumerate() {
      let idx = self.resolve(spec.node).map_err(|e| format!("joint {i}: {e}"))?;
      if by_node.insert(idx, i).is_some() {
        return Err(format!("joint {i}: its node is already a joint of this spring"));
      }
      let rest_length = length(spec.tail);
      if !(rest_length > 0.0) {
        return Err(format!("joint {i}: the tail must not be zero"));
      }
      check_settings(&spec).map_err(|e| format!("joint {i}: {e}"))?;
      if !spec.rest_rotation.iter().all(|v| v.is_finite()) {
        return Err(format!("joint {i}: the rest rotation must be finite"));
      }
      let scale = self.nodes[idx as usize].scale;
      let scaled = [spec.tail[0] * scale[0], spec.tail[1] * scale[1], spec.tail[2] * scale[2]];
      let axis = normalize_or(scaled, [0.0, 0.0, 1.0]);
      // The parent joint: resolved now and rechecked per frame (a reparent
      // falls back to the arena's matrix).
      let parent_joint = self.nodes[idx as usize].parent.and_then(|p| by_node.get(&p).copied());
      rest_rotation_check(&spec).map_err(|e| format!("joint {i}: {e}"))?;
      built.push(Joint {
        flow: Flow::new(spec.drag),
        axis,
        rest_length,
        parent_joint,
        x: [0.0; 3],
        u: [0.0; 3],
        written: quat_normalize(spec.rest_rotation),
        spec,
      });
    }
    // Parents first: a joint hanging from a later one would read its
    // parent's pose from before the step.
    for (i, j) in built.iter().enumerate() {
      let idx = super::index(j.spec.node);
      if let Some(p) = self.nodes[idx].parent.and_then(|p| by_node.get(&p).copied()) {
        if p > i {
          return Err(format!("joint {i} hangs from joint {p}: list parents first"));
        }
      }
    }
    let id = self.springs.next;
    let n = built.len();
    let mut chain = Chain {
      id,
      clock,
      center,
      joints: built,
      sim_shapes: Vec::with_capacity(colliders.len()),
      colliders,
      by_node,
      force: [0.0; 3],
      force_sim: [0.0; 3],
      parent_sims: vec![IDENTITY; n],
      solved: vec![IDENTITY; n],
    };
    self.prime_chain(&mut chain).map_err(|_| "a node of the spring died".to_string())?;
    self.springs.next += 1;
    self.springs.chains.push(chain);
    Ok(id)
  }

  /// Drop a chain, its joints put back at rest: the rest rotations
  /// written, as `reset_spring` writes them, so the bones draw as
  /// authored again (hair switched off hangs, not frozen mid-swing; a
  /// joint whose node died is skipped). Three-vrm's deleteJoint leaves the
  /// bones where the last step put them and every app restores them
  /// itself; Godot's modifiers never own the pose, so an inactive one
  /// shows the animated pose at once - the same outcome, which the
  /// solver here must write because it does own the rotations. A dropped
  /// id is fine. Whether a rotation was written.
  pub fn destroy_spring(&mut self, id: SpringId) -> bool {
    let Some(i) = self.springs.chains.iter().position(|c| c.id == id) else {
      return false;
    };
    let chain = self.springs.chains.remove(i);
    let mut wrote = false;
    for j in &chain.joints {
      if let Ok(idx) = self.resolve(j.spec.node) {
        wrote |= self.write_rotation(idx, quat_normalize(j.spec.rest_rotation));
      }
    }
    wrote
  }

  /// Put the chain back at rest now: every tail along its rest direction
  /// under the pose as the tree stands, the rest rotations written, no
  /// motion carried over (three-vrm's reset; what a teleport wants). A
  /// dropped id is an error; a chain whose node died drops here.
  pub fn reset_spring(&mut self, id: SpringId) -> Result<(), String> {
    let mut set = std::mem::take(&mut self.springs);
    let found = set.chains.iter().position(|c| c.id == id);
    let result = match found {
      None => Err(format!("spring {id} not found")),
      Some(i) => match self.prime_chain(&mut set.chains[i]) {
        Ok(_) => Ok(()),
        Err(()) => {
          set.chains.remove(i);
          Err(format!("spring {id}: a node died"))
        }
      },
    };
    self.springs = set;
    result
  }

  /// Change a joint's authored settings (the fields given); a drag change
  /// re-derives the joint's flow. Nothing re-primes: the chain goes on
  /// from where it is under the new settings. Errors: a dropped id, a
  /// joint out of range, a setting out of range.
  pub fn set_spring_joint(&mut self, id: SpringId, joint: usize, update: SpringJointUpdate) -> Result<(), String> {
    let chain = self.springs.chains.iter_mut().find(|c| c.id == id).ok_or_else(|| format!("spring {id} not found"))?;
    let count = chain.joints.len();
    let j =
      chain.joints.get_mut(joint).ok_or_else(|| format!("joint {joint} out of range (the spring has {count})"))?;
    let mut spec = j.spec;
    if let Some(v) = update.stiffness {
      spec.stiffness = v;
    }
    if let Some(v) = update.drag {
      spec.drag = v;
    }
    if let Some(v) = update.gravity_power {
      spec.gravity_power = v;
    }
    if let Some(v) = update.gravity_dir {
      spec.gravity_dir = v;
    }
    if let Some(v) = update.hit_radius {
      spec.hit_radius = v;
    }
    check_settings(&spec).map_err(|e| format!("joint {joint}: {e}"))?;
    if spec.drag != j.spec.drag {
      j.flow = Flow::new(spec.drag);
    }
    j.spec = spec;
    Ok(())
  }

  /// Set the runtime force on every tail of the chain (world space, the
  /// units of gravity power along a direction): the wind, added to each
  /// joint's authored gravity in the kick - Godot's external_force,
  /// UniVRM's ExternalForce. A constant force settles like gravity does.
  /// Errors: a dropped id, a non-finite force.
  pub fn set_spring_force(&mut self, id: SpringId, force: [f32; 3]) -> Result<(), String> {
    if !force.iter().all(|v| v.is_finite()) {
      return Err("the force must be finite".to_string());
    }
    let chain = self.springs.chains.iter_mut().find(|c| c.id == id).ok_or_else(|| format!("spring {id} not found"))?;
    chain.force = force;
    Ok(())
  }

  /// Step every chain by the app time since the last advance, each at its
  /// clock's rate, reading the parents' matrices as the tree stands now
  /// (the frame's writes included) and writing the joints' rotations
  /// through the snap path. Runs in the draw tick AFTER the frame's JS and
  /// ahead of the flush, the pose post-pass. A chain whose clock, center,
  /// joint or collider node died drops silently. A repeated call at an
  /// unchanged stamp steps nothing.
  pub fn advance_springs(&mut self) -> SpringsTick {
    let now = self.springs.last_ms.max(self.transitions.now_ms);
    let frame_dt = (now - self.springs.last_ms) / 1000.0;
    self.springs.last_ms = now;
    if self.springs.chains.is_empty() {
      return SpringsTick::default();
    }
    let mut set = std::mem::take(&mut self.springs);
    let mut tick = SpringsTick::default();
    set.chains.retain_mut(|chain| {
      let Some(rate) = super::rate_in(&self.nodes, chain.clock) else { return false };
      if self.step_chain(chain, frame_dt * rate as f64, &mut tick).is_err() {
        return false;
      }
      true
    });
    self.springs = set;
    tick
  }

  /// The frame's constants of one chain: the simulation space (its
  /// inverse returned), every non-joint parent's matrix and every collider
  /// in that space. Err = a node died.
  fn frame_setup(&mut self, chain: &mut Chain) -> Result<Mat4, ()> {
    let to_sim = match chain.center {
      Some(c) => invert_affine(&self.world(c).map_err(|_| ())?),
      None => IDENTITY,
    };
    chain.force_sim = transform_vector(&to_sim, chain.force);
    chain.sim_shapes.clear();
    for c in &chain.colliders {
      let m = multiply(to_sim, self.world(c.node).map_err(|_| ())?);
      let scale = column_scale(&m);
      chain.sim_shapes.push(match c.shape {
        SpringShape::Sphere { offset, radius, inside } => {
          SimShape::Sphere { center: transform_point(&m, offset), radius: radius * scale, inside }
        }
        SpringShape::Capsule { a, b, radius, inside } => {
          SimShape::Capsule { a: transform_point(&m, a), b: transform_point(&m, b), radius: radius * scale, inside }
        }
        SpringShape::Plane { offset, normal } => SimShape::Plane {
          point: transform_point(&m, offset),
          normal: normalize_or(transform_vector(&m, normal), [0.0, 1.0, 0.0]),
        },
      });
    }
    for i in 0..chain.joints.len() {
      let idx = self.resolve(chain.joints[i].spec.node).map_err(|_| ())?;
      let parent = self.nodes[idx as usize].parent;
      chain.joints[i].parent_joint = parent.and_then(|p| chain.by_node.get(&p).copied()).filter(|&j| j < i);
      if chain.joints[i].parent_joint.is_none() {
        chain.parent_sims[i] = match parent {
          Some(p) => multiply(to_sim, self.world(self.id_of(p)).map_err(|_| ())?),
          None => to_sim,
        };
      }
    }
    Ok(to_sim)
  }

  /// Place every tail at rest under the pose as the tree stands, with the
  /// rest rotation written and no motion: the chain draws as authored
  /// until something moves it. Whether a rotation was written.
  fn prime_chain(&mut self, chain: &mut Chain) -> Result<bool, ()> {
    self.frame_setup(chain)?;
    let mut wrote = false;
    for i in 0..chain.joints.len() {
      let (pos, rest_vec, _, parent_sim) = self.joint_frame(chain, i);
      let j = &mut chain.joints[i];
      j.x = add(pos, rest_vec);
      j.u = [0.0; 3];
      let rest = quat_normalize(j.spec.rest_rotation);
      let idx = self.resolve(j.spec.node).map_err(|_| ())?;
      wrote |= self.write_rotation(idx, rest);
      j.written = rest;
      let n = &self.nodes[idx as usize];
      chain.solved[i] = multiply(parent_sim, compose(n.position, rest, n.scale));
    }
    Ok(wrote)
  }

  /// One frame of one chain. Err = a node died.
  fn step_chain(&mut self, chain: &mut Chain, dt: f64, tick: &mut SpringsTick) -> Result<(), ()> {
    let to_sim = self.frame_setup(chain)?;
    if dt <= 0.0 {
      return Ok(());
    }

    // Substeps of at most a quarter reference step; a long gap is cut
    // short.
    let total = dt.min(SPRING_REFERENCE_STEP * MAX_REFERENCE_STEPS_PER_ADVANCE as f64);
    let substep = SPRING_REFERENCE_STEP / SUBSTEPS_PER_REFERENCE_STEP as f64;
    let steps = ((total / substep - SUBSTEP_COUNT_SLACK).ceil() as u32)
      .clamp(1, MAX_REFERENCE_STEPS_PER_ADVANCE * SUBSTEPS_PER_REFERENCE_STEP);
    let t = total / steps as f64;
    let mut moving = false;
    for _ in 0..steps {
      for i in 0..chain.joints.len() {
        let (pos, rest_vec, scale, parent_sim) = self.joint_frame(chain, i);
        let len = length(rest_vec);
        let j = &mut chain.joints[i];
        if !(len > 0.0) {
          // A bone collapsed to a point (a zero ancestor scale): nothing
          // to turn; it keeps its state.
          chain.solved[i] = parent_sim;
          continue;
        }
        let rest_dir = scale3(rest_vec, 1.0 / len);
        let gravity = normalize_or(transform_vector(&to_sim, j.spec.gravity_dir), [0.0; 3]);
        let f = add(add(scale3(rest_dir, j.spec.stiffness), scale3(gravity, j.spec.gravity_power)), chain.force_sim);
        let (free, u, kappa) = j.flow.step(j.x, j.u, f, t);
        // Length, colliders (each followed by length again).
        let mut x = pos_at_length(pos, free, len, rest_dir);
        let hit = j.spec.hit_radius * scale;
        for shape in &chain.sim_shapes {
          if let Some(pushed) = push_out(x, hit, shape, rest_dir) {
            x = pos_at_length(pos, pushed, len, rest_dir);
          }
        }
        let correction = sub(x, free);
        let u = add(u, scale3(correction, kappa));
        let speed = length(sub(x, j.x)) / t as f32;
        if speed >= SETTLE_SPEED {
          moving = true;
        }
        j.x = x;
        j.u = u;
        // The bone turns from its rest tail to the solved one, in its
        // own rest frame: the direction into the parent's space (its
        // inverse, so a scaled parent is exact), then the rest rotation
        // undone.
        let parent_inv = invert_affine(&parent_sim);
        let in_parent = normalize_or(transform_vector(&parent_inv, sub(x, pos)), rest_dir);
        let rest = quat_normalize(j.spec.rest_rotation);
        let to = rotate_vector(quat_conjugate(rest), in_parent);
        let q = quat_normalize(quat_multiply(rest, quat_from_to(j.axis, to)));
        let idx = self.resolve(j.spec.node).map_err(|_| ())?;
        if differs(q, j.written) {
          if self.write_rotation(idx, q) {
            tick.wrote = true;
          }
          j.written = q;
        }
        let n = &self.nodes[idx as usize];
        chain.solved[i] = multiply(parent_sim, compose(n.position, j.written, n.scale));
      }
    }
    if moving {
      tick.active = true;
    }
    Ok(())
  }

  /// A joint's frame this step: its position in simulation space, its
  /// rest tail vector there (direction times bone length), the scale
  /// factor from its own units to simulation units, and its parent's
  /// simulation matrix.
  fn joint_frame(&self, chain: &Chain, i: usize) -> ([f32; 3], [f32; 3], f32, Mat4) {
    let j = &chain.joints[i];
    let parent_sim = match j.parent_joint {
      Some(p) => chain.solved[p],
      None => chain.parent_sims[i],
    };
    let n = &self.nodes[super::index(j.spec.node)];
    let pos = transform_point(&parent_sim, n.position);
    let scaled = [j.spec.tail[0] * n.scale[0], j.spec.tail[1] * n.scale[1], j.spec.tail[2] * n.scale[2]];
    let rest_vec = transform_vector(&parent_sim, rotate_vector(quat_normalize(j.spec.rest_rotation), scaled));
    let scale = length(rest_vec) / j.rest_length;
    (pos, rest_vec, scale, parent_sim)
  }

  /// Write a rotation through the snap path when it differs from the
  /// node's; whether it did.
  fn write_rotation(&mut self, idx: u32, q: [f32; 4]) -> bool {
    let n = &mut self.nodes[idx as usize];
    if !differs(q, n.rotation) {
      return false;
    }
    n.rotation = q;
    n.local_dirty = true;
    self.enqueue(idx);
    true
  }
}

/// The authored settings checked: drag within 0..1, stiffness and hit
/// radius non-negative, gravity finite.
fn check_settings(spec: &SpringJoint) -> Result<(), String> {
  if !(0.0..=1.0).contains(&spec.drag) {
    return Err(format!("drag must be within 0..1, got {}", spec.drag));
  }
  if !(spec.stiffness >= 0.0) || !(spec.hit_radius >= 0.0) || !spec.gravity_power.is_finite() {
    return Err("stiffness and hit radius must be >= 0, gravity power finite".to_string());
  }
  if !spec.gravity_dir.iter().all(|v| v.is_finite()) {
    return Err("the gravity direction must be finite".to_string());
  }
  Ok(())
}

/// The rest rotation checked finite (its normalization is per use).
fn rest_rotation_check(spec: &SpringJoint) -> Result<(), String> {
  if spec.rest_rotation.iter().all(|v| v.is_finite()) && length4(spec.rest_rotation) > 0.0 {
    Ok(())
  } else {
    Err("the rest rotation must be a finite, non-zero quaternion".to_string())
  }
}

/// The tail held at the bone's length from its position; a tail at the
/// position itself takes the rest direction.
fn pos_at_length(pos: [f32; 3], tail: [f32; 3], len: f32, rest_dir: [f32; 3]) -> [f32; 3] {
  let dir = normalize_or(sub(tail, pos), rest_dir);
  add(pos, scale3(dir, len))
}

/// The tail pushed out of (or kept within) a collider, None when clear.
fn push_out(x: [f32; 3], hit: f32, shape: &SimShape, fallback: [f32; 3]) -> Option<[f32; 3]> {
  let (delta, radius, inside) = match *shape {
    SimShape::Sphere { center, radius, inside } => (sub(x, center), radius, inside),
    SimShape::Capsule { a, b, radius, inside } => (sub(x, closest_on_segment(x, a, b)), radius, inside),
    SimShape::Plane { point, normal } => {
      let d = dot(sub(x, point), normal) - hit;
      return (d < 0.0).then(|| sub(x, scale3(normal, d)));
    }
  };
  let dist = length(delta);
  if inside {
    let reach = radius - hit;
    (dist > reach && dist > 0.0).then(|| sub(x, scale3(delta, (dist - reach) / dist)))
  } else {
    let reach = radius + hit;
    if dist >= reach {
      return None;
    }
    // A tail on the center has no way out of its own: it leaves along
    // the rest direction.
    let dir = if dist > 0.0 { scale3(delta, 1.0 / dist) } else { fallback };
    Some(add(x, scale3(dir, reach - dist)))
  }
}

fn closest_on_segment(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  let ab = sub(b, a);
  let l2 = dot(ab, ab);
  if l2 <= 0.0 {
    return a;
  }
  let t = (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0);
  add(a, scale3(ab, t))
}

/// The shortest arc taking unit `a` onto unit `b`.
fn quat_from_to(a: [f32; 3], b: [f32; 3]) -> [f32; 4] {
  let r = dot(a, b) + 1.0;
  if r < ANTIPARALLEL_EPSILON {
    // Half a turn about any axis perpendicular to a.
    let axis = if a[0].abs() > a[2].abs() { [-a[1], a[0], 0.0] } else { [0.0, -a[2], a[1]] };
    let axis = normalize_or(axis, [1.0, 0.0, 0.0]);
    return [axis[0], axis[1], axis[2], 0.0];
  }
  let c = cross(a, b);
  quat_normalize([c[0], c[1], c[2], r])
}

fn quat_conjugate(q: [f32; 4]) -> [f32; 4] {
  [-q[0], -q[1], -q[2], q[3]]
}

fn differs(a: [f32; 4], b: [f32; 4]) -> bool {
  (0..4).any(|i| (a[i] - b[i]).abs() > ROTATION_EPSILON)
}

/// The scale a matrix carries along its first column: what a uniformly
/// scaled node scales every length by.
fn column_scale(m: &Mat4) -> f32 {
  length([m[0], m[1], m[2]])
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale3(v: [f32; 3], s: f32) -> [f32; 3] {
  [v[0] * s, v[1] * s, v[2] * s]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn length(v: [f32; 3]) -> f32 {
  dot(v, v).sqrt()
}

fn length4(q: [f32; 4]) -> f32 {
  (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt()
}

fn normalize_or(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
  let l = length(v);
  if l > 0.0 {
    scale3(v, 1.0 / l)
  } else {
    fallback
  }
}

/// The flow's pure step, for the tests: (x, u) after `t` seconds under
/// the kick `f` at `drag`, and the correction factor.
#[cfg(test)]
pub(crate) fn flow_step(drag: f32, x: [f32; 3], u: [f32; 3], f: [f32; 3], t: f64) -> ([f32; 3], [f32; 3], f32) {
  Flow::new(drag).step(x, u, f, t)
}
