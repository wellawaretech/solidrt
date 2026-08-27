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
  v.set_scale_x(Some(0.5));
  v.set_scale_y(Some(0.5));
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
  v.set_rotate(Some(std::f32::consts::FRAC_PI_2));
  v.set_scale_x(Some(2.0));
  v.set_scale_y(Some(2.0));
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
fn design_size_bounds_measured_in_design_space() {
  // The recommended fixed-aspect pattern (`<view flex={1} designSize={[W, H]}>`)
  // routinely has a design space wider than its box. The view's local point is
  // in design units, so testing it against the BOX numbers used to reject
  // everything past design x = box width - and a rejected view drops its whole
  // subtree, so the scene went pointer-dead on that side.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((800.0, 500.0)));
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

// Sets both overflow axes to Hidden on an already-created node, the way the
// layout plugin would; unit tests write the style directly.
fn hide_overflow(tree: &mut RenderTree, id: u64) {
  let l = tree.node_mut(id).layout_data_mut();
  l.style.overflow = taffy::Point { x: taffy::style::Overflow::Hidden, y: taffy::style::Overflow::Hidden };
}

#[test]
fn overflow_gate_is_box_space_under_minifying_design_size() {
  // overflow + designSize on one view, design LARGER than the box (fit scale
  // 0.5). The overflow clip means the layout box; measured in design units it
  // would cut at half the box and reject content in the visible bottom-right
  // quadrant (okf/backlog/overflow-viewbox-clip.md, the paper-crane/unimog
  // defect - this is the hit-side mirror of the paint fix).
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((200.0, 200.0)));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  hide_overflow(&mut tree, 2);
  // Child in the design space's bottom-right quadrant: painted at box
  // (75,75)-(100,100), fully inside the clip box.
  place(&mut tree, 3, 150.0, 150.0, 50.0, 50.0);

  // Window (80, 80) is design (160, 160): inside the box, inside the child.
  // A design-unit gate reads 160 >= 100 and drops the whole subtree.
  let path = DefaultHitTester.hit_test(&tree, Point::new(80.0, 80.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 3]);
  assert_xy(path[2].2, 10.0, 10.0);
}

#[test]
fn overflow_gate_is_box_space_under_magnifying_design_size() {
  // The opposite direction: design SMALLER than the box (fit scale 2). A
  // design-unit gate compares against the box number 100 and lets content a
  // whole box-width past the clip edge stay hittable.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((50.0, 50.0)));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  hide_overflow(&mut tree, 2);
  // Child spans design x 0..80 = box 0..160, overhanging the 100-wide box.
  place(&mut tree, 3, 0.0, 0.0, 80.0, 80.0);

  // Window (120, 20) is design (60, 10): inside the child's design rect but
  // past the clip box, so the view and its subtree must miss.
  let path = DefaultHitTester.hit_test(&tree, Point::new(120.0, 20.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1]);

  // Inside the box the same child still hits.
  let path = DefaultHitTester.hit_test(&tree, Point::new(90.0, 20.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn scroll_is_box_pixels_under_minifying_design_size() {
  // scroll + designSize: the offset means box pixels on every path, settled with
  // the box-space overflow clip (okf/backlog/overflow-viewbox-clip.md). Fit
  // scale 0.5, scroll 10 box px = 20 design units: the child's design edge at
  // 100 lands at window x = 40. An offset added raw in design units would
  // leave the edge at 45.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((200.0, 200.0)));
  v.set_scroll_x(Some(10.0));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 100.0);

  // Window (38, 20) is design (76, 40); adding the scroll in the children's
  // frame (20 design units) puts (96, 40) inside the child.
  let path = DefaultHitTester.hit_test(&tree, Point::new(38.0, 20.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 3]);
  assert_xy(path[2].2, 96.0, 40.0);

  // Window (42, 20) maps to design 104, past the child's edge - a raw
  // design-unit offset would read 94 and still hit.
  let path = DefaultHitTester.hit_test(&tree, Point::new(42.0, 20.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2]);
}

#[test]
fn scroll_is_box_pixels_under_magnifying_design_size() {
  // The opposite direction: fit scale 2, scroll 10 box px = 5 design units.
  // A raw design-unit offset overshoots by double, dropping content that is
  // still on screen.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((50.0, 50.0)));
  v.set_scroll_x(Some(10.0));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 40.0, 40.0);

  // Window (65, 20) is design (32.5, 10) + 5 scrolled = (37.5, 10), inside
  // the child's 40-wide design rect; a raw offset would read 42.5 and miss.
  let path = DefaultHitTester.hit_test(&tree, Point::new(65.0, 20.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 3]);
  assert_xy(path[2].2, 37.5, 10.0);

  // Past the scrolled edge ((40 - 5) * 2 = 70) the child misses.
  let path = DefaultHitTester.hit_test(&tree, Point::new(75.0, 20.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2]);
}

#[test]
fn design_size_fit_resolves_against_the_border_box_when_padded() {
  // A View's matrices (design-size fit, transform center) resolve against its
  // BORDER box on both the paint and hit paths; padding shrinks the content
  // box that kinds size against, never the fit
  // (okf/done/padding-box-divergence.md). Border box 100 wide with design
  // 200 is fit scale 0.5; a content-box fit (80 wide, scale 0.4) would map
  // window x = 95 to design 237.5 and reject the view entirely.
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_design_size(Some((200.0, 200.0)));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  tree.node_mut(2).layout_data_mut().computed.padding =
    taffy::Rect { left: 10.0, right: 10.0, top: 10.0, bottom: 10.0 };
  place(&mut tree, 3, 150.0, 0.0, 50.0, 100.0);

  // Window (95, 49) is design (190, 98) under the border-box fit: inside the
  // design space and inside the child at design x 150..200.
  let path = DefaultHitTester.hit_test(&tree, Point::new(95.0, 49.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 3]);
  assert_xy(path[2].2, 40.0, 98.0);
}

#[test]
fn content_box_insets_padding_and_border() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  place(&mut tree, 1, 0.0, 0.0, 100.0, 80.0);
  let l = tree.node_mut(1).layout_data_mut();
  l.computed.padding = taffy::Rect { left: 10.0, right: 6.0, top: 4.0, bottom: 2.0 };
  l.computed.border = taffy::Rect { left: 1.0, right: 1.0, top: 1.0, bottom: 1.0 };
  let c = tree.node(1).layout.as_ref().expect("laid out above").content_box();
  assert_xy(c.origin, 11.0, 5.0);
  assert_xy(Point::new(c.size.width, c.size.height), 82.0, 72.0);
}

#[test]
fn padded_rect_hits_its_border_box() {
  // A rect's default extent is the border box on paint and hit alike;
  // padding shrinks only the content box
  // (okf/done/padding-box-divergence.md).
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  tree.node_mut(2).layout_data_mut().computed.padding =
    taffy::Rect { left: 20.0, right: 20.0, top: 20.0, bottom: 20.0 };

  // Inside the padding ring (outside the content box): still a hit.
  let path = DefaultHitTester.hit_test(&tree, Point::new(95.0, 5.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2]);

  // Past the border box: a miss.
  let path = DefaultHitTester.hit_test(&tree, Point::new(105.0, 5.0));
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

#[test]
fn oval_hits_as_ellipse_not_box() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, Oval::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 50.0, 50.0, 100.0, 100.0);

  // The box corner is outside the inscribed ellipse: only the root hits.
  let path = DefaultHitTester.hit_test(&tree, Point::new(55.0, 55.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1]);

  // The center is inside the ellipse.
  let path = DefaultHitTester.hit_test(&tree, Point::new(100.0, 100.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2]);
}

// A display none subtree is skipped by the walk, not by its boxes
// (okf/done/display-none-subtree.md): stale boxes under the hidden pane do
// not hit, and the sibling behind them does.
#[test]
fn hidden_subtree_is_not_hit() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.create_node(4, attached());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.insert_node(1, 4, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 2, 0.0, 0.0, 0.0, 0.0);
  place(&mut tree, 3, 0.0, 0.0, 320.0, 300.0);
  place(&mut tree, 4, 0.0, 0.0, 400.0, 300.0);
  tree.node_mut(2).style_mut().expect("pane").display = taffy::style::Display::None;

  let path = DefaultHitTester.hit_test(&tree, Point::new(100.0, 100.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 4]);
}
