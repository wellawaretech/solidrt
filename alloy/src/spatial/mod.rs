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
mod math;
mod pick;
mod transitions;

use std::collections::HashMap;

pub use bvh::{ray_box_distance, Box3};
pub use math::{compose, invert_affine, multiply, normal_matrix, transform_point, transform_vector, IDENTITY};
pub use pick::{Hit, Shape, ShapeId};
pub use transitions::{Component, NodeTransitionConfig};

use bvh::Bvh;
use transitions::NodeTransitions;

pub type Mat4 = [f32; 16];

/// A stable node handle: arena index in the low 32 bits, generation in the
/// high 32 - a destroyed node's id never resolves again.
pub type NodeId = u64;

/// Where a node's fresh world matrix goes: the `uModel` (+ `uNormal`) params
/// of one draw entry, plus the entry's instance count as its visibility
/// switch (`count` is what "shown" restores: 1 for a plain mesh, the record
/// count for an instanced one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawSink {
  pub target: u64,
  pub draw: u64,
  pub normal: bool,
  pub count: u32,
}

/// The consumer of sink writes, one method per write kind, called in flush
/// order. The core's entire output contract: everything a flush produces
/// goes through this trait, and the core never sees where it lands (alloy's
/// Context resolves the ids against its draw entries and forwards down the
/// raster channel; tests record). Arguments are borrowed from core state -
/// an implementation copies what it keeps.
pub trait SinkWriter {
  /// A shown entry's fresh world transform: `uModel`, plus `uNormal` when
  /// the sink asked for it.
  fn write_params(&mut self, target: u64, draw: u64, model: &Mat4, normal: Option<&Mat4>);
  /// An entry's instance count - the visibility switch (0 = hidden, the
  /// sink's count = shown).
  fn write_count(&mut self, target: u64, draw: u64, count: u32);
  /// A shared-slot group's array param, rewritten whole (slot sinks share
  /// one array value; see `SharedSlotSink`).
  fn write_shared(&mut self, target: u64, name: &str, values: &[f32]);
  /// A coalesced run of instance-record floats: `values` lands at float
  /// offset `first` of vertex buffer `buffer`. At most one write per
  /// buffer per flush, however many nodes moved (see `InstanceRecordSink`).
  fn write_instances(&mut self, buffer: u64, first: u32, values: &[f32]);
}

/// How a shared-slot sink projects the node's world transform into its
/// three floats. `Direction` is `normalize(worldRotation * v)` (zeros for
/// a degenerate result); a world-position projection is the anticipated
/// sibling when a consumer arrives.
#[derive(Clone, Debug, PartialEq)]
pub enum Projection {
  /// The world direction of this LOCAL vector.
  Direction([f32; 3]),
}

/// Routes a projection of the node's world transform to one vec3 slot of
/// a target shared param: floats [index*3, index*3+3) of the `len`-float
/// array param `name`, shared by every sink naming it - the whole array
/// is one param value, re-sent when any slot changes, absent slots zero.
/// The generic form of "a scene's light directions follow the node tree":
/// the consumer picks the param name and packs non-spatial data (colors,
/// counts) itself - core never learns what the slots mean.
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
  visible: bool,
  /// Effective visibility as of the last flush (every ancestor visible too).
  shown: bool,
  sink: Option<DrawSink>,
  /// The sink's entry is switched on (instance count = `sink.count`).
  entry_on: bool,
  /// The sink owes a params write at the next shown flush: a new or
  /// re-bound sink, or a move while hidden.
  fresh: bool,
  /// Local-space tight box; with one the node has a leaf in the index.
  bounds: Option<Box3>,
  leaf: Option<u32>,
  /// Triangle data for the picking narrowphase; None = box only.
  shape: Option<ShapeId>,
  slot: Option<SharedSlotSink>,
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
  transitions: NodeTransitions,
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
      visible,
      shown: false,
      sink: None,
      entry_on: false,
      fresh: false,
      bounds: None,
      leaf: None,
      shape: None,
      slot: None,
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
  /// down node by node, so they are usually gone in the same batch). A sink
  /// on it is dropped without a write: the entry it pointed at is the
  /// consumer's to remove.
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
    if let Some(slot) = self.nodes[i as usize].slot.take() {
      self.release_slot(&slot);
    }
    if let Some(record) = self.nodes[i as usize].record.take() {
      self.release_record(&record);
    }
    self.transitions.configs.remove(&id);
    self.transitions.cancel_node(id);
    let n = &mut self.nodes[i as usize];
    n.alive = false;
    n.parent = None;
    n.sink = None;
    n.bounds = None;
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

  /// Bind (or with None unbind) the node's shared-slot sink. Binding
  /// seeds the slot at the next flush; unbinding zeroes it there. The
  /// caller flushes afterwards (the JS scheduler always does).
  pub fn set_shared_slot(&mut self, id: NodeId, sink: Option<SharedSlotSink>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(sink) = &sink {
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
    }
    if let Some(old) = self.nodes[i as usize].slot.take() {
      self.release_slot(&old);
    }
    self.nodes[i as usize].slot = sink;
    self.enqueue(i);
    Ok(())
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
      let group = self
        .instances
        .entry(sink.buffer)
        .or_insert_with(|| InstanceGroup { stride, values: Vec::new(), refs: 0, dirty: None });
      if group.stride != stride {
        return Err(format!(
          "instance buffer {} carries {}-float records, not {}",
          sink.buffer, group.stride, stride
        ));
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
    let m = &n.world;
    let c = [(b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0];
    let e = [(b[3] - b[0]) / 2.0, (b[4] - b[1]) / 2.0, (b[5] - b[2]) / 2.0];
    let w = transform_point(m, c);
    let r = [
      m[0].abs() * e[0] + m[4].abs() * e[1] + m[8].abs() * e[2],
      m[1].abs() * e[0] + m[5].abs() * e[1] + m[9].abs() * e[2],
      m[2].abs() * e[0] + m[6].abs() * e[1] + m[10].abs() * e[2],
    ];
    let tight = [w[0] - r[0], w[1] - r[1], w[2] - r[2], w[0] + r[0], w[1] + r[1], w[2] + r[2]];
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
  /// its local box is the volume. `direction` need not be normalized;
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
      // The ray in the node's local frame: an affine map preserves the
      // ray parameter, so with the local direction left unnormalized t
      // stays in world units.
      let inv = invert_affine(&n.world);
      let lo = transform_point(&inv, origin);
      let ld = transform_vector(&inv, d);
      let shape = n.shape.and_then(|sid| self.shapes.get(sid));
      let found = match shape {
        Some(shape) => pick::ray_shape(shape, lo, ld).map(|(t, face, uv, local_normal)| {
          let nm = normal_matrix(&n.world);
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

  /// Attach (or replace, or with None remove) the node's draw sink. A new
  /// sink's entry is assumed switched OFF (instance count 0, how the 3d
  /// package adds entries); the next flush turns it on with a params write
  /// if the node is shown. Removing a sink issues no write.
  pub fn set_sink(&mut self, id: NodeId, sink: Option<DrawSink>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    n.sink = sink;
    n.entry_on = false;
    n.fresh = sink.is_some();
    self.enqueue(i);
    Ok(())
  }

  /// Change the sink's "on" count (an instanced mesh's record count). If
  /// the entry is currently on, the new count goes to `out` at once;
  /// returns whether it did.
  pub fn set_sink_count(&mut self, id: NodeId, count: u32, out: &mut dyn SinkWriter) -> Result<bool, String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    let Some(sink) = n.sink.as_mut() else {
      return Err(format!("spatial node {id} has no sink"));
    };
    sink.count = count;
    if n.entry_on {
      out.write_count(sink.target, sink.draw, count);
      return Ok(true);
    }
    Ok(false)
  }

  pub fn sink(&self, id: NodeId) -> Result<Option<DrawSink>, String> {
    Ok(self.nodes[self.resolve(id)? as usize].sink)
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

  /// Recompute every queued subtree and hand the sink writes to `out`.
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
    // Shared params changed by the walk, an unbind or a destroy go out
    // once per flush, whole; a group nothing references any more goes
    // with its last write.
    self.shared.retain(|(target, name), group| {
      if group.dirty {
        group.dirty = false;
        out.write_shared(*target, name, &group.values);
      }
      group.refs > 0
    });
    // Instance staging publishes the same way: one coalesced dirty range
    // per buffer per flush, a group dropping with its last write.
    self.instances.retain(|buffer, group| {
      if let Some((lo, hi)) = group.dirty.take() {
        out.write_instances(*buffer, lo as u32, &group.values[lo..hi]);
      }
      group.refs > 0
    });
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
    n.shown = shown;
    let refit = n.bounds.is_some() && (changed || n.leaf.is_none());
    if changed {
      if let Some(slot) = n.slot.clone() {
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
        };
        let at = slot.index as usize * 3;
        if let Some(group) = self.shared.get_mut(&(slot.target, slot.name)) {
          if group.values[at..at + 3] != v {
            group.values[at..at + 3].copy_from_slice(&v);
            group.dirty = true;
          }
        }
      }
    }
    let n = &mut self.nodes[i as usize];
    if let Some(sink) = n.sink {
      if shown != n.entry_on {
        n.entry_on = shown;
        out.write_count(sink.target, sink.draw, if shown { sink.count } else { 0 });
        if shown {
          n.fresh = true;
        }
      }
      if shown && (changed || n.fresh) {
        let normal = if sink.normal { Some(normal_matrix(&n.world)) } else { None };
        out.write_params(sink.target, sink.draw, &n.world, normal.as_ref());
        n.fresh = false;
      } else if changed {
        n.fresh = true;
      }
    }
    let record = n.record;
    let record_on = n.record_on;
    let world = n.world;
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
