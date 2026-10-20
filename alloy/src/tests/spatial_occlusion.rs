// Occlusion culling (spatial/occlusion.rs, the occlusion pass and the
// second gate in mod.rs) pinned on the recording writer: a box behind a
// wall switches off and one in front stays on, the edge straddler and the
// near-plane straddler are never hidden, a moving or hidden occluder
// uncovers, instance and joint unions test as one box, the opt-outs
// (culling off, occlusion off, no view) hold, a node never occludes
// itself, the buffer only rebuilds when something moved, the triangle
// budget skips safely, the cull state names the gate, and the stats add
// up. The camera is the identity view-projection unless a test says
// otherwise: NDC is world space, z = -1 the near plane, +1 the far one.

use super::spatial::{flush, Write};
use crate::spatial::{compose, multiply, CullState, DrawOrder, DrawSink, Mat4, Occluder, Spatial, IDENTITY};

const Q: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f64; 3] = [1.0, 1.0, 1.0];
/// A box of 0.2 on a side around the origin.
const SMALL: [f64; 6] = [-0.1, -0.1, -0.1, 0.1, 0.1, 0.1];
/// A wall across most of the view at z = 0: 1.8 wide and tall.
const WALL_SIZE: f64 = 1.8;

fn sink(target: u64, draw: u64) -> DrawSink {
  DrawSink {
    target,
    draw,
    normal: false,
    count: 1,
    fade: false,
    params: true,
    order: DrawOrder::default(),
    order_feed: false,
  }
}

/// A shown node with a small box and a draw sink on target 1.
fn occludee(s: &mut Spatial, position: [f64; 3], draw: u64) -> u64 {
  let n = s.create(position, Q, ONE, true);
  s.set_bounds(n, Some(SMALL)).expect("bounds");
  s.bind_sink(n, sink(1, draw)).expect("sink");
  n
}

/// A wall quad occluder at the origin, facing the camera.
fn wall(s: &mut Spatial) -> u64 {
  let w = s.create([0.0; 3], Q, ONE, true);
  s.set_occluder(w, Some(Occluder::quad(WALL_SIZE, WALL_SIZE))).expect("occluder");
  w
}

/// The identity camera on target 1 with occlusion on.
fn identity_camera(s: &mut Spatial) {
  s.set_view(1, Some((IDENTITY, IDENTITY)));
  s.set_occlusion(1, true);
}

fn count_of(writes: &[Write], draw: u64) -> Option<u32> {
  writes.iter().rev().find_map(|w| match w {
    Write::Count { draw: d, count, .. } if *d == draw => Some(*count),
    _ => None,
  })
}

/// A GL perspective projection.
fn perspective(fov_y: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
  let f = 1.0 / (fov_y / 2.0).tan();
  let mut m = [0.0; 16];
  m[0] = f / aspect;
  m[5] = f;
  m[10] = (far + near) / (near - far);
  m[11] = -1.0;
  m[14] = 2.0 * far * near / (near - far);
  m
}

#[test]
fn a_box_behind_a_wall_is_hidden_and_one_in_front_is_not() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  wall(&mut s);
  let behind = occludee(&mut s, [0.0, 0.0, 0.5], 1);
  let front = occludee(&mut s, [0.0, 0.0, -0.5], 2);
  let beside = occludee(&mut s, [0.95, 0.0, 0.5], 3);
  let writes = flush(&mut s);
  assert_eq!(count_of(&writes, 1), None, "the hidden entry stays off: a new sink is off and gets no write");
  assert_eq!(count_of(&writes, 2), Some(1));
  assert_eq!(count_of(&writes, 3), Some(1), "a box past the wall's edge is not hidden");
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::Occluded));
  assert_eq!(s.cull_state(front, 1).expect("node"), Some(CullState::On));
  assert_eq!(s.cull_state(beside, 1).expect("node"), Some(CullState::On));
  let stats = s.occlusion_stats();
  assert_eq!(
    (stats.targets, stats.occluders, stats.triangles, stats.drawn, stats.builds, stats.occluded),
    (1, 1, 2, 2, 1, 1)
  );
}

#[test]
fn an_edge_straddler_and_a_near_plane_straddler_are_never_hidden() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  wall(&mut s);
  // Half past the wall's right edge (0.9) at x = 0.9: the rect reaches
  // uncovered texels, whose depth is infinite.
  let edge = occludee(&mut s, [0.9, 0.0, 0.5], 1);
  // Behind the wall in x and y, but a corner before the near plane.
  let near = s.create([0.0, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(near, Some([-0.1, -0.1, -1.2, 0.1, 0.1, 0.5])).expect("bounds");
  s.bind_sink(near, sink(1, 2)).expect("sink");
  let writes = flush(&mut s);
  assert_eq!(count_of(&writes, 1), Some(1));
  assert_eq!(count_of(&writes, 2), Some(1));
  assert_eq!(s.cull_state(edge, 1).expect("node"), Some(CullState::On));
}

#[test]
fn a_moving_or_hidden_occluder_uncovers_and_a_still_scene_rebuilds_nothing() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  let door = wall(&mut s);
  let behind = occludee(&mut s, [0.0, 0.0, 0.5], 1);
  flush(&mut s);
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::Occluded));
  assert!(flush(&mut s).is_empty(), "nothing moved: no rebuild, no write");
  assert_eq!(s.occlusion_stats().builds, 1);
  // The door swings away, out of the view: one build, its triangles
  // taken but clipped away whole, the box comes on with its params.
  s.set_transform(door, [5.0, 0.0, 0.0], Q, ONE).expect("move");
  let writes = flush(&mut s);
  assert_eq!(count_of(&writes, 1), Some(1));
  let stats = s.occlusion_stats();
  assert_eq!((stats.builds, stats.triangles, stats.drawn), (2, 2, 0));
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::On));
  // Back, hidden: a hidden occluder occludes nothing.
  s.set_transform(door, [0.0; 3], Q, ONE).expect("move");
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
  s.set_visible(door, false).expect("hide");
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  s.set_visible(door, true).expect("show");
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
  // The camera turning rebuilds too; a camera write with the same
  // matrices does not.
  s.set_view(1, Some((IDENTITY, IDENTITY)));
  assert!(flush(&mut s).is_empty());
  let builds = s.occlusion_stats().builds;
  s.set_view(1, Some((compose([0.0, 0.0, 0.1], Q, ONE), IDENTITY)));
  flush(&mut s);
  assert_eq!(s.occlusion_stats().builds, builds + 1);
}

#[test]
fn unions_test_as_one_box() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  wall(&mut s);
  // A part culled by two joints: one behind the wall, one past its edge.
  let part = s.create([0.0; 3], Q, ONE, true);
  let joint_a = s.create([0.0, 0.0, 0.5], Q, ONE, true);
  let joint_b = s.create([0.0, 0.0, 0.5], Q, ONE, true);
  s.set_cull_bounds(joint_a, Some(SMALL)).expect("cull bounds");
  s.set_cull_bounds(joint_b, Some(SMALL)).expect("cull bounds");
  s.set_cull_group(part, &[joint_a, joint_b]).expect("group");
  s.bind_sink(part, sink(1, 1)).expect("sink");
  flush(&mut s);
  assert_eq!(s.cull_state(part, 1).expect("node"), Some(CullState::Occluded));
  // One joint swings out past the edge: the union reaches uncovered
  // texels and the part draws.
  s.set_transform(joint_b, [1.2, 0.0, 0.5], Q, ONE).expect("move");
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  s.set_transform(joint_b, [0.0, 0.0, 0.5], Q, ONE).expect("move");
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
}

#[test]
fn the_opt_outs_hold() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  wall(&mut s);
  let behind = occludee(&mut s, [0.0, 0.0, 0.5], 1);
  flush(&mut s);
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::Occluded));
  // Culling off on the node: drawn whatever is in front of it.
  s.set_cull(behind, false, 0.0).expect("cull");
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  s.set_cull(behind, true, 0.0).expect("cull");
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
  // A margin wide enough to reach past the wall's edge keeps it drawn.
  s.set_cull(behind, true, 1.0).expect("cull");
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  s.set_cull(behind, true, 0.0).expect("cull");
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
  // Occlusion off on the target: the buffer drops and the entry comes on.
  s.set_occlusion(1, false);
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  assert_eq!(s.occlusion_stats().targets, 0);
  s.set_occlusion(1, true);
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
  // A second target with occlusion off sees the box through the wall.
  s.bind_sink(behind, sink(2, 2)).expect("sink");
  s.set_view(2, Some((IDENTITY, IDENTITY)));
  let writes = flush(&mut s);
  assert_eq!(count_of(&writes, 2), Some(1));
  assert_eq!(count_of(&writes, 1), None, "target 1 is untouched by target 2's view");
  // The view forgotten: its occlusion goes with it, nothing is hidden.
  s.set_view(1, None);
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  assert_eq!(s.occlusion_stats().targets, 0);
}

#[test]
fn a_node_never_occludes_itself_and_an_occluder_can_be_occluded() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  // A box occluder that also draws, in front of the camera.
  let front = s.create([0.0, 0.0, -0.5], Q, ONE, true);
  s.set_bounds(front, Some(SMALL)).expect("bounds");
  s.set_occluder(front, Some(Occluder::box3(SMALL))).expect("occluder");
  s.bind_sink(front, sink(1, 1)).expect("sink");
  // A second box occluder behind it, smaller on screen: hidden by the
  // first, while still occluding what sits behind it in turn.
  let middle = s.create([0.0, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(middle, Some([-0.05, -0.05, -0.05, 0.05, 0.05, 0.05])).expect("bounds");
  s.set_occluder(middle, Some(Occluder::box3([-0.05, -0.05, -0.05, 0.05, 0.05, 0.05]))).expect("occluder");
  s.bind_sink(middle, sink(1, 2)).expect("sink");
  let back = occludee(&mut s, [0.0, 0.0, 0.5], 3);
  s.set_bounds(back, Some([-0.02, -0.02, -0.02, 0.02, 0.02, 0.02])).expect("bounds");
  flush(&mut s);
  assert_eq!(s.cull_state(front, 1).expect("node"), Some(CullState::On));
  assert_eq!(s.cull_state(middle, 1).expect("node"), Some(CullState::Occluded));
  assert_eq!(s.cull_state(back, 1).expect("node"), Some(CullState::Occluded));
}

#[test]
fn a_perspective_camera_hides_what_a_wall_covers_and_a_sphere_is_solid() {
  let mut s = Spatial::new();
  // The camera at the origin looking down -z (the GL convention), the
  // view the identity.
  let proj = perspective(1.0, 16.0 / 9.0, 0.1, 100.0);
  s.set_view(1, Some((IDENTITY, proj)));
  s.set_occlusion(1, true);
  // A wall 4 units ahead, 2 by 2; a box 8 units ahead behind its middle,
  // one 8 units ahead but 3 to the side.
  let w = s.create([0.0, 0.0, -4.0], Q, ONE, true);
  s.set_occluder(w, Some(Occluder::quad(2.0, 2.0))).expect("occluder");
  let behind = occludee(&mut s, [0.0, 0.0, -8.0], 1);
  let aside = occludee(&mut s, [3.0, 0.0, -8.0], 2);
  let before = occludee(&mut s, [0.0, 0.0, -2.0], 3);
  flush(&mut s);
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::Occluded));
  assert_eq!(s.cull_state(aside, 1).expect("node"), Some(CullState::On));
  assert_eq!(s.cull_state(before, 1).expect("node"), Some(CullState::On));
  // A sphere occluder of radius 1 at the wall's place hides a small box
  // straight behind its center, not one just outside its silhouette
  // (the cone from the eye: 14.5 degrees; a box at 2.4 across at 8 ahead
  // sits at 16.7).
  s.set_occluder(w, Some(Occluder::sphere([0.0; 3], 1.0))).expect("occluder");
  let rim = occludee(&mut s, [2.4, 0.0, -8.0], 4);
  flush(&mut s);
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::Occluded));
  assert_eq!(s.cull_state(rim, 1).expect("node"), Some(CullState::On));
}

#[test]
fn a_mesh_occluder_occludes_only_behind_its_triangles_and_the_budget_skips_safely() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  // One triangle covering the view's lower-left half.
  let m = s.create([0.0; 3], Q, ONE, true);
  let tri = Occluder::mesh(vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, 1.0, 0.0]], vec![[0, 1, 2]]).expect("mesh");
  s.set_occluder(m, Some(tri)).expect("occluder");
  let covered = occludee(&mut s, [-0.5, -0.5, 0.5], 1);
  let open = occludee(&mut s, [0.5, 0.5, 0.5], 2);
  flush(&mut s);
  assert_eq!(s.cull_state(covered, 1).expect("node"), Some(CullState::Occluded));
  assert_eq!(s.cull_state(open, 1).expect("node"), Some(CullState::On));
  assert!(Occluder::mesh(vec![[0.0; 3]], vec![[0, 1, 2]]).is_err(), "an index past the positions is refused");
  // A mesh past the budget: its triangles beyond it are skipped, so the
  // wall it would have been hides nothing, and the stats say how many.
  let many: Vec<[u32; 3]> = (0..(crate::spatial::OCCLUDER_TRIANGLE_BUDGET + 8) as u32).map(|_| [0, 1, 2]).collect();
  let big = Occluder::mesh(vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, 1.0, 0.0]], many).expect("mesh");
  let far = s.create([0.0, 0.0, -0.1], Q, ONE, true);
  s.set_occluder(far, Some(big)).expect("occluder");
  flush(&mut s);
  let stats = s.occlusion_stats();
  assert_eq!(stats.skipped, 9, "the single triangle took one of the budget; the big mesh lost eight plus one");
  assert_eq!(stats.triangles, crate::spatial::OCCLUDER_TRIANGLE_BUDGET);
}

#[test]
fn clearing_an_occluder_or_destroying_its_node_uncovers() {
  let mut s = Spatial::new();
  identity_camera(&mut s);
  let w = wall(&mut s);
  let behind = occludee(&mut s, [0.0, 0.0, 0.5], 1);
  flush(&mut s);
  assert_eq!(s.cull_state(behind, 1).expect("node"), Some(CullState::Occluded));
  s.set_occluder(w, None).expect("clear");
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  assert_eq!(s.occlusion_stats().occluders, 0);
  let w2 = wall(&mut s);
  assert_eq!(count_of(&flush(&mut s), 1), Some(0));
  s.destroy(w2).expect("destroy");
  assert_eq!(count_of(&flush(&mut s), 1), Some(1));
  assert_eq!(multiply(IDENTITY, IDENTITY), IDENTITY);
}
