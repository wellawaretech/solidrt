// The picking narrowphase: triangle shapes and the ray/triangle test. A
// shape is one CPU copy of a geometry's positions (and optionally UVs)
// and indices, shared by every node referencing it, so the memory cost is
// one copy per distinct geometry, not per node. A large shape gets a
// static triangle BVH, built lazily by the first ray that reaches it, so
// a merged static scene (the batching advice) raycasts at log cost
// instead of linear in its triangles.

use super::bvh::ray_box_distance;
use super::{Box3, NodeId};

/// Triangle count below which a shape stays brute-force: a handful of
/// triangles tests faster flat than through a tree, and skipping the
/// build keeps prop-heavy scenes from growing thousands of tiny indices.
pub const BVH_MIN_TRIANGLES: usize = 64;

/// Most triangles a BVH leaf holds: smaller leaves prune better but grow
/// the tree; 8 balances box tests against triangle tests.
const LEAF_TRIANGLES: usize = 8;

/// Determinant magnitude below which a triangle is edge-on to the ray
/// and skipped (Moller-Trumbore's division would blow up).
const RAY_DET_EPSILON: f32 = 1e-12;

/// Generation-tagged like NodeId; a destroyed shape's id never resolves.
pub type ShapeId = u64;

pub struct Shape {
  /// xyz per vertex.
  pub positions: Vec<f32>,
  /// uv per vertex, same vertex count, or None.
  pub uvs: Option<Vec<f32>>,
  /// Triangle list, three indices per face.
  pub indices: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
  pub node: NodeId,
  /// World units along the (normalized) ray.
  pub distance: f32,
  pub point: [f32; 3],
  /// World-space geometric normal facing the ray: the triangle's, or
  /// the struck face's for a node tested by its box.
  pub normal: [f32; 3],
  /// Triangle index into the shape's index list; None for a box hit.
  pub face: Option<u32>,
  /// Interpolated texture UV; None without a shape or shape UVs.
  pub uv: Option<[f32; 2]>,
}

struct Slot {
  generation: u32,
  shape: Option<Shape>,
  /// The shape's triangle BVH, built by the first ray that reaches it;
  /// None until then, and forever for shapes under BVH_MIN_TRIANGLES.
  index: Option<TriBvh>,
}

#[derive(Default)]
pub(crate) struct Shapes {
  slots: Vec<Slot>,
  free: Vec<u32>,
}

impl Shapes {
  pub fn create(&mut self, shape: Shape) -> Result<ShapeId, String> {
    let count = shape.positions.len() / 3;
    if shape.positions.len() % 3 != 0 {
      return Err("shape positions must be xyz triples".to_string());
    }
    if let Some(uvs) = &shape.uvs {
      if uvs.len() != count * 2 {
        return Err(format!("shape uvs must be {} floats (2 per vertex), got {}", count * 2, uvs.len()));
      }
    }
    if shape.indices.len() % 3 != 0 {
      return Err("shape indices must be a triangle list (a multiple of 3)".to_string());
    }
    if let Some(bad) = shape.indices.iter().find(|&&i| i as usize >= count) {
      return Err(format!("shape index {bad} out of range for {count} vertices"));
    }
    let i = match self.free.pop() {
      Some(i) => {
        let slot = &mut self.slots[i as usize];
        slot.generation = slot.generation.wrapping_add(1);
        slot.shape = Some(shape);
        slot.index = None;
        i
      }
      None => {
        self.slots.push(Slot { generation: 0, shape: Some(shape), index: None });
        (self.slots.len() - 1) as u32
      }
    };
    Ok(((self.slots[i as usize].generation as u64) << 32) | i as u64)
  }

  fn index(&self, id: ShapeId) -> Option<u32> {
    let i = (id & 0xffff_ffff) as usize;
    match self.slots.get(i) {
      Some(slot) if slot.shape.is_some() && slot.generation == (id >> 32) as u32 => Some(i as u32),
      _ => None,
    }
  }

  pub fn check(&self, id: ShapeId) -> Result<(), String> {
    self.index(id).map(|_| ()).ok_or_else(|| format!("spatial shape {id} not found"))
  }

  pub fn get(&self, id: ShapeId) -> Option<&Shape> {
    self.index(id).and_then(|i| self.slots[i as usize].shape.as_ref())
  }

  pub fn destroy(&mut self, id: ShapeId) -> Result<(), String> {
    let i = self.index(id).ok_or_else(|| format!("spatial shape {id} not found"))?;
    self.slots[i as usize].shape = None;
    self.slots[i as usize].index = None;
    self.free.push(i);
    Ok(())
  }

  /// The shape with its triangle BVH, which the first query against a
  /// shape of BVH_MIN_TRIANGLES or more builds here; smaller shapes stay
  /// flat (None), since testing every triangle beats the traversal at
  /// that size. None for an unresolvable id.
  fn indexed(&mut self, id: ShapeId) -> Option<(&Shape, Option<&TriBvh>)> {
    let i = self.index(id)? as usize;
    let slot = &mut self.slots[i];
    let faces = slot.shape.as_ref()?.indices.len() / 3;
    if slot.index.is_none() && faces >= BVH_MIN_TRIANGLES {
      slot.index = Some(TriBvh::build(slot.shape.as_ref()?));
    }
    Some((slot.shape.as_ref()?, slot.index.as_ref()))
  }

  /// The narrowphase for one shape: its nearest triangle along the
  /// local-space ray, through the shape's BVH when it has one. Same
  /// result as `ray_shape`, or None for a miss or an unresolvable id.
  pub fn ray(&mut self, id: ShapeId, o: [f32; 3], d: [f32; 3]) -> Option<(f32, u32, Option<[f32; 2]>, [f32; 3])> {
    let (shape, index) = self.indexed(id)?;
    let raw = match index {
      Some(bvh) => bvh.ray(shape, o, d),
      None => ray_all(shape, o, d),
    }?;
    Some(finish(shape, raw))
  }

  /// Hand `visit` every triangle (local-space vertices) whose bounds
  /// touch the local box `b`, through the shape's BVH when it has one;
  /// a flat shape hands over all of them. Nothing for an unresolvable id.
  pub fn visit_box(&mut self, id: ShapeId, b: &Box3, visit: &mut dyn FnMut([[f32; 3]; 3])) {
    let Some((shape, index)) = self.indexed(id) else {
      return;
    };
    match index {
      Some(bvh) => bvh.visit_box(shape, b, visit),
      None => {
        for tri in shape.indices.chunks_exact(3) {
          visit(vertices(shape, tri));
        }
      }
    }
  }
}

/// A triangle's three vertices.
fn vertices(shape: &Shape, tri: &[u32]) -> [[f32; 3]; 3] {
  let at = |i: u32| -> [f32; 3] {
    let k = i as usize * 3;
    [shape.positions[k], shape.positions[k + 1], shape.positions[k + 2]]
  };
  [at(tri[0]), at(tri[1]), at(tri[2])]
}

/// t, face, barycentric u/v, unnormalized local normal - the narrowphase
/// result before UV interpolation.
type RawHit = (f32, u32, f32, f32, [f32; 3]);

/// One triangle of a shape against the local-space ray: (t, u, v,
/// unnormalized local normal), or None.
fn ray_triangle(positions: &[f32], tri: &[u32], o: [f32; 3], d: [f32; 3]) -> Option<(f32, f32, f32, [f32; 3])> {
  let at = |i: u32| -> [f32; 3] {
    let k = i as usize * 3;
    [positions[k], positions[k + 1], positions[k + 2]]
  };
  ray_points(at(tri[0]), at(tri[1]), at(tri[2]), o, d)
}

/// Three points against the ray (Moller-Trumbore, both sides): (t, u, v,
/// unnormalized normal), or None.
pub(super) fn ray_points(
  a: [f32; 3],
  b: [f32; 3],
  c: [f32; 3],
  o: [f32; 3],
  d: [f32; 3],
) -> Option<(f32, f32, f32, [f32; 3])> {
  let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
  let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
  let h = cross(d, e2);
  let det = dot(e1, h);
  if det.abs() < RAY_DET_EPSILON {
    return None;
  }
  let inv = 1.0 / det;
  let s = [o[0] - a[0], o[1] - a[1], o[2] - a[2]];
  let u = inv * dot(s, h);
  if !(0.0..=1.0).contains(&u) {
    return None;
  }
  let q = cross(s, e1);
  let v = inv * dot(d, q);
  if v < 0.0 || u + v > 1.0 {
    return None;
  }
  let t = inv * dot(e2, q);
  if t < 0.0 {
    return None;
  }
  Some((t, u, v, cross(e1, e2)))
}

/// Every triangle tested flat, nearest kept.
fn ray_all(shape: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<RawHit> {
  let mut best: Option<RawHit> = None;
  for (face, tri) in shape.indices.chunks_exact(3).enumerate() {
    if let Some((t, u, v, n)) = ray_triangle(&shape.positions, tri, o, d) {
      if best.is_none_or(|(bt, ..)| t < bt) {
        best = Some((t, face as u32, u, v, n));
      }
    }
  }
  best
}

/// Interpolate the hit's UV from its barycentrics, when the shape has UVs.
fn finish(shape: &Shape, (t, face, u, v, normal): RawHit) -> (f32, u32, Option<[f32; 2]>, [f32; 3]) {
  let uv = shape.uvs.as_ref().map(|uvs| {
    let tri = &shape.indices[face as usize * 3..face as usize * 3 + 3];
    let at = |i: u32| -> [f32; 2] {
      let k = i as usize * 2;
      [uvs[k], uvs[k + 1]]
    };
    let (a, b, c) = (at(tri[0]), at(tri[1]), at(tri[2]));
    let w = 1.0 - u - v;
    [w * a[0] + u * b[0] + v * c[0], w * a[1] + u * b[1] + v * c[1]]
  });
  (t, face, uv, normal)
}

/// Nearest triangle of `shape` along the local-space ray, every triangle
/// tested: (t, face, uv, unnormalized local normal), or None. The oracle
/// the indexed path is tested against (production small-shape rays go
/// through `Shapes::ray`, which calls `ray_all` directly).
#[cfg(test)]
pub fn ray_shape(shape: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<(f32, u32, Option<[f32; 2]>, [f32; 3])> {
  ray_all(shape, o, d).map(|raw| finish(shape, raw))
}

/// Stack capacity for BVH traversal: a median-split tree halves its face
/// count per level, so 2^32 faces are at most 32 levels deep, and the
/// walk holds one deferred child per level.
const TRAVERSAL_STACK: usize = 48;

/// A static triangle BVH over one shape. Shapes are immutable, so the
/// tree is built once - median split on the widest centroid axis, flat
/// node storage - and never refit (the scene's dynamic `Bvh` in bvh.rs is
/// a different animal: fat boxes, insert/remove, rotations). `faces` is
/// the face-id list permuted so every leaf owns a contiguous run.
struct TriBvh {
  nodes: Vec<TriNode>,
  faces: Vec<u32>,
}

struct TriNode {
  bounds: Box3,
  /// A leaf's first face in `faces` (`count` > 0), or the left child's
  /// node index (`count` == 0; the right child is `link + 1`).
  link: u32,
  count: u32,
}

const EMPTY_NODE: TriNode = TriNode { bounds: [0.0; 6], link: 0, count: 0 };

impl TriBvh {
  fn build(shape: &Shape) -> TriBvh {
    let count = shape.indices.len() / 3;
    // Per-face bounds and centroids, temporaries for the build.
    let mut boxes: Vec<Box3> = Vec::with_capacity(count);
    let mut centers: Vec<[f32; 3]> = Vec::with_capacity(count);
    for tri in shape.indices.chunks_exact(3) {
      let mut b = [f32::INFINITY, f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
      for &i in tri {
        let k = i as usize * 3;
        for a in 0..3 {
          b[a] = b[a].min(shape.positions[k + a]);
          b[a + 3] = b[a + 3].max(shape.positions[k + a]);
        }
      }
      centers.push([(b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0]);
      boxes.push(b);
    }
    let mut faces: Vec<u32> = (0..count as u32).collect();
    let mut nodes = vec![EMPTY_NODE];
    fill(&mut nodes, &mut faces, &boxes, &centers, 0, 0, count);
    TriBvh { nodes, faces }
  }

  /// Nearest triangle along the local ray: nearest child walked first,
  /// the far one deferred; a subtree the ray misses, or whose box starts
  /// past the best hit so far, is pruned whole.
  fn ray(&self, shape: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<RawHit> {
    ray_box_distance(o, d, &self.nodes[0].bounds)?;
    let mut best: Option<RawHit> = None;
    let mut stack = [0u32; TRAVERSAL_STACK];
    stack[0] = 0;
    let mut top = 1;
    while top > 0 {
      top -= 1;
      let node = &self.nodes[stack[top] as usize];
      if node.count > 0 {
        for &face in &self.faces[node.link as usize..(node.link + node.count) as usize] {
          let tri = &shape.indices[face as usize * 3..face as usize * 3 + 3];
          if let Some((t, u, v, n)) = ray_triangle(&shape.positions, tri, o, d) {
            if best.is_none_or(|(bt, ..)| t < bt) {
              best = Some((t, face, u, v, n));
            }
          }
        }
        continue;
      }
      let cut = best.map(|(t, ..)| t).unwrap_or(f32::INFINITY);
      let l = node.link as usize;
      let mut near = (ray_box_distance(o, d, &self.nodes[l].bounds), l as u32);
      let mut far = (ray_box_distance(o, d, &self.nodes[l + 1].bounds), (l + 1) as u32);
      if match (near.0, far.0) {
        (Some(a), Some(b)) => b < a,
        (None, _) => true,
        _ => false,
      } {
        std::mem::swap(&mut near, &mut far);
      }
      for (entry, child) in [far, near] {
        if entry.is_some_and(|t| t < cut) {
          debug_assert!(top < TRAVERSAL_STACK, "triangle BVH deeper than its traversal stack");
          stack[top] = child;
          top += 1;
        }
      }
    }
    best
  }

  /// Every triangle in a leaf whose box touches `b` (touching counts).
  fn visit_box(&self, shape: &Shape, b: &Box3, visit: &mut dyn FnMut([[f32; 3]; 3])) {
    let mut stack = [0u32; TRAVERSAL_STACK];
    stack[0] = 0;
    let mut top = 1;
    while top > 0 {
      top -= 1;
      let node = &self.nodes[stack[top] as usize];
      let nb = &node.bounds;
      if nb[0] > b[3] || nb[1] > b[4] || nb[2] > b[5] || nb[3] < b[0] || nb[4] < b[1] || nb[5] < b[2] {
        continue;
      }
      if node.count > 0 {
        for &face in &self.faces[node.link as usize..(node.link + node.count) as usize] {
          visit(vertices(shape, &shape.indices[face as usize * 3..face as usize * 3 + 3]));
        }
        continue;
      }
      debug_assert!(top + 1 < TRAVERSAL_STACK, "triangle BVH deeper than its traversal stack");
      stack[top] = node.link;
      stack[top + 1] = node.link + 1;
      top += 2;
    }
  }
}

/// Fill `node` with the faces in `faces[lo..hi]`: a leaf when the run is
/// small enough, else a median split on the widest centroid axis with the
/// two children pushed adjacently (the `link`/`link + 1` layout).
fn fill(nodes: &mut Vec<TriNode>, faces: &mut [u32], boxes: &[Box3], centers: &[[f32; 3]], node: usize, lo: usize, hi: usize) {
  let mut bounds = [f32::INFINITY, f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
  for &f in &faces[lo..hi] {
    let b = &boxes[f as usize];
    for a in 0..3 {
      bounds[a] = bounds[a].min(b[a]);
      bounds[a + 3] = bounds[a + 3].max(b[a + 3]);
    }
  }
  nodes[node].bounds = bounds;
  if hi - lo <= LEAF_TRIANGLES {
    nodes[node].link = lo as u32;
    nodes[node].count = (hi - lo) as u32;
    return;
  }
  let mut cmin = [f32::INFINITY; 3];
  let mut cmax = [f32::NEG_INFINITY; 3];
  for &f in &faces[lo..hi] {
    let c = &centers[f as usize];
    for a in 0..3 {
      cmin[a] = cmin[a].min(c[a]);
      cmax[a] = cmax[a].max(c[a]);
    }
  }
  let ext = [cmax[0] - cmin[0], cmax[1] - cmin[1], cmax[2] - cmin[2]];
  let axis = if ext[0] >= ext[1] && ext[0] >= ext[2] {
    0
  } else if ext[1] >= ext[2] {
    1
  } else {
    2
  };
  // The median by count keeps the tree balanced whatever the geometry
  // (equal centroids just split arbitrarily, which is fine).
  let mid = (hi - lo) / 2;
  faces[lo..hi].select_nth_unstable_by(mid, |a, b| {
    centers[*a as usize][axis].partial_cmp(&centers[*b as usize][axis]).unwrap_or(std::cmp::Ordering::Equal)
  });
  let left = nodes.len();
  nodes.push(EMPTY_NODE);
  nodes.push(EMPTY_NODE);
  nodes[node].link = left as u32;
  nodes[node].count = 0;
  fill(nodes, faces, boxes, centers, left, lo, lo + mid);
  fill(nodes, faces, boxes, centers, left + 1, lo + mid, hi);
}

pub(super) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub(super) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
