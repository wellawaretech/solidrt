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

pub use bvh::{ray_box_distance, Box3};
pub use math::{compose, invert_affine, multiply, normal_matrix, transform_point, transform_vector, IDENTITY};
pub use pick::{Hit, Shape, ShapeId};

use bvh::Bvh;

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

/// One write a flush produces, in flush order.
#[derive(Clone, Debug, PartialEq)]
pub enum SinkWrite {
  Params { target: u64, draw: u64, model: Mat4, normal: Option<Mat4> },
  Count { target: u64, draw: u64, count: u32 },
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
}

#[derive(Default)]
pub struct Spatial {
  nodes: Vec<Node>,
  free: Vec<u32>,
  queue: Vec<u32>,
  bvh: Bvh,
  pub(crate) shapes: pick::Shapes,
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
  /// the entry is currently on, the new count is written at once - the
  /// caller applies the returned write.
  pub fn set_sink_count(&mut self, id: NodeId, count: u32) -> Result<Option<SinkWrite>, String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    let Some(sink) = n.sink.as_mut() else {
      return Err(format!("spatial node {id} has no sink"));
    };
    sink.count = count;
    if n.entry_on {
      return Ok(Some(SinkWrite::Count { target: sink.target, draw: sink.draw, count }));
    }
    Ok(None)
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

  /// Recompute every queued subtree and hand the sink writes to `emit`.
  pub fn flush(&mut self, emit: &mut dyn FnMut(SinkWrite)) {
    if self.queue.is_empty() {
      return;
    }
    let queue = std::mem::take(&mut self.queue);
    for &i in &queue {
      if !self.nodes[i as usize].alive || self.has_queued_ancestor(i) {
        continue;
      }
      let (parent_world, parent_shown) = match self.nodes[i as usize].parent {
        Some(p) => (self.nodes[p as usize].world, self.nodes[p as usize].shown),
        None => (IDENTITY, true),
      };
      self.recompute(i, &parent_world, false, parent_shown, emit);
    }
    for &i in &queue {
      self.nodes[i as usize].queued = false;
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
    emit: &mut dyn FnMut(SinkWrite),
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
    if let Some(sink) = n.sink {
      if shown != n.entry_on {
        n.entry_on = shown;
        emit(SinkWrite::Count { target: sink.target, draw: sink.draw, count: if shown { sink.count } else { 0 } });
        if shown {
          n.fresh = true;
        }
      }
      if shown && (changed || n.fresh) {
        let normal = if sink.normal { Some(normal_matrix(&n.world)) } else { None };
        emit(SinkWrite::Params { target: sink.target, draw: sink.draw, model: n.world, normal });
        n.fresh = false;
      } else if changed {
        n.fresh = true;
      }
    }
    let world = n.world;
    if refit {
      self.refit_leaf(i);
    }
    let mut k = 0;
    while k < self.nodes[i as usize].children.len() {
      let c = self.nodes[i as usize].children[k];
      self.recompute(c, &world, changed, shown, emit);
      k += 1;
    }
  }
}
