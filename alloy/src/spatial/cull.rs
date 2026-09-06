// Frustum culling's geometry: a target's six clip planes, extracted from
// its view-projection (Gribb/Hartmann - the rows of the matrix summed with
// and subtracted from the last), and the world-box test against them.
// Nothing here knows what a camera is: a matrix comes in, planes come out,
// and a box either has a corner on the inside of every plane or it does
// not. A world box is the same conservative AABB the picking index keeps
// (the local box's center and half extents carried through the world
// matrix), so a rotated node culls by its enclosing axis box - Godot's and
// Unity's test, coarser than Three's sphere only for long thin shapes.

use super::bvh::Box3;
use super::math::transform_point;
use super::Mat4;

/// Six planes as (a, b, c, d): inside is `a*x + b*y + c*z + d >= 0`.
/// Normalized, so a distance along a plane normal is in world units and a
/// cull margin can widen the test by that much.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
  planes: [[f32; 4]; 6],
}

impl Frustum {
  /// The clip volume of a view-projection matrix (column-major, the GL
  /// convention: clip space -1..1 on every axis) as world-space planes.
  pub fn from_view_proj(m: &Mat4) -> Frustum {
    let row = |r: usize| [m[r], m[4 + r], m[8 + r], m[12 + r]];
    let r0 = row(0);
    let r1 = row(1);
    let r2 = row(2);
    let r3 = row(3);
    let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
    let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    let mut planes = [add(r3, r0), sub(r3, r0), add(r3, r1), sub(r3, r1), add(r3, r2), sub(r3, r2)];
    for p in planes.iter_mut() {
      let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
      if l > 0.0 {
        for v in p.iter_mut() {
          *v /= l;
        }
      }
    }
    Frustum { planes }
  }

  /// Whether the world-axis box, grown by `margin` on every side, reaches
  /// into the volume. The p-vertex test: for each plane the box corner
  /// farthest along the plane normal is the one that decides, and a box
  /// wholly behind any single plane is out. Boxes straddling two planes
  /// beyond a frustum corner pass (the classic false positive), which only
  /// costs a draw, never a missing one.
  pub fn intersects(&self, b: &Box3, margin: f32) -> bool {
    for p in &self.planes {
      let x = if p[0] >= 0.0 { b[3] } else { b[0] };
      let y = if p[1] >= 0.0 { b[4] } else { b[1] };
      let z = if p[2] >= 0.0 { b[5] } else { b[2] };
      if p[0] * x + p[1] * y + p[2] * z + p[3] < -margin {
        return false;
      }
    }
    true
  }
}

/// The world-axis box enclosing a local box carried through a world
/// matrix: the center transformed, the half extents projected onto each
/// world axis by the matrix's absolute rotation-scale.
pub fn world_box(bounds: &Box3, m: &Mat4) -> Box3 {
  let c = [(bounds[0] + bounds[3]) / 2.0, (bounds[1] + bounds[4]) / 2.0, (bounds[2] + bounds[5]) / 2.0];
  let e = [(bounds[3] - bounds[0]) / 2.0, (bounds[4] - bounds[1]) / 2.0, (bounds[5] - bounds[2]) / 2.0];
  let w = transform_point(m, c);
  let r = [
    m[0].abs() * e[0] + m[4].abs() * e[1] + m[8].abs() * e[2],
    m[1].abs() * e[0] + m[5].abs() * e[1] + m[9].abs() * e[2],
    m[2].abs() * e[0] + m[6].abs() * e[1] + m[10].abs() * e[2],
  ];
  [w[0] - r[0], w[1] - r[1], w[2] - r[2], w[0] + r[0], w[1] + r[1], w[2] + r[2]]
}

/// The box enclosing both.
pub fn union(a: &Box3, b: &Box3) -> Box3 {
  [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2]), a[3].max(b[3]), a[4].max(b[4]), a[5].max(b[5])]
}
