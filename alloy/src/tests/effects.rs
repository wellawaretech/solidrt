// Shadow and filter effects: damage classes, paint-envelope growth, and the
// fused color matrix. The draw side is covered live (the effects probe);
// these pin the arithmetic partial repaint and culling depend on.

use crate::impellers::{Point, Rect, Size};
use crate::rendertree::cull::{envelope, Extent};
use crate::rendertree::kinds::matrix_for_tests;
use crate::rendertree::*;

fn place(tree: &mut RenderTree, id: u64, x: f32, y: f32, w: f32, h: f32) {
  let l = tree.node_mut(id).layout_data_mut();
  l.computed.location = taffy::Point { x, y };
  l.computed.size = taffy::Size { width: w, height: h };
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn bounded(e: Extent) -> Rect {
  match e {
    Extent::Bounded(r) => r,
    other => panic!("expected a bounded extent, got {other:?}"),
  }
}

fn close(a: Rect, b: Rect) -> bool {
  let eps = 1e-3;
  (a.origin.x - b.origin.x).abs() < eps
    && (a.origin.y - b.origin.y).abs() < eps
    && (a.size.width - b.size.width).abs() < eps
    && (a.size.height - b.size.height).abs() < eps
}

fn shadow() -> ShadowState {
  ShadowState {
    dx: 10.0,
    dy: 20.0,
    blur: 8.0,
    spread: 4.0,
    color: crate::impellers::Color::new_srgba(0.0, 0.0, 0.0, 0.4),
  }
}

#[test]
fn shadow_and_filter_report_their_damage_class() {
  let mut r = Rectangle::default();
  assert_eq!(r.set_shadow(Some(shadow())), Damage::Paint);
  let mut v = View::default();
  // Compose, like opacity: boundary caches stay valid, composite re-applies.
  assert_eq!(v.set_filter(Some(FilterState { blur: Some(4.0), ..Default::default() })), Damage::Compose);
}

#[test]
fn shadow_grows_the_paint_envelope() {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  let mut r = Rectangle::default();
  r.set_shadow(Some(shadow()));
  tree.create_node(2, r.with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  let platform = PlatformContext::new(Vec::new());

  // Base box + AA is (-1,-1,102,102). The shadow casts the box offset by
  // (10,20) and grown by spread 4 + blur reach 8 * 0.5 * 3 = 12, so 16 per
  // side, AA on top: (-7,3,134,134). The union spans both.
  let env = bounded(envelope(&tree, 2, &platform, Size::new(400.0, 300.0)));
  assert!(close(env, rect(-7.0, -1.0, 134.0, 138.0)), "{env:?}");
}

#[test]
fn filter_blur_grows_the_subtree_envelope() {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  let mut v = View::default();
  v.set_filter(Some(FilterState { blur: Some(8.0), ..Default::default() }));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 100.0);
  let platform = PlatformContext::new(Vec::new());

  // Child + AA is (-1,-1,102,102); the blur softens outward by its reach
  // (8 * 0.5 * 3 = 12) on every side.
  let env = bounded(envelope(&tree, 2, &platform, Size::new(400.0, 300.0)));
  assert!(close(env, rect(-13.0, -13.0, 126.0, 126.0)), "{env:?}");

  // A color-only filter has no reach.
  let ElementKind::View(v) = &mut tree.node_mut(2).kind else { panic!("view") };
  v.set_filter(Some(FilterState { grayscale: Some(1.0), ..Default::default() }));
  tree.node(2).envelope.clear();
  let env = bounded(envelope(&tree, 2, &platform, Size::new(400.0, 300.0)));
  assert!(close(env, rect(-1.0, -1.0, 102.0, 102.0)), "{env:?}");
}

#[test]
fn backdrop_regions_widen_damage() {
  use crate::rendertree::damage::expand_damage_for_backdrops;
  let panel = (rect(100.0, 100.0, 200.0, 80.0), 12.0f32);

  // Damage away from the panel passes through untouched.
  let far = expand_damage_for_backdrops(Extent::Bounded(rect(500.0, 500.0, 10.0, 10.0)), &[Some(panel)]);
  assert_eq!(far, Extent::Bounded(rect(500.0, 500.0, 10.0, 10.0)));

  // Damage just outside the panel but inside the blur's reach pulls the
  // whole panel into the repaint rect.
  let near = expand_damage_for_backdrops(Extent::Bounded(rect(90.0, 110.0, 5.0, 5.0)), &[Some(panel)]);
  assert_eq!(near, Extent::Bounded(rect(90.0, 100.0, 210.0, 80.0).union(&rect(90.0, 110.0, 5.0, 5.0))));

  // One panel's growth can reach a second panel (iteration).
  let chained = expand_damage_for_backdrops(
    Extent::Bounded(rect(90.0, 110.0, 5.0, 5.0)),
    &[Some(panel), Some((rect(305.0, 100.0, 50.0, 50.0), 10.0))],
  );
  let Extent::Bounded(r) = chained else { panic!("bounded") };
  assert!(r.contains_rect(&rect(305.0, 100.0, 50.0, 50.0)), "{r:?}");

  // An unmappable region makes any damage full-frame; empty damage stays
  // empty (nothing changed, nothing to re-filter).
  assert_eq!(expand_damage_for_backdrops(Extent::Bounded(rect(0.0, 0.0, 1.0, 1.0)), &[None]), Extent::Unbounded);
  assert_eq!(expand_damage_for_backdrops(Extent::Empty, &[Some(panel)]), Extent::Empty);
}

#[test]
fn detached_backdrop_view_has_a_bounded_extent() {
  let mut tree = RenderTree::new();
  tree.create_node(1, View::default().with_layout());
  let mut v = View::default();
  v.set_backdrop_filter(Some(FilterState { blur: Some(4.0), ..Default::default() }));
  tree.create_node(2, v.no_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  let platform = PlatformContext::new(Vec::new());

  // A childless d-view normally has an Empty (cullable) envelope; with a
  // backdrop filter its box is painted content.
  let env = bounded(envelope(&tree, 2, &platform, Size::new(400.0, 300.0)));
  assert!(close(env, rect(0.0, 0.0, 400.0, 300.0)), "{env:?}");
}

#[test]
fn color_matrix_composition() {
  // No color keys: no matrix at all.
  assert!(matrix_for_tests(&FilterState { blur: Some(4.0), ..Default::default() }).is_none());

  // Full grayscale: every RGB row is the luminance row, alpha stays.
  let m = matrix_for_tests(&FilterState { grayscale: Some(1.0), ..Default::default() }).expect("matrix");
  for row in 0..3 {
    assert!((m[row * 5] - 0.213).abs() < 1e-4, "{m:?}");
    assert!((m[row * 5 + 1] - 0.715).abs() < 1e-4, "{m:?}");
    assert!((m[row * 5 + 2] - 0.072).abs() < 1e-4, "{m:?}");
    assert_eq!(m[row * 5 + 4], 0.0);
  }
  assert_eq!(&m[15..20], &[0.0, 0.0, 0.0, 1.0, 0.0]);

  // Full invert: slope -1, translation 1 - normalized, because the shipped
  // Impeller adds the column in 0..1 space despite its header's 0..255
  // claim (okf/upstream/impeller-color-matrix-translation.md).
  let m = matrix_for_tests(&FilterState { invert: Some(1.0), ..Default::default() }).expect("matrix");
  assert!((m[0] + 1.0).abs() < 1e-4, "{m:?}");
  assert!((m[4] - 1.0).abs() < 1e-4, "{m:?}");

  // Contrast pivots on 0.5: slope c, offset 0.5 - 0.5c.
  let m = matrix_for_tests(&FilterState { contrast: Some(2.0), ..Default::default() }).expect("matrix");
  assert!((m[0] - 2.0).abs() < 1e-4, "{m:?}");
  assert!((m[4] + 0.5).abs() < 1e-4, "{m:?}");

  // Composition applies grayscale before brightness (the documented order):
  // the combined red row is 2x the luminance row.
  let m = matrix_for_tests(&FilterState { grayscale: Some(1.0), brightness: Some(2.0), ..Default::default() })
    .expect("matrix");
  assert!((m[0] - 0.426).abs() < 1e-4, "{m:?}");
  assert!((m[1] - 1.430).abs() < 1e-4, "{m:?}");

  // A neutral hue rotation is the identity (sanity for the SVG matrix).
  let m = matrix_for_tests(&FilterState { hue_rotate: Some(0.0), ..Default::default() }).expect("matrix");
  for row in 0..4 {
    for col in 0..5 {
      let id = if row == col { 1.0 } else { 0.0 };
      assert!((m[row * 5 + col] - id).abs() < 1e-4, "{m:?}");
    }
  }
}
