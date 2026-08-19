use crate::rendertree::{transitions, *};

// A detached rect under a root view, with a transition declared for every
// property: the shape a `<d-rect transition={{ all: ... }}>` write hits.
fn tree_with_animated_rect(spec: TransitionSpec) -> RenderTree {
  tree_with_entry(spec.into())
}

fn tree_with_entry(entry: TransitionEntry) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().no_layout());
  tree.insert_node(1, 2, None);
  tree.edit(2, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(entry), stagger_ms: None }));
    Damage::None
  });
  tree
}

fn rect_x(tree: &RenderTree, id: u64) -> f32 {
  match tree.node(id).anim_value(AnimProp::X).expect("rect x readable") {
    transitions::AnimValue::Scalar(v) => v,
    other => panic!("x is a scalar, got {other:?}"),
  }
}

fn scalar(v: f32) -> transitions::AnimValue {
  transitions::AnimValue::Scalar(v)
}

const LINEAR_100: TransitionSpec = TransitionSpec::Tween { duration_ms: 100.0, curve: Curve::Linear };

#[test]
fn tween_write_animates_then_settles() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(1000.0);
  assert!(tree.transition_write(2, AnimProp::X, Some(scalar(80.0))), "write consumed as a transition");
  // The write itself moves nothing; the first advance at the same clock
  // paints the from-value.
  assert_eq!(rect_x(&tree, 2), 0.0);
  assert!(tree.advance_transitions(), "track runs");
  assert_eq!(rect_x(&tree, 2), 0.0);

  tree.set_transition_now(1050.0);
  assert!(tree.advance_transitions());
  assert!((rect_x(&tree, 2) - 40.0).abs() < 0.01, "linear halfway, got {}", rect_x(&tree, 2));

  tree.set_transition_now(1100.0);
  assert!(!tree.advance_transitions(), "settled");
  assert_eq!(rect_x(&tree, 2), 80.0, "settles on the exact target");
}

#[test]
fn tween_retarget_restarts_from_current_value() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(100.0)));
  tree.set_transition_now(50.0);
  tree.advance_transitions();
  assert!((rect_x(&tree, 2) - 50.0).abs() < 0.01);

  // Retarget mid-flight: CSS semantics, restart from the current value with
  // the full duration.
  tree.transition_write(2, AnimProp::X, Some(scalar(0.0)));
  tree.set_transition_now(100.0);
  tree.advance_transitions();
  assert!((rect_x(&tree, 2) - 25.0).abs() < 0.01, "halfway back from 50, got {}", rect_x(&tree, 2));
  tree.set_transition_now(150.0);
  assert!(!tree.advance_transitions());
  assert_eq!(rect_x(&tree, 2), 0.0);
}

#[test]
fn spring_moves_toward_target_and_settles() {
  let mut tree = tree_with_animated_rect(TransitionSpec::spring(300.0, 0.0));
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(100.0)));
  let mut last = 0.0f32;
  let mut now = 0.0f64;
  let mut active = true;
  // Critically damped: monotone approach, no overshoot.
  for _ in 0..600 {
    now += 16.0;
    tree.set_transition_now(now);
    active = tree.advance_transitions();
    let x = rect_x(&tree, 2);
    assert!(x >= last - 1e-3, "monotone approach, {x} after {last}");
    assert!(x <= 100.0 + 1e-3, "no overshoot at bounce 0, got {x}");
    last = x;
    if !active {
      break;
    }
  }
  assert!(!active, "spring settles within the loop");
  assert_eq!(rect_x(&tree, 2), 100.0);
}

#[test]
fn bouncy_spring_overshoots() {
  let mut tree = tree_with_animated_rect(TransitionSpec::spring(300.0, 0.5));
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(100.0)));
  let mut peak = 0.0f32;
  let mut now = 0.0f64;
  for _ in 0..600 {
    now += 16.0;
    tree.set_transition_now(now);
    let active = tree.advance_transitions();
    peak = peak.max(rect_x(&tree, 2));
    if !active {
      break;
    }
  }
  assert!(peak > 100.5, "bounce 0.5 overshoots the target, peaked at {peak}");
  assert_eq!(rect_x(&tree, 2), 100.0);
}

#[test]
fn spring_retarget_keeps_velocity() {
  let mut tree = tree_with_animated_rect(TransitionSpec::spring(400.0, 0.0));
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(100.0)));
  // Build up speed toward 100.
  tree.set_transition_now(80.0);
  tree.advance_transitions();
  let at_retarget = rect_x(&tree, 2);
  assert!(at_retarget > 1.0 && at_retarget < 99.0);

  // New target behind the motion: carried velocity keeps the value moving
  // forward briefly before it turns around (the retargeting-continuity
  // property a restarted tween lacks).
  tree.transition_write(2, AnimProp::X, Some(scalar(0.0)));
  tree.set_transition_now(96.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 2) > at_retarget, "carried velocity overshoots past {at_retarget}, got {}", rect_x(&tree, 2));
}

#[test]
fn mount_writes_snap() {
  let mut tree = RenderTree::new();
  tree.create_node(2, Rectangle::default().no_layout());
  tree.edit(2, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(LINEAR_100.into()), stagger_ms: None }));
    Damage::None
  });
  // Not inserted yet: the write is not consumed, the normal path snaps it.
  assert!(!tree.transition_write(2, AnimProp::X, Some(scalar(80.0))));
}

#[test]
fn non_numeric_write_cancels_running_track() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  tree.set_transition_now(30.0);
  tree.advance_transitions();
  // A null/non-numeric write takes over: not consumed, and the track is
  // gone so the next advance cannot overwrite the snap.
  assert!(!tree.transition_write(2, AnimProp::X, None));
  assert!(!tree.advance_transitions());
}

#[test]
fn undeclared_property_is_not_consumed() {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().no_layout());
  tree.insert_node(1, 2, None);
  assert!(!tree.transition_write(2, AnimProp::X, Some(scalar(80.0))), "no transition declared");
}

#[test]
fn attached_geometry_is_not_animatable() {
  // An attached rect's x is detached-only; the write must fall through so
  // the property path raises its usual error.
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.edit(2, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(LINEAR_100.into()), stagger_ms: None }));
    Damage::None
  });
  assert!(!tree.transition_write(2, AnimProp::X, Some(scalar(80.0))));
}

#[test]
fn paused_clock_holds_values() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  tree.set_transition_now(50.0);
  tree.advance_transitions();
  let held = rect_x(&tree, 2);
  // Same clock again (scale-0 ticks): active, but nothing moves and no
  // damage is produced.
  let revision = tree.revision();
  assert!(tree.advance_transitions());
  assert_eq!(rect_x(&tree, 2), held);
  assert_eq!(tree.revision(), revision, "a held advance writes nothing");
}

#[test]
fn destroyed_node_drops_its_track() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  tree.destroy_node(2);
  tree.set_transition_now(50.0);
  assert!(!tree.advance_transitions(), "track of a destroyed node is swept");
}

#[test]
fn curve_endpoints_and_shape() {
  let ease_out = Curve::Bezier(0.0, 0.0, 0.58, 1.0);
  assert!((ease_out.eval(0.0)).abs() < 1e-4);
  assert!((ease_out.eval(1.0) - 1.0).abs() < 1e-4);
  // ease-out front-loads progress.
  assert!(ease_out.eval(0.5) > 0.6);
  assert_eq!(Curve::Linear.eval(0.25), 0.25);
}

#[test]
fn radius_animates_uniform_only() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(0.0);
  assert!(tree.transition_write(2, AnimProp::Radius, Some(scalar(8.0))), "unset radius reads as 0");
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::Rectangle(r) => r.set_radius([1.0, 2.0, 3.0, 4.0]),
    _ => unreachable!(),
  });
  assert!(!tree.transition_write(2, AnimProp::Radius, Some(scalar(8.0))), "per-corner radii snap");
}

#[test]
fn color_animates_in_oklab_and_settles() {
  use crate::impellers::Color;
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.set_transition_now(0.0);
  // Rect default color is whatever PaintState defaults to; write a known
  // start through the setter first (snap path), then animate to blue.
  tree.edit(2, |el| {
    el.set_anim_value(AnimProp::Color, transitions::AnimValue::Color(Color::new_srgba(1.0, 0.0, 0.0, 1.0)))
  });
  let consumed = tree.transition_write(
    2,
    AnimProp::Color,
    Some(transitions::AnimValue::Color(Color::new_srgba(0.0, 0.0, 1.0, 1.0))),
  );
  assert!(consumed, "color write consumed");

  tree.set_transition_now(50.0);
  assert!(tree.advance_transitions());
  let mid = match tree.node(2).anim_value(AnimProp::Color).expect("color readable") {
    transitions::AnimValue::Color(c) => c,
    other => panic!("expected color, got {other:?}"),
  };
  // Halfway in oklab: still clearly saturated (not the gray/mud a linear
  // sRGB midpoint gives, whose red and blue would both sit at 0.5).
  assert!(mid.red > 0.05 && mid.blue > 0.05, "midpoint carries both endpoints: {mid:?}");
  assert!(mid.green < 0.4, "no green invented: {mid:?}");

  tree.set_transition_now(100.0);
  assert!(!tree.advance_transitions(), "settled");
  let end = match tree.node(2).anim_value(AnimProp::Color).expect("color readable") {
    transitions::AnimValue::Color(c) => c,
    other => panic!("expected color, got {other:?}"),
  };
  assert!((end.blue - 1.0).abs() < 1e-3 && end.red < 1e-3, "exact target: {end:?}");
}

#[test]
fn oklab_roundtrip_is_exact_enough() {
  use crate::impellers::Color;
  for (r, g, b, a) in
    [(1.0, 0.0, 0.0, 1.0), (0.0, 1.0, 0.0, 0.5), (0.1, 0.2, 0.9, 0.0), (1.0, 1.0, 1.0, 1.0), (0.0, 0.0, 0.0, 1.0)]
  {
    let c = Color::new_srgba(r, g, b, a);
    let rt = transitions::test_oklab_roundtrip(c);
    assert!(
      (rt.red - r).abs() < 1e-3
        && (rt.green - g).abs() < 1e-3
        && (rt.blue - b).abs() < 1e-3
        && (rt.alpha - a).abs() < 1e-3,
      "roundtrip drifted: ({r},{g},{b},{a}) -> {rt:?}"
    );
  }
}

#[test]
fn batched_advance_bumps_revision_once() {
  let mut tree = tree_with_animated_rect(LINEAR_100);
  tree.create_node(3, Rectangle::default().no_layout());
  tree.insert_node(1, 3, None);
  tree.edit(3, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(LINEAR_100.into()), stagger_ms: None }));
    Damage::None
  });
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  tree.transition_write(2, AnimProp::Y, Some(scalar(40.0)));
  tree.transition_write(3, AnimProp::X, Some(scalar(60.0)));
  tree.set_transition_now(50.0);
  let before = tree.revision();
  tree.advance_transitions();
  assert_eq!(tree.revision(), before + 1, "one bump per advance, not per write");
}

#[test]
fn delayed_write_holds_then_animates() {
  let mut tree = tree_with_entry(TransitionEntry { spec: LINEAR_100, delay_ms: 50.0, from: None, exit: None });
  tree.set_transition_now(0.0);
  assert!(tree.transition_write(2, AnimProp::X, Some(scalar(80.0))), "held write is consumed");
  assert!(tree.advance_transitions(), "active while held");
  tree.set_transition_now(30.0);
  assert!(tree.advance_transitions());
  assert_eq!(rect_x(&tree, 2), 0.0, "nothing moves during the hold");

  // The hold expires at 50; the write applies as if written then.
  tree.set_transition_now(50.0);
  assert!(tree.advance_transitions());
  tree.set_transition_now(100.0);
  assert!(tree.advance_transitions());
  assert!((rect_x(&tree, 2) - 40.0).abs() < 0.01, "halfway 50ms after activation, got {}", rect_x(&tree, 2));
  tree.set_transition_now(150.0);
  assert!(!tree.advance_transitions(), "settled");
  assert_eq!(rect_x(&tree, 2), 80.0);
  assert_eq!(tree.take_settled_transitions(), vec![(2, AnimProp::X)]);
}

#[test]
fn newer_write_replaces_held_write() {
  let mut tree = tree_with_entry(TransitionEntry { spec: LINEAR_100, delay_ms: 50.0, from: None, exit: None });
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  tree.set_transition_now(30.0);
  // Replaces the hold and restarts its delay: due at 80, target 40.
  tree.transition_write(2, AnimProp::X, Some(scalar(40.0)));
  tree.set_transition_now(60.0);
  assert!(tree.advance_transitions());
  assert_eq!(rect_x(&tree, 2), 0.0, "first write's activation time no longer applies");
  tree.set_transition_now(80.0);
  tree.advance_transitions();
  tree.set_transition_now(180.0);
  assert!(!tree.advance_transitions());
  assert_eq!(rect_x(&tree, 2), 40.0, "only the newer write ran");
  assert_eq!(tree.take_settled_transitions().len(), 1, "one settle for the pair");
}

#[test]
fn snap_write_drops_held_write() {
  let mut tree = tree_with_entry(TransitionEntry { spec: LINEAR_100, delay_ms: 50.0, from: None, exit: None });
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  assert!(!tree.transition_write(2, AnimProp::X, None), "snap write falls through");
  tree.set_transition_now(100.0);
  assert!(!tree.advance_transitions(), "the held write is gone");
  assert_eq!(rect_x(&tree, 2), 0.0);
}

#[test]
fn held_write_of_destroyed_node_drains_silently() {
  let mut tree = tree_with_entry(TransitionEntry { spec: LINEAR_100, delay_ms: 50.0, from: None, exit: None });
  tree.set_transition_now(0.0);
  tree.transition_write(2, AnimProp::X, Some(scalar(80.0)));
  tree.destroy_node(2);
  tree.set_transition_now(100.0);
  assert!(!tree.advance_transitions(), "due hold for a dead node drops without a track");
  assert!(tree.take_settled_transitions().is_empty());
}

#[test]
fn enter_from_animates_first_attach_only() {
  let mut tree = RenderTree::new();
  tree.set_transition_now(0.0);
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().no_layout());
  tree.edit(2, |el| {
    el.transitions = Some(Box::new(TransitionConfig {
      props: vec![(
        AnimProp::X,
        TransitionEntry { spec: LINEAR_100, delay_ms: 0.0, from: Some(scalar(100.0)), exit: None },
      )],
      all: None,
      stagger_ms: None,
    }));
    match &mut el.kind {
      ElementKind::Rectangle(r) => r.set_x(40.0),
      _ => unreachable!(),
    }
  });
  tree.insert_node(1, 2, None);
  assert_eq!(rect_x(&tree, 2), 100.0, "attach snaps to from");
  tree.set_transition_now(50.0);
  assert!(tree.advance_transitions());
  assert!((rect_x(&tree, 2) - 70.0).abs() < 0.01, "halfway from 100 to the mounted 40, got {}", rect_x(&tree, 2));
  tree.set_transition_now(100.0);
  assert!(!tree.advance_transitions());
  assert_eq!(rect_x(&tree, 2), 40.0, "settles on the mounted value");
  assert_eq!(tree.take_settled_transitions(), vec![(2, AnimProp::X)], "enter animations report their settle");

  // A move re-runs no enter animation.
  tree.detach_node(1, 2);
  tree.insert_node(1, 2, None);
  assert_eq!(rect_x(&tree, 2), 40.0);
  tree.set_transition_now(150.0);
  assert!(!tree.advance_transitions(), "no track after a re-insert");
}

#[test]
fn enter_from_with_delay_holds_at_from() {
  let mut tree = RenderTree::new();
  tree.set_transition_now(0.0);
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().no_layout());
  tree.edit(2, |el| {
    el.transitions = Some(Box::new(TransitionConfig {
      props: vec![(
        AnimProp::X,
        TransitionEntry { spec: LINEAR_100, delay_ms: 50.0, from: Some(scalar(100.0)), exit: None },
      )],
      all: None,
      stagger_ms: None,
    }));
    match &mut el.kind {
      ElementKind::Rectangle(r) => r.set_x(40.0),
      _ => unreachable!(),
    }
  });
  tree.insert_node(1, 2, None);
  tree.set_transition_now(30.0);
  assert!(tree.advance_transitions(), "active during the hold");
  assert_eq!(rect_x(&tree, 2), 100.0, "sits at from until the hold expires");
  tree.set_transition_now(80.0);
  tree.advance_transitions();
  tree.set_transition_now(130.0);
  tree.advance_transitions();
  assert!((rect_x(&tree, 2) - 70.0).abs() < 0.01, "halfway 50ms after activation, got {}", rect_x(&tree, 2));
}

// Exit animations: a removed node with `exit` values stays painted, animates
// them, and is freed when they settle. See tree.rs detach_node/begin_exit.

fn tree_with_exit_rect(entry: TransitionEntry) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.set_transition_now(0.0);
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().no_layout());
  tree.insert_node(1, 2, None);
  tree.edit(2, |el| {
    el.transitions =
      Some(Box::new(TransitionConfig { props: vec![(AnimProp::X, entry)], all: None, stagger_ms: None }));
    Damage::None
  });
  tree
}

const EXIT_200: TransitionEntry =
  TransitionEntry { spec: LINEAR_100, delay_ms: 0.0, from: None, exit: Some(transitions::AnimValue::Scalar(200.0)) };

#[test]
fn exit_animates_removal_then_frees() {
  let mut tree = tree_with_exit_rect(EXIT_200);
  // The renderer's removal: detach, then the sweep destroys.
  tree.detach_node(1, 2);
  assert!(tree.node(1).children.contains(&2), "exiting node stays painted");
  tree.destroy_node(2);
  assert!(tree.try_node(2).is_some(), "destroy defers while the exit runs");

  tree.set_transition_now(50.0);
  assert!(tree.advance_transitions());
  assert!((rect_x(&tree, 2) - 100.0).abs() < 0.01, "halfway to the exit value, got {}", rect_x(&tree, 2));

  tree.set_transition_now(100.0);
  tree.advance_transitions();
  assert!(tree.try_node(2).is_none(), "freed at the exit settle");
  assert!(tree.node(1).children.is_empty(), "unlinked from the parent");
  assert!(tree.take_settled_transitions().is_empty(), "exits never fire onTransitionEnd");
}

#[test]
fn exit_reinsert_is_a_move() {
  let mut tree = tree_with_exit_rect(EXIT_200);
  tree.detach_node(1, 2);
  // Solid re-inserts the same node: a move, not a removal.
  tree.insert_node(1, 2, None);
  tree.set_transition_now(100.0);
  assert!(!tree.advance_transitions(), "abandoned exit leaves no track");
  assert_eq!(rect_x(&tree, 2), 0.0, "value untouched");
  assert!(tree.try_node(2).is_some());
  // The node behaves normally afterwards: a later removal exits again.
  tree.detach_node(1, 2);
  tree.destroy_node(2);
  tree.set_transition_now(250.0);
  tree.advance_transitions();
  assert!(tree.try_node(2).is_none(), "second removal exits and frees");
}

#[test]
fn exit_already_at_target_detaches_instantly() {
  let mut tree = tree_with_exit_rect(EXIT_200);
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::Rectangle(r) => r.set_x(200.0),
    _ => unreachable!(),
  });
  tree.detach_node(1, 2);
  assert!(tree.node(1).children.is_empty(), "nothing to animate, instant detach");
  tree.destroy_node(2);
  assert!(tree.try_node(2).is_none(), "and an instant free");
}

#[test]
fn destroy_without_detach_skips_the_exit() {
  // Forced teardown (no renderer detach first) stays instant.
  let mut tree = tree_with_exit_rect(EXIT_200);
  tree.destroy_node(2);
  assert!(tree.try_node(2).is_none());
  tree.set_transition_now(100.0);
  assert!(!tree.advance_transitions());
}

#[test]
fn exit_with_delay_holds_then_leaves() {
  let mut tree = tree_with_exit_rect(TransitionEntry {
    spec: LINEAR_100,
    delay_ms: 50.0,
    from: None,
    exit: Some(transitions::AnimValue::Scalar(200.0)),
  });
  tree.detach_node(1, 2);
  tree.destroy_node(2);
  tree.set_transition_now(30.0);
  assert!(tree.advance_transitions(), "active during the hold");
  assert_eq!(rect_x(&tree, 2), 0.0, "sits in place until the hold expires");
  tree.set_transition_now(50.0);
  tree.advance_transitions();
  tree.set_transition_now(100.0);
  tree.advance_transitions();
  assert!((rect_x(&tree, 2) - 100.0).abs() < 0.01, "halfway 50ms after activation, got {}", rect_x(&tree, 2));
  tree.set_transition_now(150.0);
  tree.advance_transitions();
  assert!(tree.try_node(2).is_none(), "freed after the delayed exit settles");
}

// Group stagger: a `stagger` declaration on an ancestor spreads descendant
// enters and exits across time, index * stagger_ms each, counted per frame.

fn entry_from_100() -> TransitionEntry {
  TransitionEntry { spec: LINEAR_100, delay_ms: 0.0, from: Some(scalar(100.0)), exit: None }
}

// A root view marked as a stagger group (50ms), with `n` detached rects
// mounted at x=40 whose transition enters from 100.
fn tree_with_stagger_group(n: u64) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.set_transition_now(0.0);
  tree.create_node(1, View::default().with_layout());
  tree.edit(1, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: None, stagger_ms: Some(50.0) }));
    Damage::None
  });
  for id in 10..10 + n {
    tree.create_node(id, Rectangle::default().no_layout());
    tree.edit(id, |el| {
      el.transitions =
        Some(Box::new(TransitionConfig { props: vec![(AnimProp::X, entry_from_100())], all: None, stagger_ms: None }));
      match &mut el.kind {
        ElementKind::Rectangle(r) => r.set_x(40.0),
        _ => unreachable!(),
      }
    });
    tree.insert_node(1, id, None);
  }
  tree
}

#[test]
fn stagger_spreads_group_enters() {
  let mut tree = tree_with_stagger_group(3);
  // All three sit at `from` after the mount.
  for id in 10..13 {
    assert_eq!(rect_x(&tree, id), 100.0, "node {id} snapped to from");
  }
  // 25ms in: only the first (index 0, no extra delay) moves.
  tree.set_transition_now(25.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 10) < 100.0, "first item moves immediately");
  assert_eq!(rect_x(&tree, 11), 100.0, "second item held (50ms)");
  assert_eq!(rect_x(&tree, 12), 100.0, "third item held (100ms)");
  // 60ms activates the second; by 90ms it moves while the third holds.
  tree.set_transition_now(60.0);
  tree.advance_transitions();
  tree.set_transition_now(90.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 11) < 100.0, "second item cascades in");
  assert_eq!(rect_x(&tree, 12), 100.0, "third item still held");
  // 110ms activates the third; everyone settles on the mounted value.
  tree.set_transition_now(110.0);
  tree.advance_transitions();
  tree.set_transition_now(140.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 12) < 100.0, "third item cascades in last");
  tree.set_transition_now(300.0);
  assert!(!tree.advance_transitions());
  for id in 10..13 {
    assert_eq!(rect_x(&tree, id), 40.0, "node {id} settled");
  }
}

#[test]
fn stagger_counts_per_frame() {
  let mut tree = tree_with_stagger_group(1);
  // A second child mounted on a LATER frame starts its own count at zero:
  // no accumulated delay from the earlier mount.
  tree.set_transition_now(16.0);
  tree.create_node(20, Rectangle::default().no_layout());
  tree.edit(20, |el| {
    el.transitions =
      Some(Box::new(TransitionConfig { props: vec![(AnimProp::X, entry_from_100())], all: None, stagger_ms: None }));
    match &mut el.kind {
      ElementKind::Rectangle(r) => r.set_x(40.0),
      _ => unreachable!(),
    }
  });
  tree.insert_node(1, 20, None);
  tree.set_transition_now(30.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 20) < 100.0, "index restarted at 0: moves without a held delay");
}

#[test]
fn stagger_spreads_group_exits() {
  let mut tree = RenderTree::new();
  tree.set_transition_now(0.0);
  tree.create_node(1, View::default().with_layout());
  tree.edit(1, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: None, stagger_ms: Some(50.0) }));
    Damage::None
  });
  for id in 10..13 {
    tree.create_node(id, Rectangle::default().no_layout());
    tree.edit(id, |el| {
      el.transitions =
        Some(Box::new(TransitionConfig { props: vec![(AnimProp::X, EXIT_200)], all: None, stagger_ms: None }));
      Damage::None
    });
    tree.insert_node(1, id, None);
  }
  // Remove all three in one tick (a list teardown).
  tree.set_transition_now(1000.0);
  for id in 10..13 {
    tree.detach_node(1, id);
    tree.destroy_node(id);
  }
  // First item exits immediately; the others hold their slots.
  tree.set_transition_now(1025.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 10) > 0.0, "first item exits immediately");
  assert_eq!(rect_x(&tree, 11), 0.0, "second item holds (50ms)");
  assert_eq!(rect_x(&tree, 12), 0.0, "third item holds (100ms)");
  // 1060 activates the second, 1105 the third; the first frees at 1100+.
  tree.set_transition_now(1060.0);
  tree.advance_transitions();
  tree.set_transition_now(1085.0);
  tree.advance_transitions();
  assert!(rect_x(&tree, 11) > 0.0, "second item cascades out");
  assert_eq!(rect_x(&tree, 12), 0.0, "third item still holds");
  tree.set_transition_now(1105.0);
  tree.advance_transitions();
  assert!(tree.try_node(10).is_none(), "first item left first");
  tree.set_transition_now(1130.0);
  tree.advance_transitions();
  assert!(tree.try_node(12).is_some(), "third item still exiting");
  assert!(rect_x(&tree, 12) > 0.0 && rect_x(&tree, 12) < 200.0, "third item mid-flight, got {}", rect_x(&tree, 12));
  tree.set_transition_now(1300.0);
  tree.advance_transitions();
  for id in 10..13 {
    assert!(tree.try_node(id).is_none(), "node {id} freed after its slot in the cascade");
  }
  assert!(tree.node(1).children.is_empty());
}
