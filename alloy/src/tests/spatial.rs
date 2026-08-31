use crate::spatial::{compose, multiply, DrawSink, Mat4, SinkWriter, Spatial, IDENTITY};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];

// The recording SinkWriter: one owned variant per trait method, so the
// tests assert on flush output by equality.
#[derive(Clone, Debug, PartialEq)]
enum Write {
  Params { target: u64, draw: u64, model: Mat4, normal: Option<Mat4> },
  Count { target: u64, draw: u64, count: u32 },
  Shared { target: u64, name: String, values: Vec<f32> },
  Instances { buffer: u64, first: u32, values: Vec<f32> },
}

struct Recorder(Vec<Write>);

impl SinkWriter for Recorder {
  fn write_params(&mut self, target: u64, draw: u64, model: &Mat4, normal: Option<&Mat4>) {
    self.0.push(Write::Params { target, draw, model: *model, normal: normal.copied() });
  }
  fn write_count(&mut self, target: u64, draw: u64, count: u32) {
    self.0.push(Write::Count { target, draw, count });
  }
  fn write_shared(&mut self, target: u64, name: &str, values: &[f32]) {
    self.0.push(Write::Shared { target, name: name.to_string(), values: values.to_vec() });
  }
  fn write_instances(&mut self, buffer: u64, lo: u32, hi: u32, values: &[f32]) {
    // Record the range slice under `first`, matching the plain write path.
    self.0.push(Write::Instances { buffer, first: lo, values: values[lo as usize..hi as usize].to_vec() });
  }
}

fn sink(draw: u64) -> DrawSink {
  DrawSink { target: 1, draw, normal: false, count: 1 }
}

fn flush(s: &mut Spatial) -> Vec<Write> {
  let mut out = Recorder(Vec::new());
  s.flush(&mut out);
  out.0
}

fn model(w: &Write) -> [f32; 16] {
  match w {
    Write::Params { model, .. } => *model,
    other => panic!("expected params, got {other:?}"),
  }
}

#[test]
fn first_flush_switches_entry_on_and_writes_world() {
  let mut s = Spatial::new();
  let root = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let child = s.create([0.0, 2.0, 0.0], Q, [2.0, 2.0, 2.0], true);
  s.set_parent(child, Some(root)).expect("parent");
  s.bind_sink(child, sink(7)).expect("sink");
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 2);
  assert_eq!(writes[0], Write::Count { target: 1, draw: 7, count: 1 });
  let expected = multiply(compose([1.0, 0.0, 0.0], Q, ONE), compose([0.0, 2.0, 0.0], Q, [2.0, 2.0, 2.0]));
  assert_eq!(model(&writes[1]), expected);
  assert_eq!(model(&writes[1])[12..15], [1.0, 2.0, 0.0]);
  assert!(flush(&mut s).is_empty(), "a clean tree writes nothing");
}

#[test]
fn moving_a_node_writes_only_its_subtree() {
  let mut s = Spatial::new();
  let root = s.create([0.0; 3], Q, ONE, true);
  let a = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let b = s.create([2.0, 0.0, 0.0], Q, ONE, true);
  let a_child = s.create([0.0, 1.0, 0.0], Q, ONE, true);
  s.set_parent(a, Some(root)).expect("parent");
  s.set_parent(b, Some(root)).expect("parent");
  s.set_parent(a_child, Some(a)).expect("parent");
  for (n, d) in [(a, 1), (b, 2), (a_child, 3)] {
    s.bind_sink(n, sink(d)).expect("sink");
  }
  flush(&mut s);
  s.set_transform(a, [5.0, 0.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  let draws: Vec<u64> = writes
    .iter()
    .map(|w| match w {
      Write::Params { draw, .. } => *draw,
      Write::Count { draw, .. } => *draw,
      other => panic!("no slot or record sinks here, got {other:?}"),
    })
    .collect();
  assert_eq!(draws, vec![1, 3], "sibling b untouched");
  assert_eq!(model(&writes[1])[12..15], [5.0, 1.0, 0.0]);
}

#[test]
fn hiding_flips_counts_and_unhide_rewrites_params() {
  let mut s = Spatial::new();
  let root = s.create([0.0; 3], Q, ONE, true);
  let m = s.create([0.0; 3], Q, ONE, true);
  s.set_parent(m, Some(root)).expect("parent");
  s.bind_sink(m, DrawSink { target: 1, draw: 9, normal: false, count: 4 }).expect("sink");
  flush(&mut s);
  s.set_visible(root, false).expect("hide");
  assert_eq!(flush(&mut s), vec![Write::Count { target: 1, draw: 9, count: 0 }]);
  assert!(!s.shown(m).expect("alive"));
  // Moved while hidden: no write now, a params write on unhide.
  s.set_transform(m, [3.0, 0.0, 0.0], Q, ONE).expect("move");
  assert!(flush(&mut s).is_empty());
  s.set_visible(root, true).expect("show");
  let writes = flush(&mut s);
  assert_eq!(writes[0], Write::Count { target: 1, draw: 9, count: 4 });
  assert_eq!(model(&writes[1])[12], 3.0);
}

#[test]
fn sinks_are_per_target_and_one_move_feeds_them_all() {
  let mut s = Spatial::new();
  let m = s.create([0.0; 3], Q, ONE, true);
  s.bind_sink(m, DrawSink { target: 1, draw: 7, normal: false, count: 1 }).expect("sink");
  s.bind_sink(m, DrawSink { target: 2, draw: 8, normal: true, count: 1 }).expect("sink");
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 4, "count + params per sink: {writes:?}");
  assert_eq!(writes[0], Write::Count { target: 1, draw: 7, count: 1 });
  assert_eq!(writes[2], Write::Count { target: 2, draw: 8, count: 1 });
  // Rebinding on a target replaces that sink; the other stays (and, the
  // node being re-queued, gets a params rewrite - the reparent rule).
  s.bind_sink(m, DrawSink { target: 1, draw: 9, normal: false, count: 1 }).expect("rebind");
  let writes = flush(&mut s);
  assert!(writes.contains(&Write::Count { target: 1, draw: 9, count: 1 }));
  assert!(writes.contains(&Write::Params { target: 1, draw: 9, model: IDENTITY, normal: None }));
  assert!(!writes.iter().any(|w| matches!(w, Write::Count { target: 2, .. })));
  // One move, one params write per sink; uNormal only where asked.
  s.set_transform(m, [2.0, 0.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 2);
  assert_eq!(model(&writes[0])[12], 2.0);
  assert_eq!(model(&writes[1])[12], 2.0);
  assert!(writes.iter().any(|w| matches!(w, Write::Params { target: 1, draw: 9, normal: None, .. })));
  assert!(writes.iter().any(|w| matches!(w, Write::Params { target: 2, draw: 8, normal: Some(_), .. })));
  // The count write fans out to every entry that is on.
  let mut out = Recorder(Vec::new());
  assert!(s.set_sink_count(m, 5, &mut out).expect("count"));
  assert_eq!(out.0.len(), 2);
  assert!(out.0.contains(&Write::Count { target: 1, draw: 9, count: 5 }));
  assert!(out.0.contains(&Write::Count { target: 2, draw: 8, count: 5 }));
  // Unbinding one target leaves the other; hiding then writes one count.
  s.unbind_sink(m, Some(1)).expect("unbind");
  flush(&mut s);
  s.set_visible(m, false).expect("hide");
  assert_eq!(flush(&mut s), vec![Write::Count { target: 2, draw: 8, count: 0 }]);
  s.unbind_sink(m, None).expect("unbind all");
  assert!(s.set_sink_count(m, 1, &mut out).is_err());
}

#[test]
fn slot_sinks_are_per_target() {
  let mut s = Spatial::new();
  let a = s.create([0.0; 3], Q, ONE, true);
  let sink = |target: u64| SharedSlotSink {
    target,
    name: "uLightDir".to_string(),
    len: 3,
    index: 0,
    projection: Projection::Direction([0.0, -1.0, 0.0]),
  };
  s.bind_shared_slot(a, sink(1)).expect("bind");
  s.bind_shared_slot(a, sink(2)).expect("bind");
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 2, "one array per target: {writes:?}");
  // Rebinding on target 1 replaces (no leaked group); unbinding target 2
  // zeroes only its array.
  s.bind_shared_slot(a, sink(1)).expect("rebind");
  s.unbind_shared_slot(a, Some(2)).expect("unbind");
  let writes = flush(&mut s);
  assert!(writes.contains(&Write::Shared { target: 2, name: "uLightDir".to_string(), values: vec![0.0; 3] }));
  assert!(writes.contains(&Write::Shared { target: 1, name: "uLightDir".to_string(), values: vec![0.0, -1.0, 0.0] }));
  assert!(flush(&mut s).is_empty());
}

#[test]
fn world_reads_through_pending_writes() {
  let mut s = Spatial::new();
  let root = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let m = s.create([0.0, 1.0, 0.0], Q, ONE, true);
  s.set_parent(m, Some(root)).expect("parent");
  assert_eq!(s.world(m).expect("world")[12..15], [1.0, 1.0, 0.0]);
  flush(&mut s);
  s.set_transform(root, [10.0, 0.0, 0.0], Q, ONE).expect("move");
  assert_eq!(s.world(m).expect("world")[12..15], [10.0, 1.0, 0.0]);
  // The read cleared nothing: the flush still sees the move.
  s.bind_sink(m, sink(1)).expect("sink");
  assert_eq!(model(&flush(&mut s)[1])[12..15], [10.0, 1.0, 0.0]);
}

#[test]
fn destroyed_ids_never_resolve_and_children_become_roots() {
  let mut s = Spatial::new();
  let p = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let c = s.create([0.0, 1.0, 0.0], Q, ONE, true);
  s.set_parent(c, Some(p)).expect("parent");
  flush(&mut s);
  s.destroy(p).expect("destroy");
  assert!(s.world(p).is_err());
  let reused = s.create([0.0; 3], Q, ONE, true);
  assert_ne!(reused, p);
  flush(&mut s);
  assert_eq!(s.world(c).expect("world"), compose([0.0, 1.0, 0.0], Q, ONE));
  assert_eq!(s.world(reused).expect("world"), IDENTITY);
}

#[test]
fn reparent_rejects_cycles() {
  let mut s = Spatial::new();
  let a = s.create([0.0; 3], Q, ONE, true);
  let b = s.create([0.0; 3], Q, ONE, true);
  s.set_parent(b, Some(a)).expect("parent");
  assert!(s.set_parent(a, Some(b)).is_err());
}

// --- stage 2: index and picking ---

use crate::spatial::{ray_box_distance, ray_shape, NodeId, Shape, BVH_MIN_TRIANGLES};

fn quad_shape(uvs: bool) -> Shape {
  // Unit quad in the xy plane at z = 0, two triangles.
  Shape {
    positions: vec![-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0],
    uvs: uvs.then(|| vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]),
    indices: vec![0, 1, 2, 0, 2, 3],
  }
}

#[test]
fn ray_box_distance_edge_cases() {
  let b = [-1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
  assert_eq!(ray_box_distance([0.0; 3], [1.0, 0.0, 0.0], &b), Some(0.0), "inside box hits at 0");
  assert_eq!(ray_box_distance([5.0, 0.0, 0.0], [1.0, 0.0, 0.0], &b), None, "box behind ray misses");
  assert!(ray_box_distance([0.0, 0.0, -5.0], [0.0, 0.0, 1.0], &b).is_some(), "axis-parallel ray hits");
  assert_eq!(ray_box_distance([0.0, 2.0, -5.0], [0.0, 0.0, 1.0], &b), None, "parallel outside slab misses");
  let flat = [-1.0, 0.0, -1.0, 1.0, 0.0, 1.0];
  assert_eq!(ray_box_distance([0.0, 5.0, 0.0], [0.0, -1.0, 0.0], &flat), Some(5.0), "flat box hits at 5");
}

#[test]
fn box_hits_sorted_and_hidden_skipped() {
  let mut s = Spatial::new();
  let root = s.create([0.0; 3], Q, ONE, true);
  let near = s.create([0.0, 0.0, -2.0], Q, ONE, true);
  let far = s.create([0.0, 0.0, -6.0], Q, [2.0, 2.0, 2.0], true);
  let aside = s.create([5.0, 0.0, -4.0], Q, ONE, true);
  for n in [near, far, aside] {
    s.set_parent(n, Some(root)).expect("parent");
    s.set_bounds(n, Some([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5])).expect("bounds");
  }
  flush(&mut s);
  let hits = s.raycast([0.0, 0.0, 0.0], [0.0, 0.0, -2.0]);
  assert_eq!(hits.iter().map(|h| h.node).collect::<Vec<_>>(), vec![near, far]);
  assert!((hits[0].distance - 1.5).abs() < 1e-5);
  // The far box is scaled 2x: its near face sits at -6 + 1 = -5.
  assert!((hits[1].distance - 5.0).abs() < 1e-5);
  assert_eq!(hits[0].face, None);
  assert_eq!(hits[0].normal, None);
  s.set_visible(near, false).expect("hide");
  flush(&mut s);
  let hits = s.raycast([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
  assert_eq!(hits.iter().map(|h| h.node).collect::<Vec<_>>(), vec![far]);
  s.set_bounds(far, None).expect("clear");
  flush(&mut s);
  assert!(s.raycast([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]).is_empty());
}

#[test]
fn overlap_is_exact_for_rotated_rects_and_skips_hidden() {
  let mut s = Spatial::new();
  let half = std::f32::consts::FRAC_PI_4 / 2.0;
  let q45 = [0.0, 0.0, half.sin(), half.cos()];
  let flat = [-0.5, -0.5, 0.0, 0.5, 0.5, 0.0];
  let spun = s.create([0.0; 3], q45, ONE, true);
  let off = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  let hidden = s.create([10.0, 0.0, 0.0], Q, ONE, false);
  for n in [spun, off, hidden] {
    s.set_bounds(n, Some(flat)).expect("bounds");
  }
  flush(&mut s);
  let mut all = s.overlap([-1.0, -1.0, -1.0, 11.0, 1.0, 1.0]);
  all.sort_unstable();
  let mut expect = vec![spun, off];
  expect.sort_unstable();
  assert_eq!(all, expect, "hidden nodes never report");
  // Inside the rotated square's world AABB but outside the square itself
  // (the 45-degree diamond ends at |x| + |y| = sqrt(0.5)): the separating
  // axes make the test exact, never AABB-conservative.
  assert!(s.overlap([0.55, 0.55, -1.0, 0.8, 0.8, 1.0]).is_empty());
  // A point query is the degenerate box, same edge.
  assert_eq!(s.overlap([0.6, 0.0, 0.0, 0.6, 0.0, 0.0]), vec![spun]);
  assert!(s.overlap([0.6, 0.6, 0.0, 0.6, 0.6, 0.0]).is_empty());
  // Moves land at the flush, like raycast.
  s.set_transform(off, [20.0, 0.0, 0.0], Q, ONE).expect("move");
  assert_eq!(s.overlap([9.0, -1.0, -1.0, 11.0, 1.0, 1.0]), vec![off], "pre-flush query sees the old pose");
  flush(&mut s);
  assert!(s.overlap([9.0, -1.0, -1.0, 11.0, 1.0, 1.0]).is_empty());
  assert_eq!(s.overlap([19.0, -1.0, -1.0, 21.0, 1.0, 1.0]), vec![off]);
}

#[test]
fn triangle_hit_carries_face_uv_and_normal() {
  let mut s = Spatial::new();
  let n = s.create([0.0, 0.0, -3.0], Q, [2.0, 2.0, 2.0], true);
  let shape = s.create_shape(quad_shape(true)).expect("shape");
  s.set_bounds(n, Some([-1.0, -1.0, 0.0, 1.0, 1.0, 0.0])).expect("bounds");
  s.set_shape(n, Some(shape)).expect("shape");
  flush(&mut s);
  // Through the upper-left quadrant: world (-1, 1) is local (-0.5, 0.5)
  // after the 2x scale; that is the second triangle (0, 2, 3).
  let hits = s.raycast([-1.0, 1.0, 0.0], [0.0, 0.0, -1.0]);
  assert_eq!(hits.len(), 1);
  let h = &hits[0];
  assert!((h.distance - 3.0).abs() < 1e-5);
  assert_eq!(h.face, Some(1));
  let uv = h.uv.expect("uv");
  assert!((uv[0] - 0.25).abs() < 1e-5 && (uv[1] - 0.75).abs() < 1e-5, "uv {uv:?}");
  assert_eq!(h.normal, Some([0.0, 0.0, 1.0]), "normal faces the ray");
  // Outside the quad: the triangle test says miss.
  assert!(s.raycast([2.5, 2.5, 0.0], [0.0, 0.0, -1.0]).is_empty());
  // From behind, the normal flips to face the ray.
  let back = s.raycast([0.0, 0.0, -6.0], [0.0, 0.0, 1.0]);
  assert_eq!(back[0].normal, Some([0.0, 0.0, -1.0]));
  s.destroy_shape(shape).expect("destroy");
  assert!(s.set_shape(n, Some(shape)).is_err());
  // A node whose shape is gone falls back to its box.
  assert_eq!(s.raycast([0.0, 0.0, 0.0], [0.0, 0.0, -1.0])[0].face, None);
}

// A heightfield over x/z, `side` cells square, two triangles per cell:
// vertex heights vary so faces are not coplanar, UVs span 0..1.
fn grid_shape(side: usize) -> Shape {
  let verts = side + 1;
  let mut positions = Vec::with_capacity(verts * verts * 3);
  let mut uvs = Vec::with_capacity(verts * verts * 2);
  for z in 0..verts {
    for x in 0..verts {
      let h = ((x * 3 + z * 5) % 7) as f32 * 0.1;
      positions.extend_from_slice(&[x as f32, h, z as f32]);
      uvs.extend_from_slice(&[x as f32 / side as f32, z as f32 / side as f32]);
    }
  }
  let mut indices = Vec::with_capacity(side * side * 6);
  for z in 0..side {
    for x in 0..side {
      let a = (z * verts + x) as u32;
      let b = a + 1;
      let c = a + verts as u32;
      let d = c + 1;
      indices.extend_from_slice(&[a, b, c, b, d, c]);
    }
  }
  Shape { positions, uvs: Some(uvs), indices }
}

#[test]
fn shape_bvh_matches_the_linear_oracle() {
  // A heightfield well over the indexing threshold: the first ray builds
  // the shape's triangle BVH, and every later hit must match the
  // brute-force path exactly.
  let side = 24;
  let shape = grid_shape(side);
  assert!(shape.indices.len() / 3 >= BVH_MIN_TRIANGLES * 2, "the grid must be big enough to index");
  let oracle = grid_shape(side);
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let sid = s.create_shape(shape).expect("shape");
  s.set_bounds(n, Some([0.0, 0.0, 0.0, side as f32, 1.0, side as f32])).expect("bounds");
  s.set_shape(n, Some(sid)).expect("set shape");
  flush(&mut s);
  let mut state: u32 = 99;
  let mut rand = move || {
    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    (state >> 8) as f32 / (1u32 << 24) as f32
  };
  let mut hit_count = 0;
  for _ in 0..200 {
    let o = [rand() * (side as f32 + 4.0) - 2.0, 5.0, rand() * (side as f32 + 4.0) - 2.0];
    // Near-vertical rays land in triangle interiors (an exact edge is
    // measure-zero), so face ids compare exactly.
    let d = [rand() * 0.2 - 0.1, -1.0, rand() * 0.2 - 0.1];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let dn = [d[0] / len, d[1] / len, d[2] / len];
    let got = s.raycast(o, d);
    match ray_shape(&oracle, o, dn) {
      Some((t, face, uv, _)) => {
        hit_count += 1;
        assert_eq!(got.len(), 1, "one hit for a ray the oracle hits");
        let h = &got[0];
        assert!((h.distance - t).abs() < 1e-3, "distance {} vs oracle {t}", h.distance);
        assert_eq!(h.face, Some(face));
        let (gu, wu) = (h.uv.expect("uv"), uv.expect("oracle uv"));
        assert!((gu[0] - wu[0]).abs() < 1e-3 && (gu[1] - wu[1]).abs() < 1e-3, "uv {gu:?} vs {wu:?}");
        let normal = h.normal.expect("normal");
        assert!(normal[1] > 0.0, "a downward ray sees the up-facing side");
      }
      None => assert!(got.is_empty(), "the index must not invent hits"),
    }
  }
  assert!(hit_count > 100, "the sweep must mostly hit the grid ({hit_count} of 200)");
}

#[test]
fn shape_slots_rebuild_their_index_on_reuse() {
  let mut s = Spatial::new();
  let n = s.create([0.0; 3], Q, ONE, true);
  let big = s.create_shape(grid_shape(12)).expect("shape");
  s.set_bounds(n, Some([0.0, 0.0, 0.0, 12.0, 1.0, 12.0])).expect("bounds");
  s.set_shape(n, Some(big)).expect("set shape");
  flush(&mut s);
  assert!(!s.raycast([6.0, 5.0, 6.0], [0.0, -1.0, 0.0]).is_empty(), "the first ray builds the index and hits");
  // Destroying frees the slot; the next create reuses it, and a stale
  // index from the old geometry must not survive into the new shape.
  s.set_shape(n, None).expect("clear shape");
  s.destroy_shape(big).expect("destroy");
  let mut moved = grid_shape(12);
  for p in moved.positions.chunks_exact_mut(3) {
    p[0] += 20.0;
  }
  let again = s.create_shape(moved).expect("reused slot");
  s.set_bounds(n, Some([20.0, 0.0, 0.0, 32.0, 1.0, 12.0])).expect("bounds 2");
  s.set_shape(n, Some(again)).expect("set shape 2");
  flush(&mut s);
  assert!(s.raycast([6.0, 5.0, 6.0], [0.0, -1.0, 0.0]).is_empty(), "the old geometry is gone");
  assert!(!s.raycast([26.0, 5.0, 6.0], [0.0, -1.0, 0.0]).is_empty(), "the reused slot indexes the new shape");
}

#[test]
fn identical_centroids_still_split_and_hit() {
  // Coincident quads, every centroid equal: the worst case for a median
  // split. The build must terminate (the median splits by count) and the
  // ray still reports the shared plane.
  let mut positions = Vec::new();
  let mut indices = Vec::new();
  for k in 0..BVH_MIN_TRIANGLES as u32 {
    let b = k * 4;
    positions.extend_from_slice(&[-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0]);
    indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
  }
  let mut s = Spatial::new();
  let n = s.create([0.0, 0.0, -3.0], Q, ONE, true);
  let sid = s.create_shape(Shape { positions, uvs: None, indices }).expect("shape");
  s.set_bounds(n, Some([-1.0, -1.0, 0.0, 1.0, 1.0, 0.0])).expect("bounds");
  s.set_shape(n, Some(sid)).expect("set shape");
  flush(&mut s);
  let hits = s.raycast([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
  assert_eq!(hits.len(), 1);
  assert!((hits[0].distance - 3.0).abs() < 1e-5);
  assert!(hits[0].face.is_some());
}

#[test]
fn index_matches_linear_oracle() {
  // Deterministic LCG; boxes scattered, some moved, some removed, then
  // random rays against a brute-force oracle over the tight boxes.
  let mut state: u32 = 12345;
  let mut rand = move || {
    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    (state >> 8) as f32 / (1u32 << 24) as f32
  };
  let mut s = Spatial::new();
  let mut nodes: Vec<(NodeId, [f32; 3], [f32; 3])> = Vec::new();
  for _ in 0..200 {
    let p = [rand() * 40.0 - 20.0, rand() * 40.0 - 20.0, rand() * 40.0 - 20.0];
    let n = s.create(p, Q, ONE, true);
    let e = [rand() + 0.1, rand() + 0.1, rand() + 0.1];
    s.set_bounds(n, Some([-e[0], -e[1], -e[2], e[0], e[1], e[2]])).expect("bounds");
    nodes.push((n, p, e));
  }
  flush(&mut s);
  for k in 0..60 {
    let p = [rand() * 40.0 - 20.0, rand() * 40.0 - 20.0, rand() * 40.0 - 20.0];
    s.set_transform(nodes[k * 3].0, p, Q, ONE).expect("move");
    nodes[k * 3].1 = p;
  }
  for k in 0..20 {
    s.destroy(nodes[k * 7 + 1].0).expect("destroy");
    nodes[k * 7 + 1].2 = [0.0; 3];
  }
  flush(&mut s);
  for _ in 0..100 {
    let o = [rand() * 60.0 - 30.0, rand() * 60.0 - 30.0, rand() * 60.0 - 30.0];
    let d = [rand() - 0.5, rand() - 0.5, rand() - 0.5];
    let got: Vec<NodeId> = s.raycast(o, d).into_iter().map(|h| h.node).collect();
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let dn = [d[0] / len, d[1] / len, d[2] / len];
    let mut want: Vec<(f32, NodeId)> = nodes
      .iter()
      .filter(|(_, _, e)| e[0] > 0.0)
      .filter_map(|(n, p, e)| {
        let b = [p[0] - e[0], p[1] - e[1], p[2] - e[2], p[0] + e[0], p[1] + e[1], p[2] + e[2]];
        ray_box_distance(o, dn, &b).map(|t| (t, *n))
      })
      .collect();
    want.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("ordered"));
    let want: Vec<NodeId> = want.into_iter().map(|(_, n)| n).collect();
    assert_eq!(got, want);
  }
}

#[test]
fn ordered_insertion_stays_shallow() {
  // A grid inserted row by row is the adversarial order for an SAH tree
  // without rotations (it degenerates into chains hundreds deep); the
  // rotations must keep it near log2(n).
  let mut s = Spatial::new();
  let side = 55;
  for i in 0..side * side {
    let x = (i % side) as f32;
    let z = (i / side) as f32;
    let n = s.create([x, 0.0, z], Q, ONE, true);
    s.set_bounds(n, Some([-0.3, -0.3, -0.3, 0.3, 0.3, 0.3])).expect("bounds");
  }
  flush(&mut s);
  let depth = s.index_depth();
  assert!(depth <= 40, "grid of {} leaves built a tree {depth} deep", side * side);
}

use crate::spatial::{Projection, SharedSlotSink};

#[test]
fn shared_slots_follow_rotation_zero_on_unbind_and_drop_their_group() {
  let mut s = Spatial::new();
  let root = s.create([0.0; 3], Q, ONE, true);
  let a = s.create([0.0; 3], Q, ONE, true);
  let b = s.create([0.0; 3], Q, ONE, true);
  s.set_parent(a, Some(root)).expect("parent");
  s.set_parent(b, Some(root)).expect("parent");
  let sink = |index: u32, v: [f32; 3]| SharedSlotSink {
    target: 9,
    name: "uLightDir".to_string(),
    len: 6,
    index,
    projection: Projection::Direction(v),
  };
  // The scaled local vector normalizes away; slot 1 starts as zeros.
  s.bind_shared_slot(a, sink(0, [0.0, -2.0, 0.0])).expect("bind");
  let writes = flush(&mut s);
  assert_eq!(
    writes,
    vec![Write::Shared { target: 9, name: "uLightDir".to_string(), values: vec![0.0, -1.0, 0.0, 0.0, 0.0, 0.0] }]
  );
  // Rotating an ANCESTOR re-emits with the rotated direction: 90 degrees
  // about z carries -y to +x. An unrelated move re-emits nothing.
  let half = (0.5f32).sqrt();
  s.set_transform(root, [0.0; 3], [0.0, 0.0, half, half], ONE).expect("rotate");
  let writes = flush(&mut s);
  let Write::Shared { values, .. } = &writes[0] else { panic!("expected shared write") };
  assert!((values[0] - 1.0).abs() < 1e-5 && values[1].abs() < 1e-5, "rotated slot {values:?}");
  s.set_transform(b, [5.0, 0.0, 0.0], [0.0, 0.0, half, half], ONE).expect("move");
  assert!(flush(&mut s).is_empty(), "translation changes no direction");
  // A second sink shares the group; slot len mismatch is rejected.
  s.bind_shared_slot(b, sink(1, [0.0, 0.0, 1.0])).expect("bind");
  assert!(s.bind_shared_slot(b, SharedSlotSink { len: 9, ..sink(1, [0.0, 0.0, 1.0]) }).is_err());
  flush(&mut s);
  // Unbind zeroes the slot; destroying the last holder emits the final
  // zeroed array and drops the group (no further writes).
  s.unbind_shared_slot(b, None).expect("unbind");
  let writes = flush(&mut s);
  let Write::Shared { values, .. } = &writes[0] else { panic!("expected shared write") };
  assert_eq!(values[3..6], [0.0, 0.0, 0.0]);
  s.destroy(a).expect("destroy");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Shared { target: 9, name: "uLightDir".to_string(), values: vec![0.0; 6] }]);
  assert!(flush(&mut s).is_empty());
}

use crate::spatial::{InstanceProjection, InstanceRecordSink};

fn record(buffer: u64, index: u32) -> Option<InstanceRecordSink> {
  Some(InstanceRecordSink { buffer, index, projection: InstanceProjection::Pose2D })
}

#[test]
fn pose_records_decompose_world_and_preserve_mirroring() {
  let mut s = Spatial::new();
  let root = s.create([10.0, 20.0, 0.0], Q, ONE, true);
  let child = s.create([0.0; 3], [0.0, 0.0, (0.5f32).sqrt(), (0.5f32).sqrt()], [2.0, 3.0, 1.0], true);
  s.set_parent(child, Some(root)).expect("parent");
  s.set_instance_record(child, record(4, 0)).expect("bind");
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 1);
  let Write::Instances { buffer: 4, first: 0, values } = &writes[0] else { panic!("expected instances") };
  assert!((values[0] - 10.0).abs() < 1e-5 && (values[1] - 20.0).abs() < 1e-5, "translation {values:?}");
  assert!((values[2] - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "angle {values:?}");
  assert!((values[3] - 2.0).abs() < 1e-5 && (values[4] - 3.0).abs() < 1e-5, "scale {values:?}");
  // A mirroring matrix keeps positive sx and carries the flip in sy.
  s.set_transform(child, [0.0; 3], Q, [-2.0, 3.0, 1.0]).expect("mirror");
  let writes = flush(&mut s);
  let Write::Instances { values, .. } = &writes[0] else { panic!("expected instances") };
  assert!((values[2] - std::f32::consts::PI).abs() < 1e-5, "mirror angle {values:?}");
  assert!((values[3] - 2.0).abs() < 1e-5 && (values[4] + 3.0).abs() < 1e-5, "mirror scale {values:?}");
}

#[test]
fn pose_records_batch_into_one_coalesced_write() {
  let mut s = Spatial::new();
  let a = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let b = s.create([2.0, 0.0, 0.0], Q, ONE, true);
  s.set_instance_record(a, record(4, 0)).expect("bind");
  s.set_instance_record(b, record(4, 3)).expect("bind");
  // Both slots dirty: ONE write spanning them, unbound slots 1-2 zeros.
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 1);
  let Write::Instances { buffer: 4, first: 0, values } = &writes[0] else { panic!("expected instances") };
  assert_eq!(values.len(), 20);
  assert_eq!(values[0..5], [1.0, 0.0, 0.0, 1.0, 1.0]);
  assert_eq!(values[5..15], [0.0; 10]);
  assert_eq!(values[15..20], [2.0, 0.0, 0.0, 1.0, 1.0]);
  // Only b moves: the write shrinks to its slot.
  s.set_transform(b, [5.0, 6.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 15, values: vec![5.0, 6.0, 0.0, 1.0, 1.0] }]);
  // A write landing on the identical pose publishes nothing.
  s.set_transform(b, [5.0, 6.0, 0.0], Q, ONE).expect("rewrite");
  assert!(flush(&mut s).is_empty());
}

#[test]
fn hidden_record_slots_zero_and_the_group_drops_with_its_last_sink() {
  let mut s = Spatial::new();
  let a = s.create([1.0, 2.0, 0.0], Q, ONE, true);
  s.set_instance_record(a, record(4, 0)).expect("bind");
  flush(&mut s);
  // Hiding zeroes the slot (zero scale collapses the instance).
  s.set_visible(a, false).expect("hide");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 0, values: vec![0.0; 5] }]);
  // Unhiding rewrites the pose even though the matrix never changed.
  s.set_visible(a, true).expect("show");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 0, values: vec![1.0, 2.0, 0.0, 1.0, 1.0] }]);
  // Unbinding zeroes and drops the group with the final write: a later
  // move of the node publishes nothing.
  s.set_instance_record(a, None).expect("unbind");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 0, values: vec![0.0; 5] }]);
  s.set_transform(a, [9.0, 9.0, 0.0], Q, ONE).expect("move");
  assert!(flush(&mut s).is_empty());
}

#[test]
fn retargeting_records_republishes_whole_to_the_new_buffer() {
  let mut s = Spatial::new();
  let a = s.create([1.0, 2.0, 0.0], Q, ONE, true);
  let b = s.create([3.0, 4.0, 0.0], Q, ONE, true);
  s.set_instance_record(a, record(4, 0)).expect("bind");
  s.set_instance_record(b, record(4, 1)).expect("bind");
  flush(&mut s);
  assert_eq!(s.records_extent(4), Some(10));
  assert_eq!(s.records_extent(9), None);
  // The growth swap: everything republishes into the new buffer at the
  // next flush, slots unchanged, even though no node moved.
  s.retarget_records(4, 9).expect("retarget");
  assert_eq!(s.records_extent(9), Some(10));
  let writes = flush(&mut s);
  assert_eq!(
    writes,
    vec![Write::Instances { buffer: 9, first: 0, values: vec![1.0, 2.0, 0.0, 1.0, 1.0, 3.0, 4.0, 0.0, 1.0, 1.0] }]
  );
  // Later writes follow the sinks to the new buffer; the old id is inert
  // and free for an unrelated fresh group.
  s.set_transform(b, [5.0, 6.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 9, first: 5, values: vec![5.0, 6.0, 0.0, 1.0, 1.0] }]);
  let c = s.create([7.0, 0.0, 0.0], Q, ONE, true);
  s.set_instance_record(c, record(4, 0)).expect("rebind old id");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 0, values: vec![7.0, 0.0, 0.0, 1.0, 1.0] }]);
  // Errors: no records on the source; a destination already carrying some.
  let err = s.retarget_records(77, 9).expect_err("empty source must error");
  assert!(err.contains("no instance records"), "{err}");
  let err = s.retarget_records(4, 9).expect_err("occupied destination must error");
  assert!(err.contains("already carries"), "{err}");
}

#[test]
fn destroying_a_bound_node_zeroes_its_slot() {
  let mut s = Spatial::new();
  let a = s.create([1.0, 2.0, 0.0], Q, ONE, true);
  let b = s.create([3.0, 4.0, 0.0], Q, ONE, true);
  s.set_instance_record(a, record(4, 0)).expect("bind");
  s.set_instance_record(b, record(4, 1)).expect("bind");
  flush(&mut s);
  s.destroy(a).expect("destroy");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 0, values: vec![0.0; 5] }]);
  // The surviving sink keeps the group alive.
  s.set_transform(b, [7.0, 8.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  assert_eq!(writes, vec![Write::Instances { buffer: 4, first: 5, values: vec![7.0, 8.0, 0.0, 1.0, 1.0] }]);
}
