// Volume queries on the index: what a capsule or box overlaps, and where
// a moving one first touches. The narrowphase runs in WORLD space against
// triangles carried through their node's matrix - a query volume pulled
// into a node's local frame would distort under non-uniform scale, which
// a ray survives and a sphere does not - so a node's shape BVH is only
// walked with the query's local-space box. Both sweeps are exact: the
// capsule sweep is a sphere sweep against the triangle extruded along the
// capsule's segment (the PhysX construction), the box sweep a separating-
// axis test with the box's interval moving in time. No iteration, no
// tolerance to tune.

use super::math::rotate_vector;
use super::pick::{cross, dot};
use super::{Box3, NodeId};

/// Squared length below which a segment is a point and a separating axis
/// (an edge/edge cross product) is degenerate; the point's sphere and
/// the parallel pair's other axes cover what the skipped test would.
const DEGENERATE_EPSILON: f32 = 1e-12;

type V3 = [f32; 3];

/// A world-space query volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volume {
  /// Every point within `radius` of the segment a-b; a sphere when a == b.
  Capsule { a: V3, b: V3, radius: f32 },
  /// An oriented box: `half` extents along the axes of the unit
  /// quaternion `rotation` (xyzw) about `center`.
  Box { center: V3, half: V3, rotation: [f32; 4] },
}

/// One node overlapped by a volume: its deepest contact.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlap {
  pub node: NodeId,
  /// World-space point on the node's surface.
  pub point: V3,
  /// Unit direction out of the node: the volume moved `depth` along it
  /// clears the contact.
  pub normal: V3,
  pub depth: f32,
}

/// A moving volume's first touch of one node.
#[derive(Clone, Debug, PartialEq)]
pub struct Impact {
  pub node: NodeId,
  /// Fraction of the motion (0..=1); 0 for a volume already in contact
  /// and moving in.
  pub time: f32,
  /// World-space touch point on the node's surface.
  pub point: V3,
  /// Unit normal at the touch, facing the volume (its dot with the
  /// motion is negative).
  pub normal: V3,
}

/// A contact against one triangle: (point on the triangle, normal out of
/// the triangle, depth).
pub(crate) type Contact = (V3, V3, f32);

/// A volume prepared for a query: the box with its axes unpacked.
pub(crate) enum Query {
  Capsule { a: V3, b: V3, radius: f32 },
  Box(Obb),
}

pub(crate) struct Obb {
  center: V3,
  half: V3,
  axes: [V3; 3],
}

impl Query {
  pub fn new(volume: &Volume) -> Query {
    match *volume {
      Volume::Capsule { a, b, radius } => Query::Capsule { a, b, radius },
      Volume::Box { center, half, rotation } => Query::Box(Obb {
        center,
        half,
        axes: [
          rotate_vector(rotation, [1.0, 0.0, 0.0]),
          rotate_vector(rotation, [0.0, 1.0, 0.0]),
          rotate_vector(rotation, [0.0, 0.0, 1.0]),
        ],
      }),
    }
  }

  /// The world box holding the volume, and the whole of its motion.
  pub fn bounds(&self, motion: Option<V3>) -> Box3 {
    let (lo, hi) = match self {
      Query::Capsule { a, b, radius } => (
        [a[0].min(b[0]) - radius, a[1].min(b[1]) - radius, a[2].min(b[2]) - radius],
        [a[0].max(b[0]) + radius, a[1].max(b[1]) + radius, a[2].max(b[2]) + radius],
      ),
      Query::Box(obb) => {
        let mut r = [0.0; 3];
        for i in 0..3 {
          r[i] = (0..3).map(|k| obb.half[k] * obb.axes[k][i].abs()).sum();
        }
        (sub(obb.center, r), add(obb.center, r))
      }
    };
    let mut b = [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]];
    if let Some(m) = motion {
      for i in 0..3 {
        b[i] = b[i].min(b[i] + m[i]);
        b[i + 3] = b[i + 3].max(b[i + 3] + m[i]);
      }
    }
    b
  }

  /// The contact with one world-space triangle (touching counts, depth
  /// 0), or None. A degenerate (zero-area) triangle never contacts.
  pub fn overlap_triangle(&self, tri: &[V3; 3]) -> Option<Contact> {
    let tn = triangle_normal(tri)?;
    match self {
      Query::Capsule { a, b, radius } => capsule_contact(*a, *b, *radius, tri, tn),
      Query::Box(obb) => {
        let (local, _) = obb.localize(tri, [0.0; 3]);
        let sat = sat(&local, obb.half, [0.0; 3])?;
        let (axis, normal, depth) = sat.deepest;
        let world = obb.world_dir(normal);
        Some((obb.contact_point(axis, world, tri, [0.0; 3]), world, depth))
      }
    }
  }

  /// The first touch of the volume moved by `motion` with one world-space
  /// triangle: (time, point, normal), or None. A volume already in
  /// contact reports time 0 only while the motion closes in; leaving or
  /// sliding along the contact is no hit, which is what lets a slide
  /// along a wall proceed.
  pub fn sweep_triangle(&self, motion: V3, tri: &[V3; 3]) -> Option<(f32, V3, V3)> {
    let tn = triangle_normal(tri)?;
    match self {
      Query::Capsule { a, b, radius } => {
        if let Some((point, normal, _)) = capsule_contact(*a, *b, *radius, tri, tn) {
          return (dot(motion, normal) < 0.0).then_some((0.0, point, normal));
        }
        let center = scale(add(*a, *b), 0.5);
        let h = scale(sub(*b, *a), 0.5);
        let t = prism_sweep(center, motion, *radius, tri, h)?;
        let at = add(*a, scale(motion, t));
        let bt = add(*b, scale(motion, t));
        let (ps, pt, d) = segment_triangle(at, bt, tri);
        let normal = if d > 0.0 { scale(sub(ps, pt), 1.0 / d) } else { against(tn, motion) };
        Some((t, pt, normal))
      }
      Query::Box(obb) => {
        let (local, lm) = obb.localize(tri, motion);
        let sat = sat(&local, obb.half, lm)?;
        let (axis, normal, t) = match sat.entry {
          Some((axis, normal, t)) if t > 0.0 => (axis, normal, t),
          _ => {
            let (axis, normal, _) = sat.deepest;
            if dot(normal, lm) >= 0.0 {
              return None;
            }
            (axis, normal, 0.0)
          }
        };
        let world = obb.world_dir(normal);
        Some((t, obb.contact_point(axis, world, tri, scale(motion, t)), world))
      }
    }
  }
}

/// The capsule's contact with a triangle: the closest pair when apart,
/// else the triangle plane's normal toward the segment with the depth
/// that lifts the deeper endpoint clear.
fn capsule_contact(a: V3, b: V3, radius: f32, tri: &[V3; 3], tn: V3) -> Option<Contact> {
  let (ps, pt, dist) = segment_triangle(a, b, tri);
  if dist > radius {
    return None;
  }
  if dist > 0.0 {
    return Some((pt, scale(sub(ps, pt), 1.0 / dist), radius - dist));
  }
  let mid = scale(add(a, b), 0.5);
  let n = if dot(tn, sub(mid, tri[0])) < 0.0 { neg(tn) } else { tn };
  let depth = radius - dot(n, sub(a, tri[0])).min(dot(n, sub(b, tri[0])));
  Some((pt, n, depth))
}

/// The unit normal of a triangle, or None for a degenerate one.
fn triangle_normal(tri: &[V3; 3]) -> Option<V3> {
  let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
  let l2 = dot(n, n);
  (l2 >= DEGENERATE_EPSILON).then(|| scale(n, 1.0 / l2.sqrt()))
}

/// `n` or its negation, whichever opposes `motion`.
fn against(n: V3, motion: V3) -> V3 {
  if dot(n, motion) > 0.0 {
    neg(n)
  } else {
    n
  }
}

/// Closest points between the segment a-b and the triangle: (on the
/// segment, on the triangle, distance). A segment piercing the triangle
/// reports the pierce point twice at distance 0; otherwise the closest
/// pair lies at a segment endpoint or on a triangle edge, so it is the
/// nearest of those five candidates.
pub(crate) fn segment_triangle(a: V3, b: V3, tri: &[V3; 3]) -> (V3, V3, f32) {
  if let Some(p) = segment_pierce(a, sub(b, a), tri) {
    return (p, p, 0.0);
  }
  let mut best = {
    let q = closest_point_triangle(a, tri);
    (a, q, dist2(a, q))
  };
  let mut consider = |p: V3, q: V3| {
    let d = dist2(p, q);
    if d < best.2 {
      best = (p, q, d);
    }
  };
  consider(b, closest_point_triangle(b, tri));
  for i in 0..3 {
    let (p, q) = closest_segment_segment(a, b, tri[i], tri[(i + 1) % 3]);
    consider(p, q);
  }
  (best.0, best.1, best.2.sqrt())
}

/// Where the segment o + s d (s in 0..=1) crosses the triangle
/// (Moller-Trumbore, both sides), or None.
fn segment_pierce(o: V3, d: V3, tri: &[V3; 3]) -> Option<V3> {
  let e1 = sub(tri[1], tri[0]);
  let e2 = sub(tri[2], tri[0]);
  let h = cross(d, e2);
  let det = dot(e1, h);
  if det.abs() < DEGENERATE_EPSILON {
    return None;
  }
  let inv = 1.0 / det;
  let s = sub(o, tri[0]);
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
  (0.0..=1.0).contains(&t).then(|| add(o, scale(d, t)))
}

/// The point of the triangle nearest `p` (Ericson, Real-Time Collision
/// Detection 5.1.5: the Voronoi region of each feature in turn).
fn closest_point_triangle(p: V3, tri: &[V3; 3]) -> V3 {
  let [a, b, c] = *tri;
  let ab = sub(b, a);
  let ac = sub(c, a);
  let ap = sub(p, a);
  let d1 = dot(ab, ap);
  let d2 = dot(ac, ap);
  if d1 <= 0.0 && d2 <= 0.0 {
    return a;
  }
  let bp = sub(p, b);
  let d3 = dot(ab, bp);
  let d4 = dot(ac, bp);
  if d3 >= 0.0 && d4 <= d3 {
    return b;
  }
  let vc = d1 * d4 - d3 * d2;
  if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
    return add(a, scale(ab, d1 / (d1 - d3)));
  }
  let cp = sub(p, c);
  let d5 = dot(ab, cp);
  let d6 = dot(ac, cp);
  if d6 >= 0.0 && d5 <= d6 {
    return c;
  }
  let vb = d5 * d2 - d1 * d6;
  if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
    return add(a, scale(ac, d2 / (d2 - d6)));
  }
  let va = d3 * d6 - d5 * d4;
  if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
    return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
  }
  let denom = 1.0 / (va + vb + vc);
  add(add(a, scale(ab, vb * denom)), scale(ac, vc * denom))
}

/// Closest points between the segments p1-q1 and p2-q2 (Ericson 5.1.9).
fn closest_segment_segment(p1: V3, q1: V3, p2: V3, q2: V3) -> (V3, V3) {
  let d1 = sub(q1, p1);
  let d2 = sub(q2, p2);
  let r = sub(p1, p2);
  let a = dot(d1, d1);
  let e = dot(d2, d2);
  let f = dot(d2, r);
  let (s, t);
  if a <= DEGENERATE_EPSILON && e <= DEGENERATE_EPSILON {
    return (p1, p2);
  }
  if a <= DEGENERATE_EPSILON {
    s = 0.0;
    t = (f / e).clamp(0.0, 1.0);
  } else {
    let c = dot(d1, r);
    if e <= DEGENERATE_EPSILON {
      t = 0.0;
      s = (-c / a).clamp(0.0, 1.0);
    } else {
      let b = dot(d1, d2);
      let denom = a * e - b * b;
      let s0 = if denom != 0.0 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
      let t0 = (b * s0 + f) / e;
      if t0 < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
      } else if t0 > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
      } else {
        s = s0;
        t = t0;
      }
    }
  }
  (add(p1, scale(d1, s)), add(p2, scale(d2, t)))
}

/// Time of first touch of a sphere (`center` moving by `motion`, radius
/// `r`) against the triangle extruded by +-`h` - the capsule's segment
/// swept over the triangle - or None within the motion. The sphere starts
/// outside (the caller checked), so its center enters the prism grown by
/// `r` through the nearest of: a face's offset plane (hit inside the
/// face), an edge's cylinder, or a vertex's sphere. Every face is tried
/// from both sides: a flat prism (the segment in the triangle plane, or a
/// sphere's zero segment) has no outside, and for a solid one the
/// wrong-side plane can only score behind the true entry.
fn prism_sweep(center: V3, motion: V3, r: f32, tri: &[V3; 3], h: V3) -> Option<f32> {
  let top = [add(tri[0], h), add(tri[1], h), add(tri[2], h)];
  let bottom = [sub(tri[0], h), sub(tri[1], h), sub(tri[2], h)];
  let mut best = f32::INFINITY;
  let mut keep = |t: Option<f32>| {
    if let Some(t) = t {
      best = best.min(t);
    }
  };
  keep(face_sweep(center, motion, r, &top));
  keep(face_sweep(center, motion, r, &bottom));
  for i in 0..3 {
    let j = (i + 1) % 3;
    keep(face_sweep(center, motion, r, &[top[i], top[j], bottom[j], bottom[i]]));
    keep(edge_sweep(center, motion, r, top[i], top[j]));
    keep(edge_sweep(center, motion, r, bottom[i], bottom[j]));
    keep(edge_sweep(center, motion, r, top[i], bottom[i]));
    keep(vertex_sweep(center, motion, r, top[i]));
    keep(vertex_sweep(center, motion, r, bottom[i]));
  }
  (best <= 1.0).then_some(best)
}

/// The sphere's entry through either offset plane of a convex polygon,
/// landing inside it.
fn face_sweep(c: V3, m: V3, r: f32, poly: &[V3]) -> Option<f32> {
  let wn = cross(sub(poly[1], poly[0]), sub(poly[2], poly[0]));
  let l2 = dot(wn, wn);
  if l2 < DEGENERATE_EPSILON {
    return None;
  }
  let unit = scale(wn, 1.0 / l2.sqrt());
  let mut best: Option<f32> = None;
  for n in [unit, neg(unit)] {
    let denom = dot(n, m);
    if denom >= 0.0 {
      continue;
    }
    let t = (dot(n, poly[0]) + r - dot(n, c)) / denom;
    if !(0.0..=1.0).contains(&t) {
      continue;
    }
    let p = sub(add(c, scale(m, t)), scale(n, r));
    let inside = (0..poly.len()).all(|i| {
      let q = poly[(i + 1) % poly.len()];
      dot(cross(sub(q, poly[i]), sub(p, poly[i])), wn) >= 0.0
    });
    if inside && best.is_none_or(|b| t < b) {
      best = Some(t);
    }
  }
  best
}

/// The sphere's entry into the finite cylinder of radius `r` around the
/// edge p-q.
fn edge_sweep(c: V3, m: V3, r: f32, p: V3, q: V3) -> Option<f32> {
  let d = sub(q, p);
  let len2 = dot(d, d);
  if len2 < DEGENERATE_EPSILON {
    return None;
  }
  let w = sub(c, p);
  let dm = dot(m, d) / len2;
  let dw = dot(w, d) / len2;
  let t = quadratic_entry(sub(m, scale(d, dm)), sub(w, scale(d, dw)), r)?;
  let s = dw + dm * t;
  (0.0..=1.0).contains(&s).then_some(t)
}

/// The sphere's entry into the sphere of radius `r` at `v`.
fn vertex_sweep(c: V3, m: V3, r: f32, v: V3) -> Option<f32> {
  quadratic_entry(m, sub(c, v), r)
}

/// First t in 0..=1 with |w + t m| == r, the entry of a point moving by
/// `m` from offset `w` into a sphere of radius `r` (a ray parallel to a
/// cylinder axis arrives here with a zero `m`, and is no entry).
fn quadratic_entry(m: V3, w: V3, r: f32) -> Option<f32> {
  let a = dot(m, m);
  if a < DEGENERATE_EPSILON {
    return None;
  }
  let b = 2.0 * dot(w, m);
  let c = dot(w, w) - r * r;
  let disc = b * b - 4.0 * a * c;
  if disc < 0.0 {
    return None;
  }
  let t = (-b - disc.sqrt()) / (2.0 * a);
  (0.0..=1.0).contains(&t).then_some(t)
}

/// A separating-axis result for one triangle in the box's frame.
struct Sat {
  /// The least-penetration axis of the pose at time 0: (axis index,
  /// unit normal into the box, depth).
  deepest: (usize, V3, f32),
  /// The moving box's first touch: (axis index, unit normal into the
  /// box, time). None when the intervals never meet within the motion.
  entry: Option<(usize, V3, f32)>,
}

/// The box-frame axes: the triangle's normal first (so it wins a tie
/// with a parallel box face, and the contact point comes from the plane
/// rule), the box's own faces (1..4), then each box axis crossed with
/// each triangle edge (4..13).
const NORMAL_AXIS: usize = 0;
const BOX_AXES: std::ops::Range<usize> = 1..4;
const CROSS_AXES: usize = 4;

/// Separating axes of the box [-half, half] at the origin against a
/// triangle already in the box's frame, moving by `motion`: the box's
/// interval on each axis slides in time, the triangle's stands, and their
/// meeting window is intersected over the axes. None when separated at
/// time 0 on an axis the motion never closes, or never met at all.
fn sat(tri: &[V3; 3], half: V3, motion: V3) -> Option<Sat> {
  let edges = [sub(tri[1], tri[0]), sub(tri[2], tri[1]), sub(tri[0], tri[2])];
  let mut axes = [[0.0f32; 3]; 13];
  axes[NORMAL_AXIS] = cross(edges[0], edges[1]);
  for i in 0..3 {
    axes[BOX_AXES.start + i][i] = 1.0;
    for j in 0..3 {
      axes[CROSS_AXES + i * 3 + j] = cross(axes[BOX_AXES.start + i], edges[j]);
    }
  }
  let mut deepest: Option<(usize, V3, f32)> = None;
  let mut first: Option<(usize, V3, f32)> = None;
  let mut last = f32::INFINITY;
  let mut apart_now = false;
  for (k, axis) in axes.iter().enumerate() {
    let l2 = dot(*axis, *axis);
    if l2 < DEGENERATE_EPSILON {
      continue;
    }
    let len = l2.sqrt();
    let unit = scale(*axis, 1.0 / len);
    let reach: f32 = (0..3).map(|i| half[i] * unit[i].abs()).sum();
    let p = [dot(unit, tri[0]), dot(unit, tri[1]), dot(unit, tri[2])];
    let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
    // Static: the shorter of the two pushes that separate the intervals,
    // the box's [-reach, reach] up past `hi` or down past `lo`.
    let push_up = hi + reach;
    let push_down = reach - lo;
    if push_up < 0.0 || push_down < 0.0 {
      apart_now = true;
    } else {
      let (depth, n) = if push_up < push_down { (push_up, unit) } else { (push_down, neg(unit)) };
      if deepest.is_none_or(|(_, _, d)| depth < d) {
        deepest = Some((k, n, depth));
      }
    }
    // Swept: when the box's interval [-reach, reach] + v t meets [lo, hi].
    let v = dot(unit, motion);
    if v == 0.0 {
      if push_up < 0.0 || push_down < 0.0 {
        return None;
      }
      continue;
    }
    let (enter, exit, n) = if v > 0.0 {
      ((lo - reach) / v, (hi + reach) / v, neg(unit))
    } else {
      ((hi + reach) / v, (lo - reach) / v, unit)
    };
    if first.is_none_or(|(_, _, t)| enter > t) {
      first = Some((k, n, enter));
    }
    last = last.min(exit);
  }
  let entry = first.filter(|(_, _, t)| *t <= last && *t <= 1.0 && last >= 0.0);
  match deepest {
    Some(deepest) if !apart_now => Some(Sat { deepest, entry }),
    _ => entry.map(|e| Sat { deepest: e, entry: Some(e) }),
  }
}

impl Obb {
  /// The triangle and motion in the box's frame (box at the origin, axes
  /// the coordinate axes).
  fn localize(&self, tri: &[V3; 3], motion: V3) -> ([V3; 3], V3) {
    let to = |v: V3| -> V3 {
      let d = sub(v, self.center);
      [dot(d, self.axes[0]), dot(d, self.axes[1]), dot(d, self.axes[2])]
    };
    let lm = [dot(motion, self.axes[0]), dot(motion, self.axes[1]), dot(motion, self.axes[2])];
    ([to(tri[0]), to(tri[1]), to(tri[2])], lm)
  }

  /// A box-frame direction back in world space.
  fn world_dir(&self, v: V3) -> V3 {
    add(add(scale(self.axes[0], v[0]), scale(self.axes[1], v[1])), scale(self.axes[2], v[2]))
  }

  /// The corner deepest along -`n`, with the box displaced by `offset`.
  fn support(&self, n: V3, offset: V3) -> V3 {
    let mut p = add(self.center, offset);
    for i in 0..3 {
      let s = if dot(self.axes[i], n) > 0.0 { -self.half[i] } else { self.half[i] };
      p = add(p, scale(self.axes[i], s));
    }
    p
  }

  /// The world-space contact point for a SAT axis, with the box displaced
  /// by `offset`: against a box face, the triangle vertex deepest into
  /// the box; against the triangle's plane, the deepest box corner
  /// dropped onto that plane; for an edge pair, the triangle edge's
  /// closest point to the box edge.
  fn contact_point(&self, axis: usize, n: V3, tri: &[V3; 3], offset: V3) -> V3 {
    if BOX_AXES.contains(&axis) {
      let mut best = tri[0];
      for v in &tri[1..] {
        if dot(*v, n) < dot(best, n) {
          best = *v;
        }
      }
      return best;
    }
    let corner = self.support(n, offset);
    if axis == NORMAL_AXIS {
      let tn = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
      let tn = scale(tn, 1.0 / dot(tn, tn).sqrt());
      return sub(corner, scale(tn, dot(sub(corner, tri[0]), tn)));
    }
    let i = (axis - CROSS_AXES) / 3;
    let j = (axis - CROSS_AXES) % 3;
    // The box edge along axis i through the support corner runs back
    // into the box, away from the corner's side.
    let side = if dot(self.axes[i], n) > 0.0 { 1.0 } else { -1.0 };
    let far = add(corner, scale(self.axes[i], 2.0 * self.half[i] * side));
    let (_, on_tri) = closest_segment_segment(corner, far, tri[j], tri[(j + 1) % 3]);
    on_tri
  }
}

fn add(a: V3, b: V3) -> V3 {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: V3, s: f32) -> V3 {
  [a[0] * s, a[1] * s, a[2] * s]
}

fn neg(a: V3) -> V3 {
  [-a[0], -a[1], -a[2]]
}

fn dist2(a: V3, b: V3) -> f32 {
  let d = sub(a, b);
  dot(d, d)
}
