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
/// cull margin can widen the test by that much. The planes are in the
/// target's ORIGIN space (the view-projection they came from is relative
/// to `origin`, see `Spatial::set_origin`); a world box is shifted there
/// before the test, in f64, so a frustum a million units out tests as
/// sharply as one at zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
  planes: [[f64; 4]; 6],
  origin: [f64; 3],
}

impl Frustum {
  /// The clip volume of a view-projection matrix (column-major, the GL
  /// convention: clip space -1..1 on every axis) as planes in the space
  /// the matrix maps from: the target's origin space.
  pub fn from_view_proj(m: &Mat4, origin: [f64; 3]) -> Frustum {
    let row = |r: usize| [m[r], m[4 + r], m[8 + r], m[12 + r]];
    let r0 = row(0);
    let r1 = row(1);
    let r2 = row(2);
    let r3 = row(3);
    let add = |a: [f64; 4], b: [f64; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
    let sub = |a: [f64; 4], b: [f64; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    let mut planes = [add(r3, r0), sub(r3, r0), add(r3, r1), sub(r3, r1), add(r3, r2), sub(r3, r2)];
    for p in planes.iter_mut() {
      let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
      if l > 0.0 {
        for v in p.iter_mut() {
          *v /= l;
        }
      }
    }
    Frustum { planes, origin }
  }

  /// Whether the world-axis box, grown by `margin` on every side, reaches
  /// into the volume. The p-vertex test: for each plane the box corner
  /// farthest along the plane normal is the one that decides, and a box
  /// wholly behind any single plane is out. Boxes straddling two planes
  /// beyond a frustum corner pass (the classic false positive), which only
  /// costs a draw, never a missing one.
  pub fn intersects(&self, b: &Box3, margin: f64) -> bool {
    let b = shift_box(b, self.origin);
    self.planes.iter().all(|p| reach(p, &b) >= -margin)
  }

  /// The residency gate's grid test: whether the box reaches strictly
  /// INSIDE the side and near planes pushed out by `margin`. The far
  /// plane is left out (a draw distance; the gate's depth reach is the
  /// margin itself, see `residency`), and touching a pushed-out plane at
  /// a face does not count, so a camera aligned to a grid never loads the
  /// ring just beyond its edge. Touching is read with `TOUCH_EPS` of the
  /// box's size: the exact zero the rule names is a rounding coin flip
  /// for a viewport edge that sits on a grid line.
  pub fn reaches_sides(&self, b: &Box3, margin: f64) -> bool {
    let b = shift_box(b, self.origin);
    let size = (b[3] - b[0]).max(b[4] - b[1]).max(b[5] - b[2]).max(0.0);
    let eps = size * TOUCH_EPS;
    self.planes[..FAR_PLANE].iter().all(|p| reach(p, &b) > -margin + eps)
  }
}

/// A world box taken relative to `origin`.
pub fn shift_box(b: &Box3, origin: [f64; 3]) -> Box3 {
  [b[0] - origin[0], b[1] - origin[1], b[2] - origin[2], b[3] - origin[0], b[4] - origin[1], b[5] - origin[2]]
}

/// How far inside a pushed-out plane a box's face may lie, as a fraction
/// of the box's largest extent, and still count as touching rather than
/// reaching (see `Frustum::reaches_sides`): far above the rounding of an
/// unprojected corner, far below any distance a gate margin means.
const TOUCH_EPS: f64 = 1e-9;

/// The index of the far plane in `Frustum::planes` (left, right, bottom,
/// top, near, far - the order `from_view_proj` extracts them in).
const FAR_PLANE: usize = 5;

/// How far the box's corner farthest along the plane normal lies inside
/// the plane (negative: the whole box is behind it).
fn reach(p: &[f64; 4], b: &Box3) -> f64 {
  let x = if p[0] >= 0.0 { b[3] } else { b[0] };
  let y = if p[1] >= 0.0 { b[4] } else { b[1] };
  let z = if p[2] >= 0.0 { b[5] } else { b[2] };
  p[0] * x + p[1] * y + p[2] * z + p[3]
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
