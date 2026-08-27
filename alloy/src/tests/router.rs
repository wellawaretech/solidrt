use crate::rendertree::*;
use crate::{Modifiers, PointerType};

// Scene for every test: root 1 (200x200) with child 2 at (10,20) 100x100 and
// child 3 at (120,20) 60x60. (50,50) hits [1,2]; (150,50) hits [1,3].
// Nodes start with no event interest; tests grant what they exercise.
fn scene() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, View::default().with_layout());
  tree.create_node(3, View::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 10.0, 20.0, 100.0, 100.0);
  place(&mut tree, 3, 120.0, 20.0, 60.0, 60.0);
  tree
}

fn listen(tree: &mut RenderTree, id: u64, bits: u32) {
  tree.edit(id, |el| {
    el.set_event_interest(EventInterest(bits));
    Damage::None
  });
}

fn listen_all(tree: &mut RenderTree) {
  for id in [1, 2, 3] {
    listen(tree, id, EventInterest::KNOWN);
  }
}

// Writes a computed layout directly: unit tests have no GPU/platform context,
// so taffy never runs and placements are set by hand.
fn place(tree: &mut RenderTree, id: u64, x: f32, y: f32, w: f32, h: f32) {
  let l = tree.node_mut(id).layout_data_mut();
  l.computed.location = taffy::Point { x, y };
  l.computed.size = taffy::Size { width: w, height: h };
}

fn mouse_move(x: f32, y: f32) -> InputEvent {
  InputEvent::PointerMove {
    pointer_id: 0,
    pointer_type: PointerType::Mouse,
    x,
    y,
    dx: 0.0,
    dy: 0.0,
    modifiers: Modifiers::default(),
  }
}

fn assert_xy(got: Point, x: f32, y: f32) {
  let eps = 1e-3;
  assert!((got.x - x).abs() < eps && (got.y - y).abs() < eps, "expected ({x}, {y}), got ({}, {})", got.x, got.y);
}

#[test]
fn down_freezes_routing_until_up() {
  let mut tree = scene();
  listen_all(&mut tree);
  let mut router = PointerRouter::default();
  let m = Modifiers::default();

  let events = router.dispatch(
    &tree,
    InputEvent::PointerDown {
      pointer_id: 0,
      pointer_type: PointerType::Mouse,
      button: 0,
      x: 50.0,
      y: 50.0,
      modifiers: m,
    },
  );
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Down { button: 0 }));
  assert_eq!(events[0].targets, vec![1, 2]);
  assert_eq!(events[0].target, 2);

  // Move off node 2: the Move still routes along the frozen [1, 2] with exact
  // projected locals, while the hover update sees the live path [1, 3].
  let events = router.dispatch(&tree, mouse_move(150.0, 50.0));
  assert_eq!(events.len(), 2);
  assert!(matches!(events[0].kind, RoutedKind::Move { .. }));
  assert_eq!(events[0].targets, vec![1, 2]);
  assert_xy(events[0].locals[1], 140.0, 30.0);
  assert_xy(events[0].parents[1], 150.0, 50.0);
  assert!(matches!(events[1].kind, RoutedKind::Enter));
  assert_eq!(events[1].targets, vec![1, 3]);

  // The up routes along the frozen path too, and unfreezes.
  let events = router.dispatch(
    &tree,
    InputEvent::PointerUp {
      pointer_id: 0,
      pointer_type: PointerType::Mouse,
      button: 0,
      x: 150.0,
      y: 50.0,
      modifiers: m,
    },
  );
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Up { button: 0 }));
  assert_eq!(events[0].targets, vec![1, 2]);

  let events = router.dispatch(&tree, mouse_move(150.0, 50.0));
  assert_eq!(events.len(), 1, "unfrozen move follows the live path with no hover delta");
  assert!(matches!(events[0].kind, RoutedKind::Move { .. }));
  assert_eq!(events[0].targets, vec![1, 3]);
}

#[test]
fn hover_diff_leaves_deepest_first() {
  let mut tree = scene();
  listen_all(&mut tree);
  let mut router = PointerRouter::default();
  let key = (PointerType::Mouse, 0);
  let m = Modifiers::default();

  let events = router.refresh_hover(&tree, vec![(key, (50.0, 50.0))], m);
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Enter));
  assert_eq!(events[0].targets, vec![1, 2]);

  // Cursor over node 3: leave [2] (deepest-first, shared prefix [1] stays),
  // then enter [3]. Leave's target is the old path's deepest node.
  let events = router.refresh_hover(&tree, vec![(key, (150.0, 50.0))], m);
  assert_eq!(events.len(), 2);
  assert!(matches!(events[0].kind, RoutedKind::Leave));
  assert_eq!(events[0].targets, vec![2]);
  assert_eq!(events[0].target, 2);
  assert!(matches!(events[1].kind, RoutedKind::Enter));
  assert_eq!(events[1].targets, vec![3]);
  assert_eq!(events[1].target, 3);

  let events = router.refresh_hover(&tree, vec![(key, (150.0, 50.0))], m);
  assert!(events.is_empty(), "unchanged path produces no deliveries");
}

#[test]
fn touch_up_synthesizes_leave_and_clears_hover() {
  let mut tree = scene();
  listen_all(&mut tree);
  let mut router = PointerRouter::default();
  let m = Modifiers::default();
  let touch_move = |x, y| InputEvent::PointerMove {
    pointer_id: 7,
    pointer_type: PointerType::Touch,
    x,
    y,
    dx: 0.0,
    dy: 0.0,
    modifiers: m,
  };

  let events = router.dispatch(&tree, touch_move(50.0, 50.0));
  assert_eq!(events.len(), 2);
  assert!(matches!(events[1].kind, RoutedKind::Enter));

  let events = router.dispatch(
    &tree,
    InputEvent::PointerUp {
      pointer_id: 7,
      pointer_type: PointerType::Touch,
      button: 0,
      x: 50.0,
      y: 50.0,
      modifiers: m,
    },
  );
  assert_eq!(events.len(), 2);
  assert!(matches!(events[0].kind, RoutedKind::Up { .. }));
  assert!(matches!(events[1].kind, RoutedKind::Leave));
  assert_eq!(events[1].targets, vec![2, 1], "final leave covers the whole hovered path, deepest-first");
  assert_eq!(events[1].target, 2);

  // The hover entry died with the touch: the next contact enters afresh.
  let events = router.dispatch(&tree, touch_move(50.0, 50.0));
  assert_eq!(events.len(), 2);
  assert!(matches!(events[1].kind, RoutedKind::Enter));
  assert_eq!(events[1].targets, vec![1, 2]);
}

#[test]
fn gated_deliveries_still_update_hover() {
  let mut tree = scene();
  let mut router = PointerRouter::default();

  // No node listens for anything: a move builds no deliveries at all.
  let events = router.dispatch(&tree, mouse_move(50.0, 50.0));
  assert!(events.is_empty(), "no listeners must mean no deliveries");

  // But the hovered path was stored: with Leave interest granted on node 2,
  // moving away delivers the leave for a path that was entered silently.
  listen(&mut tree, 2, EventInterest::LEAVE);
  let events = router.dispatch(&tree, mouse_move(150.0, 50.0));
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Leave));
  assert_eq!(events[0].targets, vec![2]);
}

#[test]
fn ancestor_interest_keeps_gesture_moves_flowing() {
  // The titlebar-drag case: down on a child, moves observed by an ancestor.
  let mut tree = scene();
  listen(&mut tree, 1, EventInterest::MOVE);
  let mut router = PointerRouter::default();
  let m = Modifiers::default();

  let events = router.dispatch(
    &tree,
    InputEvent::PointerDown {
      pointer_id: 0,
      pointer_type: PointerType::Mouse,
      button: 0,
      x: 50.0,
      y: 50.0,
      modifiers: m,
    },
  );
  assert_eq!(events.len(), 1, "down always delivers, even with no interest bits");

  // Off every element entirely: the frozen path [1, 2] carries node 1's move
  // interest, so the gesture's moves keep delivering along it.
  let events = router.dispatch(&tree, mouse_move(300.0, 300.0));
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Move { .. }));
  assert_eq!(events[0].targets, vec![1, 2]);
}

#[test]
fn wheel_and_up_gating() {
  let mut tree = scene();
  let mut router = PointerRouter::default();
  let m = Modifiers::default();
  let wheel = |x, y| InputEvent::Wheel {
    pointer_id: 0,
    pointer_type: PointerType::Mouse,
    x,
    y,
    delta_x: 0.0,
    delta_y: 1.0,
    modifiers: m,
  };

  assert!(router.dispatch(&tree, wheel(50.0, 50.0)).is_empty(), "wheel with no listener is gated");
  listen(&mut tree, 1, EventInterest::WHEEL);
  let events = router.dispatch(&tree, wheel(50.0, 50.0));
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Wheel { .. }));
  assert_eq!(events[0].targets, vec![1, 2]);

  // Up always delivers, like down.
  let events = router.dispatch(
    &tree,
    InputEvent::PointerUp {
      pointer_id: 0,
      pointer_type: PointerType::Mouse,
      button: 0,
      x: 50.0,
      y: 50.0,
      modifiers: m,
    },
  );
  assert_eq!(events.len(), 1);
  assert!(matches!(events[0].kind, RoutedKind::Up { .. }));
}
