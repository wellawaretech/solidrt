use crate::impellers::Point;
use crate::rendertree::composite::layout_phase;
use crate::rendertree::hit::{DefaultHitTester, HitTester};
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;

// Layout slides (okf/done/transition-layout-animations.md,
// okf/done/transition-layout-size.md): a node declaring a `layout`
// transition slides from the box it was painted at to the box a layout
// gives it, position and size. Driven through the real layout phase with
// the headless context the layout tests use.

const LINEAR_100: TransitionSpec = TransitionSpec::Tween { duration_ms: 100.0, curve: Curve::Linear };

// A Context with no raster thread behind it: laying out views never sends a
// command, so a dangling channel is enough.
fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
}

fn attached() -> Element {
  View::default().with_layout()
}

fn size(tree: &mut RenderTree, id: u64, w: f32, h: f32) {
  let style = tree.node_mut(id).style_mut().expect("laid-out node");
  style.size = taffy::Size { width: length(w), height: length(h) };
}

fn slide_entry(delay_ms: f32) -> TransitionEntry {
  TransitionEntry { spec: LINEAR_100, delay_ms, from: None, exit: None }
}

fn declare_layout(tree: &mut RenderTree, id: u64) {
  tree.edit(id, |el| {
    el.transitions = Some(Box::new(TransitionConfig { layout: Some(slide_entry(0.0)), ..Default::default() }));
    Damage::None
  });
}

// A 400x300 column of three 400x50 rows, each declaring a layout slide:
// root(1) > rows 2, 3, 4 at y 0, 50, 100.
fn column() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  for id in [2, 3, 4] {
    tree.create_node(id, attached());
    tree.insert_node(1, id, None).expect("insert");
    size(&mut tree, id, 400.0, 50.0);
    declare_layout(&mut tree, id);
  }
  tree.root = Some(1);
  tree.node_mut(1).style_mut().expect("root").flex_direction = FlexDirection::Column;
  size(&mut tree, 1, 400.0, 300.0);
  tree
}

fn layout(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) {
  platform.set_window_size(400.0, 300.0);
  layout_phase(tree, platform, alloy);
}

// Stands in for the paint walk having shown the nodes (Element::painted).
fn paint(tree: &RenderTree, ids: &[u64]) {
  for &id in ids {
    tree.node(id).lifecycle.painted.set(true);
  }
}

// A laid-out, painted column at clock 0.
fn shown() -> (RenderTree, PlatformContext, crate::Context) {
  let mut tree = column();
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  tree.set_transition_now(0.0);
  layout(&mut tree, &platform, &alloy);
  paint(&tree, &[1, 2, 3, 4]);
  (tree, platform, alloy)
}

fn painted_y(tree: &RenderTree, id: u64) -> f32 {
  tree.node(id).placement().y
}

fn solved_y(tree: &RenderTree, id: u64) -> f32 {
  tree.node(id).layout_data().location().y
}

fn sliding(tree: &RenderTree, id: u64) -> bool {
  tree.slide_remaining(id).is_some()
}

fn painted_h(tree: &RenderTree, id: u64) -> f32 {
  tree.node(id).painted_size().expect("laid out").height
}

fn solved_h(tree: &RenderTree, id: u64) -> f32 {
  tree.node(id).layout_data().size().height
}

fn set_height(tree: &mut RenderTree, id: u64, h: f32) {
  tree.edit(id, |el| {
    el.style_mut().expect("laid-out node").size.height = length(h);
    Damage::Layout
  });
}

// A child of row 3 filling it (`fraction` of its height), so the box the
// row's children are laid out against is readable from the child's size.
fn fill_row_3(tree: &mut RenderTree, id: u64, fraction: f32) {
  tree.create_node(id, attached());
  tree.node_mut(id).style_mut().expect("child").size = taffy::Size { width: percent(1.0), height: percent(fraction) };
  tree.insert_node(3, id, None).expect("insert child");
}

fn assert_near(got: f32, want: f32) {
  assert!((got - want).abs() < 0.01, "expected {want}, got {got}");
}

// The reflow frame paints the rows where they were and the slide runs from
// the next advance, settling on the solved box with an end event.
#[test]
fn removing_a_row_slides_the_rows_below_up() {
  let (mut tree, platform, alloy) = shown();
  assert_eq!(solved_y(&tree, 3), 50.0);
  tree.detach_node(1, 2);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!((solved_y(&tree, 3), solved_y(&tree, 4)), (0.0, 50.0));
  assert_eq!((painted_y(&tree, 3), painted_y(&tree, 4)), (50.0, 100.0));
  let remaining = tree.slide_remaining(3).expect("mid-slide");
  assert_eq!((remaining.offset, remaining.growth), (Vector::new(0.0, -50.0), Vector::zero()));
  assert!(tree.advance_transitions(), "slides run");
  assert_eq!(painted_y(&tree, 3), 50.0, "nothing moves at the start clock");

  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  assert_near(painted_y(&tree, 3), 25.0);
  assert_near(painted_y(&tree, 4), 75.0);

  tree.set_transition_now(1100.0);
  assert!(!tree.advance_transitions(), "settled");
  assert_eq!((painted_y(&tree, 3), painted_y(&tree, 4)), (0.0, 50.0));
  assert!(!sliding(&tree, 3) && !sliding(&tree, 4), "back on the solved boxes");
  let settled = tree.take_settled_transitions();
  assert!(settled.contains(&(3, AnimProp::Layout)) && settled.contains(&(4, AnimProp::Layout)), "{settled:?}");
}

#[test]
fn inserting_a_row_slides_the_rows_below_apart_and_never_the_new_one() {
  let (mut tree, platform, alloy) = shown();
  tree.create_node(5, attached());
  size(&mut tree, 5, 400.0, 50.0);
  declare_layout(&mut tree, 5);
  tree.insert_node(1, 5, Some(3)).expect("insert before row 3");
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(solved_y(&tree, 5), 50.0);
  assert_eq!(painted_y(&tree, 5), 50.0, "a first layout has nothing to slide from");
  assert!(!sliding(&tree, 5));
  assert_eq!((painted_y(&tree, 3), solved_y(&tree, 3)), (50.0, 100.0));
  assert!(sliding(&tree, 3) && sliding(&tree, 4));
  assert!(!sliding(&tree, 2), "a row that did not move stays put");
}

// Solid's move is a detach and an insert under the same parent in one
// tick: every row the reorder displaced slides.
#[test]
fn a_reorder_slides_every_moved_row() {
  let (mut tree, platform, alloy) = shown();
  tree.detach_node(1, 4);
  tree.insert_node(1, 4, Some(2)).expect("re-insert first");
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!((solved_y(&tree, 4), painted_y(&tree, 4)), (0.0, 100.0));
  assert_eq!((solved_y(&tree, 2), painted_y(&tree, 2)), (50.0, 0.0));
  assert_eq!((solved_y(&tree, 3), painted_y(&tree, 3)), (100.0, 50.0));
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  assert_eq!((painted_y(&tree, 4), painted_y(&tree, 2), painted_y(&tree, 3)), (0.0, 50.0, 100.0));
  assert!(!sliding(&tree, 2) && !sliding(&tree, 3) && !sliding(&tree, 4));
}

#[test]
fn a_node_never_painted_snaps() {
  let mut tree = column();
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  tree.set_transition_now(0.0);
  layout(&mut tree, &platform, &alloy);
  // Laid out, never shown: the remove reflows rows 3 and 4 without a slide.
  tree.detach_node(1, 2);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(painted_y(&tree, 3), 0.0);
  assert!(!sliding(&tree, 3));
  assert!(!tree.advance_transitions(), "no track runs");
}

// The old box is in another parent's frame: a reparent snaps the node into
// its new parent and anchors it there, so its next reflow slides.
#[test]
fn a_reparent_snaps_and_anchors_under_the_new_parent() {
  let (mut tree, platform, alloy) = shown();
  tree.create_node(6, attached());
  size(&mut tree, 6, 400.0, 100.0);
  tree.insert_node(1, 6, None).expect("insert container");
  tree.detach_node(1, 4);
  tree.insert_node(6, 4, None).expect("move into the container");
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(painted_y(&tree, 4), 0.0);
  assert!(!sliding(&tree, 4));

  tree.create_node(7, attached());
  size(&mut tree, 7, 400.0, 50.0);
  tree.insert_node(6, 7, Some(4)).expect("insert before row 4");
  layout(&mut tree, &platform, &alloy);
  assert_eq!((solved_y(&tree, 4), painted_y(&tree, 4)), (50.0, 0.0));
  assert!(sliding(&tree, 4));
}

// taffy's hidden pass writes the empty box: a row hidden mid-column snaps
// out (nothing is painted at the empty box) and comes back with a snap too
// (nothing was painted there), while the rows around it slide both ways.
#[test]
fn hiding_and_showing_never_slide_from_the_empty_box() {
  let (mut tree, platform, alloy) = shown();
  let display = |tree: &mut RenderTree, display: Display| {
    tree.edit(3, |e| {
      e.style_mut().expect("row").display = display;
      Damage::Layout
    });
  };
  display(&mut tree, Display::None);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert!(!sliding(&tree, 3), "hidden: no box to slide to");
  assert_eq!((painted_y(&tree, 4), solved_y(&tree, 4)), (100.0, 50.0));
  assert!(sliding(&tree, 4));
  tree.set_transition_now(1100.0);
  tree.advance_transitions();

  display(&mut tree, Display::Flex);
  tree.set_transition_now(2000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(painted_y(&tree, 3), 50.0);
  assert!(!sliding(&tree, 3), "shown again from the empty box, it snaps");
  assert_eq!((painted_y(&tree, 4), solved_y(&tree, 4)), (50.0, 100.0));
  assert!(sliding(&tree, 4));
}

// The lane is the painted location, so a second reflow is a retarget from
// the point the node is at: no jump on the reflow frame, and the new
// motion runs from there.
#[test]
fn a_reflow_mid_slide_retargets_from_the_painted_point() {
  let (mut tree, platform, alloy) = shown();
  tree.detach_node(1, 2);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  assert_near(painted_y(&tree, 3), 25.0);

  // A new row lands in front of row 3 while it slides up: its destination
  // moves back down to 50.
  tree.create_node(5, attached());
  size(&mut tree, 5, 400.0, 50.0);
  tree.insert_node(1, 5, Some(3)).expect("insert");
  layout(&mut tree, &platform, &alloy);
  assert_eq!(solved_y(&tree, 3), 50.0);
  assert_near(painted_y(&tree, 3), 25.0);
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  assert_near(painted_y(&tree, 3), 37.5);
}

#[test]
fn clearing_the_declaration_snaps_to_the_solved_box() {
  let (mut tree, platform, alloy) = shown();
  tree.detach_node(1, 2);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  assert_near(painted_y(&tree, 3), 25.0);
  tree.edit(3, |el| {
    el.transitions = None;
    Damage::None
  });
  assert_eq!(painted_y(&tree, 3), 0.0);
  assert!(tree.node(3).lifecycle.slide.is_none());
  tree.set_transition_now(1075.0);
  tree.advance_transitions();
  assert_eq!(painted_y(&tree, 3), 0.0, "no stale track writes the lane");
  assert!(tree.node(3).lifecycle.slide.is_none());
}

#[test]
fn a_delayed_slide_holds_at_the_old_box_until_due() {
  let (mut tree, platform, alloy) = shown();
  for id in [3, 4] {
    tree.edit(id, |el| {
      el.transitions.as_mut().expect("config").layout = Some(slide_entry(50.0));
      Damage::None
    });
  }
  tree.detach_node(1, 2);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  tree.set_transition_now(1040.0);
  tree.advance_transitions();
  assert_eq!(painted_y(&tree, 3), 50.0, "held");
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  assert_near(painted_y(&tree, 3), 25.0);
}

// An exit pop-out and a slide side by side: the exiting row stays painted
// at its last box while the rows below slide up past it.
#[test]
fn an_exiting_row_keeps_its_box_while_the_rows_below_slide() {
  let (mut tree, platform, alloy) = shown();
  tree.edit(2, |el| {
    let exit = Endpoint { value: AnimValue::Scalar(0.0), spec: LINEAR_100, delay_ms: 0.0 };
    el.transitions = Some(Box::new(TransitionConfig {
      props: vec![(
        AnimProp::Opacity,
        TransitionEntry { spec: LINEAR_100, delay_ms: 0.0, from: None, exit: Some(exit) },
      )],
      layout: Some(slide_entry(0.0)),
      ..Default::default()
    }));
    Damage::None
  });
  tree.set_transition_now(1000.0);
  tree.detach_node(1, 2);
  assert!(tree.node(2).lifecycle.exiting);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(painted_y(&tree, 2), 0.0);
  assert!(!sliding(&tree, 2), "out of the flow, painted at its last box");
  assert_eq!((painted_y(&tree, 3), solved_y(&tree, 3)), (50.0, 0.0));
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  assert!(tree.node(2).parent.is_none(), "the exit settled and unlinked the row");
  assert_eq!(painted_y(&tree, 3), 0.0);
}

// What you see is what you tap: the hit test reads the painted position.
#[test]
fn hit_testing_follows_the_painted_position() {
  let (mut tree, platform, alloy) = shown();
  tree.detach_node(1, 2);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  let hit =
    |tree: &RenderTree, y: f32| DefaultHitTester.hit_test(tree, Point::new(10.0, y)).last().map(|&(id, _, _)| id);
  assert_eq!(hit(&tree, 60.0), Some(3), "row 3 is painted at its old box");
  assert_eq!(hit(&tree, 10.0), Some(1), "nothing at its solved box yet");
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  assert_eq!(hit(&tree, 10.0), Some(3));
}

// The dev tooling's dump reports where a node is painted, and the offset it
// has still to cover: a slide and a jump read apart frame by frame.
#[test]
fn the_snapshot_reports_the_painted_position() {
  let (mut tree, platform, alloy) = shown();
  tree.detach_node(1, 2);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  let node = tree.snapshot_from(Some(3), Some(0)).expect("row 3");
  assert_eq!(node.y, 50.0);
  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  let node = tree.snapshot_from(Some(3), Some(0)).expect("row 3");
  assert_near(node.y, 25.0);
  let remaining = tree.slide_remaining(3).expect("mid-slide");
  assert_near(remaining.offset.x, 0.0);
  assert_near(remaining.offset.y, -25.0);
}

// The lane is the whole box: a row that grows slides its size from the
// painted one, its children are laid out against the painted box on every
// frame of the motion, and the rows below slide with its bottom edge.
#[test]
fn growing_a_row_slides_its_size_and_lays_its_children_out_against_it() {
  let (mut tree, platform, alloy) = shown();
  fill_row_3(&mut tree, 5, 1.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(solved_h(&tree, 5), 50.0);
  paint(&tree, &[5]);

  set_height(&mut tree, 3, 150.0);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!((solved_h(&tree, 3), painted_h(&tree, 3)), (150.0, 50.0));
  assert_eq!(solved_h(&tree, 5), 50.0, "the child fills the painted box, not the solved one");
  assert_eq!((solved_y(&tree, 4), painted_y(&tree, 4)), (200.0, 100.0));
  let remaining = tree.slide_remaining(3).expect("mid-slide");
  assert_eq!((remaining.offset, remaining.growth), (Vector::zero(), Vector::new(0.0, 100.0)));

  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  layout(&mut tree, &platform, &alloy);
  assert_near(painted_h(&tree, 3), 100.0);
  assert_near(solved_h(&tree, 5), 100.0);
  assert_near(painted_y(&tree, 4), 150.0);
  let node = tree.snapshot_from(Some(3), Some(0)).expect("row 3");
  assert_near(node.height, 100.0);

  tree.set_transition_now(1100.0);
  assert!(!tree.advance_transitions(), "settled");
  layout(&mut tree, &platform, &alloy);
  assert_eq!(painted_h(&tree, 3), 150.0);
  assert_eq!(solved_h(&tree, 5), 150.0, "the settle lands the child on its solved box");
  assert!(!sliding(&tree, 3));
  assert!(tree.take_settled_transitions().contains(&(3, AnimProp::Layout)));
  layout(&mut tree, &platform, &alloy);
  assert_eq!(solved_h(&tree, 5), 150.0, "a settled row lays its children out once more, then no more");
}

// A declaring child under a resizing ancestor is carried by the ancestor's
// motion: the animated sub-layout places it every frame, and a slide of its
// own would fight that box, so none runs.
#[test]
fn a_resizing_ancestor_carries_its_declaring_children() {
  let (mut tree, platform, alloy) = shown();
  fill_row_3(&mut tree, 5, 0.5);
  declare_layout(&mut tree, 5);
  layout(&mut tree, &platform, &alloy);
  paint(&tree, &[5]);
  assert_eq!(solved_h(&tree, 5), 25.0);

  set_height(&mut tree, 3, 150.0);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(solved_h(&tree, 5), 25.0, "half of the painted 50");
  assert!(!sliding(&tree, 5), "no slide of its own");
  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  layout(&mut tree, &platform, &alloy);
  assert_near(solved_h(&tree, 5), 50.0);
  assert!(!sliding(&tree, 5));

  // Its own reflow, once the ancestor rests, slides again.
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  layout(&mut tree, &platform, &alloy);
  assert_eq!(solved_h(&tree, 5), 75.0);
  tree.edit(5, |el| {
    el.style_mut().expect("child").size.height = percent(1.0);
    Damage::Layout
  });
  tree.set_transition_now(2000.0);
  layout(&mut tree, &platform, &alloy);
  assert_eq!((solved_h(&tree, 5), painted_h(&tree, 5)), (150.0, 75.0));
  assert!(sliding(&tree, 5));
}

// Clearing the declaration mid-resize snaps the node to its solved box and
// its children with it.
#[test]
fn clearing_the_declaration_mid_resize_snaps_the_children_too() {
  let (mut tree, platform, alloy) = shown();
  fill_row_3(&mut tree, 5, 1.0);
  layout(&mut tree, &platform, &alloy);
  paint(&tree, &[5]);
  set_height(&mut tree, 3, 150.0);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  tree.set_transition_now(1050.0);
  tree.advance_transitions();
  layout(&mut tree, &platform, &alloy);
  assert_near(solved_h(&tree, 5), 100.0);
  tree.edit(3, |el| {
    el.transitions = None;
    Damage::None
  });
  layout(&mut tree, &platform, &alloy);
  assert_eq!(painted_h(&tree, 3), 150.0);
  assert_eq!(solved_h(&tree, 5), 150.0);
}

// What you see is what you tap, in size too: the hit test reads the painted
// box, so the grown row's solved area below its painted bottom still
// belongs to the row painted there.
#[test]
fn hit_testing_follows_the_painted_size() {
  let (mut tree, platform, alloy) = shown();
  set_height(&mut tree, 3, 150.0);
  tree.set_transition_now(1000.0);
  layout(&mut tree, &platform, &alloy);
  let hit =
    |tree: &RenderTree, y: f32| DefaultHitTester.hit_test(tree, Point::new(10.0, y)).last().map(|&(id, _, _)| id);
  assert_eq!(hit(&tree, 75.0), Some(3));
  assert_eq!(hit(&tree, 120.0), Some(4), "row 4 is painted where row 3's solved box now reaches");
  tree.set_transition_now(1100.0);
  tree.advance_transitions();
  assert_eq!(hit(&tree, 120.0), Some(3));
}
