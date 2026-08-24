// The picking narrowphase: triangle shapes and the ray/triangle test. A
// shape is one CPU copy of a geometry's positions (and optionally UVs)
// and indices, shared by every node referencing it, so the memory cost is
// one copy per distinct geometry, not per node.

use super::{transform_point, Box3, Mat4, NodeId};

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
  /// World-space geometric normal facing the ray; None for a box hit.
  pub normal: Option<[f32; 3]>,
  /// Triangle index into the shape's index list; None for a box hit.
  pub face: Option<u32>,
  /// Interpolated texture UV; None without a shape or shape UVs.
  pub uv: Option<[f32; 2]>,
}

#[derive(Default)]
pub(crate) struct Shapes {
  slots: Vec<(u32, Option<Shape>)>,
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
        slot.0 = slot.0.wrapping_add(1);
        slot.1 = Some(shape);
        i
      }
      None => {
        self.slots.push((0, Some(shape)));
        (self.slots.len() - 1) as u32
      }
    };
    Ok(((self.slots[i as usize].0 as u64) << 32) | i as u64)
  }

  fn index(&self, id: ShapeId) -> Option<u32> {
    let i = (id & 0xffff_ffff) as usize;
    match self.slots.get(i) {
      Some((gen, Some(_))) if *gen == (id >> 32) as u32 => Some(i as u32),
      _ => None,
    }
  }

  pub fn check(&self, id: ShapeId) -> Result<(), String> {
    self.index(id).map(|_| ()).ok_or_else(|| format!("spatial shape {id} not found"))
  }

  pub fn get(&self, id: ShapeId) -> Option<&Shape> {
    self.index(id).and_then(|i| self.slots[i as usize].1.as_ref())
  }

  pub fn destroy(&mut self, id: ShapeId) -> Result<(), String> {
    let i = self.index(id).ok_or_else(|| format!("spatial shape {id} not found"))?;
    self.slots[i as usize].1 = None;
    self.free.push(i);
    Ok(())
  }
}

/// Nearest triangle of `shape` along the local-space ray (Moller-Trumbore,
/// both sides): (t, face, uv, unnormalized local normal), or None.
pub fn ray_shape(shape: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<(f32, u32, Option<[f32; 2]>, [f32; 3])> {
  let p = &shape.positions;
  let at = |i: u32| -> [f32; 3] {
    let k = i as usize * 3;
    [p[k], p[k + 1], p[k + 2]]
  };
  let mut best: Option<(f32, u32, f32, f32, [f32; 3])> = None;
  for (face, tri) in shape.indices.chunks_exact(3).enumerate() {
    let a = at(tri[0]);
    let b = at(tri[1]);
    let c = at(tri[2]);
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-12 {
      continue;
    }
    let inv = 1.0 / det;
    let s = [o[0] - a[0], o[1] - a[1], o[2] - a[2]];
    let u = inv * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
      continue;
    }
    let q = cross(s, e1);
    let v = inv * dot(d, q);
    if v < 0.0 || u + v > 1.0 {
      continue;
    }
    let t = inv * dot(e2, q);
    if t < 0.0 {
      continue;
    }
    if best.is_none_or(|(bt, ..)| t < bt) {
      best = Some((t, face as u32, u, v, cross(e1, e2)));
    }
  }
  let (t, face, u, v, normal) = best?;
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
  Some((t, face, uv, normal))
}

/// Does the node's LOCAL box, carried through its world matrix, overlap
/// the world-axis box `query` (touching counts)? Separating axes of both
/// boxes - the three world axes and the three transformed local axes
/// (kept unnormalized, which stays valid under scale and shear). Exact
/// for a rotated flat rect, the 2d picking case; a genuinely 3D pair can
/// only err conservative on the edge-edge cross axes this skips.
pub fn box_overlap(m: &Mat4, local: &Box3, query: &Box3) -> bool {
  let lc = [(local[0] + local[3]) / 2.0, (local[1] + local[4]) / 2.0, (local[2] + local[5]) / 2.0];
  let le = [(local[3] - local[0]) / 2.0, (local[4] - local[1]) / 2.0, (local[5] - local[2]) / 2.0];
  let qc = [(query[0] + query[3]) / 2.0, (query[1] + query[4]) / 2.0, (query[2] + query[5]) / 2.0];
  let qe = [(query[3] - query[0]) / 2.0, (query[4] - query[1]) / 2.0, (query[5] - query[2]) / 2.0];
  let c = transform_point(m, lc);
  let d = [c[0] - qc[0], c[1] - qc[1], c[2] - qc[2]];
  let u = |j: usize| -> [f32; 3] { [m[j * 4], m[j * 4 + 1], m[j * 4 + 2]] };
  for i in 0..3 {
    let reach = qe[i] + (0..3).map(|j| le[j] * u(j)[i].abs()).sum::<f32>();
    if d[i].abs() > reach {
      return false;
    }
  }
  for j in 0..3 {
    let a = u(j);
    let reach = (0..3).map(|k| le[k] * dot(u(k), a).abs()).sum::<f32>()
      + qe[0] * a[0].abs()
      + qe[1] * a[1].abs()
      + qe[2] * a[2].abs();
    if dot(d, a).abs() > reach {
      return false;
    }
  }
  true
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
