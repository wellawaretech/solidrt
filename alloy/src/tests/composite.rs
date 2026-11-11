use crate::renderer::prelude::*;
use crate::renderer::{DisplayListBuilder, DrawStyle};
use crate::rendertree::composite::{apply_content_changes, paint_phase};
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
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
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

// Glass under a fading group (okf/done/backdrop-under-group-opacity.md):
// the panels' backdrop layers are emitted ahead of the group's opacity
// layer - on the inline path and on a Recording boundary's replay alike -
// and only then: an opaque group, a fade without glass, and a filtered
// group (a backdrop root by design) pre-emit nothing.
#[test]
fn glass_under_a_fading_group_is_prepainted() {
  let mut tree = split();
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);
  let paint = |tree: &mut RenderTree| paint_phase(&mut DisplayListBuilder::new(None), tree, &platform, &alloy);
  let set_opacity = |tree: &mut RenderTree, id: u64, v: Option<f32>| {
    tree.edit(id, |el| match &mut el.kind {
      ElementKind::View(v2) => v2.set_opacity(v),
      _ => unreachable!(),
    })
  };

  let baseline = paint(&mut tree);
  assert_eq!(baseline.backdrops_prepainted, 0);

  // A fade with no glass under it: nothing to pre-emit.
  set_opacity(&mut tree, 2, Some(0.5));
  assert_eq!(paint(&mut tree).backdrops_prepainted, 0);

  // The pane's header becomes a glass panel: pre-emitted once per frame
  // while the pane fades, and the regular walk still counts its nodes once.
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::View(v) => v.set_backdrop_filter(Some(FilterState { blur: Some(4.0), ..Default::default() })),
    _ => unreachable!(),
  });
  let fading = paint(&mut tree);
  assert_eq!(fading.backdrops_prepainted, 1);
  assert_eq!(fading.nodes_painted, baseline.nodes_painted);

  // Opaque again: the panel reads the window on its own.
  set_opacity(&mut tree, 2, None);
  assert_eq!(paint(&mut tree).backdrops_prepainted, 0);

  // A filtered group is a backdrop root: its panels see its effect layer.
  set_opacity(&mut tree, 2, Some(0.5));
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::View(v) => v.set_filter(Some(FilterState { blur: Some(2.0), ..Default::default() })),
    _ => unreachable!(),
  });
  assert_eq!(paint(&mut tree).backdrops_prepainted, 0);
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::View(v) => v.set_filter(None),
    _ => unreachable!(),
  });

  // A fading Recording boundary: the replay applies the opacity as a group
  // too, so the pass runs on the recording frame and on every reuse frame.
  tree.edit(2, |el| {
    el.repaint_boundary = BoundaryMode::Recording;
    Damage::None
  });
  let recorded = paint(&mut tree);
  assert_eq!(recorded.boundaries_recorded, 1);
  assert_eq!(recorded.backdrops_prepainted, 1);
  let reused = paint(&mut tree);
  assert_eq!(reused.boundaries_reused, 1);
  assert_eq!(reused.backdrops_prepainted, 1);
}

// The walk's frames stay fractional (rendertree/grid.rs): a child laid out
// at a fractional position is placed there, and its box is rounded once,
// where it is drawn (the box offset of record_node), not where it is
// placed. Its extent cell records the fractional place (with the
// envelope's 1 px AA outset above it) at every display scale: the present
// damage floors, ceils and pads it, which holds the rounded edge. Layout
// itself keeps the fractional box.
#[test]
fn child_boxes_are_placed_fractionally_and_rounded_at_the_draw() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 400.0, 16.98);
  size(&mut tree, 3, 400.0, 16.98);
  let alloy = headless();

  let extent_at = |tree: &mut RenderTree, scale: f32| {
    let platform = PlatformContext::new(Vec::new());
    platform.set_window_size(400.0, 300.0);
    platform.set_display_scale(scale);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &platform, &alloy);
    match tree.node(3).last_extent.get() {
      crate::rendertree::cull::Extent::Bounded(r) => r,
      other => panic!("expected a bounded extent, got {other:?}"),
    }
  };
  let close = |a: f32, b: f32| (a - b).abs() < 1e-3;

  let at_1x = extent_at(&mut tree, 1.0);
  assert!(close(at_1x.origin.y, 16.98 - 1.0), "{at_1x:?}");
  assert!(close(tree.node(3).placement().y, 16.98), "layout keeps the fractional box");

  let at_1_5x = extent_at(&mut tree, 1.5);
  assert!(close(at_1_5x.origin.y, 16.98 - 1.0), "{at_1_5x:?}");
}

const ZOOMER: u64 = 2;
const SHAPE: u64 = 3;

// The scale the zoomer rests at after its one write; within the motion
// bound of a text raster, so the recording waits for rest as a text's does.
const ZOOM_TO: f32 = 1.5;
// Frames enough for a scale written once to come to rest and the
// recording to re-record, and for any second re-record to show.
const FRAMES_TO_REST: usize = 8;

// root(1, 400x300) > zoomer(2, 200x100, a Recording boundary) > shape(3)
fn zoomer_with(shape: Element) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut zoomer = attached();
  zoomer.repaint_boundary = BoundaryMode::Recording;
  tree.create_node(ZOOMER, zoomer);
  tree.create_node(SHAPE, shape);
  tree.insert_node(1, ZOOMER, None).expect("insert");
  tree.insert_node(ZOOMER, SHAPE, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, ZOOMER, 200.0, 100.0);
  if tree.node_mut(SHAPE).style_mut().is_some() {
    size(&mut tree, SHAPE, 50.0, 20.0);
  }
  tree
}

fn stroked() -> Rectangle {
  let mut rect = Rectangle::default();
  rect.paint.draw_style = DrawStyle::Stroke;
  rect.paint.stroke_width = 1.0;
  rect
}

// The rotation, in degrees, the boundary is recorded under or written to:
// any rotation leaves the frame with no single device width for a stroke.
const TILT_DEG: f32 = 10.0;
// The display scale of the rotation tests: one where a fresh upright
// record floors a 1 px stroke (to 1 / 1.5) instead of keeping it.
const DISPLAY_SCALE: f32 = 1.5;

// A write on the zoomer's view.
fn on_zoomer(tree: &mut RenderTree, write: impl FnOnce(&mut View) -> Damage) {
  tree.edit(ZOOMER, |e| {
    let ElementKind::View(view) = &mut e.kind else { panic!("the zoomer is a view") };
    write(view)
  });
}

// The recordings the zoomer makes at `display_scale`: one on the first
// frame (after `setup` on the boundary view), then the count over the
// frames after `write` on it. Both are static writes, Compose damage on a
// hoisted boundary matrix: neither re-records the boundary by itself.
fn records_after(
  tree: &mut RenderTree,
  display_scale: f32,
  setup: impl FnOnce(&mut View) -> Damage,
  write: impl FnOnce(&mut View) -> Damage,
) -> (u32, u32) {
  let platform = crate::tests::text_platform();
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);
  platform.set_display_scale(display_scale);
  let paint = |tree: &mut RenderTree| {
    apply_content_changes(tree, &platform, &alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &platform, &alloy).boundaries_recorded
  };
  on_zoomer(tree, setup);
  let first = paint(tree);
  on_zoomer(tree, write);
  let after = (0..FRAMES_TO_REST).map(|_| paint(tree)).sum();
  (first, after)
}

// One static scale write on the boundary.
fn records_after_a_zoom(tree: &mut RenderTree) -> (u32, u32) {
  records_after(
    tree,
    1.0,
    |_| Damage::None,
    |view| {
      view.set_scale_x(Some(ZOOM_TO));
      view.set_scale_y(Some(ZOOM_TO))
    },
  )
}

// A stroke floored to device pixels is made for one scale, as a text layer
// is (okf/done/border-width-device-pixels.md): a recording that holds one
// and no text re-records once for a static scale write, where the scale
// comes to rest, and not again. (Under a scale transition it re-records at
// the start and at the end, as a recording holding text does.)
#[test]
fn a_recording_with_a_floored_stroke_re_records_for_a_static_scale_write() {
  let mut tree = zoomer_with(stroked().with_layout());
  assert_eq!(records_after_a_zoom(&mut tree), (1, 1));
}

// A recording of vector content made for no scale replays at any scale: a
// filled rect, and a stroked rect with geometry of its own (the animation
// lane, whose stroke stays as written).
#[test]
fn a_recording_without_a_floored_stroke_does_not_re_record_for_scale() {
  let mut filled = zoomer_with(Rectangle::default().with_layout());
  assert_eq!(records_after_a_zoom(&mut filled), (1, 0));
  let own = Rectangle { w: Some(30.0), ..stroked() };
  let mut detached = zoomer_with(own.no_layout());
  assert_eq!(records_after_a_zoom(&mut detached), (1, 0));
}

// A recording made under a rotation holds the declared stroke width, and
// the longer-axis scale does not change when the frame comes upright: it
// is made again once the frame can floor the stroke, and not again.
#[test]
fn a_recording_made_under_a_rotation_re_records_when_upright() {
  let mut tree = zoomer_with(stroked().with_layout());
  let upright = records_after(&mut tree, DISPLAY_SCALE, |v| v.set_rotate(Some(TILT_DEG)), |v| v.set_rotate(Some(0.0)));
  assert_eq!(upright, (1, 1));
}

// Content made for no scale has no width to fix: a filled rect recorded
// under a rotation replays when the frame comes upright.
#[test]
fn a_filled_recording_made_under_a_rotation_replays_when_upright() {
  let mut tree = zoomer_with(Rectangle::default().with_layout());
  let upright = records_after(&mut tree, DISPLAY_SCALE, |v| v.set_rotate(Some(TILT_DEG)), |v| v.set_rotate(Some(0.0)));
  assert_eq!(upright, (1, 0));
}

// The other direction is left: a stroke floored upright and replayed under
// a rotation is a fraction thinner, on edges that are resampled anyway.
#[test]
fn a_recording_made_upright_replays_under_a_rotation() {
  let mut tree = zoomer_with(stroked().with_layout());
  let tilted = records_after(&mut tree, DISPLAY_SCALE, |_| Damage::None, |v| v.set_rotate(Some(TILT_DEG)));
  assert_eq!(tilted, (1, 0));
}

// How far a recorded stroke width may sit from the expected one: float
// noise.
const WIDTH_EPS: f32 = 1e-4;

// The width the zoomer's cached recording strokes its shape at, read from
// the list the RecordingCache holds.
fn recorded_stroke_width(tree: &RenderTree) -> f32 {
  let cache = tree.node(ZOOMER).paint_cache.borrow();
  let Some(PaintCache::Recording(rec)) = &*cache else { panic!("the zoomer holds a recording") };
  let mut widths = crate::renderer::oplog::placed(rec.dl.ops()).into_iter().filter_map(|p| match p.op {
    crate::renderer::oplog::Op::DrawRect { paint, .. } if paint.draw_style == DrawStyle::Stroke => {
      Some(paint.stroke_width)
    }
    _ => None,
  });
  let width = widths.next().expect("the recording strokes the shape");
  assert!(widths.next().is_none(), "one stroked rect");
  width
}

// A recording made again for a new scale holds the new floor, not only a
// new count: at 1x the 1 px stroke is recorded at its declared width, and
// once a static scale write of 1.5 comes to rest at its floor for 1.5x, one
// device pixel (1 / 1.5 in frame units).
#[test]
fn a_recording_made_again_for_a_new_scale_holds_its_floor() {
  let mut tree = zoomer_with(stroked().with_layout());
  let platform = crate::tests::text_platform();
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);
  let paint = |tree: &mut RenderTree| {
    apply_content_changes(tree, &platform, &alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &platform, &alloy);
  };
  let close = |a: f32, b: f32| (a - b).abs() < WIDTH_EPS;

  paint(&mut tree);
  let before = recorded_stroke_width(&tree);
  assert!(close(before, 1.0), "{before}");
  on_zoomer(&mut tree, |view| {
    view.set_scale_x(Some(ZOOM_TO));
    view.set_scale_y(Some(ZOOM_TO))
  });
  for _ in 0..FRAMES_TO_REST {
    paint(&mut tree);
  }
  let after = recorded_stroke_width(&tree);
  assert!(close(after, 1.0 / ZOOM_TO), "{after}");
}
