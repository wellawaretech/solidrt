use crate::rendertree::{transitions, *};

// A detached rect under a root view, with a transition declared for every
// property: the shape a `<d-rect transition={{ all: ... }}>` write hits.
fn tree_with_animated_rect(spec: TransitionSpec) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  tree.create_node(2, Rectangle::default().no_layout());
  tree.insert_node(1, 2, None);
  tree.edit(2, |el| {
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(spec) }));
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
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(LINEAR_100) }));
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
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(LINEAR_100) }));
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
    el.transitions = Some(Box::new(TransitionConfig { props: vec![], all: Some(LINEAR_100) }));
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
