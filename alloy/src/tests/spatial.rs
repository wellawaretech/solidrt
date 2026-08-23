use crate::spatial::{compose, multiply, DrawSink, SinkWrite, Spatial, IDENTITY};

const Q: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f32; 3] = [1.0, 1.0, 1.0];

fn sink(draw: u64) -> Option<DrawSink> {
  Some(DrawSink { target: 1, draw, normal: false, count: 1 })
}

fn flush(s: &mut Spatial) -> Vec<SinkWrite> {
  let mut out = Vec::new();
  s.flush(&mut |w| out.push(w));
  out
}

fn model(w: &SinkWrite) -> [f32; 16] {
  match w {
    SinkWrite::Params { model, .. } => *model,
    other => panic!("expected params, got {other:?}"),
  }
}

#[test]
fn first_flush_switches_entry_on_and_writes_world() {
  let mut s = Spatial::new();
  let root = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let child = s.create([0.0, 2.0, 0.0], Q, [2.0, 2.0, 2.0], true);
  s.set_parent(child, Some(root)).expect("parent");
  s.set_sink(child, sink(7)).expect("sink");
  let writes = flush(&mut s);
  assert_eq!(writes.len(), 2);
  assert_eq!(writes[0], SinkWrite::Count { target: 1, draw: 7, count: 1 });
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
    s.set_sink(n, sink(d)).expect("sink");
  }
  flush(&mut s);
  s.set_transform(a, [5.0, 0.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  let draws: Vec<u64> = writes
    .iter()
    .map(|w| match w {
      SinkWrite::Params { draw, .. } => *draw,
      SinkWrite::Count { draw, .. } => *draw,
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
  s.set_sink(m, Some(DrawSink { target: 1, draw: 9, normal: false, count: 4 })).expect("sink");
  flush(&mut s);
  s.set_visible(root, false).expect("hide");
  assert_eq!(flush(&mut s), vec![SinkWrite::Count { target: 1, draw: 9, count: 0 }]);
  assert!(!s.shown(m).expect("alive"));
  // Moved while hidden: no write now, a params write on unhide.
  s.set_transform(m, [3.0, 0.0, 0.0], Q, ONE).expect("move");
  assert!(flush(&mut s).is_empty());
  s.set_visible(root, true).expect("show");
  let writes = flush(&mut s);
  assert_eq!(writes[0], SinkWrite::Count { target: 1, draw: 9, count: 4 });
  assert_eq!(model(&writes[1])[12], 3.0);
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
  s.set_sink(m, sink(1)).expect("sink");
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

use crate::spatial::{ray_box_distance, NodeId, Shape};

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
