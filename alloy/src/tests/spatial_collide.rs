use super::spatial::{flush, grid_shape};
use crate::spatial::{segment_triangle, MoveOptions, Query, QueryFilter, Shape, Spatial, Volume, BVH_MIN_TRIANGLES};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];
const TOLERANCE: f32 = 1e-4;

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
  (0..3).all(|i| (a[i] - b[i]).abs() < TOLERANCE)
}

fn close(a: f32, b: f32) -> bool {
  (a - b).abs() < TOLERANCE
}

/// A quarter turn about the axis (unit quaternion).
fn quarter_turn(axis: [f32; 3]) -> [f32; 4] {
  let half = std::f32::consts::FRAC_PI_4;
  [axis[0] * half.sin(), axis[1] * half.sin(), axis[2] * half.sin(), half.cos()]
}

fn sphere(center: [f32; 3], radius: f32) -> Volume {
  Volume::Capsule { a: center, b: center, radius }
}

fn capsule(a: [f32; 3], b: [f32; 3], radius: f32) -> Volume {
  Volume::Capsule { a, b, radius }
}

fn obb(center: [f32; 3], half: [f32; 3], rotation: [f32; 4]) -> Volume {
  Volume::Box { center, half, rotation }
}

/// Two triangles spanning [-size, size] in the plane `axis` == 0.
fn quad_shape(axis: usize, size: f32) -> Shape {
  let mut positions = Vec::new();
  for (u, v) in [(-size, -size), (size, -size), (size, size), (-size, size)] {
    let mut p = [0.0; 3];
    p[(axis + 1) % 3] = u;
    p[(axis + 2) % 3] = v;
    positions.extend_from_slice(&p);
  }
  Shape { positions, uvs: None, indices: vec![0, 1, 2, 0, 2, 3] }
}

/// The unit cube, -0.5..0.5, as twelve triangles.
fn cube_shape() -> Shape {
  let mut positions = Vec::new();
  for i in 0..8 {
    positions.extend_from_slice(&[
      if i & 1 != 0 { 0.5 } else { -0.5 },
      if i & 2 != 0 { 0.5 } else { -0.5 },
      if i & 4 != 0 { 0.5 } else { -0.5 },
    ]);
  }
  let mut indices = Vec::new();
  for f in [[0, 2, 6, 4], [1, 3, 7, 5], [0, 1, 5, 4], [2, 3, 7, 6], [0, 1, 3, 2], [4, 5, 7, 6]] {
    indices.extend_from_slice(&[f[0], f[1], f[2], f[0], f[2], f[3]]);
  }
  Shape { positions, uvs: None, indices }
}

/// A node carrying `shape` (its tight box `bounds`) at `position`.
fn add_shape(
  s: &mut Spatial,
  shape: Shape,
  bounds: [f32; 6],
  position: [f32; 3],
  rotation: [f32; 4],
  scale: [f32; 3],
) -> u64 {
  let n = s.create(position, rotation, scale, true);
  let sid = s.create_shape(shape).expect("shape");
  s.set_bounds(n, Some(bounds)).expect("bounds");
  s.set_shape(n, Some(sid)).expect("set shape");
  n
}

fn floor(s: &mut Spatial) -> u64 {
  add_shape(s, quad_shape(1, 5.0), [-5.0, 0.0, -5.0, 5.0, 0.0, 5.0], [0.0; 3], Q, ONE)
}

fn wall(s: &mut Spatial) -> u64 {
  add_shape(s, quad_shape(0, 5.0), [0.0, -5.0, -5.0, 0.0, 5.0, 5.0], [0.0; 3], Q, ONE)
}

#[test]
fn segment_triangle_closest_pairs() {
  let tri = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
  let (ps, pt, d) = segment_triangle([0.5, 0.5, 1.0], [0.5, 0.5, 2.0], &tri);
  assert!(near(ps, [0.5, 0.5, 1.0]) && near(pt, [0.5, 0.5, 0.0]) && close(d, 1.0), "endpoint over the interior");
  let (ps, pt, d) = segment_triangle([0.5, 0.5, -1.0], [0.5, 0.5, 1.0], &tri);
  assert!(near(ps, [0.5, 0.5, 0.0]) && near(pt, [0.5, 0.5, 0.0]) && close(d, 0.0), "a pierce is distance 0");
  let (ps, pt, d) = segment_triangle([-1.0, 0.5, 0.0], [-1.0, 0.5, 3.0], &tri);
  assert!(near(ps, [-1.0, 0.5, 0.0]) && near(pt, [0.0, 0.5, 0.0]) && close(d, 1.0), "nearest edge");
  let (_, pt, d) = segment_triangle([3.0, 3.0, 0.0], [4.0, 4.0, 0.0], &tri);
  assert!(near(pt, [1.0, 1.0, 0.0]) && close(d, 8.0f32.sqrt()), "beyond the hypotenuse");
}

#[test]
fn sphere_overlap_reports_depth_and_normal() {
  let mut s = Spatial::new();
  let f = floor(&mut s);
  flush(&mut s);
  let hits = s.overlap(&sphere([1.0, 0.3, 2.0], 0.5), &QueryFilter::default()).expect("overlap");
  assert_eq!(hits.len(), 1);
  assert_eq!(hits[0].node, f);
  assert!(close(hits[0].depth, 0.2), "depth {}", hits[0].depth);
  assert!(near(hits[0].normal, [0.0, 1.0, 0.0]));
  assert!(near(hits[0].point, [1.0, 0.0, 2.0]));
  assert!(
    s.overlap(&sphere([1.0, 0.6, 2.0], 0.5), &QueryFilter::default()).expect("overlap").is_empty(),
    "clear of the floor"
  );
  let touching = s.overlap(&sphere([1.0, 0.5, 2.0], 0.5), &QueryFilter::default()).expect("overlap");
  assert_eq!(touching.len(), 1, "touching counts");
  assert!(close(touching[0].depth, 0.0));
  assert!(
    s.overlap(&sphere([7.0, 0.1, 0.0], 0.5), &QueryFilter::default()).expect("overlap").is_empty(),
    "beside the floor"
  );
  let pierced = s.overlap(&sphere([1.0, -0.2, 2.0], 0.5), &QueryFilter::default()).expect("overlap");
  assert_eq!(pierced.len(), 1, "a center below the floor still contacts");
  assert!(near(pierced[0].normal, [0.0, -1.0, 0.0]), "the push is out on the center's side");
  assert!(close(pierced[0].depth, 0.5 - 0.2), "depth {}", pierced[0].depth);
  s.set_visible(f, false).expect("hide");
  flush(&mut s);
  assert!(
    s.overlap(&sphere([1.0, 0.3, 2.0], 0.5), &QueryFilter::default()).expect("overlap").is_empty(),
    "hidden nodes are skipped"
  );
}

#[test]
fn box_only_node_is_its_twelve_triangles() {
  let mut s = Spatial::new();
  let n = s.create([5.0, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(n, Some([-1.0, -1.0, -1.0, 1.0, 1.0, 1.0])).expect("bounds");
  flush(&mut s);
  assert!(s.overlap(&sphere([3.2, 0.0, 0.0], 0.5), &QueryFilter::default()).expect("overlap").is_empty());
  let hits = s.overlap(&sphere([3.7, 0.0, 0.0], 0.5), &QueryFilter::default()).expect("overlap");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].depth, 0.2) && near(hits[0].normal, [-1.0, 0.0, 0.0]) && near(hits[0].point, [4.0, 0.0, 0.0]));
  let hits = s.sweep(&sphere([0.0, 0.0, 0.0], 0.5), [4.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.875), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [-1.0, 0.0, 0.0]) && near(hits[0].point, [4.0, 0.0, 0.0]));
}

#[test]
fn sphere_sweep_hits_the_wall_exactly() {
  let mut s = Spatial::new();
  let w = wall(&mut s);
  flush(&mut s);
  let hits = s.sweep(&sphere([-3.0, 0.0, 0.0], 0.5), [5.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert_eq!(hits[0].node, w);
  assert!(close(hits[0].time, 0.5), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [-1.0, 0.0, 0.0]));
  assert!(near(hits[0].point, [0.0, 0.0, 0.0]));
  assert!(
    s.sweep(&sphere([-3.0, 0.0, 0.0], 0.5), [1.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "falls short"
  );
  assert!(
    s.sweep(&sphere([-3.0, 0.0, 7.0], 0.5), [5.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "passes beside"
  );
  assert!(
    s.sweep(&sphere([3.0, 0.0, 0.0], 0.5), [5.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "moves away"
  );
  // Grazing the wall's edge at z = 5, 0.3 off it: the sphere meets the
  // edge when its center is sqrt(0.5^2 - 0.3^2) = 0.4 short of the plane.
  let hits = s.sweep(&sphere([-3.0, 0.0, 5.3], 0.5), [5.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1, "an edge hit");
  assert!(close(hits[0].time, 2.6 / 5.0), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [-0.8, 0.0, 0.6]), "normal {:?}", hits[0].normal);
  assert!(near(hits[0].point, [0.0, 0.0, 5.0]));
  assert!(
    s.sweep(&sphere([-3.0, 0.0, 0.0], 0.5), [0.0; 3], &QueryFilter::default()).expect("sweep").is_empty(),
    "a zero motion touches nothing"
  );
}

#[test]
fn capsule_sweep_lands_and_slides() {
  let mut s = Spatial::new();
  floor(&mut s);
  flush(&mut s);
  let hits =
    s.sweep(&capsule([0.0, 1.5, 0.0], [0.0, 2.5, 0.0], 0.5), [0.0, -2.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.5), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [0.0, 1.0, 0.0]) && near(hits[0].point, [0.0, 0.0, 0.0]));
  // Resting on the floor: a slide along it is no hit, a push into it is
  // an immediate one, and lifting off is none.
  let resting = capsule([0.0, 0.5, 0.0], [0.0, 1.5, 0.0], 0.5);
  assert!(
    s.sweep(&resting, [1.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "sliding along the contact"
  );
  let hits = s.sweep(&resting, [1.0, -1.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(hits[0].time < TOLERANCE && near(hits[0].normal, [0.0, 1.0, 0.0]));
  assert!(
    s.sweep(&resting, [0.0, 1.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "leaving the contact"
  );
  // A tilted capsule lands on its lower end.
  let hits =
    s.sweep(&capsule([0.0, 2.0, 0.0], [1.0, 3.0, 0.0], 0.5), [0.0, -3.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.5), "time {}", hits[0].time);
  assert!(near(hits[0].point, [0.0, 0.0, 0.0]) && near(hits[0].normal, [0.0, 1.0, 0.0]));
}

#[test]
fn capsule_sweep_hits_a_wall_along_its_length() {
  let mut s = Spatial::new();
  wall(&mut s);
  flush(&mut s);
  let hits = s
    .sweep(&capsule([-3.0, 0.0, 0.0], [-3.0, 1.0, 0.0], 0.5), [5.0, 0.0, 0.0], &QueryFilter::default())
    .expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.5), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [-1.0, 0.0, 0.0]));
  assert!(close(hits[0].point[0], 0.0) && (0.0..=1.0).contains(&hits[0].point[1]));
  let hits = s.overlap(&capsule([-0.3, 0.0, 0.0], [-0.3, 1.0, 0.0], 0.5), &QueryFilter::default()).expect("overlap");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].depth, 0.2) && near(hits[0].normal, [-1.0, 0.0, 0.0]));
}

#[test]
fn queries_hold_under_scale_and_rotation() {
  let mut s = Spatial::new();
  let n = add_shape(&mut s, cube_shape(), [-0.5, -0.5, -0.5, 0.5, 0.5, 0.5], [0.0; 3], Q, [2.0, 1.0, 1.0]);
  flush(&mut s);
  // Faces at x = +-1: the sphere touches at center x = -1.5.
  let hits = s.sweep(&sphere([-4.0, 0.0, 0.0], 0.5), [4.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.625), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [-1.0, 0.0, 0.0]) && near(hits[0].point, [-1.0, 0.0, 0.0]));
  // A quarter turn about y swings the long side onto z: faces at x = +-0.5.
  s.set_transform(n, [0.0; 3], quarter_turn([0.0, 1.0, 0.0]), [2.0, 1.0, 1.0]).expect("rotate");
  flush(&mut s);
  let hits = s.sweep(&sphere([-4.0, 0.0, 0.0], 0.5), [4.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.75), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [-1.0, 0.0, 0.0]) && near(hits[0].point, [-0.5, 0.0, 0.0]));
  let hits = s.overlap(&sphere([-0.9, 0.0, 0.0], 0.5), &QueryFilter::default()).expect("overlap");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].depth, 0.1) && near(hits[0].normal, [-1.0, 0.0, 0.0]), "{:?}", hits[0]);
  let hits = s.sweep(&sphere([0.0, 0.0, -4.0], 0.5), [0.0, 0.0, 4.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.625), "the long side now faces z: time {}", hits[0].time);
}

#[test]
fn box_overlap_and_sweep() {
  let mut s = Spatial::new();
  floor(&mut s);
  flush(&mut s);
  let half = [0.5, 0.5, 0.5];
  let hits = s.overlap(&obb([1.0, 0.4, 1.0], half, Q), &QueryFilter::default()).expect("overlap");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].depth, 0.1) && near(hits[0].normal, [0.0, 1.0, 0.0]), "{:?}", hits[0]);
  assert!(close(hits[0].point[1], 0.0), "the contact lies on the floor");
  assert!(s.overlap(&obb([1.0, 0.6, 1.0], half, Q), &QueryFilter::default()).expect("overlap").is_empty());
  let hits = s.sweep(&obb([0.0, 2.0, 0.0], half, Q), [0.0, -3.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.5) && near(hits[0].normal, [0.0, 1.0, 0.0]), "{:?}", hits[0]);
  assert!(close(hits[0].point[1], 0.0));
  // Turned 45 degrees about z the box lands on an edge, sqrt(0.5) below its center.
  let hits = s
    .sweep(
      &obb([0.0, 2.0, 0.0], half, [0.0, 0.0, (std::f32::consts::FRAC_PI_8).sin(), (std::f32::consts::FRAC_PI_8).cos()]),
      [0.0, -3.0, 0.0],
      &QueryFilter::default(),
    )
    .expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, (2.0 - 0.5f32.sqrt()) / 3.0), "time {}", hits[0].time);
  assert!(near(hits[0].normal, [0.0, 1.0, 0.0]), "{:?}", hits[0]);
  let p = hits[0].point;
  assert!(close(p[0], 0.0) && close(p[1], 0.0) && p[2].abs() <= 0.5, "the contact lies on the landing edge: {p:?}");
  assert!(
    s.sweep(&obb([0.0, 0.5, 0.0], half, Q), [1.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "sliding along the floor"
  );
  let hits = s.sweep(&obb([0.0, 0.5, 0.0], half, Q), [1.0, -1.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(hits[0].time < TOLERANCE && near(hits[0].normal, [0.0, 1.0, 0.0]));
  let mut s = Spatial::new();
  wall(&mut s);
  flush(&mut s);
  let hits = s.sweep(&obb([-3.0, 0.0, 0.0], half, Q), [5.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep");
  assert_eq!(hits.len(), 1);
  assert!(close(hits[0].time, 0.5) && near(hits[0].normal, [-1.0, 0.0, 0.0]), "{:?}", hits[0]);
  assert!(
    s.sweep(&obb([-3.0, 0.0, 7.0], half, Q), [5.0, 0.0, 0.0], &QueryFilter::default()).expect("sweep").is_empty(),
    "passes beside"
  );
}

#[test]
fn volume_queries_match_the_linear_oracle() {
  // The heightfield sits past the indexing threshold and off the origin,
  // so the shape BVH walk and the world-space carry both get exercised;
  // the oracle runs the same per-triangle tests over every triangle.
  let side = 24;
  let shape = grid_shape(side);
  assert!(shape.indices.len() / 3 >= BVH_MIN_TRIANGLES * 2);
  let offset = [10.0, 0.0, -3.0];
  let oracle: Vec<[[f32; 3]; 3]> = shape
    .indices
    .chunks_exact(3)
    .map(|tri| {
      let at = |i: u32| {
        let k = i as usize * 3;
        [shape.positions[k] + offset[0], shape.positions[k + 1] + offset[1], shape.positions[k + 2] + offset[2]]
      };
      [at(tri[0]), at(tri[1]), at(tri[2])]
    })
    .collect();
  let mut s = Spatial::new();
  let n = add_shape(&mut s, shape, [0.0, 0.0, 0.0, side as f32, 1.0, side as f32], offset, Q, ONE);
  flush(&mut s);
  let mut state: u32 = 7;
  let mut rand = move || {
    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    (state >> 8) as f32 / (1u32 << 24) as f32
  };
  let (mut overlaps, mut impacts) = (0, 0);
  for k in 0..150 {
    let x = offset[0] + rand() * (side as f32 + 4.0) - 2.0;
    let z = offset[2] + rand() * (side as f32 + 4.0) - 2.0;
    let radius = 0.2 + rand() * 0.8;
    let volume = match k % 3 {
      0 => sphere([x, rand() * 0.8, z], radius),
      1 => capsule([x, rand() * 0.8, z], [x + rand() - 0.5, rand() * 0.8 + 0.5, z + rand() - 0.5], radius),
      _ => obb([x, rand() * 0.8, z], [radius, radius * 0.5, radius], quarter_turn([0.0, 1.0, 0.0])),
    };
    let query = Query::new(&volume);
    let expected = oracle
      .iter()
      .filter_map(|tri| query.overlap_triangle(tri))
      .map(|c| c.2)
      .fold(None, |best: Option<f32>, d| Some(best.map_or(d, |b| b.max(d))));
    let got = s.overlap(&volume, &QueryFilter::default()).expect("overlap");
    match expected {
      Some(depth) => {
        overlaps += 1;
        assert_eq!(got.len(), 1, "one overlap for a volume the oracle touches");
        assert_eq!(got[0].node, n);
        assert!(close(got[0].depth, depth), "depth {} vs oracle {depth}", got[0].depth);
      }
      None => assert!(got.is_empty(), "the index must not invent overlaps"),
    }
    let start = match volume {
      Volume::Capsule { a, b, radius } => capsule([a[0], a[1] + 3.0, a[2]], [b[0], b[1] + 3.0, b[2]], radius),
      Volume::Box { center, half, rotation } => obb([center[0], center[1] + 3.0, center[2]], half, rotation),
    };
    let motion = [rand() - 0.5, -4.0, rand() - 0.5];
    let query = Query::new(&start);
    let expected = oracle
      .iter()
      .filter_map(|tri| query.sweep_triangle(motion, tri))
      .map(|h| h.0)
      .fold(None, |best: Option<f32>, t| Some(best.map_or(t, |b| b.min(t))));
    let got = s.sweep(&start, motion, &QueryFilter::default()).expect("sweep");
    match expected {
      Some(t) => {
        impacts += 1;
        assert_eq!(got.len(), 1, "one impact for a sweep the oracle lands");
        assert!(close(got[0].time, t), "time {} vs oracle {t}", got[0].time);
        assert!(got[0].normal[1] > 0.0, "a falling volume lands on an up-facing side");
      }
      None => assert!(got.is_empty(), "the index must not invent impacts"),
    }
  }
  assert!(
    overlaps > 40 && impacts > 100,
    "the cases must mostly touch the grid ({overlaps} overlaps, {impacts} impacts)"
  );
}

// moveAndSlide's loop logic (skin, slide, floor/wall/ceiling, snap,
// depenetration, the filter) over the real narrowphase: a floor quad,
// walls, a ceiling and a slope big enough to stand in for planes.

/// The mover's own skin, what the landing gaps below are measured in.
const MOVE_SKIN: f32 = 0.01;

fn ball(x: f32, y: f32, z: f32) -> Volume {
  sphere([x, y, z], 0.5)
}

fn wall_at(s: &mut Spatial, x: f32) -> u64 {
  add_shape(s, quad_shape(0, 5.0), [0.0, -5.0, -5.0, 0.0, 5.0, 5.0], [x, 0.0, 0.0], Q, ONE)
}

fn slide(s: &mut Spatial, volume: Volume, motion: [f32; 3]) -> crate::spatial::MoveResult {
  s.move_and_slide(&volume, motion, &MoveOptions::default(), &QueryFilter::default()).expect("move")
}

#[test]
fn move_and_slide_moves_freely_with_nothing_in_the_way() {
  let mut s = Spatial::new();
  flush(&mut s);
  let r = slide(&mut s, ball(0.0, 5.0, 0.0), [1.0, -2.0, 3.0]);
  assert!(near(r.motion, [1.0, -2.0, 3.0]));
  assert!(r.floor.is_none() && !r.wall && r.hits.is_empty());
}

#[test]
fn move_and_slide_lands_a_skin_short_and_walks_under_gravity() {
  let mut s = Spatial::new();
  floor(&mut s);
  flush(&mut s);
  let r = slide(&mut s, ball(0.0, 2.0, 0.0), [0.0, -3.0, 0.0]);
  assert!(near(r.motion, [0.0, -(1.5 - MOVE_SKIN), 0.0]), "{:?}", r.motion);
  assert!(near(r.floor.expect("floor"), [0.0, 1.0, 0.0]) && !r.wall);
  let r = slide(&mut s, ball(0.0, 0.5 + MOVE_SKIN, 0.0), [1.0, -0.2, 0.0]);
  assert!(near(r.motion, [1.0, 0.0, 0.0]), "the floor absorbs the fall: {:?}", r.motion);
  assert!(near(r.floor.expect("floor"), [0.0, 1.0, 0.0]));
}

#[test]
fn move_and_slide_slides_along_walls_and_stops_in_a_corner() {
  let mut s = Spatial::new();
  floor(&mut s);
  wall_at(&mut s, 5.0);
  flush(&mut s);
  let r = slide(&mut s, ball(0.0, 0.5 + MOVE_SKIN, 0.0), [10.0, 0.0, 3.0]);
  assert!(near(r.motion, [4.5 - MOVE_SKIN, 0.0, 3.0]), "{:?}", r.motion);
  assert!(r.wall && r.floor.is_some());
  // A second wall across z closes the corner.
  add_shape(&mut s, quad_shape(2, 5.0), [-5.0, -5.0, 0.0, 5.0, 5.0, 0.0], [0.0, 0.0, 5.0], Q, ONE);
  flush(&mut s);
  let r = slide(&mut s, ball(4.5 - MOVE_SKIN, 0.5 + MOVE_SKIN, 4.5 - MOVE_SKIN), [1.0, 0.0, 1.0]);
  assert!((r.motion[0] * r.motion[0] + r.motion[2] * r.motion[2]).sqrt() < 1e-5, "{:?}", r.motion);
  assert!(r.wall);
}

#[test]
fn move_and_slide_bumps_a_ceiling_without_snapping() {
  let mut s = Spatial::new();
  floor(&mut s);
  add_shape(&mut s, quad_shape(1, 5.0), [-5.0, 0.0, -5.0, 5.0, 0.0, 5.0], [0.0, 3.0, 0.0], Q, ONE);
  flush(&mut s);
  let r = slide(&mut s, ball(0.0, 1.0, 0.0), [0.0, 5.0, 0.0]);
  assert!(near(r.motion, [0.0, 1.5 - MOVE_SKIN, 0.0]), "{:?}", r.motion);
  assert!(r.ceiling && r.floor.is_none());
}

#[test]
fn move_and_slide_slides_down_a_slope_too_steep_to_stand_on() {
  let mut s = Spatial::new();
  // The floor quad tipped 60 degrees about z: its normal (sin 60, cos 60, 0).
  let tilt = -std::f32::consts::FRAC_PI_3 / 2.0;
  add_shape(
    &mut s,
    quad_shape(1, 20.0),
    [-20.0, 0.0, -20.0, 20.0, 0.0, 20.0],
    [0.0; 3],
    [0.0, 0.0, tilt.sin(), tilt.cos()],
    ONE,
  );
  flush(&mut s);
  let r = slide(&mut s, ball(2.0, 2.0, 0.0), [0.0, -6.0, 0.0]);
  assert!(r.floor.is_none() && r.wall, "{:?}", r);
  assert!(r.motion[0] > 0.5 && r.motion[1] < -4.0, "{:?}", r.motion);
}

#[test]
fn move_and_slide_pushes_out_of_a_surface_it_starts_inside() {
  let mut s = Spatial::new();
  floor(&mut s);
  flush(&mut s);
  let r = slide(&mut s, ball(0.0, 0.3, 0.0), [0.0; 3]);
  assert!(near(r.motion, [0.0, 0.2 + MOVE_SKIN, 0.0]), "{:?}", r.motion);
  assert!(r.floor.is_some());
}

#[test]
fn move_and_slide_snaps_down_within_reach_never_upward() {
  let mut s = Spatial::new();
  floor(&mut s);
  flush(&mut s);
  let r = slide(&mut s, ball(0.0, 0.55, 0.0), [1.0, 0.0, 0.0]);
  assert!(near(r.motion, [1.0, -(0.05 - MOVE_SKIN), 0.0]), "{:?}", r.motion);
  assert!(r.floor.is_some());
  let off = MoveOptions { floor_snap: 0.0, ..MoveOptions::default() };
  let r = s.move_and_slide(&ball(0.0, 0.55, 0.0), [1.0, 0.0, 0.0], &off, &QueryFilter::default()).expect("move");
  assert!(near(r.motion, [1.0, 0.0, 0.0]) && r.floor.is_none());
  let rising = slide(&mut s, ball(0.0, 0.55, 0.0), [1.0, 0.1, 0.0]);
  assert!(near(rising.motion, [1.0, 0.1, 0.0]) && rising.floor.is_none());
  let far = slide(&mut s, ball(0.0, 1.0, 0.0), [1.0, 0.0, 0.0]);
  assert!(far.floor.is_none());
}

#[test]
fn move_and_slide_honors_the_query_filter() {
  let mut s = Spatial::new();
  let f = floor(&mut s);
  let other = s.create([0.0; 3], Q, ONE, true);
  flush(&mut s);
  let only_other = QueryFilter { nodes: Some(vec![other]), ..QueryFilter::default() };
  let r = s.move_and_slide(&ball(0.0, 2.0, 0.0), [0.0, -3.0, 0.0], &MoveOptions::default(), &only_other).expect("move");
  assert!(near(r.motion, [0.0, -3.0, 0.0]), "an excluded floor is fallen through: {:?}", r.motion);
  s.set_layers(f, 2).expect("layers");
  let masked = QueryFilter { layers: Some(1), ..QueryFilter::default() };
  let r = s.move_and_slide(&ball(0.0, 2.0, 0.0), [0.0, -3.0, 0.0], &MoveOptions::default(), &masked).expect("move");
  assert!(near(r.motion, [0.0, -3.0, 0.0]), "a masked-out floor too: {:?}", r.motion);
  let under = QueryFilter { root: Some(f), ..QueryFilter::default() };
  let r = s.move_and_slide(&ball(0.0, 2.0, 0.0), [0.0, -3.0, 0.0], &MoveOptions::default(), &under).expect("move");
  assert!(r.floor.is_some(), "the root itself is under the root");
}
