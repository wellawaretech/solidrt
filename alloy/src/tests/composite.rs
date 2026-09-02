use crate::impellers::DisplayListBuilder;
use crate::rendertree::composite::paint_phase;
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;

fn attached() -> Element {
  View::default().with_layout()
}

// A Context with no raster thread behind it: recording views and
// rectangles never sends a command, so a dangling channel is enough.
fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats)
}

fn size(tree: &mut RenderTree, id: u64, w: f32, h: f32) {
  let style = tree.node_mut(id).style_mut().expect("laid-out node");
  style.size = taffy::Size { width: length(w), height: length(h) };
}

// root(1, row, 400x300) > pane(2, 320 wide) > header(3), body(4)
//                       > detail(5, flex 1)
// Every node has a real box, so nothing is viewport-culled in the baseline.
fn split() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.create_node(4, Rectangle::default().with_layout());
  tree.create_node(5, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.insert_node(2, 4, None).expect("insert");
  tree.insert_node(1, 5, None).expect("insert");
  tree.root = Some(1);
  tree.node_mut(1).style_mut().expect("root").flex_direction = FlexDirection::Row;
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 320.0, 300.0);
  size(&mut tree, 3, 320.0, 51.0);
  size(&mut tree, 4, 320.0, 100.0);
  tree.node_mut(5).style_mut().expect("detail").flex_grow = 1.0;
  tree
}

// The paint walk never enters a display none subtree
// (okf/done/display-none-subtree.md): the pane and its two descendants
// drop out of the painted-node count.
#[test]
fn hidden_subtree_is_not_painted() {
  let mut tree = split();
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);

  let all = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy).nodes_painted;

  tree.edit(2, |e| {
    e.style_mut().expect("pane").display = Display::None;
    Damage::Layout
  });
  let hidden = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy).nodes_painted;
  assert_eq!(hidden, all - 3, "painted {hidden} of {all}");
}

// A capture inside a boundary whose cache is valid must still be serviced:
// the cached composite leg descends into a discarded builder to reach the
// node, and the cache itself stays reused (it is not re-recorded for the
// capture's sake). Headless, the readback RPC fails with the raster-thread
// error - which proves the walk REACHED the node; the never-reached failure
// would mean the cached boundary skipped it.
#[test]
fn capture_inside_cached_boundary_is_reached() {
  let mut tree = split();
  tree.edit(2, |el| {
    el.repaint_boundary = BoundaryMode::Recording;
    Damage::None
  });
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);

  let first = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(first.boundaries_recorded, 1, "first paint fills the pane's cache");

  let outcome = std::rc::Rc::new(std::cell::RefCell::new(None));
  let seen = outcome.clone();
  alloy.request_capture(3, Box::new(move |result| *seen.borrow_mut() = Some(result.map(|_| ()))));
  let second = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(second.boundaries_reused, 1, "the capture descent must not drop the cache");

  let result = outcome.borrow_mut().take().expect("capture callback must run during the paint");
  let err = result.expect_err("headless capture cannot read back");
  assert!(err.contains("raster thread exited"), "unexpected capture error: {err}");
}

// A capture whose node the walk cannot reach (absent, or under a hidden
// ancestor) fails with the never-reached message rather than hanging.
#[test]
fn capture_of_unreachable_node_fails_as_never_reached() {
  let mut tree = split();
  tree.edit(2, |e| {
    e.style_mut().expect("pane").display = Display::None;
    Damage::Layout
  });
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);

  let outcome = std::rc::Rc::new(std::cell::RefCell::new(None));
  for (id, seen) in [(99, outcome.clone()), (3, outcome.clone())] {
    alloy.request_capture(id, Box::new(move |result| *seen.borrow_mut() = Some(result.map(|_| ()))));
    paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
    let result = outcome.borrow_mut().take().expect("capture callback must run during the paint");
    let err = result.expect_err("unreachable capture must fail");
    assert!(err.contains("never reached"), "unexpected capture error for node {id}: {err}");
  }
}

// Nested: the descent through a cached boundary reaches a boundary inside it
// (serviced at its build_recursive entry), and the discarded walk is fully
// isolated - the frame's boundary stats report only the composite that
// actually drew, not the reuse the discarded descent replayed.
#[test]
fn capture_descent_through_nested_boundaries_is_isolated() {
  let mut tree = split();
  for id in [2, 3] {
    tree.edit(id, |el| {
      el.repaint_boundary = BoundaryMode::Recording;
      Damage::None
    });
  }
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);

  let first = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(first.boundaries_recorded, 2, "both boundaries record on the first paint");

  let outcome = std::rc::Rc::new(std::cell::RefCell::new(None));
  let seen = outcome.clone();
  alloy.request_capture(3, Box::new(move |result| *seen.borrow_mut() = Some(result.map(|_| ()))));
  let second = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(second.boundaries_reused, 1, "only the outer composite counts; the descent is discarded");
  assert_eq!(second.boundaries_recorded, 0, "nothing re-records for a capture");

  let result = outcome.borrow_mut().take().expect("capture callback must run during the paint");
  let err = result.expect_err("headless capture cannot read back");
  assert!(err.contains("raster thread exited"), "unexpected capture error: {err}");
}

// A backdrop panel baked inside a reused Recording boundary re-filters the
// live window at every replay, so damage within its reach must still widen
// to cover it on frames that never enter the subtree. The cache summarizes
// its panels at record time; the cached leg pushes the boundary's live
// extent in their place. Damage out of reach must stay tight.
#[test]
fn baked_backdrop_widens_reuse_frame_damage() {
  let mut tree = split();
  tree.edit(2, |el| {
    el.repaint_boundary = BoundaryMode::Recording;
    Damage::None
  });
  // The pane's header becomes a glass panel: 320x51 at the window origin.
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::View(v) => v.set_backdrop_filter(Some(FilterState { blur: Some(4.0), ..Default::default() })),
    _ => unreachable!(),
  });
  // Two specks OUTSIDE the boundary subtree (damaging a node inside it
  // would drop the cache): one over the glass, one in the far corner.
  let mut speck = |id: u64, x: f32, y: f32| {
    tree.create_node(id, attached());
    tree.insert_node(1, id, None).expect("insert");
    let style = tree.node_mut(id).style_mut().expect("speck");
    style.position = Position::Absolute;
    style.inset.left = length(x);
    style.inset.top = length(y);
    style.size = taffy::Size { width: length(10.0), height: length(10.0) };
  };
  speck(6, 300.0, 20.0);
  speck(7, 350.0, 250.0);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);

  let first = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(first.boundaries_recorded, 1);
  assert_eq!(first.damage_px, 400.0 * 300.0, "first frame is fully damaged");

  // Damage over the glass: the resolve must pull the whole panel in, not
  // just the speck, without going full-window.
  tree.edit(6, |_| Damage::Paint);
  let near = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(near.boundaries_reused, 1, "the pane must reuse its recording");
  assert!(
    near.damage_px >= 320.0 * 51.0 && near.damage_px < 400.0 * 300.0,
    "damage must widen to the baked panel without going full: {} px^2",
    near.damage_px
  );

  // Damage in the far corner, out of blur reach: no widening.
  tree.edit(7, |_| Damage::Paint);
  let far = paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, &alloy);
  assert_eq!(far.boundaries_reused, 1);
  assert!(far.damage_px <= 40.0 * 40.0, "far damage must stay tight: {} px^2", far.damage_px);
}
