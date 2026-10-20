// Inverse kinematics: a stack of solvers over a model's joints, declared
// once and stepped natively every frame in the pose post-pass, after the
// frame's JS and BEFORE the spring bones - Unity's Animator IK pass
// (animation, OnAnimatorIK, LateUpdate), Godot's SkeletonModifier3D stack
// (modifiers in child order after process), Unreal's Control Rig nodes.
// Four kinds in the engines' vocabulary: `LookAt` (a bone or a chain of
// bones turns its forward axis to a target, distributed by shares, yaw
// and pitch limits with a damped approach, Godot's LookAtModifier3D),
// `TwoBone` (root, mid, tip solved in closed form, a pole or the animated
// bend plane, a soft reach band, stretch), `Chain` (root to tip of any
// length, FABRIK or CCD, a magnet, per-joint hinge or cone limits) and
// `Feet` (legs whose targets are ground hits the core takes itself, the
// hips lowered so both feet reach).
//
// Every frame each solver sees the PRIMARY pose - what the players, the
// transitions and the frame's JS wrote - not its own last result: a joint
// the stack owns is re-solved from the rotation that was written over
// the stack's last write (a write that nobody made leaves last frame's
// input in force), then the solvers run in declared order, each over the
// previous one's result, and the stack writes the final pose through the
// snap path. Solves run in world space from matrices composed on the fly
// over the stack's working pose, so a model placed, turned or scaled in
// the world solves identically, and bone lengths follow the pose as it
// stands (an animated or ancestor scale is honoured). The solves are
// stateless except the eases (weights, the look-at transition, the hips
// offset), all per-second rates over the frame delta at the clock's rate.
//
// Engine independent: native types in and out (rendertree rules).

use std::collections::HashMap;

use super::clips::quat_multiply;
use super::math::{invert_affine, rotate_vector, transform_point, transform_vector};
use super::transitions::{quat_normalize, slerp};
use super::{compose, multiply, Mat4, NodeId, QueryFilter, RigId, Spatial, IDENTITY};

pub type IkId = u64;

/// A solved rotation within this of the node's (per quaternion component)
/// is not written: the float noise of a settled stack stays out of the
/// flush.
const ROTATION_EPSILON: f64 = 1e-6;
/// Same, for a solved position (world units).
const POSITION_EPSILON: f64 = 1e-6;
/// Two unit vectors closer than this to antiparallel turn about a
/// perpendicular instead of the degenerate cross product.
const ANTIPARALLEL_EPSILON: f64 = 1e-6;
/// A direction shorter than this has no direction: the solve keeps the
/// pose it has for that joint.
const DIRECTION_EPSILON: f64 = 1e-9;
/// A look-at direction within this of the up axis (|sin pitch| above it)
/// has no yaw: the previous yaw holds, Godot's parallel-axis guard.
const POLE_SIN_LIMIT: f64 = 0.9999;
/// Past this fraction of a half turn the target is "behind" the bone:
/// a yaw sign change there is a flip through the back, which starts the
/// look-at transition instead of snapping across.
const FLIP_ANGLE: f64 = std::f64::consts::FRAC_PI_2;
/// Most iterations a chain solve may take whatever the spec asks: a
/// runaway setting cannot stall a frame.
const MAX_CHAIN_ITERATIONS: u32 = 256;
/// A chain whose interior joints' mean offset from the root-target axis
/// is below this fraction of its length lies on that axis: FABRIK then
/// nudges its middle joint by `DEGENERATE_NUDGE` of the length to break
/// the symmetry.
const DEGENERATE_RADIAL: f64 = 1e-3;
const DEGENERATE_NUDGE: f64 = 0.05;
/// The hips offset is settled when within this (world units) of its
/// target: the stack asks for no frames past it.
const HIPS_SETTLE: f64 = 1e-5;
/// A weight within this of its target lands on it.
const WEIGHT_SETTLE: f64 = 1e-6;

/// Where a solver aims: a live node (read from the arena at solve time,
/// pending writes included, so a moving target costs nothing) or a point,
/// in world space or a node's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IkPoint {
  Node(NodeId),
  Point { position: [f64; 3], space: Option<NodeId> },
}

/// A target: a node (position and rotation), or a point with an
/// optional rotation in the same space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IkTarget {
  Node(NodeId),
  Point { position: [f64; 3], rotation: Option<[f64; 4]>, space: Option<NodeId> },
}

/// A roll reference for a look-at: a direction in world space or in a
/// node's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IkUp {
  pub vector: [f64; 3],
  pub space: Option<NodeId>,
}

/// A chain joint's rotation limit, in the joint's own frame relative to
/// its primary (input) rotation: a hinge turns about one local axis
/// within `min..max` radians; a cone bounds the swing away from the bone
/// axis (the direction to the next joint) and the twist about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IkLimit {
  Hinge { axis: [f64; 3], min: f64, max: f64 },
  Cone { swing: f64, twist: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookAtBone {
  pub node: NodeId,
  /// The bone's share of the aim (any positive scale; the shares are
  /// taken as fractions of what remains, the last bone taking all of it,
  /// so the chain reaches the target exactly).
  pub share: f64,
}

/// Yaw and pitch limits about the last bone's rest forward, radians,
/// `[min, max]` each (asymmetric allowed). `damp` is the fraction of each
/// limit past which the approach eases toward the limit instead of
/// clamping (1 = a hard clamp at the limit, Godot's damp threshold).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookAtLimit {
  pub yaw: [f64; 2],
  pub pitch: [f64; 2],
  pub damp: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LookAtSpec {
  /// Root first (spine, neck, head): each turns in order and the next
  /// sees the result.
  pub bones: Vec<LookAtBone>,
  pub target: Option<IkTarget>,
  /// The bones' forward axis, in their own space.
  pub forward: [f64; 3],
  /// The bones' up axis, in their own space: the yaw axis, and what the
  /// roll reference aligns.
  pub up_axis: [f64; 3],
  /// Roll reference; None keeps the shortest arc (no roll beyond what
  /// yaw and pitch give).
  pub up: Option<IkUp>,
  pub limit: Option<LookAtLimit>,
  /// The time-based transition on a target change, a target lost or
  /// found, or a flip through the back, milliseconds; 0 snaps.
  pub ease_ms: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TwoBoneSpec {
  pub root: NodeId,
  pub mid: NodeId,
  pub tip: NodeId,
  pub target: Option<IkTarget>,
  /// The bend plane: the mid joint is turned toward the pole; None keeps
  /// the animated bend direction (the rest direction when the limb is
  /// straight).
  pub pole: Option<IkPoint>,
  /// A twist about the root-to-target axis applied after the pole,
  /// radians.
  pub pole_angle: f64,
  pub position_weight: f64,
  /// How much the tip takes the target's rotation (a node's, or the
  /// point's when given).
  pub rotation_weight: f64,
  /// The soft reach band as a fraction of the limb length: within it the
  /// tip eases toward full extension instead of popping straight; 0 off.
  pub soft: f64,
  /// The most the limb may lengthen past full reach, as a fraction (0
  /// off): the mid and tip positions scale along their offsets.
  pub stretch: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainMethod {
  Fabrik,
  Ccd,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChainSpec {
  pub root: NodeId,
  pub tip: NodeId,
  pub target: Option<IkTarget>,
  pub method: ChainMethod,
  pub iterations: u32,
  /// World units: the solve stops once the tip is within it.
  pub tolerance: f64,
  /// FABRIK's pole: the chain's interior joints turn about the root-tip
  /// axis toward it.
  pub magnet: Option<IkPoint>,
  pub limits: Vec<(NodeId, IkLimit)>,
  pub rotation_weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegSpec {
  pub root: NodeId,
  pub mid: NodeId,
  pub tip: NodeId,
  /// The sole's distance below the tip joint along up; None measures the
  /// tip's height above the clock node's plane at creation.
  pub height: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FeetSpec {
  /// The pelvis: lowered so that both feet reach.
  pub hips: NodeId,
  pub legs: Vec<LegSpec>,
  /// World-space up: the rays run against it, the hips move along it.
  pub up: [f64; 3],
  /// What the ground rays see (a collider layer, the scene's root).
  pub filter: QueryFilter,
  /// How far above and below the animated foot the ray reaches.
  pub rise: f64,
  pub drop: f64,
  /// A foot within this height of the model's ground plane (above its
  /// rest height) is planted; over the next `plant_band` it lifts.
  pub plant_height: f64,
  pub plant_band: f64,
  /// The hips offset's time constant, milliseconds.
  pub hips_ease_ms: f64,
  /// How much each planted foot aligns to the ground normal.
  pub rotation_weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IkSolverSpec {
  LookAt(LookAtSpec),
  TwoBone(TwoBoneSpec),
  Chain(ChainSpec),
  Feet(FeetSpec),
}

impl IkSolverSpec {
  fn kind(&self) -> &'static str {
    match self {
      IkSolverSpec::LookAt(_) => "lookAt",
      IkSolverSpec::TwoBone(_) => "twoBone",
      IkSolverSpec::Chain(_) => "chain",
      IkSolverSpec::Feet(_) => "feet",
    }
  }
}

/// What `advance_ik` produced.
#[derive(Clone, Copy, Debug, Default)]
pub struct IkTick {
  /// An ease is still running: keep requesting frames.
  pub active: bool,
  /// A joint changed: this frame must flush and paint.
  pub wrote: bool,
}

/// One joint the stack owns: the primary pose it solves from this frame,
/// the working pose the solvers edit, and what the stack last wrote.
struct JointState {
  node: NodeId,
  input_rot: [f64; 4],
  input_pos: [f64; 3],
  work_rot: [f64; 4],
  work_pos: [f64; 3],
  written_rot: [f64; 4],
  written_pos: [f64; 3],
}

/// The look-at's transition (Godot's time-based interpolation): from the
/// pose it had when triggered toward the fresh solve, over `ease_ms`.
struct LookAtState {
  /// Per bone, the local rotation the last frame's solve produced.
  last: Vec<[f64; 4]>,
  from: Vec<[f64; 4]>,
  /// 1 at the trigger, 0 when done.
  remaining: f64,
  /// The yaw of the previous frame, for the flip test; None before the
  /// first solve with a target.
  prev_yaw: Option<f64>,
  had_target: bool,
}

struct TwoBoneState {
  /// The bend direction at creation, in the root's parent space: the
  /// fallback when the limb is straight.
  rest_bend: [f64; 3],
}

struct LegState {
  height: f64,
  rest_bend: [f64; 3],
  /// The model's up in the tip's own frame at creation: what aligns to
  /// the ground normal.
  up_local: [f64; 3],
}

struct FeetState {
  legs: Vec<LegState>,
  offset: f64,
  offset_target: f64,
}

enum SolverState {
  LookAt(LookAtState),
  TwoBone(TwoBoneState),
  Chain,
  Feet(FeetState),
}

struct Solver {
  spec: IkSolverSpec,
  state: SolverState,
  weight: f64,
  weight_target: f64,
  /// Per second; 0 when at the target.
  weight_rate: f64,
  /// The last solve's residual.
  reach: f64,
  /// A joint or target node died: skipped from then on.
  dead: bool,
  /// The weight follows a rig's curve (an authored foot-plant track)
  /// instead of its own ramp: read every advance.
  curve: Option<(RigId, String)>,
}

struct Stack {
  id: IkId,
  clock: NodeId,
  solvers: Vec<Solver>,
  joints: Vec<JointState>,
  /// By arena index of a joint's node.
  by_node: HashMap<u32, usize>,
}

#[derive(Default)]
pub(super) struct IkSet {
  stacks: Vec<Stack>,
  next: IkId,
  /// The clock of the last advance; started with the transition clock by
  /// the arena (mod.rs set_transition_now).
  pub(super) last_ms: f64,
}

/// A node on a solved path with its local pose (the stack's working
/// state) and its world frame as composed this pass.
#[derive(Clone, Copy)]
struct PathNode {
  idx: u32,
  pos: [f64; 3],
  rot: [f64; 4],
  scale: [f64; 3],
  world: Mat4,
}

/// The frame a path hangs from: its root's parent. Matrices only: a
/// quaternion composed down the path cannot carry a mirroring ancestor
/// (a negative scale), the matrix does, and every rotation the solvers
/// need is read off its orthonormal part.
#[derive(Clone, Copy)]
struct Frame {
  world: Mat4,
}

impl Spatial {
  /// Declare a stack: the solvers in order, stepped at `clock`'s rate
  /// (`set_time_scale`; for `Feet` also the model whose ground plane the
  /// plant test reads). Every node must be live and every chain a
  /// descendant path. The joints start from the pose as the tree stands.
  pub fn create_ik(&mut self, clock: NodeId, solvers: Vec<(IkSolverSpec, f64)>) -> Result<IkId, String> {
    self.resolve(clock).map_err(|e| format!("clock: {e}"))?;
    let mut stack = Stack {
      id: self.ik.next,
      clock,
      solvers: Vec::with_capacity(solvers.len()),
      joints: Vec::new(),
      by_node: HashMap::new(),
    };
    for (i, (spec, weight)) in solvers.into_iter().enumerate() {
      let solver = self.build_solver(&mut stack, spec, weight).map_err(|e| format!("solver {i}: {e}"))?;
      stack.solvers.push(solver);
    }
    let id = stack.id;
    self.ik.next += 1;
    self.ik.stacks.push(stack);
    Ok(id)
  }

  /// Drop a stack, its joints put back to the pose the players and the
  /// frame's JS wrote (the stack's input), so a disposed aim lets go of
  /// the bones. A dropped id is fine. Whether a joint was written.
  pub fn destroy_ik(&mut self, id: IkId) -> bool {
    let Some(i) = self.ik.stacks.iter().position(|s| s.id == id) else {
      return false;
    };
    let stack = self.ik.stacks.remove(i);
    let mut wrote = false;
    for j in &stack.joints {
      if let Ok(idx) = self.resolve(j.node) {
        wrote |= self.write_rotation(idx, j.input_rot);
        wrote |= self.write_position(idx, j.input_pos);
      }
    }
    wrote
  }

  /// Replace a solver's settings live: the same kind, any joints (a new
  /// chain re-registers); the weight and its ramp carry over, the eases
  /// restart. Errors: a dropped id, an index out of range, a kind change,
  /// a spec that fails `create_ik`'s checks.
  pub fn set_ik_solver(&mut self, id: IkId, index: usize, spec: IkSolverSpec) -> Result<(), String> {
    let mut set = std::mem::take(&mut self.ik);
    let result = (|| {
      let stack = set.stacks.iter_mut().find(|s| s.id == id).ok_or_else(|| format!("ik {id} not found"))?;
      let count = stack.solvers.len();
      let old =
        stack.solvers.get(index).ok_or_else(|| format!("solver {index} out of range (the stack has {count})"))?;
      if old.spec.kind() != spec.kind() {
        return Err(format!("solver {index} is a {} solver, not {}", old.spec.kind(), spec.kind()));
      }
      let (weight, target, rate, curve) = (old.weight, old.weight_target, old.weight_rate, old.curve.clone());
      let mut solver = self.build_solver(stack, spec, weight)?;
      solver.weight_target = target;
      solver.weight_rate = rate;
      solver.curve = curve;
      stack.solvers[index] = solver;
      Ok(())
    })();
    self.ik = set;
    result
  }

  /// Move a solver's target (None: no target; a look-at then eases back
  /// to the primary pose). A `Feet` solver's targets are the ground:
  /// error.
  pub fn set_ik_target(&mut self, id: IkId, index: usize, target: Option<IkTarget>) -> Result<(), String> {
    if let Some(t) = &target {
      self.check_target(t)?;
    }
    let stack = self.ik.stacks.iter_mut().find(|s| s.id == id).ok_or_else(|| format!("ik {id} not found"))?;
    let count = stack.solvers.len();
    let solver =
      stack.solvers.get_mut(index).ok_or_else(|| format!("solver {index} out of range (the stack has {count})"))?;
    match (&mut solver.spec, &mut solver.state) {
      (IkSolverSpec::LookAt(s), SolverState::LookAt(state)) => {
        if target_switches(&s.target, &target) {
          start_transition(state, s.ease_ms);
        }
        s.target = target;
      }
      (IkSolverSpec::TwoBone(s), _) => s.target = target,
      (IkSolverSpec::Chain(s), _) => s.target = target,
      (IkSolverSpec::Feet(_), _) => return Err("a feet solver's targets are the ground".to_string()),
      _ => unreachable!("solver state matches its spec"),
    }
    Ok(())
  }

  /// Ramp a solver's weight to `weight` over `fade_ms` of the clock's
  /// time (linear, the players' fade shape); 0 snaps. A weight at 0
  /// costs no solve. Errors: a dropped id, an index out of range, a
  /// weight outside 0..1.
  pub fn set_ik_weight(&mut self, id: IkId, index: usize, weight: f64, fade_ms: f64) -> Result<(), String> {
    if !(0.0..=1.0).contains(&weight) {
      return Err(format!("weight must be within 0..1, got {weight}"));
    }
    if !(fade_ms >= 0.0) {
      return Err("the fade must be >= 0 ms".to_string());
    }
    let stack = self.ik.stacks.iter_mut().find(|s| s.id == id).ok_or_else(|| format!("ik {id} not found"))?;
    let count = stack.solvers.len();
    let solver =
      stack.solvers.get_mut(index).ok_or_else(|| format!("solver {index} out of range (the stack has {count})"))?;
    solver.curve = None;
    solver.weight_target = weight;
    if fade_ms <= 0.0 || (weight - solver.weight).abs() <= WEIGHT_SETTLE {
      solver.weight = weight;
      solver.weight_rate = 0.0;
    } else {
      solver.weight_rate = (weight - solver.weight) / (fade_ms / 1000.0);
    }
    Ok(())
  }

  /// Bind a solver's weight to a rig's curve (`curve_value`): every
  /// advance the weight takes the curve's value as of that frame's
  /// players (a frame the curve is not fed keeps the last weight). A
  /// `set_ik_weight` unbinds. Errors: a dropped id, an index out of
  /// range, an unknown rig.
  pub fn bind_ik_weight_curve(&mut self, id: IkId, index: usize, rig: RigId, name: String) -> Result<(), String> {
    self.rig_index(rig)?;
    let stack = self.ik.stacks.iter_mut().find(|s| s.id == id).ok_or_else(|| format!("ik {id} not found"))?;
    let count = stack.solvers.len();
    let solver =
      stack.solvers.get_mut(index).ok_or_else(|| format!("solver {index} out of range (the stack has {count})"))?;
    solver.curve = Some((rig, name));
    solver.weight_rate = 0.0;
    Ok(())
  }

  /// The last solve's residual: the distance left to the target (world
  /// units; a look-at's angle left, radians; a feet solver's largest
  /// over its planted feet). 0 before any solve or for a solver without
  /// a target.
  pub fn ik_reach(&self, id: IkId, index: usize) -> Result<f64, String> {
    let stack = self.ik.stacks.iter().find(|s| s.id == id).ok_or_else(|| format!("ik {id} not found"))?;
    let count = stack.solvers.len();
    let solver =
      stack.solvers.get(index).ok_or_else(|| format!("solver {index} out of range (the stack has {count})"))?;
    Ok(solver.reach)
  }

  /// The solver's current weight (its ramp included), for tests and
  /// probes.
  pub fn ik_weight(&self, id: IkId, index: usize) -> Result<f64, String> {
    let stack = self.ik.stacks.iter().find(|s| s.id == id).ok_or_else(|| format!("ik {id} not found"))?;
    let count = stack.solvers.len();
    let solver =
      stack.solvers.get(index).ok_or_else(|| format!("solver {index} out of range (the stack has {count})"))?;
    Ok(solver.weight)
  }

  /// Step every stack by the app time since the last advance, each at
  /// its clock's rate: the weights ramp, every solver runs over the
  /// primary pose in declared order and the joints are written through
  /// the snap path. Runs in the draw tick AFTER the frame's JS, ahead of
  /// the springs and the flush. A stack whose clock died drops; a solver
  /// whose joint or target node died is skipped from then on. A repeated
  /// call at an unchanged stamp ramps nothing but still solves (a target
  /// written since must land).
  pub fn advance_ik(&mut self) -> IkTick {
    let now = self.ik.last_ms.max(self.transitions.now_ms);
    let frame_dt = (now - self.ik.last_ms) / 1000.0;
    self.ik.last_ms = now;
    if self.ik.stacks.is_empty() {
      return IkTick::default();
    }
    let mut set = std::mem::take(&mut self.ik);
    let mut tick = IkTick::default();
    set.stacks.retain_mut(|stack| {
      let Some(rate) = super::rate_in(&self.nodes, stack.clock) else { return false };
      for solver in &mut stack.solvers {
        if let Some((rig, name)) = &solver.curve {
          if let Ok(Some(v)) = self.curve_value(*rig, name) {
            solver.weight = v.clamp(0.0, 1.0);
            solver.weight_target = solver.weight;
          }
        }
      }
      self.step_stack(stack, (frame_dt * rate as f64) as f64, &mut tick);
      true
    });
    self.ik = set;
    tick
  }

  fn check_point(&self, p: &IkPoint) -> Result<(), String> {
    match p {
      IkPoint::Node(n) => self.resolve(*n).map(|_| ()).map_err(|e| format!("node: {e}")),
      IkPoint::Point { position, space } => {
        if !position.iter().all(|v| v.is_finite()) {
          return Err("the position must be finite".to_string());
        }
        if let Some(s) = space {
          self.resolve(*s).map_err(|e| format!("space: {e}"))?;
        }
        Ok(())
      }
    }
  }

  fn check_target(&self, t: &IkTarget) -> Result<(), String> {
    match t {
      IkTarget::Node(n) => self.resolve(*n).map(|_| ()).map_err(|e| format!("target: {e}")),
      IkTarget::Point { position, rotation, space } => {
        if !position.iter().all(|v| v.is_finite()) {
          return Err("the target position must be finite".to_string());
        }
        if let Some(r) = rotation {
          if !r.iter().all(|v| v.is_finite()) || length4(*r) == 0.0 {
            return Err("the target rotation must be a finite, non-zero quaternion".to_string());
          }
        }
        if let Some(s) = space {
          self.resolve(*s).map_err(|e| format!("target space: {e}"))?;
        }
        Ok(())
      }
    }
  }

  /// The arena path from `root` down to `tip` (both included), or an
  /// error when `tip` is not under `root`.
  fn path_between(&self, root: NodeId, tip: NodeId, what: &str) -> Result<Vec<u32>, String> {
    let r = self.resolve(root).map_err(|e| format!("{what} root: {e}"))?;
    let t = self.resolve(tip).map_err(|e| format!("{what} tip: {e}"))?;
    let mut path = vec![t];
    let mut cursor = self.nodes[t as usize].parent;
    while path.last() != Some(&r) {
      let Some(p) = cursor else {
        return Err(format!("{what}: the tip is not under the root"));
      };
      path.push(p);
      cursor = self.nodes[p as usize].parent;
    }
    path.reverse();
    Ok(path)
  }

  fn register(&self, stack: &mut Stack, idx: u32) -> usize {
    if let Some(&j) = stack.by_node.get(&idx) {
      return j;
    }
    let n = &self.nodes[idx as usize];
    let rot = quat_normalize(n.rotation);
    stack.joints.push(JointState {
      node: self.id_of(idx),
      input_rot: rot,
      input_pos: n.position,
      work_rot: rot,
      work_pos: n.position,
      written_rot: rot,
      written_pos: n.position,
    });
    let j = stack.joints.len() - 1;
    stack.by_node.insert(idx, j);
    j
  }

  fn build_solver(&self, stack: &mut Stack, spec: IkSolverSpec, weight: f64) -> Result<Solver, String> {
    if !(0.0..=1.0).contains(&weight) {
      return Err(format!("weight must be within 0..1, got {weight}"));
    }
    let state = match &spec {
      IkSolverSpec::LookAt(s) => {
        if s.bones.is_empty() {
          return Err("a look-at needs at least one bone".to_string());
        }
        if length(s.forward) == 0.0
          || length(s.up_axis) == 0.0
          || !s.forward.iter().chain(s.up_axis.iter()).all(|v| v.is_finite())
        {
          return Err("the forward and up axes must be finite and non-zero".to_string());
        }
        if length(cross(s.forward, s.up_axis)) == 0.0 {
          return Err("the forward and up axes must not be parallel".to_string());
        }
        if !(s.ease_ms >= 0.0) {
          return Err("the ease must be >= 0 ms".to_string());
        }
        if let Some(l) = &s.limit {
          if !(l.yaw[0] <= 0.0 && l.yaw[1] >= 0.0 && l.pitch[0] <= 0.0 && l.pitch[1] >= 0.0) {
            return Err("limits must bracket 0 (min <= 0 <= max)".to_string());
          }
          if !(0.0..=1.0).contains(&l.damp) {
            return Err("damp must be within 0..1".to_string());
          }
        }
        if let Some(t) = &s.target {
          self.check_target(t)?;
        }
        if let Some(u) = &s.up {
          if length(u.vector) == 0.0 {
            return Err("the up reference must be non-zero".to_string());
          }
          if let Some(sp) = u.space {
            self.resolve(sp).map_err(|e| format!("up space: {e}"))?;
          }
        }
        let mut last = Vec::with_capacity(s.bones.len());
        for (i, b) in s.bones.iter().enumerate() {
          if !(b.share > 0.0) {
            return Err(format!("bone {i}: the share must be > 0"));
          }
          let idx = self.resolve(b.node).map_err(|e| format!("bone {i}: {e}"))?;
          let j = self.register(stack, idx);
          last.push(stack.joints[j].work_rot);
        }
        SolverState::LookAt(LookAtState {
          from: last.clone(),
          last,
          remaining: 0.0,
          prev_yaw: None,
          had_target: s.target.is_some(),
        })
      }
      IkSolverSpec::TwoBone(s) => {
        if let Some(t) = &s.target {
          self.check_target(t)?;
        }
        if let Some(p) = &s.pole {
          self.check_point(p).map_err(|e| format!("pole: {e}"))?;
        }
        if !(s.position_weight >= 0.0
          && s.position_weight <= 1.0
          && s.rotation_weight >= 0.0
          && s.rotation_weight <= 1.0)
        {
          return Err("position and rotation weights must be within 0..1".to_string());
        }
        if !(s.soft >= 0.0 && s.soft < 1.0) || !(s.stretch >= 0.0) || !s.pole_angle.is_finite() {
          return Err("soft must be within 0..1, stretch >= 0, the pole angle finite".to_string());
        }
        let (path, mid_at) = self.two_bone_path(s.root, s.mid, s.tip)?;
        for &idx in &path {
          self.register(stack, idx);
        }
        let rest_bend = self.rest_bend(stack, &path, mid_at);
        SolverState::TwoBone(TwoBoneState { rest_bend })
      }
      IkSolverSpec::Chain(s) => {
        if let Some(t) = &s.target {
          self.check_target(t)?;
        }
        if let Some(m) = &s.magnet {
          self.check_point(m).map_err(|e| format!("magnet: {e}"))?;
        }
        if !(s.rotation_weight >= 0.0 && s.rotation_weight <= 1.0) {
          return Err("the rotation weight must be within 0..1".to_string());
        }
        if s.iterations == 0 || !(s.tolerance >= 0.0) {
          return Err("iterations must be >= 1 and the tolerance >= 0".to_string());
        }
        let path = self.path_between(s.root, s.tip, "chain")?;
        if path.len() < 2 {
          return Err("a chain needs a tip below its root".to_string());
        }
        for (i, (node, limit)) in s.limits.iter().enumerate() {
          let idx = self.resolve(*node).map_err(|e| format!("limit {i}: {e}"))?;
          if !path.contains(&idx) {
            return Err(format!("limit {i}: its node is not on the chain"));
          }
          match limit {
            IkLimit::Hinge { axis, min, max } => {
              if length(*axis) == 0.0 || !(min <= &0.0 && max >= &0.0) {
                return Err(format!("limit {i}: a hinge needs a non-zero axis and min <= 0 <= max"));
              }
            }
            IkLimit::Cone { swing, twist } => {
              if !(swing >= &0.0 && twist >= &0.0) {
                return Err(format!("limit {i}: a cone needs swing and twist >= 0"));
              }
            }
          }
        }
        for &idx in &path {
          self.register(stack, idx);
        }
        SolverState::Chain
      }
      IkSolverSpec::Feet(s) => {
        if s.legs.is_empty() {
          return Err("a feet solver needs at least one leg".to_string());
        }
        if length(s.up) == 0.0 || !s.up.iter().all(|v| v.is_finite()) {
          return Err("up must be finite and non-zero".to_string());
        }
        if !(s.rise >= 0.0 && s.drop >= 0.0 && s.plant_band > 0.0 && s.hips_ease_ms >= 0.0)
          || !s.plant_height.is_finite()
        {
          return Err("rise, drop and the ease must be >= 0, the plant band > 0, the plant height finite".to_string());
        }
        if !(s.rotation_weight >= 0.0 && s.rotation_weight <= 1.0) {
          return Err("the rotation weight must be within 0..1".to_string());
        }
        self.filter_root(&s.filter).map_err(|e| format!("filter: {e}"))?;
        let hips = self.resolve(s.hips).map_err(|e| format!("hips: {e}"))?;
        self.register(stack, hips);
        let up = normalize_or(s.up, [0.0, 1.0, 0.0]);
        let clock_pos = {
          let m = self.world(stack.clock).map_err(|e| format!("clock: {e}"))?;
          [m[12], m[13], m[14]]
        };
        let mut legs = Vec::with_capacity(s.legs.len());
        for (i, leg) in s.legs.iter().enumerate() {
          let (path, mid_at) = self.two_bone_path(leg.root, leg.mid, leg.tip).map_err(|e| format!("leg {i}: {e}"))?;
          if !self.is_under(path[0], hips) {
            return Err(format!("leg {i}: its root is not under the hips"));
          }
          for &idx in &path {
            self.register(stack, idx);
          }
          let rest_bend = self.rest_bend(stack, &path, mid_at);
          let tip = *path.last().expect("a path has a tip");
          let tip_world = self.world(self.id_of(tip)).map_err(|e| format!("leg {i}: {e}"))?;
          let height = match leg.height {
            Some(h) => {
              if !(h >= 0.0) {
                return Err(format!("leg {i}: the height must be >= 0"));
              }
              h
            }
            None => dot(sub([tip_world[12], tip_world[13], tip_world[14]], clock_pos), up).max(0.0),
          };
          let up_local = normalize_or(transform_vector_transposed(&orthonormal(&tip_world), up), [0.0, 1.0, 0.0]);
          legs.push(LegState { height, rest_bend, up_local });
        }
        SolverState::Feet(FeetState { legs, offset: 0.0, offset_target: 0.0 })
      }
    };
    Ok(Solver { spec, state, weight, weight_target: weight, weight_rate: 0.0, reach: 0.0, dead: false, curve: None })
  }

  /// Whether `idx` is `ancestor` or sits under it.
  fn is_under(&self, idx: u32, ancestor: u32) -> bool {
    let mut cursor = Some(idx);
    while let Some(c) = cursor {
      if c == ancestor {
        return true;
      }
      cursor = self.nodes[c as usize].parent;
    }
    false
  }

  /// The root-to-tip path of a two-bone limb and the mid's index on it.
  fn two_bone_path(&self, root: NodeId, mid: NodeId, tip: NodeId) -> Result<(Vec<u32>, usize), String> {
    let path = self.path_between(root, tip, "limb")?;
    let m = self.resolve(mid).map_err(|e| format!("limb mid: {e}"))?;
    let mid_at = path
      .iter()
      .position(|&i| i == m)
      .ok_or_else(|| "limb: the mid is not between the root and the tip".to_string())?;
    if mid_at == 0 || mid_at == path.len() - 1 {
      return Err("limb: the mid must lie strictly between the root and the tip".to_string());
    }
    Ok((path, mid_at))
  }

  /// The limb's bend direction as the tree stands, in the root's parent
  /// space: the mid's offset from the root-tip line, or a perpendicular
  /// of the limb when it is straight.
  fn rest_bend(&self, stack: &Stack, path: &[u32], mid_at: usize) -> [f64; 3] {
    let frame = self.parent_frame(stack, path[0]);
    let nodes = self.compose_path(stack, path, &frame);
    let (a, b, c) =
      (position_of(&nodes[0].world), position_of(&nodes[mid_at].world), position_of(&nodes[nodes.len() - 1].world));
    let bend = bend_direction(a, b, c);
    let inv = invert_affine(&frame.world);
    normalize_or(transform_vector(&inv, bend), [0.0, 0.0, 1.0])
  }

  /// The local pose of a node: the stack's working pose when it owns the
  /// node, else the arena's.
  fn local_of(&self, stack: &Stack, idx: u32) -> ([f64; 3], [f64; 4], [f64; 3]) {
    let n = &self.nodes[idx as usize];
    match stack.by_node.get(&idx) {
      Some(&j) => (stack.joints[j].work_pos, stack.joints[j].work_rot, n.scale),
      None => (n.position, quat_normalize(n.rotation), n.scale),
    }
  }

  /// The world frame of a node's parent, composed from the top over the
  /// stack's working pose (so an earlier solver's result is seen).
  fn parent_frame(&self, stack: &Stack, idx: u32) -> Frame {
    let mut chain = Vec::new();
    let mut cursor = self.nodes[idx as usize].parent;
    while let Some(c) = cursor {
      chain.push(c);
      cursor = self.nodes[c as usize].parent;
    }
    let mut world = IDENTITY;
    for &c in chain.iter().rev() {
      let (p, r, s) = self.local_of(stack, c);
      world = multiply(world, compose(p, r, s));
    }
    Frame { world }
  }

  fn compose_path(&self, stack: &Stack, path: &[u32], frame: &Frame) -> Vec<PathNode> {
    let mut out = Vec::with_capacity(path.len());
    for &idx in path {
      let (pos, rot, scale) = self.local_of(stack, idx);
      out.push(PathNode { idx, pos, rot, scale, world: IDENTITY });
    }
    recompose(&mut out, 0, frame);
    out
  }

  /// A point in world space: a node's origin, or a point in world or a
  /// node's frame. None when a node died.
  fn point_world(&self, p: &IkPoint) -> Option<[f64; 3]> {
    match p {
      IkPoint::Node(n) => self.world(*n).ok().map(|m| position_of(&m)),
      IkPoint::Point { position, space: None } => Some(*position),
      IkPoint::Point { position, space: Some(s) } => self.world(*s).ok().map(|m| transform_point(&m, *position)),
    }
  }

  /// A target in world space: position and rotation (when it has one).
  fn target_world(&self, t: &IkTarget) -> Option<([f64; 3], Option<[f64; 4]>)> {
    match t {
      IkTarget::Node(n) => self.world(*n).ok().map(|m| (position_of(&m), Some(proper_rotation_of(&m)))),
      IkTarget::Point { position, rotation, space: None } => Some((*position, rotation.map(quat_normalize))),
      IkTarget::Point { position, rotation, space: Some(s) } => self.world(*s).ok().map(|m| {
        let r = rotation.map(|r| quat_normalize(quat_multiply(proper_rotation_of(&m), quat_normalize(r))));
        (transform_point(&m, *position), r)
      }),
    }
  }

  fn up_world(&self, u: &IkUp) -> Option<[f64; 3]> {
    match u.space {
      None => Some(u.vector),
      Some(s) => self.world(s).ok().map(|m| transform_vector(&m, u.vector)),
    }
  }

  /// One frame of one stack.
  fn step_stack(&mut self, stack: &mut Stack, dt: f64, tick: &mut IkTick) {
    // The primary pose: a joint written over since the stack's last write
    // carries that write as its input.
    let mut dead_joint = false;
    for j in &mut stack.joints {
      match self.resolve(j.node) {
        Ok(idx) => {
          let n = &self.nodes[idx as usize];
          let rot = quat_normalize(n.rotation);
          if differs4(rot, j.written_rot) {
            j.input_rot = rot;
          }
          if differs3(n.position, j.written_pos) {
            j.input_pos = n.position;
          }
          j.work_rot = j.input_rot;
          j.work_pos = j.input_pos;
        }
        Err(_) => dead_joint = true,
      }
    }
    if dead_joint {
      self.retire_dead(stack);
    }
    let mut any = false;
    for k in 0..stack.solvers.len() {
      if stack.solvers[k].dead {
        continue;
      }
      // The weight ramp.
      {
        let s = &mut stack.solvers[k];
        if s.weight_rate != 0.0 {
          let next = s.weight + s.weight_rate * dt;
          let arrived =
            (s.weight_rate > 0.0 && next >= s.weight_target) || (s.weight_rate < 0.0 && next <= s.weight_target);
          if arrived {
            s.weight = s.weight_target;
            s.weight_rate = 0.0;
          } else {
            s.weight = next;
            tick.active = true;
          }
        }
      }
      let weight = stack.solvers[k].weight;
      let mut solver = std::mem::replace(&mut stack.solvers[k].state, SolverState::Chain);
      let spec = stack.solvers[k].spec.clone();
      let outcome = match (&spec, &mut solver) {
        (IkSolverSpec::LookAt(s), SolverState::LookAt(state)) => self.solve_look_at(stack, s, state, weight, dt, tick),
        (IkSolverSpec::TwoBone(s), SolverState::TwoBone(state)) => {
          if weight > 0.0 {
            self.solve_two_bone(stack, s, state, weight)
          } else {
            Some(0.0)
          }
        }
        (IkSolverSpec::Chain(s), SolverState::Chain) => {
          if weight > 0.0 {
            self.solve_chain(stack, s, weight)
          } else {
            Some(0.0)
          }
        }
        (IkSolverSpec::Feet(s), SolverState::Feet(state)) => self.solve_feet(stack, s, state, weight, dt, tick),
        _ => unreachable!("solver state matches its spec"),
      };
      let s = &mut stack.solvers[k];
      s.state = solver;
      match outcome {
        Some(reach) => s.reach = reach,
        None => s.dead = true,
      }
      any = true;
    }
    if !any {
      return;
    }
    for j in &mut stack.joints {
      let Ok(idx) = self.resolve(j.node) else { continue };
      if self.write_rotation(idx, j.work_rot) {
        tick.wrote = true;
      }
      j.written_rot = j.work_rot;
      if self.write_position(idx, j.work_pos) {
        tick.wrote = true;
      }
      j.written_pos = j.work_pos;
    }
  }

  /// Mark every solver naming a dead joint dead.
  fn retire_dead(&self, stack: &mut Stack) {
    let is_dead = |n: NodeId| self.resolve(n).is_err();
    for s in &mut stack.solvers {
      let gone = match &s.spec {
        IkSolverSpec::LookAt(l) => l.bones.iter().any(|b| is_dead(b.node)),
        IkSolverSpec::TwoBone(t) => is_dead(t.root) || is_dead(t.mid) || is_dead(t.tip),
        IkSolverSpec::Chain(c) => is_dead(c.root) || is_dead(c.tip),
        IkSolverSpec::Feet(f) => {
          is_dead(f.hips) || f.legs.iter().any(|l| is_dead(l.root) || is_dead(l.mid) || is_dead(l.tip))
        }
      };
      if gone {
        s.dead = true;
      }
    }
  }

  /// Write a position through the snap path when it differs from the
  /// node's; whether it did.
  fn write_position(&mut self, idx: u32, p: [f64; 3]) -> bool {
    let n = &mut self.nodes[idx as usize];
    if !differs3(p, n.position) {
      return false;
    }
    n.position = p;
    n.local_dirty = true;
    self.enqueue(idx);
    true
  }

  // ---- look-at ----------------------------------------------------------

  /// None = a node died. Some(residual angle).
  fn solve_look_at(
    &self,
    stack: &mut Stack,
    s: &LookAtSpec,
    state: &mut LookAtState,
    weight: f64,
    dt: f64,
    tick: &mut IkTick,
  ) -> Option<f64> {
    let n = s.bones.len();
    let f = normalize_or(s.forward, [0.0, 0.0, 1.0]);
    let u = normalize_or(s.up_axis, [0.0, 1.0, 0.0]);
    let target = match &s.target {
      Some(t) => Some(self.target_world(t)?.0),
      None => None,
    };
    let up_ref = match &s.up {
      Some(u) => Some(self.up_world(u)?),
      None => None,
    };
    if target.is_some() != state.had_target {
      start_transition(state, s.ease_ms);
      state.had_target = target.is_some();
    }
    // The chain's input frames, root first.
    let mut idxs = Vec::with_capacity(n);
    for b in &s.bones {
      idxs.push(self.resolve(b.node).ok()?);
    }
    let input: Vec<[f64; 4]> = idxs.iter().map(|&i| stack.joints[stack.by_node[&i]].work_rot).collect();
    let share_total: f64 = s.bones.iter().map(|b| b.share).sum();
    let mut residual = 0.0;
    let mut dest = input.clone();
    if let Some(target) = target {
      // The aim direction, measured from the last bone: clamped in that
      // bone's input frame when limited.
      let last = idxs[n - 1];
      let last_frame = self.parent_frame(stack, last);
      let (lp, lr, ls) = self.local_of(stack, last);
      let last_world = multiply(last_frame.world, compose(lp, lr, ls));
      let last_axes = orthonormal(&last_world);
      let head = position_of(&last_world);
      let to_target = sub(target, head);
      if length(to_target) <= DIRECTION_EPSILON {
        return Some(0.0);
      }
      let d_world = normalize_or(to_target, f);
      let d_local = normalize_or(transform_vector_transposed(&last_axes, d_world), f);
      let (mut yaw, mut pitch) = yaw_pitch(d_local, f, u);
      if yaw.is_nan() {
        yaw = state.prev_yaw.unwrap_or(0.0);
      }
      if let Some(prev) = state.prev_yaw {
        if prev.signum() != yaw.signum() && prev.abs() > FLIP_ANGLE && yaw.abs() > FLIP_ANGLE {
          start_transition(state, s.ease_ms);
        }
      }
      state.prev_yaw = Some(yaw);
      let mut aim = d_world;
      let mut clamped = false;
      if let Some(l) = &s.limit {
        let cy = clamp_damped(yaw, l.yaw, l.damp);
        let cp = clamp_damped(pitch, l.pitch, l.damp);
        if cy != yaw || cp != pitch {
          clamped = true;
          yaw = cy;
          pitch = cp;
          aim = normalize_or(transform_vector(&last_axes, direction_of(yaw, pitch, f, u)), d_world);
        }
      }
      // Each bone takes its share of what remains; the last takes all.
      let mut remaining = share_total;
      for (k, &idx) in idxs.iter().enumerate() {
        let fraction = if k + 1 == n { 1.0 } else { (s.bones[k].share / remaining).min(1.0) };
        remaining -= s.bones[k].share;
        let frame = self.parent_frame(stack, idx);
        let (p, r, sc) = self.local_of(stack, idx);
        let world = multiply(frame.world, compose(p, r, sc));
        // The last bone aims at the point itself unless clamped: the
        // parallax of its own position is exact there.
        let goal = if k + 1 == n && !clamped { normalize_or(sub(target, position_of(&world)), aim) } else { aim };
        let full = aim_delta(&world, f, u, goal, up_ref);
        let step = slerp([0.0, 0.0, 0.0, 1.0], full, fraction);
        let local = quat_normalize(quat_multiply(local_delta(&frame.world, step), r));
        dest[k] = local;
        let j = stack.by_node[&idx];
        stack.joints[j].work_rot = local;
        if k + 1 == n {
          let fw = rotate_vector(step, normalize_or(transform_vector(&world, f), f));
          residual = angle_between(fw, d_world);
        }
      }
    } else {
      state.prev_yaw = None;
    }
    // The transition, then the weight.
    if state.remaining > 0.0 && s.ease_ms > 0.0 {
      state.remaining = (state.remaining - dt * 1000.0 / s.ease_ms).max(0.0);
    }
    let via_rest = s.limit.is_some();
    for k in 0..n {
      let blended = if state.remaining > 0.0 {
        let eased = 1.0 - state.remaining;
        if via_rest {
          if eased < 0.5 {
            slerp(state.from[k], input[k], eased * 2.0)
          } else {
            slerp(input[k], dest[k], (eased - 0.5) * 2.0)
          }
        } else {
          slerp(state.from[k], dest[k], eased)
        }
      } else {
        dest[k]
      };
      state.last[k] = blended;
      let local = slerp(input[k], blended, weight);
      let j = stack.by_node[&idxs[k]];
      stack.joints[j].work_rot = local;
    }
    if state.remaining > 0.0 {
      tick.active = true;
    }
    Some(residual)
  }

  // ---- two-bone ---------------------------------------------------------

  fn solve_two_bone(&self, stack: &mut Stack, s: &TwoBoneSpec, state: &TwoBoneState, weight: f64) -> Option<f64> {
    let Some(target) = &s.target else { return Some(0.0) };
    let (target_pos, target_rot) = self.target_world(target)?;
    let pole = match &s.pole {
      Some(p) => Some(self.point_world(p)?),
      None => None,
    };
    let (path, mid_at) = self.two_bone_path(s.root, s.mid, s.tip).ok()?;
    let frame = self.parent_frame(stack, path[0]);
    let mut nodes = self.compose_path(stack, &path, &frame);
    let before: Vec<([f64; 4], [f64; 3])> = nodes.iter().map(|n| (n.rot, n.pos)).collect();
    let rest_bend = transform_vector(&frame.world, state.rest_bend);
    let reach = two_bone(&mut nodes, mid_at, &frame, target_pos, pole, rest_bend, s.pole_angle, s.soft, s.stretch);
    if s.rotation_weight > 0.0 {
      if let Some(tr) = target_rot {
        let last = nodes.len() - 1;
        let delta = orientation_delta(&nodes[last].world, tr, s.rotation_weight);
        turn(&mut nodes, last, delta, &frame, None);
      }
    }
    let w = weight * s.position_weight;
    for (k, n) in nodes.iter().enumerate() {
      let j = stack.by_node[&n.idx];
      let (rot0, pos0) = before[k];
      // The tip's rotation blends by the solver weight alone: the
      // position weight is the reach, not the grip.
      let rw = if k + 1 == nodes.len() { weight } else { w };
      stack.joints[j].work_rot = slerp(rot0, n.rot, rw);
      stack.joints[j].work_pos = lerp3(pos0, n.pos, w);
    }
    Some(reach)
  }

  // ---- chain ------------------------------------------------------------

  fn solve_chain(&self, stack: &mut Stack, s: &ChainSpec, weight: f64) -> Option<f64> {
    let Some(target) = &s.target else { return Some(0.0) };
    let (target_pos, target_rot) = self.target_world(target)?;
    let magnet = match &s.magnet {
      Some(m) => Some(self.point_world(m)?),
      None => None,
    };
    let path = self.path_between(s.root, s.tip, "chain").ok()?;
    let frame = self.parent_frame(stack, path[0]);
    let mut nodes = self.compose_path(stack, &path, &frame);
    let before: Vec<[f64; 4]> = nodes.iter().map(|n| n.rot).collect();
    // Limits by path position, with the bone axis (toward the next node)
    // in the joint's input frame.
    let mut limits: Vec<Option<(IkLimit, [f64; 3])>> = vec![None; nodes.len()];
    for (node, limit) in &s.limits {
      let idx = self.resolve(*node).ok()?;
      if let Some(k) = nodes.iter().position(|n| n.idx == idx) {
        if k + 1 < nodes.len() {
          let axis = normalize_or(nodes[k + 1].pos, [0.0, 1.0, 0.0]);
          limits[k] = Some((*limit, axis));
        }
      }
    }
    let iterations = s.iterations.min(MAX_CHAIN_ITERATIONS);
    let reach = match s.method {
      ChainMethod::Fabrik => fabrik(&mut nodes, &before, &limits, &frame, target_pos, magnet, iterations, s.tolerance),
      ChainMethod::Ccd => ccd(&mut nodes, &before, &limits, &frame, target_pos, iterations, s.tolerance),
    };
    if s.rotation_weight > 0.0 {
      if let Some(tr) = target_rot {
        let last = nodes.len() - 1;
        let delta = orientation_delta(&nodes[last].world, tr, s.rotation_weight);
        turn(&mut nodes, last, delta, &frame, None);
      }
    }
    for (k, n) in nodes.iter().enumerate() {
      let j = stack.by_node[&n.idx];
      stack.joints[j].work_rot = slerp(before[k], n.rot, weight);
    }
    Some(reach)
  }

  // ---- feet -------------------------------------------------------------

  fn solve_feet(
    &mut self,
    stack: &mut Stack,
    s: &FeetSpec,
    state: &mut FeetState,
    weight: f64,
    dt: f64,
    tick: &mut IkTick,
  ) -> Option<f64> {
    let up = normalize_or(s.up, [0.0, 1.0, 0.0]);
    let down = scale3(up, -1.0);
    let hips = self.resolve(s.hips).ok()?;
    let clock_pos = position_of(&self.world(stack.clock).ok()?);
    // Pass 1: the animated feet and their ground.
    struct Foot {
      path: Vec<u32>,
      mid_at: usize,
      pos: [f64; 3],
      ground: Option<([f64; 3], [f64; 3])>,
      plant: f64,
    }
    let mut feet = Vec::with_capacity(s.legs.len());
    for (i, leg) in s.legs.iter().enumerate() {
      let (path, mid_at) = self.two_bone_path(leg.root, leg.mid, leg.tip).ok()?;
      let frame = self.parent_frame(stack, path[0]);
      let nodes = self.compose_path(stack, &path, &frame);
      let pos = position_of(&nodes[nodes.len() - 1].world);
      let origin = add(pos, scale3(up, s.rise));
      let hits = self.raycast(origin, down, &s.filter).ok()?;
      let limit = s.rise + s.drop;
      let ground =
        hits.iter().find(|h| h.distance <= limit).map(|h| (add(h.point, scale3(up, state.legs[i].height)), h.normal));
      let lift = dot(sub(pos, clock_pos), up) - state.legs[i].height;
      let plant = (1.0 - (lift - s.plant_height) / s.plant_band).clamp(0.0, 1.0);
      feet.push(Foot { path, mid_at, pos, ground, plant });
    }
    // The hips: lowered (or raised) by the planted foot that needs it
    // most, eased.
    let mut target = 0.0f64;
    let mut planted = false;
    for f in &feet {
      if let Some((g, _)) = f.ground {
        if f.plant > 0.0 {
          let delta = dot(sub(g, f.pos), up) * f.plant;
          target = if planted { target.min(delta) } else { delta };
          planted = true;
        }
      }
    }
    state.offset_target = target.clamp(-s.drop, s.rise) * weight;
    let gap = state.offset_target - state.offset;
    if gap.abs() > HIPS_SETTLE {
      if s.hips_ease_ms > 0.0 {
        let k = 1.0 - (-dt * 1000.0 / s.hips_ease_ms).exp();
        state.offset += gap * k;
        tick.active = true;
      } else {
        state.offset = state.offset_target;
      }
    } else {
      state.offset = state.offset_target;
    }
    {
      let frame = self.parent_frame(stack, hips);
      let inv = invert_affine(&frame.world);
      let shift = transform_vector(&inv, scale3(up, state.offset));
      let j = stack.by_node[&hips];
      stack.joints[j].work_pos = add(stack.joints[j].input_pos, shift);
    }
    // Pass 2: each planted leg reaches its ground target from under the
    // moved hips, the foot aligned to the normal.
    let mut reach = 0.0f64;
    for (i, f) in feet.iter().enumerate() {
      let Some((goal, normal)) = f.ground else { continue };
      let w = weight * f.plant;
      if w <= 0.0 {
        continue;
      }
      let frame = self.parent_frame(stack, f.path[0]);
      let mut nodes = self.compose_path(stack, &f.path, &frame);
      let before: Vec<([f64; 4], [f64; 3])> = nodes.iter().map(|n| (n.rot, n.pos)).collect();
      let now = position_of(&nodes[nodes.len() - 1].world);
      let target = lerp3(now, goal, f.plant);
      let rest_bend = transform_vector(&frame.world, state.legs[i].rest_bend);
      let r = two_bone(&mut nodes, f.mid_at, &frame, target, None, rest_bend, 0.0, 0.0, 0.0);
      reach = reach.max(r);
      if s.rotation_weight > 0.0 {
        let last = nodes.len() - 1;
        let foot_up = normalize_or(transform_vector(&nodes[last].world, state.legs[i].up_local), normal);
        let align = slerp([0.0, 0.0, 0.0, 1.0], quat_from_to(foot_up, normal), s.rotation_weight);
        turn(&mut nodes, last, align, &frame, None);
      }
      for (k, n) in nodes.iter().enumerate() {
        let j = stack.by_node[&n.idx];
        let (rot0, pos0) = before[k];
        stack.joints[j].work_rot = slerp(rot0, n.rot, w);
        stack.joints[j].work_pos = lerp3(pos0, n.pos, w);
      }
    }
    Some(reach)
  }
}

/// Whether a target change is a switch (Godot's set_target: a transition
/// starts) rather than a move: a point moved to another point is a move
/// (a cursor fed every frame tracks), anything else is a switch.
fn target_switches(a: &Option<IkTarget>, b: &Option<IkTarget>) -> bool {
  match (a, b) {
    (None, None) => false,
    (Some(IkTarget::Node(x)), Some(IkTarget::Node(y))) => x != y,
    (Some(IkTarget::Point { space: sa, .. }), Some(IkTarget::Point { space: sb, .. })) => sa != sb,
    _ => true,
  }
}

fn start_transition(state: &mut LookAtState, ease_ms: f64) {
  if ease_ms <= 0.0 {
    return;
  }
  state.from.clone_from(&state.last);
  state.remaining = 1.0;
}

/// Recompute the world frames of `nodes[from..]` from their locals.
fn recompose(nodes: &mut [PathNode], from: usize, frame: &Frame) {
  for k in from..nodes.len() {
    let pw = if k == 0 { frame.world } else { nodes[k - 1].world };
    let n = &mut nodes[k];
    n.world = multiply(pw, compose(n.pos, n.rot, n.scale));
  }
}

/// Turn path node `k` by the world-space `delta`, its local set from its
/// parent's frame, the limit (when any) applied against its input
/// rotation, and the path below recomposed.
fn turn(
  nodes: &mut [PathNode],
  k: usize,
  delta: [f64; 4],
  frame: &Frame,
  input: Option<([f64; 4], &Option<(IkLimit, [f64; 3])>)>,
) {
  let parent = if k == 0 { frame.world } else { nodes[k - 1].world };
  let mut local = quat_normalize(quat_multiply(local_delta(&parent, delta), nodes[k].rot));
  if let Some((input_rot, Some((limit, axis)))) = input {
    local = clamp_limit(local, input_rot, limit, *axis);
  }
  nodes[k].rot = local;
  recompose(nodes, k, frame);
}

/// The analytic two-bone solve over `nodes` (root first, the mid at
/// `mid_at`, the tip last): the mid's interior angle set by the law of
/// cosines, the root turned to bring the tip onto the target, the bend
/// plane turned toward the pole (else kept), a twist by `pole_angle`,
/// `soft` easing the reach near full extension and `stretch` lengthening
/// the limb past it. Returns the residual.
#[allow(clippy::too_many_arguments)]
fn two_bone(
  nodes: &mut [PathNode],
  mid_at: usize,
  frame: &Frame,
  target: [f64; 3],
  pole: Option<[f64; 3]>,
  rest_bend: [f64; 3],
  pole_angle: f64,
  soft: f64,
  stretch: f64,
) -> f64 {
  let last = nodes.len() - 1;
  let a = position_of(&nodes[0].world);
  let b = position_of(&nodes[mid_at].world);
  let c = position_of(&nodes[last].world);
  let l1 = length(sub(b, a));
  let l2 = length(sub(c, b));
  let total = l1 + l2;
  if total <= DIRECTION_EPSILON {
    return length(sub(target, c));
  }
  let at = sub(target, a);
  let d_raw = length(at);
  if d_raw <= DIRECTION_EPSILON {
    return length(sub(target, c));
  }
  // The soft band: the effective distance eases toward full extension.
  let mut d = d_raw;
  if soft > 0.0 {
    let band = soft * total;
    let start = total - band;
    if d > start {
      d = total - band * (-(d - start) / band).exp();
    }
  }
  // Stretch: past full reach the limb lengthens along its bones.
  let mut scale = 1.0;
  if stretch > 0.0 && d_raw > total {
    scale = (d_raw / total).min(1.0 + stretch);
    for n in nodes.iter_mut().skip(1) {
      n.pos = scale3(n.pos, scale);
    }
    recompose(nodes, 0, frame);
  }
  let (l1, l2, total) = (l1 * scale, l2 * scale, total * scale);
  let d = d.clamp((l1 - l2).abs(), total);
  // The mid: its interior angle.
  let a = position_of(&nodes[0].world);
  let b = position_of(&nodes[mid_at].world);
  let c = position_of(&nodes[last].world);
  let ab = sub(b, a);
  let bc = sub(c, b);
  let old_angle = triangle_angle(length(sub(c, a)), l1, l2);
  let new_angle = triangle_angle(d, l1, l2);
  let bend = bend_direction(a, b, c);
  let bend = if length(bend) > DIRECTION_EPSILON { bend } else { rest_bend };
  let mut axis = cross(ab, bc);
  if length(axis) <= DIRECTION_EPSILON {
    axis = cross(ab, bend);
  }
  let axis = normalize_or(axis, [0.0, 0.0, 1.0]);
  turn(nodes, mid_at, quat_axis_angle(axis, old_angle - new_angle), frame, None);
  // The root: the tip onto the target's direction.
  let c = position_of(&nodes[last].world);
  let ac = sub(c, a);
  turn(nodes, 0, quat_from_to(normalize_or(ac, at), normalize_or(at, ac)), frame, None);
  // The bend plane: the mid toward the pole, in the plane across the
  // root-target axis.
  let axis_t = normalize_or(at, [0.0, 1.0, 0.0]);
  let b = position_of(&nodes[mid_at].world);
  let b_proj = reject(sub(b, a), axis_t);
  let pole_dir = match pole {
    Some(p) => reject(sub(p, a), axis_t),
    None => [0.0; 3],
  };
  if length(b_proj) > DIRECTION_EPSILON && length(pole_dir) > DIRECTION_EPSILON {
    let delta = quat_from_to(normalize_or(b_proj, axis_t), normalize_or(pole_dir, axis_t));
    turn(nodes, 0, delta, frame, None);
  }
  if pole_angle != 0.0 {
    turn(nodes, 0, quat_axis_angle(axis_t, pole_angle), frame, None);
  }
  length(sub(target, position_of(&nodes[last].world)))
}

/// FABRIK over the path's positions, each sweep followed by the rotations
/// that realize it (limits applied there, Godot's and Unreal's
/// post-projection), until the tip is within tolerance.
#[allow(clippy::too_many_arguments)]
fn fabrik(
  nodes: &mut [PathNode],
  input: &[[f64; 4]],
  limits: &[Option<(IkLimit, [f64; 3])>],
  frame: &Frame,
  target: [f64; 3],
  magnet: Option<[f64; 3]>,
  iterations: u32,
  tolerance: f64,
) -> f64 {
  let n = nodes.len();
  let root = position_of(&nodes[0].world);
  let mut p: Vec<[f64; 3]> = nodes.iter().map(|x| position_of(&x.world)).collect();
  let lengths: Vec<f64> = (0..n - 1).map(|k| length(sub(p[k + 1], p[k]))).collect();
  let total: f64 = lengths.iter().sum();
  let mut residual = length(sub(target, p[n - 1]));
  let to_target = sub(target, root);
  let to_tip = sub(p[n - 1], root);
  if residual > tolerance && length(to_target) > DIRECTION_EPSILON && length(to_tip) > DIRECTION_EPSILON {
    // A rigid turn of the chain onto the target's direction first: a
    // straight reach is then exact at once and a bent one starts near
    // its answer. From a chain pointing elsewhere the sweeps converge
    // slowly near full extension, and ten of them leave it bent until
    // the target steps out of reach and the straight branch snaps it.
    let q = quat_from_to(normalize_or(to_tip, to_target), normalize_or(to_target, to_tip));
    for x in p.iter_mut().skip(1) {
      *x = add(root, rotate_vector(q, sub(*x, root)));
    }
    // A chain lying on the target's axis has nothing to fold toward
    // (every sweep lands back on the line): the middle joint takes a
    // small step toward the magnet, else across the axis.
    if n >= 3 && length(to_target) < total {
      let axis = normalize_or(to_target, [0.0, 0.0, 1.0]);
      let mut mean = [0.0; 3];
      for x in &p[1..n - 1] {
        mean = add(mean, reject(sub(*x, root), axis));
      }
      if length(mean) <= DEGENERATE_RADIAL * total {
        let side = magnet.map(|m| reject(sub(m, root), axis)).filter(|v| length(*v) > DIRECTION_EPSILON);
        let side = side.unwrap_or_else(|| perpendicular(axis));
        p[n / 2] = add(p[n / 2], scale3(normalize_or(side, axis), DEGENERATE_NUDGE * total));
      }
    }
  }
  for _ in 0..iterations {
    if residual <= tolerance {
      break;
    }
    if length(sub(target, root)) >= total {
      // Out of reach: straight toward it.
      let dir = normalize_or(sub(target, root), [0.0, 1.0, 0.0]);
      for k in 0..n - 1 {
        p[k + 1] = add(p[k], scale3(dir, lengths[k]));
      }
    } else {
      p[n - 1] = target;
      for k in (0..n - 1).rev() {
        let dir = normalize_or(sub(p[k], p[k + 1]), [0.0, 1.0, 0.0]);
        p[k] = add(p[k + 1], scale3(dir, lengths[k]));
      }
      p[0] = root;
      for k in 0..n - 1 {
        let dir = normalize_or(sub(p[k + 1], p[k]), [0.0, 1.0, 0.0]);
        p[k + 1] = add(p[k], scale3(dir, lengths[k]));
      }
    }
    if let Some(m) = magnet {
      magnet_turn(&mut p, m, total);
    }
    // Realize the positions as rotations, root first, limits applied.
    for k in 0..n - 1 {
      let here = position_of(&nodes[k].world);
      let cur = sub(position_of(&nodes[k + 1].world), here);
      let want = sub(p[k + 1], here);
      if length(cur) <= DIRECTION_EPSILON || length(want) <= DIRECTION_EPSILON {
        continue;
      }
      let delta = quat_from_to(normalize_or(cur, want), normalize_or(want, cur));
      turn(nodes, k, delta, frame, Some((input[k], &limits[k])));
    }
    for (k, x) in nodes.iter().enumerate() {
      p[k] = position_of(&x.world);
    }
    residual = length(sub(target, p[n - 1]));
  }
  residual
}

/// The interior points turned about the root-tip axis so their mean
/// radial direction faces the magnet: one rigid turn ABOUT THE AXIS (an
/// explicit axis-angle, never a shortest arc between the radials, which
/// for a straight chain is a turn away from the axis by float noise), so
/// every length holds; a chain within the degenerate band of the axis
/// has no side to turn and is left alone.
fn magnet_turn(p: &mut [[f64; 3]], magnet: [f64; 3], total: f64) {
  let n = p.len();
  if n < 3 {
    return;
  }
  let root = p[0];
  let axis = sub(p[n - 1], root);
  if length(axis) <= DIRECTION_EPSILON {
    return;
  }
  let axis = normalize_or(axis, [0.0, 1.0, 0.0]);
  let mut mean = [0.0; 3];
  for q in &p[1..n - 1] {
    mean = add(mean, reject(sub(*q, root), axis));
  }
  let want = reject(sub(magnet, root), axis);
  if length(mean) <= DEGENERATE_RADIAL * total || length(want) <= DIRECTION_EPSILON {
    return;
  }
  let a = normalize_or(mean, axis);
  let b = normalize_or(want, axis);
  let angle = dot(cross(a, b), axis).atan2(dot(a, b));
  let q = quat_axis_angle(axis, angle);
  for x in &mut p[1..n - 1] {
    *x = add(root, rotate_vector(q, sub(*x, root)));
  }
}

/// CCD: from the joint before the tip back to the root, each turns the
/// tip toward the target, limits applied, until within tolerance.
fn ccd(
  nodes: &mut [PathNode],
  input: &[[f64; 4]],
  limits: &[Option<(IkLimit, [f64; 3])>],
  frame: &Frame,
  target: [f64; 3],
  iterations: u32,
  tolerance: f64,
) -> f64 {
  let n = nodes.len();
  let mut residual = length(sub(target, position_of(&nodes[n - 1].world)));
  for _ in 0..iterations {
    if residual <= tolerance {
      break;
    }
    for k in (0..n - 1).rev() {
      let here = position_of(&nodes[k].world);
      let to_tip = sub(position_of(&nodes[n - 1].world), here);
      let to_target = sub(target, here);
      if length(to_tip) <= DIRECTION_EPSILON || length(to_target) <= DIRECTION_EPSILON {
        continue;
      }
      let delta = quat_from_to(normalize_or(to_tip, to_target), normalize_or(to_target, to_tip));
      turn(nodes, k, delta, frame, Some((input[k], &limits[k])));
    }
    residual = length(sub(target, position_of(&nodes[n - 1].world)));
  }
  residual
}

/// `local` held within the limit relative to `input` (both the joint's
/// local rotation): the difference decomposed into swing and twist about
/// the limit's axis and each part bounded.
fn clamp_limit(local: [f64; 4], input: [f64; 4], limit: &IkLimit, bone_axis: [f64; 3]) -> [f64; 4] {
  let delta = quat_normalize(quat_multiply(quat_conjugate(input), local));
  let clamped = match *limit {
    IkLimit::Hinge { axis, min, max } => {
      let axis = normalize_or(axis, bone_axis);
      let (_, twist) = swing_twist(delta, axis);
      let angle = twist_angle(twist, axis).clamp(min, max);
      quat_axis_angle(axis, angle)
    }
    IkLimit::Cone { swing, twist } => {
      let (s, t) = swing_twist(delta, bone_axis);
      let s = clamp_angle(s, swing);
      let t_angle = twist_angle(t, bone_axis).clamp(-twist, twist);
      quat_normalize(quat_multiply(s, quat_axis_angle(bone_axis, t_angle)))
    }
  };
  quat_normalize(quat_multiply(input, clamped))
}

/// q = swing * twist, the twist about `axis` (unit): (swing, twist).
fn swing_twist(q: [f64; 4], axis: [f64; 3]) -> ([f64; 4], [f64; 4]) {
  let q = if q[3] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
  let proj = dot([q[0], q[1], q[2]], axis);
  let twist = quat_normalize([axis[0] * proj, axis[1] * proj, axis[2] * proj, q[3]]);
  let swing = quat_normalize(quat_multiply(q, quat_conjugate(twist)));
  (swing, twist)
}

/// The signed angle of a twist quaternion about its axis.
fn twist_angle(twist: [f64; 4], axis: [f64; 3]) -> f64 {
  let s = dot([twist[0], twist[1], twist[2]], axis);
  2.0 * s.atan2(twist[3])
}

/// A rotation's angle held to at most `max` about its own axis.
fn clamp_angle(q: [f64; 4], max: f64) -> [f64; 4] {
  let q = if q[3] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
  let s = length([q[0], q[1], q[2]]);
  let angle = 2.0 * s.atan2(q[3]);
  if angle <= max || s <= DIRECTION_EPSILON {
    return q;
  }
  quat_axis_angle(scale3([q[0], q[1], q[2]], 1.0 / s), max)
}

/// The yaw (about `u`, positive toward right = u x f) and pitch (toward
/// `u`) of a unit direction in a frame of forward `f` and up `u`. The
/// yaw is NaN at the poles.
fn yaw_pitch(d: [f64; 3], f: [f64; 3], u: [f64; 3]) -> (f64, f64) {
  let r = normalize_or(cross(u, f), [1.0, 0.0, 0.0]);
  let s = dot(d, u).clamp(-1.0, 1.0);
  let pitch = s.asin();
  if s.abs() > POLE_SIN_LIMIT {
    return (f64::NAN, pitch);
  }
  (dot(d, r).atan2(dot(d, f)), pitch)
}

fn direction_of(yaw: f64, pitch: f64, f: [f64; 3], u: [f64; 3]) -> [f64; 3] {
  let r = normalize_or(cross(u, f), [1.0, 0.0, 0.0]);
  let cp = pitch.cos();
  normalize_or(add(add(scale3(f, cp * yaw.cos()), scale3(r, cp * yaw.sin())), scale3(u, pitch.sin())), f)
}

/// An angle held within `[min, max]`: a hard clamp when `damp` is 1,
/// else past `damp` of the limit an exponential approach to it (Godot's
/// damped remap), continuous in value and slope.
fn clamp_damped(a: f64, limit: [f64; 2], damp: f64) -> f64 {
  let (sign, bound) = if a >= 0.0 { (1.0, limit[1]) } else { (-1.0, -limit[0]) };
  let m = a.abs();
  let start = bound * damp;
  if m <= start {
    return a;
  }
  let room = bound - start;
  if room <= 0.0 {
    return sign * bound;
  }
  sign * (start + room * (1.0 - (-(m - start) / room).exp()))
}

/// The world-space rotation taking a bone of world matrix `world`, local
/// forward `f` and local up `u`, so that its forward is `goal`: the
/// shortest arc, or with `up_ref` the frame whose up leans to it.
fn aim_delta(world: &Mat4, f: [f64; 3], u: [f64; 3], goal: [f64; 3], up_ref: Option<[f64; 3]>) -> [f64; 4] {
  let fw = normalize_or(transform_vector(world, f), f);
  let arc = quat_from_to(fw, goal);
  let Some(up_ref) = up_ref else { return arc };
  // The up after the arc, against the reference projected across goal:
  // a twist about goal closes the roll.
  let up_now = reject(rotate_vector(arc, normalize_or(transform_vector(world, u), u)), goal);
  let up_want = reject(up_ref, goal);
  if length(up_now) <= DIRECTION_EPSILON || length(up_want) <= DIRECTION_EPSILON {
    return arc;
  }
  let twist = quat_from_to(normalize_or(up_now, goal), normalize_or(up_want, goal));
  quat_normalize(quat_multiply(twist, arc))
}

/// The world-space turn taking a node of world matrix `world` part of
/// the way (`weight`) to the orientation `target`, both read in Three's
/// decompose convention (a mirrored frame is a proper rotation with its
/// x axis negated), so a target orientation under a mirrored arm means
/// what it means there.
fn orientation_delta(world: &Mat4, target: [f64; 4], weight: f64) -> [f64; 4] {
  let now = proper_rotation_of(world);
  let goal = slerp(now, target, weight);
  quat_normalize(quat_multiply(goal, quat_conjugate(now)))
}

/// A world-space turn `delta` carried into the local frame of a node
/// whose parent's world matrix is `parent`: `Pn^T * D * Pn` over the
/// parent's orthonormal part, a proper rotation even when that part is
/// a mirror (a conjugation of a rotation by a reflection is a rotation),
/// which is what a quaternion inverse of the parent cannot give.
fn local_delta(parent: &Mat4, delta: [f64; 4]) -> [f64; 4] {
  let pn = orthonormal(parent);
  let m = multiply(multiply(transpose(&pn), compose([0.0; 3], delta, [1.0; 3])), pn);
  rotation_of(&m)
}

/// The orthonormal part of an affine matrix: its columns normalized with
/// their signs kept (a mirror stays a mirror), the translation dropped.
fn orthonormal(m: &Mat4) -> Mat4 {
  let x = normalize_or([m[0], m[1], m[2]], [1.0, 0.0, 0.0]);
  let y = normalize_or([m[4], m[5], m[6]], [0.0, 1.0, 0.0]);
  let z = normalize_or([m[8], m[9], m[10]], [0.0, 0.0, 1.0]);
  [x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// The transpose of the upper 3x3 (the inverse of an orthonormal part),
/// as a Mat4.
fn transpose(m: &Mat4) -> Mat4 {
  [m[0], m[4], m[8], 0.0, m[1], m[5], m[9], 0.0, m[2], m[6], m[10], 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// `transpose(upper 3x3 of m) * v`: a world vector into the frame of an
/// orthonormal `m`.
fn transform_vector_transposed(m: &Mat4, v: [f64; 3]) -> [f64; 3] {
  [
    m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
    m[4] * v[0] + m[5] * v[1] + m[6] * v[2],
    m[8] * v[0] + m[9] * v[1] + m[10] * v[2],
  ]
}

/// The proper rotation of a possibly mirrored frame, Three's decompose
/// convention: the orthonormal part with its x axis negated when the
/// determinant is negative.
fn proper_rotation_of(m: &Mat4) -> [f64; 4] {
  let mut n = orthonormal(m);
  let det =
    n[0] * (n[5] * n[10] - n[6] * n[9]) - n[4] * (n[1] * n[10] - n[2] * n[9]) + n[8] * (n[1] * n[6] - n[2] * n[5]);
  if det < 0.0 {
    n[0] = -n[0];
    n[1] = -n[1];
    n[2] = -n[2];
  }
  rotation_of(&n)
}

/// The interior angle opposite side `a` in a triangle of sides a, b, c
/// (the angle between b and c).
fn triangle_angle(a: f64, b: f64, c: f64) -> f64 {
  let denom = 2.0 * b * c;
  if denom <= 0.0 {
    return 0.0;
  }
  ((b * b + c * c - a * a) / denom).clamp(-1.0, 1.0).acos()
}

/// The mid's offset from the root-tip line (zero when straight).
fn bend_direction(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
  let axis = sub(c, a);
  if length(axis) <= DIRECTION_EPSILON {
    return sub(b, a);
  }
  reject(sub(b, a), normalize_or(axis, [0.0, 1.0, 0.0]))
}

/// Some unit vector perpendicular to unit `axis`.
fn perpendicular(axis: [f64; 3]) -> [f64; 3] {
  let pick = if axis[0].abs() > axis[2].abs() { [-axis[1], axis[0], 0.0] } else { [0.0, -axis[2], axis[1]] };
  normalize_or(pick, [1.0, 0.0, 0.0])
}

/// `v` without its component along unit `axis`.
fn reject(v: [f64; 3], axis: [f64; 3]) -> [f64; 3] {
  sub(v, scale3(axis, dot(v, axis)))
}

fn angle_between(a: [f64; 3], b: [f64; 3]) -> f64 {
  let la = length(a);
  let lb = length(b);
  if la <= 0.0 || lb <= 0.0 {
    return 0.0;
  }
  (dot(a, b) / (la * lb)).clamp(-1.0, 1.0).acos()
}

fn position_of(m: &Mat4) -> [f64; 3] {
  [m[12], m[13], m[14]]
}

/// The rotation of an affine matrix with a proper orthonormal part (its
/// columns normalized, so a uniform or per-axis scale drops out; a mirror
/// goes through `proper_rotation_of`).
fn rotation_of(m: &Mat4) -> [f64; 4] {
  let x = normalize_or([m[0], m[1], m[2]], [1.0, 0.0, 0.0]);
  let y = normalize_or([m[4], m[5], m[6]], [0.0, 1.0, 0.0]);
  let z = normalize_or([m[8], m[9], m[10]], [0.0, 0.0, 1.0]);
  let trace = x[0] + y[1] + z[2];
  let q = if trace > 0.0 {
    let s = (trace + 1.0).sqrt() * 2.0;
    [(y[2] - z[1]) / s, (z[0] - x[2]) / s, (x[1] - y[0]) / s, 0.25 * s]
  } else if x[0] > y[1] && x[0] > z[2] {
    let s = (1.0 + x[0] - y[1] - z[2]).sqrt() * 2.0;
    [0.25 * s, (y[0] + x[1]) / s, (z[0] + x[2]) / s, (y[2] - z[1]) / s]
  } else if y[1] > z[2] {
    let s = (1.0 + y[1] - x[0] - z[2]).sqrt() * 2.0;
    [(y[0] + x[1]) / s, 0.25 * s, (z[1] + y[2]) / s, (z[0] - x[2]) / s]
  } else {
    let s = (1.0 + z[2] - x[0] - y[1]).sqrt() * 2.0;
    [(z[0] + x[2]) / s, (z[1] + y[2]) / s, 0.25 * s, (x[1] - y[0]) / s]
  };
  quat_normalize(q)
}

fn quat_axis_angle(axis: [f64; 3], angle: f64) -> [f64; 4] {
  let axis = normalize_or(axis, [0.0, 0.0, 1.0]);
  let h = angle * 0.5;
  let s = h.sin();
  quat_normalize([axis[0] * s, axis[1] * s, axis[2] * s, h.cos()])
}

/// The shortest arc taking unit `a` onto unit `b`.
fn quat_from_to(a: [f64; 3], b: [f64; 3]) -> [f64; 4] {
  let a = normalize_or(a, [0.0, 0.0, 1.0]);
  let b = normalize_or(b, a);
  let r = dot(a, b) + 1.0;
  if r < ANTIPARALLEL_EPSILON {
    let axis = if a[0].abs() > a[2].abs() { [-a[1], a[0], 0.0] } else { [0.0, -a[2], a[1]] };
    let axis = normalize_or(axis, [1.0, 0.0, 0.0]);
    return [axis[0], axis[1], axis[2], 0.0];
  }
  let c = cross(a, b);
  quat_normalize([c[0], c[1], c[2], r])
}

fn quat_conjugate(q: [f64; 4]) -> [f64; 4] {
  [-q[0], -q[1], -q[2], q[3]]
}

fn differs4(a: [f64; 4], b: [f64; 4]) -> bool {
  (0..4).any(|i| (a[i] - b[i]).abs() > ROTATION_EPSILON)
}

fn differs3(a: [f64; 3], b: [f64; 3]) -> bool {
  (0..3).any(|i| (a[i] - b[i]).abs() > POSITION_EPSILON)
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
  [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale3(v: [f64; 3], s: f64) -> [f64; 3] {
  [v[0] * s, v[1] * s, v[2] * s]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn length(v: [f64; 3]) -> f64 {
  dot(v, v).sqrt()
}

fn length4(q: [f64; 4]) -> f64 {
  (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt()
}

fn normalize_or(v: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
  let l = length(v);
  if l > 0.0 {
    scale3(v, 1.0 / l)
  } else {
    fallback
  }
}

/// The damped clamp, for the tests.
#[cfg(test)]
pub(crate) fn clamp_damped_for_test(a: f64, limit: [f64; 2], damp: f64) -> f64 {
  clamp_damped(a, limit, damp)
}
