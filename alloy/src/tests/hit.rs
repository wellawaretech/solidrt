use crate::rendertree::hit::{locals_along_path, DefaultHitTester, HitTester};
use crate::rendertree::*;

fn attached() -> Element {
  View::default().with_layout()
}

// Writes a computed layout directly: unit tests have no GPU/platform context,
// so taffy never runs and placements are set by hand.
fn place(tree: &mut RenderTree, id: u64, x: f32, y: f32, w: f32, h: f32) {
  let l = tree.node_mut(id).layout_data_mut();
  l.computed.location = taffy::Point { x, y };
  l.computed.size = taffy::Size { width: w, height: h };
}

fn assert_xy(got: Point, x: f32, y: f32) {
  let eps = 1e-3;
  assert!((got.x - x).abs() < eps && (got.y - y).abs() < eps, "expected ({x}, {y}), got ({}, {})", got.x, got.y);
}

#[test]
fn locals_compose_translations() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 10.0, 20.0, 100.0, 100.0);
  place(&mut tree, 3, 5.0, 5.0, 20.0, 20.0);

  let locals = locals_along_path(&tree, &[1, 2, 3], Point::new(40.0, 50.0));
  assert_eq!(locals.len(), 3);
  assert_xy(locals[0], 40.0, 50.0);
  assert_xy(locals[1], 30.0, 30.0);
  assert_xy(locals[2], 25.0, 25.0);
}

#[test]
fn locals_exact_outside_bounds() {
  // A live hit test would reject these points; the projection must not care.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_scale_x(0.5);
  v.set_scale_y(0.5);
  tree.create_node(2, v.with_layout());
  tree.insert_node(1, 2, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 50.0, 50.0, 100.0, 100.0);

  // Scale 0.5 around the child's center (50, 50): the inverse doubles the
  // offset from the center. (160, 160) window -> (110, 110) in the child's
  // parent slot -> (170, 170) local, well outside the 100x100 box.
  let locals = locals_along_path(&tree, &[1, 2], Point::new(160.0, 160.0));
  assert_eq!(locals.len(), 2);
  assert_xy(locals[1], 170.0, 170.0);
}

#[test]
fn locals_match_live_hit_test() {
  // The projection replays hit_recursive's math; on a chain the pointer is
  // actually over, both must agree exactly, transforms included.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_rotate(std::f32::consts::FRAC_PI_2);
  v.set_scale_x(2.0);
  v.set_scale_y(2.0);
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 40.0, 40.0, 100.0, 100.0);
  place(&mut tree, 3, 10.0, 10.0, 60.0, 60.0);

  // Slightly off node 2's transform center (90, 90 in window space), so the
  // rotation and scale inversions are exercised but the whole chain stays hit.
  let point = Point::new(94.0, 92.0);
  let path = DefaultHitTester.hit_test(&tree, point);
  assert_eq!(path.len(), 3, "point must be over the whole chain for this test");
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  let locals = locals_along_path(&tree, &ids, point);
  assert_eq!(locals.len(), path.len());
  for (i, &(_, _, expected)) in path.iter().enumerate() {
    assert_xy(locals[i], expected.x, expected.y);
  }
}

#[test]
fn view_box_bounds_measured_in_design_space() {
  // The recommended fixed-aspect pattern (`<view flex={1} viewBox={[W, H]}>`)
  // routinely has a design space wider than its box. The view's local point is
  // in design units, so testing it against the BOX numbers used to reject
  // everything past design x = box width - and a rejected view drops its whole
  // subtree, so the scene went pointer-dead on that side.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_view_box(800.0, 500.0);
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 2, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 3, 600.0, 100.0, 100.0, 100.0);

  // Fit scale 0.5 (400/800 is the tighter axis), centered vertically by
  // (300 - 500*0.5) / 2 = 25. Design (650, 150) -> window (325, 100), which is
  // past the box width of 400 in design units but well inside the design space.
  let path = DefaultHitTester.hit_test(&tree, Point::new(325.0, 100.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 3]);
  // The child's local point is in design units too, no scale factor in sight.
  assert_xy(path[2].2, 50.0, 50.0);

  // The flip side of design-space bounds: the letterbox bars map OUTSIDE the
  // design space (window y = 10 -> design y = -30 here), so the view misses
  // there and the point falls through to whatever is behind it.
  let path = DefaultHitTester.hit_test(&tree, Point::new(200.0, 10.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1]);
}

#[test]
fn locals_truncate_at_missing_node() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.insert_node(1, 2, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 10.0, 10.0, 100.0, 100.0);

  // Node 99 died mid-drag; the frames below it are meaningless.
  let locals = locals_along_path(&tree, &[1, 99, 2], Point::new(50.0, 50.0));
  assert_eq!(locals.len(), 1);
  assert_xy(locals[0], 50.0, 50.0);
}
