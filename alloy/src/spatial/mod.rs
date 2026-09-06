// The spatial core: a transform hierarchy whose flush recomputes only what
// changed and hands fresh world matrices to sinks. Generic on purpose - no
// camera, no mesh, no lights - so any consumer with a tree of transforms
// (the 3d package first, a 2D sprite scene or a skeleton just as well) gets
// the interpreter-hostile part of a scene graph in native code. Lives on
// the main thread beside `Context`, which turns sink writes into raster
// commands; the raster thread is untouched. Engine independent: native
// types in and out, no scripting references (rendertree rules).
//
// Cost model: a write marks one node and queues it; the flush recomputes
// the subtrees under the queued nodes that have no queued ancestor, so a
// moved node costs its own subtree and nothing else, however big the tree.
// Node ids carry a generation and are never reused.

mod bvh;
mod cull;
mod math;
mod pick;
mod players;
mod transitions;

use std::collections::HashMap;

pub use bvh::{ray_box_distance, Box3};
pub use math::{compose, invert_affine, multiply, normal_matrix, transform_point, transform_vector, IDENTITY};
pub use pick::{Hit, Shape, ShapeId};
// The linear narrowphase and the indexing threshold, for tests: the
// brute-force path is the oracle the BVH path is checked against.
#[cfg(test)]
#[cfg(test)]
pub(crate) use pick::{ray_shape, BVH_MIN_TRIANGLES};
pub use players::{
  ChannelInterpolation, ChannelPath, ClipChannel, ClipEvent, ClipId, PlayerId, PlayerUpdate, PlayersTick,
};
// The pure sampler, for the differential tests.
#[cfg(test)]
pub(crate) use players::sample as sample_channel;
pub use transitions::{Component, NodeTransitionConfig};

use bvh::Bvh;
use cull::{union, world_box, Frustum};
use transitions::NodeTransitions;

pub type Mat4 = [f32; 16];

/// A stable node handle: arena index in the low 32 bits, generation in the
/// high 32 - a destroyed node's id never resolves again.
pub type NodeId = u64;

/// Where a node's fresh world matrix goes: the `uModel` (+ `uNormal`) params
/// of one draw entry, plus the entry's instance count as its visibility
/// switch (`count` is what "shown" restores: 1 for a plain mesh, the record
/// count for an instanced one). A node carries one draw sink PER TARGET
/// (a mesh drawn by the scene and by each of its views), all fed by the
/// same flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawSink {
  pub target: u64,
  pub draw: u64,
  pub normal: bool,
  pub count: u32,
}

/// A bound draw sink and its per-entry flush state.
#[derive(Clone, Copy)]
struct BoundSink {
  sink: DrawSink,
  /// The entry is switched on (instance count = `sink.count`).
  entry_on: bool,
  /// The entry owes a params write at the next shown flush: newly bound,
  /// or the node moved while hidden.
  fresh: bool,
}

/// The consumer of sink writes, one method per write kind, called in flush
/// order. The core's entire output contract: everything a flush produces
/// goes through this trait, and the core never sees where it lands (alloy's
/// Context resolves the ids against its draw entries and forwards down the
/// raster channel; tests record). Arguments are borrowed from core state -
/// an implementation copies what it keeps.
///
/// Every write returns whether it landed. False means the resource is gone
/// (a destroyed target, buffer or texture; ids are never reused, so it is
/// gone for good), and the core releases the binding that produced the
/// write: the draw sink, or the slot/record/palette group. A dead binding
/// thus costs one write, not one per frame.
pub trait SinkWriter {
  /// A shown entry's fresh world transform: `uModel`, plus `uNormal` when
  /// the sink asked for it.
  fn write_params(&mut self, target: u64, draw: u64, model: &Mat4, normal: Option<&Mat4>) -> bool;
  /// An entry's instance count - the visibility switch (0 = hidden, the
  /// sink's count = shown).
  fn write_count(&mut self, target: u64, draw: u64, count: u32) -> bool;
  /// A shared-slot group's array param, rewritten whole (slot sinks share
  /// one array value; see `SharedSlotSink`).
  fn write_shared(&mut self, target: u64, name: &str, values: &[f32]) -> bool;
  /// One buffer's staged instance records: the coalesced dirty float range
  /// `[lo, hi)` plus `values`, the WHOLE staging mirror - so a writer that
  /// must publish the full record set (an ordered instance buffer gathers
  /// into draw order, where a partial range has no stable position) can
  /// reach every record, while the plain path writes just the range. At
  /// most one write per buffer per flush, however many nodes moved (see
  /// `InstanceRecordSink`).
  fn write_instances(&mut self, buffer: u64, lo: u32, hi: u32, values: &[f32]) -> bool;
  /// A float texture's fresh rows, whole: 16 floats (one column-major mat4,
  /// one row of a 4-texel-wide rgba32f texture) per bound slot. At most one
  /// write per texture per flush (see `TextureSlotSink`).
  fn write_texture(&mut self, texture: u64, values: &[f32]) -> bool;
}

/// How a shared-slot sink projects the node's world transform into its
/// three floats. `Direction` is `normalize(worldRotation * v)` (zeros for
/// a degenerate result); `Position` is the world translation - the pair a
/// positional light needs (a spot light feeds both arrays of one target).
#[derive(Clone, Debug, PartialEq)]
pub enum Projection {
  /// The world direction of this LOCAL vector.
  Direction([f32; 3]),
  /// The node's world position.
  Position,
}

/// Routes a projection of the node's world transform to one vec3 slot of
/// a target shared param: floats [index*3, index*3+3) of the `len`-float
/// array param `name`, shared by every sink naming it - the whole array
/// is one param value, re-sent when any slot changes, absent slots zero.
/// The generic form of "a scene's light directions follow the node tree":
/// the consumer picks the param name and packs non-spatial data (colors,
/// counts) itself - core never learns what the slots mean. A node carries
/// one slot sink per (target, param name), so one node may feed several
/// arrays of one target (a spot light: its direction and its position).
#[derive(Clone, Debug, PartialEq)]
pub struct SharedSlotSink {
  pub target: u64,
  pub name: String,
  /// Total floats of the array param (a multiple of 3).
  pub len: u32,
  /// vec3 slot index within it.
  pub index: u32,
  pub projection: Projection,
}

/// One shared-param array and the sinks feeding it.
struct SharedGroup {
  values: Vec<f32>,
  refs: u32,
  dirty: bool,
}

/// How an instance-record sink projects the node's world transform into
/// its slot's floats. `Pose2D` is `[x, y, angle, sx, sy]`: world xy
/// translation, the rotation of the local x axis in the world xy plane
/// (`atan2(m[1], m[0])`), and the xy scale, `sy` negated when the matrix
/// mirrors (negative 2x2 determinant) so handedness survives the round
/// trip. A full-matrix projection is the anticipated 3d sibling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceProjection {
  Pose2D,
}

impl InstanceProjection {
  /// Floats per record slot.
  pub fn floats(&self) -> u32 {
    match self {
      InstanceProjection::Pose2D => 5,
    }
  }
}

/// Routes a projection of the node's world transform to slot `index` of a
/// vertex buffer used as an instance buffer: floats [index*stride,
/// (index+1)*stride) where stride is the projection's float count. The
/// bridge between the transform hierarchy and instanced rendering - one
/// node per drawn instance, the draw itself untouched. Writes batch: the
/// flush accumulates every slot into a staging mirror and publishes one
/// coalesced dirty range per buffer, so a thousand nodes moved by one
/// producer step cost one buffer write. A hidden node's slot zeroes
/// (zero scale collapses the instance); so does an unbound or destroyed
/// node's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceRecordSink {
  pub buffer: u64,
  pub index: u32,
  pub projection: InstanceProjection,
}

/// Routes the node's world matrix, post-multiplied by a constant matrix,
/// to row `row` of a float texture: 16 floats (one column-major mat4, the
/// four rgba32f texels of that row) at `values[row*16, (row+1)*16)`. The
/// bridge between the transform hierarchy and matrix palettes a vertex
/// shader texelFetches - a skin's bone palette is joint nodes bound row by
/// row with `post` the joint's inverse bind. Writes batch like instance
/// records: the flush stages every changed row and publishes each dirty
/// texture once, whole. A node carries one texture slot per texture.
///
/// The texture's group may carry an ANCHOR node: published rows are then
/// `inverse(anchorWorld) * nodeWorld * post`, making the palette local to
/// the anchor (a model root keeps its skin palette in model space, so the
/// mesh's own `uModel` still places it). The anchor must be an ANCESTOR of
/// every bound node - only then does an anchor move restage every row (its
/// whole subtree recomputes); this is the consumer's contract, unchecked.
/// Rows update while hidden (a palette feeds a mesh whose own sink handles
/// visibility), and an unbound or destroyed node's row keeps its last
/// value (a zeroed bone matrix would collapse the vertices weighted to it;
/// teardown destroys the texture in the same batch anyway).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSlotSink {
  pub texture: u64,
  pub row: u32,
  pub post: Mat4,
}

/// One palette texture's staging mirror and the sinks feeding it. `values`
/// holds `nodeWorld * post` per row; the anchor inverse applies at publish.
struct PaletteGroup {
  anchor: Option<NodeId>,
  values: Vec<f32>,
  refs: u32,
  dirty: bool,
}

/// One instance buffer's staging mirror and the sinks feeding it.
struct InstanceGroup {
  stride: u32,
  values: Vec<f32>,
  refs: u32,
  /// Dirty float range [lo, hi) into `values`; None = clean.
  dirty: Option<(usize, usize)>,
}

impl InstanceGroup {
  fn mark(&mut self, lo: usize, hi: usize) {
    self.dirty = match self.dirty {
      Some((a, b)) => Some((a.min(lo), b.max(hi))),
      None => Some((lo, hi)),
    };
  }
}

/// The `Pose2D` decomposition of a world matrix (see `InstanceProjection`).
fn pose2d(m: &Mat4) -> [f32; 5] {
  let sx = (m[0] * m[0] + m[1] * m[1]).sqrt();
  let sy = (m[4] * m[4] + m[5] * m[5]).sqrt();
  let mirrored = m[0] * m[5] - m[1] * m[4] < 0.0;
  [m[12], m[13], m[1].atan2(m[0]), sx, if mirrored { -sy } else { sy }]
}

struct Node {
  generation: u32,
  alive: bool,
  parent: Option<u32>,
  children: Vec<u32>,
  position: [f32; 3],
  rotation: [f32; 4],
  scale: [f32; 3],
  local: Mat4,
  world: Mat4,
  /// The local matrix needs recomposing from position/rotation/scale.
  local_dirty: bool,
  /// Queued for the next flush (a transform, visibility or parent change).
  queued: bool,
  /// Recomputed or re-shown by this flush's walk: the cull pass re-tests
  /// every sink (a target's frustum moving re-tests them anyway).
  queued_touch: bool,
  visible: bool,
  /// Effective visibility as of the last flush (every ancestor visible too).
  shown: bool,
  /// One per target.
  sinks: Vec<BoundSink>,
  /// Local-space tight box; with one the node has a leaf in the index.
  bounds: Option<Box3>,
  leaf: Option<u32>,
  /// A local box for culling ONLY (a skeleton joint's influence region):
  /// keeps the node out of the picking index while its world box still
  /// follows the flush. Culling reads this, else `bounds`.
  cull_bounds: Option<Box3>,
  /// The world-axis box of `cull_bounds`/`bounds` as of the last
  /// recompute; None without either.
  world_box: Option<Box3>,
  /// Whether frustums gate this node's draw sinks at all (the per-object
  /// opt-out for geometry a vertex stage moves beyond its box).
  cull: bool,
  /// World units the frustum test widens the box by on every side.
  cull_margin: f32,
  /// Nodes whose world boxes, united, stand in for this node's own in the
  /// frustum test (a skinned part culled by its joints' boxes, so the box
  /// follows the pose). Empty = the node's own box.
  cull_group: Vec<NodeId>,
  /// Nodes whose cull group this node is in: a move here re-tests them.
  cull_owners: Vec<NodeId>,
  /// Triangle data for the picking narrowphase; None = box only.
  shape: Option<ShapeId>,
  /// One per target.
  slots: Vec<SharedSlotSink>,
  /// One per texture.
  texture_slots: Vec<TextureSlotSink>,
  record: Option<InstanceRecordSink>,
  /// The record slot holds this node's shown pose (false = zeroed or
  /// never written; the next shown flush writes it).
  record_on: bool,
}

#[derive(Default)]
pub struct Spatial {
  nodes: Vec<Node>,
  free: Vec<u32>,
  queue: Vec<u32>,
  bvh: Bvh,
  pub(crate) shapes: pick::Shapes,
  shared: HashMap<(u64, String), SharedGroup>,
  instances: HashMap<u64, InstanceGroup>,
  palettes: HashMap<u64, PaletteGroup>,
  transitions: NodeTransitions,
  players: players::PlayerSet,
  /// Per target, the clip volume its draw sinks are gated by (a target
  /// without one never culls).
  frustums: HashMap<u64, Frustum>,
  /// Targets whose frustum changed since the last flush: every sink on
  /// them is re-tested by the cull pass.
  frustum_dirty: Vec<u64>,
  /// Nodes the walk recomputed or re-shown this flush: the cull pass
  /// re-tests their sinks even when no frustum moved.
  touched: Vec<u32>,
}

fn index(id: NodeId) -> usize {
  (id & 0xffff_ffff) as usize
}

fn generation(id: NodeId) -> u32 {
  (id >> 32) as u32
}

impl Spatial {
  pub fn new() -> Self {
    Self::default()
  }

  fn resolve(&self, id: NodeId) -> Result<u32, String> {
    let i = index(id);
    match self.nodes.get(i) {
      Some(n) if n.alive && n.generation == generation(id) => Ok(i as u32),
      _ => Err(format!("spatial node {id} not found")),
    }
  }

  fn enqueue(&mut self, i: u32) {
    let n = &mut self.nodes[i as usize];
    if !n.queued {
      n.queued = true;
      self.queue.push(i);
    }
  }

  /// A new root node with the given local transform; `visible` false hides
  /// its whole subtree. Starts queued, so the first flush computes it.
  pub fn create(&mut self, position: [f32; 3], rotation: [f32; 4], scale: [f32; 3], visible: bool) -> NodeId {
    let fresh = Node {
      generation: 0,
      alive: true,
      parent: None,
      children: Vec::new(),
      position,
      rotation,
      scale,
      local: IDENTITY,
      world: IDENTITY,
      local_dirty: true,
      queued: false,
      queued_touch: false,
      visible,
      shown: false,
      sinks: Vec::new(),
      bounds: None,
      leaf: None,
      cull_bounds: None,
      world_box: None,
      cull: true,
      cull_margin: 0.0,
      cull_group: Vec::new(),
      cull_owners: Vec::new(),
      shape: None,
      slots: Vec::new(),
      texture_slots: Vec::new(),
      record: None,
      record_on: false,
    };
    let i = match self.free.pop() {
      Some(i) => {
        let generation = self.nodes[i as usize].generation.wrapping_add(1);
        self.nodes[i as usize] = Node { generation, ..fresh };
        i
      }
      None => {
        self.nodes.push(fresh);
        (self.nodes.len() - 1) as u32
      }
    };
    self.enqueue(i);
    ((self.nodes[i as usize].generation as u64) << 32) | i as u64
  }

  /// Free a node. Its children become roots (the consumer tears a subtree
  /// down node by node, so they are usually gone in the same batch). Its
  /// draw sinks are dropped without a write: the entries they pointed at
  /// are the consumer's to remove.
  pub fn destroy(&mut self, id: NodeId) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(p) = self.nodes[i as usize].parent {
      self.nodes[p as usize].children.retain(|&c| c != i);
    }
    let children = std::mem::take(&mut self.nodes[i as usize].children);
    for c in children {
      self.nodes[c as usize].parent = None;
      self.enqueue(c);
    }
    if let Some(leaf) = self.nodes[i as usize].leaf.take() {
      self.bvh.remove(leaf);
    }
    for slot in std::mem::take(&mut self.nodes[i as usize].slots) {
      self.release_slot(&slot);
    }
    for slot in std::mem::take(&mut self.nodes[i as usize].texture_slots) {
      self.release_texture_slot(&slot);
    }
    if let Some(record) = self.nodes[i as usize].record.take() {
      self.release_record(&record);
    }
    self.transitions.configs.remove(&id);
    self.transitions.cancel_node(id);
    let n = &mut self.nodes[i as usize];
    n.alive = false;
    n.parent = None;
    n.sinks.clear();
    n.bounds = None;
    n.cull_bounds = None;
    n.world_box = None;
    n.cull_group.clear();
    n.cull_owners.clear();
    n.shape = None;
    self.free.push(i);
    Ok(())
  }

  fn id_of(&self, i: u32) -> NodeId {
    ((self.nodes[i as usize].generation as u64) << 32) | i as u64
  }

  /// Set (or with None clear) the node's local tight box. With a box the
  /// node is in the index: its leaf follows the world matrix through the
  /// flush, and hidden nodes stay in (skipped at query time, so unhiding
  /// never queries a stale box).
  pub fn set_bounds(&mut self, id: NodeId, bounds: Option<Box3>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if bounds.is_none() {
      if let Some(leaf) = self.nodes[i as usize].leaf.take() {
        self.bvh.remove(leaf);
      }
    }
    self.nodes[i as usize].bounds = bounds;
    self.enqueue(i);
    Ok(())
  }

  /// The clip volume gating every draw sink on `target` (None lifts it):
  /// the target's view-projection, column-major. Entries whose node box
  /// (grown by its margin) falls wholly outside it read instance count 0,
  /// exactly like a hidden node, and come back with a fresh params write.
  /// Nodes without a box, or with culling off, are never gated.
  pub fn set_frustum(&mut self, target: u64, view_proj: Option<Mat4>) {
    let changed = match view_proj {
      Some(m) => {
        let f = Frustum::from_view_proj(&m);
        self.frustums.insert(target, f) != Some(f)
      }
      None => self.frustums.remove(&target).is_some(),
    };
    if changed && !self.frustum_dirty.contains(&target) {
      self.frustum_dirty.push(target);
    }
  }

  /// Whether frustums gate the node's draw sinks, and the world-unit
  /// margin the test widens its box by.
  pub fn set_cull(&mut self, id: NodeId, enabled: bool, margin: f32) -> Result<(), String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    n.cull = enabled;
    n.cull_margin = margin;
    self.touch(i);
    Ok(())
  }

  /// Hand the node to the next cull pass without a recompute (its boxes
  /// are current; only the gate changed).
  fn touch(&mut self, i: u32) {
    let n = &mut self.nodes[i as usize];
    if !n.queued_touch && !n.sinks.is_empty() {
      n.queued_touch = true;
      self.touched.push(i);
    }
  }

  /// A local box for culling only - the node stays out of the picking
  /// index (`set_bounds` is the indexed one). None falls back to `bounds`.
  pub fn set_cull_bounds(&mut self, id: NodeId, bounds: Option<Box3>) -> Result<(), String> {
    let i = self.resolve(id)?;
    self.nodes[i as usize].cull_bounds = bounds;
    self.enqueue(i);
    Ok(())
  }

  /// Cull the node by the union of these nodes' world boxes instead of
  /// its own (empty restores its own). A member without a box, or gone,
  /// contributes nothing; with no member box at all the node is not culled.
  pub fn set_cull_group(&mut self, id: NodeId, members: &[NodeId]) -> Result<(), String> {
    let i = self.resolve(id)?;
    let mut indices = Vec::with_capacity(members.len());
    for &m in members {
      indices.push(self.resolve(m)?);
    }
    let old = std::mem::replace(&mut self.nodes[i as usize].cull_group, members.to_vec());
    for m in old {
      if let Ok(j) = self.resolve(m) {
        self.nodes[j as usize].cull_owners.retain(|&o| o != id);
      }
    }
    for j in indices {
      self.nodes[j as usize].cull_owners.push(id);
    }
    self.touch(i);
    Ok(())
  }

  /// The box the frustum test reads for node `i`: its group's union, else
  /// its own world box. None = nothing to test, never culled.
  fn cull_box(&self, i: u32) -> Option<Box3> {
    let n = &self.nodes[i as usize];
    if n.cull_group.is_empty() {
      return n.world_box;
    }
    let mut acc: Option<Box3> = None;
    for &m in &n.cull_group {
      let Ok(j) = self.resolve(m) else {
        continue;
      };
      if let Some(b) = self.nodes[j as usize].world_box {
        acc = Some(match acc {
          Some(a) => union(&a, &b),
          None => b,
        });
      }
    }
    acc
  }

  /// Whether `target`'s frustum lets node `i` draw.
  fn frustum_allows(&self, i: u32, target: u64) -> bool {
    let n = &self.nodes[i as usize];
    if !n.cull {
      return true;
    }
    let Some(f) = self.frustums.get(&target) else {
      return true;
    };
    match self.cull_box(i) {
      Some(b) => f.intersects(&b, n.cull_margin),
      None => true,
    }
  }

  /// The visibility switches, after the walk: every sink of a touched
  /// node, and every sink on a target whose frustum moved, is set to
  /// "shown and inside the frustum". A flip writes the count; a sink
  /// turning on with a stale entry (bound, or moved while off) gets its
  /// params too. A write that does not land releases the sink.
  fn cull_pass(&mut self, out: &mut dyn SinkWriter) {
    let dirty = std::mem::take(&mut self.frustum_dirty);
    let touched = std::mem::take(&mut self.touched);
    let candidates: Vec<u32> = if dirty.is_empty() {
      touched
    } else {
      (0..self.nodes.len() as u32).filter(|&i| self.nodes[i as usize].alive && !self.nodes[i as usize].sinks.is_empty()).collect()
    };
    for i in candidates {
      let n = &self.nodes[i as usize];
      if !n.alive || n.sinks.is_empty() {
        continue;
      }
      let shown = n.shown;
      let world = n.world;
      let every = n.queued_touch;
      let mut sinks = std::mem::take(&mut self.nodes[i as usize].sinks);
      let mut normal: Option<Mat4> = None;
      sinks.retain_mut(|b| {
        let sink = b.sink;
        if !every && !dirty.contains(&sink.target) {
          return true;
        }
        let want = shown && self.frustum_allows(i, sink.target);
        if want == b.entry_on {
          return true;
        }
        b.entry_on = want;
        if !out.write_count(sink.target, sink.draw, if want { sink.count } else { 0 }) {
          return false;
        }
        if want && b.fresh {
          if sink.normal && normal.is_none() {
            normal = Some(normal_matrix(&world));
          }
          b.fresh = false;
          return out.write_params(sink.target, sink.draw, &world, if sink.normal { normal.as_ref() } else { None });
        }
        true
      });
      let n = &mut self.nodes[i as usize];
      n.sinks = sinks;
      n.queued_touch = false;
    }
  }

  /// Attach (or with None detach) triangle data for the narrowphase. The
  /// node still needs bounds to be found at all.
  pub fn set_shape(&mut self, id: NodeId, shape: Option<ShapeId>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(sid) = shape {
      self.shapes.check(sid)?;
    }
    self.nodes[i as usize].shape = shape;
    Ok(())
  }

  /// Bind the node's shared-slot sink on the sink's (target, param name),
  /// replacing the one it had there (the abandoned slot zeroes); a sink
  /// on another param of the same target stays. Binding seeds the slot
  /// at the next flush. The caller flushes afterwards (the JS scheduler
  /// always does).
  pub fn bind_shared_slot(&mut self, id: NodeId, sink: SharedSlotSink) -> Result<(), String> {
    let i = self.resolve(id)?;
    if sink.len == 0 || sink.len % 3 != 0 {
      return Err(format!("shared-slot sink len {} is not a multiple of 3", sink.len));
    }
    if (sink.index + 1) * 3 > sink.len {
      return Err(format!("shared-slot sink slot {} does not fit {} floats", sink.index, sink.len));
    }
    let group = self.shared.entry((sink.target, sink.name.clone())).or_insert_with(|| SharedGroup {
      values: vec![0.0; sink.len as usize],
      refs: 0,
      dirty: false,
    });
    if group.values.len() != sink.len as usize {
      return Err(format!(
        "shared param '{}' on target {} is {} floats, not {}",
        sink.name,
        sink.target,
        group.values.len(),
        sink.len
      ));
    }
    group.refs += 1;
    let slots = &mut self.nodes[i as usize].slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let replaced = s.target == sink.target && s.name == sink.name;
      if replaced {
        released.push(s.clone());
      }
      !replaced
    });
    for slot in &released {
      self.release_slot(slot);
    }
    self.nodes[i as usize].slots.push(sink);
    self.enqueue(i);
    Ok(())
  }

  /// Remove the node's slot sink on `target` (or every slot sink with
  /// None); the abandoned slots zero at the next flush.
  pub fn unbind_shared_slot(&mut self, id: NodeId, target: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let slots = &mut self.nodes[i as usize].slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let keep = target.is_some_and(|t| t != s.target);
      if !keep {
        released.push(s.clone());
      }
      keep
    });
    for slot in &released {
      self.release_slot(slot);
    }
    self.enqueue(i);
    Ok(())
  }

  /// Bind the node's texture slot on the sink's texture, replacing the one
  /// it had there (a re-bind to another row abandons the old row, which
  /// keeps its last value). Every bind on one texture must name the same
  /// `anchor` (one anchor per palette, like one stride per instance
  /// buffer); the first bind sets it. Fit against the actual texture is
  /// the caller's check (Context validates at bind); the staging mirror
  /// grows to the highest bound row. Binding stages the row at the next
  /// flush - the queued node recomputes unconditionally.
  pub fn bind_texture_slot(&mut self, id: NodeId, sink: TextureSlotSink, anchor: Option<NodeId>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(a) = anchor {
      self.resolve(a)?;
    }
    let group = self.palettes.entry(sink.texture).or_insert_with(|| PaletteGroup {
      anchor,
      values: Vec::new(),
      refs: 0,
      dirty: false,
    });
    if group.refs > 0 && group.anchor != anchor {
      return Err(format!(
        "texture {} palette is anchored to {:?}, not {:?} (one anchor per texture)",
        sink.texture, group.anchor, anchor
      ));
    }
    group.anchor = anchor;
    let need = (sink.row as usize + 1) * 16;
    if group.values.len() < need {
      group.values.resize(need, 0.0);
    }
    group.refs += 1;
    let slots = &mut self.nodes[i as usize].texture_slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let replaced = s.texture == sink.texture;
      if replaced {
        released.push(*s);
      }
      !replaced
    });
    for slot in &released {
      self.release_texture_slot(slot);
    }
    self.nodes[i as usize].texture_slots.push(sink);
    self.enqueue(i);
    Ok(())
  }

  /// Remove the node's texture slot on `texture` (or every texture slot
  /// with None); the abandoned rows keep their last value.
  pub fn unbind_texture_slot(&mut self, id: NodeId, texture: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let slots = &mut self.nodes[i as usize].texture_slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let keep = texture.is_some_and(|t| t != s.texture);
      if !keep {
        released.push(*s);
      }
      keep
    });
    for slot in &released {
      self.release_texture_slot(slot);
    }
    Ok(())
  }

  /// Drop one texture slot's claim on its group; the group itself is
  /// dropped at the next flush once unreferenced. The row is NOT zeroed
  /// (see `TextureSlotSink`).
  fn release_texture_slot(&mut self, sink: &TextureSlotSink) {
    if let Some(group) = self.palettes.get_mut(&sink.texture) {
      group.refs = group.refs.saturating_sub(1);
    }
  }

  /// Bind (or with None unbind) the node's instance-record sink. Binding
  /// writes the slot at the next flush; unbinding zeroes it there. Every
  /// sink on one buffer must carry the same projection (one stride per
  /// buffer). Slot fit against the actual GPU buffer is the caller's
  /// check (Context validates at bind); the staging mirror grows to the
  /// highest bound slot.
  pub fn set_instance_record(&mut self, id: NodeId, sink: Option<InstanceRecordSink>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(sink) = &sink {
      let stride = sink.projection.floats();
      let group = self.instances.entry(sink.buffer).or_insert_with(|| InstanceGroup {
        stride,
        values: Vec::new(),
        refs: 0,
        dirty: None,
      });
      if group.stride != stride {
        return Err(format!("instance buffer {} carries {}-float records, not {}", sink.buffer, group.stride, stride));
      }
      let need = (sink.index as usize + 1) * stride as usize;
      if group.values.len() < need {
        group.values.resize(need, 0.0);
      }
      group.refs += 1;
    }
    if let Some(old) = self.nodes[i as usize].record.take() {
      self.release_record(&old);
    }
    let n = &mut self.nodes[i as usize];
    n.record = sink;
    n.record_on = false;
    self.enqueue(i);
    Ok(())
  }

  /// The floats the record sinks on `buffer` occupy (through the highest
  /// bound slot); None when no sink names the buffer. What the caller
  /// checks a retarget destination's size against.
  pub fn records_extent(&self, buffer: u64) -> Option<usize> {
    self.instances.get(&buffer).map(|group| group.values.len())
  }

  /// Move every record sink on buffer `old` to buffer `new`: the staging
  /// mirror moves with them and the whole used range republishes at the
  /// next flush, so a population outgrowing its buffer swaps in a larger
  /// one with ONE call and ONE bulk write instead of a rebind per node.
  /// Slot indices are untouched. The caller validates that `new` exists
  /// and fits (`records_extent`); `new` must not already carry records.
  pub fn retarget_records(&mut self, old: u64, new: u64) -> Result<(), String> {
    if old == new {
      return Ok(());
    }
    if !self.instances.contains_key(&old) {
      return Err(format!("no instance records are bound to buffer {old}"));
    }
    if self.instances.contains_key(&new) {
      return Err(format!("buffer {new} already carries instance records"));
    }
    let mut group = self.instances.remove(&old).expect("source group checked above");
    if !group.values.is_empty() {
      group.dirty = Some((0, group.values.len()));
    }
    self.instances.insert(new, group);
    for n in self.nodes.iter_mut() {
      if let Some(record) = n.record.as_mut() {
        if record.buffer == old {
          record.buffer = new;
        }
      }
    }
    Ok(())
  }

  /// Drop one record sink's claim on its group: the slot zeroes at the
  /// next flush, and the group itself is dropped there once unreferenced.
  fn release_record(&mut self, sink: &InstanceRecordSink) {
    if let Some(group) = self.instances.get_mut(&sink.buffer) {
      let stride = group.stride as usize;
      let at = sink.index as usize * stride;
      if group.values[at..at + stride].iter().any(|&v| v != 0.0) {
        group.values[at..at + stride].fill(0.0);
        group.mark(at, at + stride);
      }
      group.refs = group.refs.saturating_sub(1);
    }
  }

  /// Stage one record slot's fresh values; a no-op when they are unchanged.
  fn stage_record(&mut self, sink: &InstanceRecordSink, values: &[f32]) {
    if let Some(group) = self.instances.get_mut(&sink.buffer) {
      let at = sink.index as usize * group.stride as usize;
      let slot = &mut group.values[at..at + values.len()];
      if slot != values {
        slot.copy_from_slice(values);
        group.mark(at, at + values.len());
      }
    }
  }

  /// Drop one sink's claim on its group: the slot zeroes at the next
  /// flush, and the group itself is dropped there once unreferenced.
  fn release_slot(&mut self, sink: &SharedSlotSink) {
    if let Some(group) = self.shared.get_mut(&(sink.target, sink.name.clone())) {
      let at = sink.index as usize * 3;
      group.values[at..at + 3].fill(0.0);
      group.dirty = true;
      group.refs = group.refs.saturating_sub(1);
    }
  }

  pub fn create_shape(&mut self, shape: Shape) -> Result<ShapeId, String> {
    self.shapes.create(shape)
  }

  pub fn destroy_shape(&mut self, id: ShapeId) -> Result<(), String> {
    self.shapes.destroy(id)
  }

  /// The leaf's tight world box from the local box carried through the
  /// world matrix (the standard AABB-of-a-transformed-AABB construction).
  fn refit_leaf(&mut self, i: u32) {
    let n = &self.nodes[i as usize];
    let Some(b) = n.bounds else {
      return;
    };
    let tight = world_box(&b, &n.world);
    match n.leaf {
      Some(leaf) => self.bvh.update(leaf, &tight),
      None => {
        let leaf = self.bvh.insert(i, &tight);
        self.nodes[i as usize].leaf = Some(leaf);
      }
    }
  }

  /// Longest root-to-leaf path of the index (tests only).
  #[cfg(test)]
  pub(crate) fn index_depth(&self) -> usize {
    self.bvh.depth()
  }

  /// Every shown node with bounds the ray strikes, nearest first. A node
  /// with a shape is tested per triangle (hit carries `face`, `uv` when the
  /// shape has UVs, and the world-space geometric `normal`); without one
  /// its local box is the volume. The first ray reaching a large shape
  /// builds its triangle BVH (see pick.rs), so repeated rays against a
  /// merged scene stay log-cost. `direction` need not be normalized;
  /// distances are world units.
  pub fn raycast(&mut self, origin: [f32; 3], direction: [f32; 3]) -> Vec<Hit> {
    let len = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
    if len == 0.0 {
      return Vec::new();
    }
    let d = [direction[0] / len, direction[1] / len, direction[2] / len];
    let mut candidates = Vec::new();
    self.bvh.raycast(origin, d, &mut |i| candidates.push(i));
    let mut hits = Vec::new();
    for i in candidates {
      let n = &self.nodes[i as usize];
      if !n.alive || !n.shown {
        continue;
      }
      let Some(bounds) = n.bounds else {
        continue;
      };
      let world = n.world;
      // A shape id that no longer resolves (destroyed) falls back to the
      // box, like a node that never had one.
      let shape = n.shape.filter(|&sid| self.shapes.get(sid).is_some());
      // The ray in the node's local frame: an affine map preserves the
      // ray parameter, so with the local direction left unnormalized t
      // stays in world units.
      let inv = invert_affine(&world);
      let lo = transform_point(&inv, origin);
      let ld = transform_vector(&inv, d);
      let found = match shape {
        Some(sid) => self.shapes.ray(sid, lo, ld).map(|(t, face, uv, local_normal)| {
          let nm = normal_matrix(&world);
          let wn = transform_vector(&nm, local_normal);
          let l = (wn[0] * wn[0] + wn[1] * wn[1] + wn[2] * wn[2]).sqrt();
          let mut normal = if l > 0.0 { [wn[0] / l, wn[1] / l, wn[2] / l] } else { wn };
          // Face the ray, whichever side was struck.
          if normal[0] * d[0] + normal[1] * d[1] + normal[2] * d[2] > 0.0 {
            normal = [-normal[0], -normal[1], -normal[2]];
          }
          (t, Some(face), uv, Some(normal))
        }),
        None => ray_box_distance(lo, ld, &bounds).map(|t| (t, None, None, None)),
      };
      if let Some((t, face, uv, normal)) = found {
        hits.push(Hit {
          node: self.id_of(i),
          distance: t,
          point: [origin[0] + d[0] * t, origin[1] + d[1] * t, origin[2] + d[2] * t],
          normal,
          face,
          uv,
        });
      }
    }
    hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
    hits
  }

  /// Every shown node with bounds whose local box, carried through its
  /// world matrix, overlaps the world-axis box `bounds` (touching counts;
  /// a point is min == max). Broadphase through the index, narrowphase by
  /// separating axes (`pick::box_overlap`), so a rotated flat rect tests
  /// exactly, never by its world AABB. Unordered; reads the index as of
  /// the last flush, like `raycast`.
  pub fn overlap(&mut self, bounds: Box3) -> Vec<NodeId> {
    let mut candidates = Vec::new();
    self.bvh.query(&bounds, &mut |i| candidates.push(i));
    let mut out = Vec::new();
    for i in candidates {
      let n = &self.nodes[i as usize];
      if !n.alive || !n.shown {
        continue;
      }
      let Some(local) = n.bounds else {
        continue;
      };
      if pick::box_overlap(&n.world, &local, &bounds) {
        out.push(self.id_of(i));
      }
    }
    out
  }

  /// Re-parent a node (None = make it a root). Errs on a cycle.
  pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let p = match parent {
      Some(pid) => Some(self.resolve(pid)?),
      None => None,
    };
    if let Some(p) = p {
      let mut cursor = Some(p);
      while let Some(c) = cursor {
        if c == i {
          return Err(format!("spatial node {id} cannot be its own ancestor"));
        }
        cursor = self.nodes[c as usize].parent;
      }
    }
    if self.nodes[i as usize].parent == p {
      return Ok(());
    }
    if let Some(old) = self.nodes[i as usize].parent {
      self.nodes[old as usize].children.retain(|&c| c != i);
    }
    self.nodes[i as usize].parent = p;
    if let Some(p) = p {
      self.nodes[p as usize].children.push(i);
    }
    self.enqueue(i);
    Ok(())
  }

  /// Replace the local transform. The consumer compares before calling; an
  /// unchanged write still queues the node.
  pub fn set_transform(
    &mut self,
    id: NodeId,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
  ) -> Result<(), String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    n.position = position;
    n.rotation = rotation;
    n.scale = scale;
    n.local_dirty = true;
    self.enqueue(i);
    Ok(())
  }

  /// Declare (or with None clear) the node's transition config. With one
  /// set, `write_transform` animates instead of snapping. Clearing cancels
  /// the node's running tracks in place: it keeps its mid-flight transform,
  /// no settled events fire, and later writes snap. Replacing a config does
  /// not retroactively affect running tracks (element semantics).
  pub fn set_node_transition(&mut self, id: NodeId, config: Option<NodeTransitionConfig>) -> Result<(), String> {
    self.resolve(id)?;
    match config {
      Some(c) => {
        self.transitions.configs.insert(id, c);
      }
      None => {
        self.transitions.configs.remove(&id);
        self.transitions.cancel_node(id);
      }
    }
    Ok(())
  }

  /// Replace the local transform THROUGH the node's transition declaration:
  /// a component with a declared spec animates toward the written value
  /// (the write is a target), one without snaps. Without a declaration the
  /// whole write snaps, exactly `set_transform`. A component matching its
  /// running track's target (or its resting value) is left alone - the
  /// full-TRS write shape re-sends unchanged components on every call.
  /// Returns whether anything changed (a track started or retargeted, or a
  /// snap moved the node) - the caller's frame-demand signal. A raw
  /// `set_transform` never consults or cancels tracks: a running track
  /// overwrites it at the next advance (last write wins, the producer
  /// rule).
  pub fn write_transform(
    &mut self,
    id: NodeId,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
  ) -> Result<bool, String> {
    let i = self.resolve(id)?;
    let Some(config) = self.transitions.configs.get(&id).copied() else {
      let n = &self.nodes[i as usize];
      if n.position == position && n.rotation == rotation && n.scale == scale {
        return Ok(false);
      }
      self.set_transform(id, position, rotation, scale)?;
      return Ok(true);
    };
    let n = &self.nodes[i as usize];
    let (cur_p, cur_q, cur_s) = (n.position, n.rotation, n.scale);
    let mut animated = false;
    let mut snapped = false;
    match config.entry_for(Component::Position) {
      Some(spec) => animated |= self.transitions.retarget_linear(id, Component::Position, cur_p, position, spec),
      None => {
        if cur_p != position {
          self.nodes[i as usize].position = position;
          snapped = true;
        }
      }
    }
    match config.entry_for(Component::Scale) {
      Some(spec) => animated |= self.transitions.retarget_linear(id, Component::Scale, cur_s, scale, spec),
      None => {
        if cur_s != scale {
          self.nodes[i as usize].scale = scale;
          snapped = true;
        }
      }
    }
    match config.entry_for(Component::Rotation) {
      Some(spec) => animated |= self.transitions.retarget_rotation(id, cur_q, rotation, spec),
      None => {
        if cur_q != rotation {
          self.nodes[i as usize].rotation = rotation;
          snapped = true;
        }
      }
    }
    if snapped {
      let n = &mut self.nodes[i as usize];
      n.local_dirty = true;
      self.enqueue(i);
    }
    Ok(animated || snapped)
  }

  /// Stamp the animation clock (app-time ms, the paced timeline). Stamped
  /// once per frame before any frame work runs, so writes and the advance
  /// agree on time; pause/scale/step semantics ride in with the stamp.
  pub fn set_transition_now(&mut self, now_ms: f64) {
    self.transitions.now_ms = now_ms;
  }

  /// Advance every running track to the stamped clock, writing the
  /// interpolated TRS through the ordinary snap path (nodes queue; the
  /// next flush propagates). Settled tracks land the target exactly and
  /// report via `take_settled_transitions`; tracks of freed nodes drop
  /// silently. Returns whether any track still runs - the embedder's
  /// signal to keep requesting frames. A repeated call at an unchanged
  /// clock (the paused path) writes nothing.
  pub fn advance_transitions(&mut self) -> bool {
    let now = self.transitions.now_ms;
    if self.transitions.is_empty() {
      self.transitions.last_ms = now;
      return false;
    }
    let dt = (now - self.transitions.last_ms).max(0.0);
    self.transitions.last_ms = now;
    let mut linear = std::mem::take(&mut self.transitions.linear);
    linear.retain_mut(|track| {
      let Ok(i) = self.resolve(track.node) else {
        return false;
      };
      let (value, settled) = track.advance(now, dt);
      let n = &mut self.nodes[i as usize];
      let slot = match track.component {
        Component::Position => &mut n.position,
        Component::Scale => &mut n.scale,
        // Rotation tracks live in their own list.
        Component::Rotation => unreachable!("rotation track in the linear list"),
      };
      if *slot != value {
        *slot = value;
        n.local_dirty = true;
        self.enqueue(i);
      }
      if settled {
        self.transitions.settled.push((track.node, track.component));
        return false;
      }
      true
    });
    linear.append(&mut self.transitions.linear);
    self.transitions.linear = linear;
    let mut rotation = std::mem::take(&mut self.transitions.rotation);
    rotation.retain_mut(|track| {
      let Ok(i) = self.resolve(track.node) else {
        return false;
      };
      let (value, settled) = track.advance(now, dt);
      let n = &mut self.nodes[i as usize];
      if n.rotation != value {
        n.rotation = value;
        n.local_dirty = true;
        self.enqueue(i);
      }
      if settled {
        self.transitions.settled.push((track.node, Component::Rotation));
        return false;
      }
      true
    });
    rotation.append(&mut self.transitions.rotation);
    self.transitions.rotation = rotation;
    !self.transitions.is_empty()
  }

  /// The (node, component) pairs whose tracks settled since the last drain
  /// (the onTransitionEnd feed). Cancelled tracks never appear.
  pub fn take_settled_transitions(&mut self) -> Vec<(NodeId, Component)> {
    std::mem::take(&mut self.transitions.settled)
  }

  pub fn set_visible(&mut self, id: NodeId, visible: bool) -> Result<(), String> {
    let i = self.resolve(id)?;
    if self.nodes[i as usize].visible == visible {
      return Ok(());
    }
    self.nodes[i as usize].visible = visible;
    self.enqueue(i);
    Ok(())
  }

  /// Attach the node's draw sink on the sink's target, replacing the one
  /// it had there. A new sink's entry is assumed switched OFF (instance
  /// count 0, how the 3d package adds entries); the next flush turns it on
  /// with a params write if the node is shown. The node re-queues, so its
  /// other sinks get a params rewrite in that flush too (a queued node
  /// recomputes unconditionally, the reparent rule).
  pub fn bind_sink(&mut self, id: NodeId, sink: DrawSink) -> Result<(), String> {
    let i = self.resolve(id)?;
    let sinks = &mut self.nodes[i as usize].sinks;
    sinks.retain(|b| b.sink.target != sink.target);
    sinks.push(BoundSink { sink, entry_on: false, fresh: true });
    self.enqueue(i);
    Ok(())
  }

  /// Remove the node's draw sink on `target` (or every draw sink with
  /// None). Issues no write: the entries are the consumer's to remove.
  pub fn unbind_sink(&mut self, id: NodeId, target: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    self.nodes[i as usize].sinks.retain(|b| target.is_some_and(|t| t != b.sink.target));
    self.enqueue(i);
    Ok(())
  }

  /// Change the "on" count of every draw sink (an instanced mesh's record
  /// count). Entries currently on get the new count through `out` at once;
  /// returns whether any did.
  pub fn set_sink_count(&mut self, id: NodeId, count: u32, out: &mut dyn SinkWriter) -> Result<bool, String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    if n.sinks.is_empty() {
      return Err(format!("spatial node {id} has no draw sink"));
    }
    let mut wrote = false;
    n.sinks.retain_mut(|b| {
      b.sink.count = count;
      if !b.entry_on {
        return true;
      }
      let landed = out.write_count(b.sink.target, b.sink.draw, count);
      wrote |= landed;
      landed
    });
    Ok(wrote)
  }

  /// Effective visibility as of the last flush.
  pub fn shown(&self, id: NodeId) -> Result<bool, String> {
    Ok(self.nodes[self.resolve(id)? as usize].shown)
  }

  /// The node's world matrix as the tree stands NOW, pending writes
  /// included: a dirty chain is composed on the fly without clearing any
  /// flag, so the next flush still sees it. O(depth) when something above
  /// is dirty, a copy otherwise.
  pub fn world(&self, id: NodeId) -> Result<Mat4, String> {
    let i = self.resolve(id)?;
    let mut chain = Vec::new();
    let mut cursor = Some(i);
    let mut top_dirty = None;
    while let Some(c) = cursor {
      let n = &self.nodes[c as usize];
      if n.queued || n.local_dirty {
        top_dirty = Some(chain.len());
      }
      chain.push(c);
      cursor = n.parent;
    }
    let Some(top) = top_dirty else {
      return Ok(self.nodes[i as usize].world);
    };
    let mut world = match self.nodes[chain[top] as usize].parent {
      Some(p) => self.nodes[p as usize].world,
      None => IDENTITY,
    };
    for &c in chain[..=top].iter().rev() {
      let n = &self.nodes[c as usize];
      let local = if n.local_dirty { compose(n.position, n.rotation, n.scale) } else { n.local };
      world = multiply(world, local);
    }
    Ok(world)
  }

  /// Recompute every queued subtree and hand the sink writes to `out`. A
  /// group whose write did not land is dropped with it (see `SinkWriter`);
  /// the slots still naming it stage nothing, so it never writes again.
  pub fn flush(&mut self, out: &mut dyn SinkWriter) {
    if !self.queue.is_empty() {
      let queue = std::mem::take(&mut self.queue);
      for &i in &queue {
        if !self.nodes[i as usize].alive || self.has_queued_ancestor(i) {
          continue;
        }
        let (parent_world, parent_shown) = match self.nodes[i as usize].parent {
          Some(p) => (self.nodes[p as usize].world, self.nodes[p as usize].shown),
          None => (IDENTITY, true),
        };
        self.recompute(i, &parent_world, false, parent_shown, out);
      }
      for &i in &queue {
        self.nodes[i as usize].queued = false;
      }
    }
    if !self.touched.is_empty() || !self.frustum_dirty.is_empty() {
      self.cull_pass(out);
    }
    // Shared params changed by the walk, an unbind or a destroy go out
    // once per flush, whole; a group nothing references any more goes
    // with its last write.
    self.shared.retain(|(target, name), group| {
      let mut landed = true;
      if group.dirty {
        group.dirty = false;
        landed = out.write_shared(*target, name, &group.values);
      }
      landed && group.refs > 0
    });
    // Instance staging publishes the same way: one coalesced dirty range
    // per buffer per flush, a group dropping with its last write.
    self.instances.retain(|buffer, group| {
      let mut landed = true;
      if let Some((lo, hi)) = group.dirty.take() {
        landed = out.write_instances(*buffer, lo as u32, hi as u32, &group.values);
      }
      landed && group.refs > 0
    });
    // Palette staging publishes whole per texture. Anchored rows relativize
    // here, at the flush's end: the anchor's world is fresh (an anchor move
    // restaged every row, since it is an ancestor of every bound node), and
    // one inverse covers the palette. A dead anchor falls back to identity
    // (the consumer tears joints down before their model root, so live
    // sinks never see it).
    if self.palettes.values().any(|g| g.dirty || g.refs == 0) {
      let mut palettes = std::mem::take(&mut self.palettes);
      let mut scratch: Vec<f32> = Vec::new();
      palettes.retain(|texture, group| {
        let mut landed = true;
        if group.dirty {
          group.dirty = false;
          landed = match group.anchor {
            None => out.write_texture(*texture, &group.values),
            Some(a) => {
              let inv = match self.resolve(a) {
                Ok(i) => invert_affine(&self.nodes[i as usize].world),
                Err(_) => IDENTITY,
              };
              scratch.clear();
              for row in group.values.chunks(16) {
                let m: Mat4 = row.try_into().expect("palette rows are 16 floats");
                scratch.extend_from_slice(&multiply(inv, m));
              }
              out.write_texture(*texture, &scratch)
            }
          };
        }
        landed && group.refs > 0
      });
      self.palettes = palettes;
    }
  }

  fn has_queued_ancestor(&self, i: u32) -> bool {
    let mut cursor = self.nodes[i as usize].parent;
    while let Some(c) = cursor {
      if self.nodes[c as usize].queued {
        return true;
      }
      cursor = self.nodes[c as usize].parent;
    }
    false
  }

  fn recompute(
    &mut self,
    i: u32,
    parent_world: &Mat4,
    parent_changed: bool,
    parent_shown: bool,
    out: &mut dyn SinkWriter,
  ) {
    let n = &mut self.nodes[i as usize];
    let mut changed = parent_changed;
    if n.local_dirty {
      n.local = compose(n.position, n.rotation, n.scale);
      n.local_dirty = false;
      changed = true;
    }
    // A queued node recomputes unconditionally: a re-parent or a sink
    // change leaves the matrices alone but the subtree still has to be
    // revisited (a new parent means a new world matrix).
    if changed || n.queued {
      n.world = multiply(*parent_world, n.local);
      changed = true;
    }
    let shown = parent_shown && n.visible;
    let touched = changed || shown != n.shown || n.queued;
    n.shown = shown;
    let refit = n.bounds.is_some() && (changed || n.leaf.is_none());
    if changed || n.world_box.is_none() {
      n.world_box = n.cull_bounds.or(n.bounds).map(|b| world_box(&b, &n.world));
    }

    if changed {
      // Disjoint borrows: the node's slots read, the shared groups written.
      let shared = &mut self.shared;
      for slot in &n.slots {
        let v = match slot.projection {
          Projection::Direction(local) => {
            let v = transform_vector(&n.world, local);
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 0.0 {
              [v[0] / l, v[1] / l, v[2] / l]
            } else {
              [0.0; 3]
            }
          }
          Projection::Position => [n.world[12], n.world[13], n.world[14]],
        };
        let at = slot.index as usize * 3;
        if let Some(group) = shared.get_mut(&(slot.target, slot.name.clone())) {
          if group.values[at..at + 3] != v {
            group.values[at..at + 3].copy_from_slice(&v);
            group.dirty = true;
          }
        }
      }
      let palettes = &mut self.palettes;
      for slot in &n.texture_slots {
        if let Some(group) = palettes.get_mut(&slot.texture) {
          let m = multiply(n.world, slot.post);
          let at = slot.row as usize * 16;
          if group.values[at..at + 16] != m {
            group.values[at..at + 16].copy_from_slice(&m);
            group.dirty = true;
          }
        }
      }
    }
    let world = self.nodes[i as usize].world;
    if changed && !self.nodes[i as usize].cull_owners.is_empty() {
      // A group member moved: the owners re-test against the fresh union.
      let owners = self.nodes[i as usize].cull_owners.clone();
      for o in owners {
        if let Ok(j) = self.resolve(o) {
          self.touch(j);
        }
      }
    }
    if touched && !self.nodes[i as usize].sinks.is_empty() && !self.nodes[i as usize].queued_touch {
      self.nodes[i as usize].queued_touch = true;
      self.touched.push(i);
    }
    // The inverse-transpose is one matrix however many sinks ask for it.
    let mut normal: Option<Mat4> = None;
    // Entries that are on AND staying on get their fresh matrix here; the
    // switch itself (visibility and the frustum) is the cull pass's, after
    // the walk, so a group's boxes are all current when it is decided. An
    // entry that is off, or about to go off, remembers it owes a params
    // write for when it turns on (a group member not yet recomputed can
    // make the test here say off while the pass says on: the owed write
    // covers that). A sink whose entry is gone (a write that did not
    // land) is released here: the entry was the consumer's to remove, and
    // it did.
    let mut k = 0;
    while k < self.nodes[i as usize].sinks.len() {
      let b = self.nodes[i as usize].sinks[k];
      let staying_on = shown && b.entry_on && self.frustum_allows(i, b.sink.target);
      let sinks = &mut self.nodes[i as usize].sinks;
      if staying_on && (changed || b.fresh) {
        if b.sink.normal && normal.is_none() {
          normal = Some(normal_matrix(&world));
        }
        sinks[k].fresh = false;
        if !out.write_params(b.sink.target, b.sink.draw, &world, if b.sink.normal { normal.as_ref() } else { None }) {
          sinks.remove(k);
          continue;
        }
      } else if changed {
        sinks[k].fresh = true;
      }
      k += 1;
    }
    let record = self.nodes[i as usize].record;
    let record_on = self.nodes[i as usize].record_on;
    if let Some(rec) = record {
      match rec.projection {
        InstanceProjection::Pose2D => {
          if shown && (changed || !record_on) {
            self.stage_record(&rec, &pose2d(&world));
            self.nodes[i as usize].record_on = true;
          } else if !shown && record_on {
            self.stage_record(&rec, &[0.0; 5]);
            self.nodes[i as usize].record_on = false;
          }
        }
      }
    }
    if refit {
      self.refit_leaf(i);
    }
    let mut k = 0;
    while k < self.nodes[i as usize].children.len() {
      let c = self.nodes[i as usize].children[k];
      self.recompute(c, &world, changed, shown, out);
      k += 1;
    }
  }
}
