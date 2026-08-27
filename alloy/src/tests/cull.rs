use crate::impellers::{DrawStyle, Matrix, Point, Rect, Size};
use crate::rendertree::cull::{envelope, CullRect, Extent};
use crate::rendertree::*;
use taffy::style::Overflow;

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

fn clip(tree: &mut RenderTree, id: u64) {
  let l = tree.node_mut(id).layout_data_mut();
  l.style.overflow.x = Overflow::Hidden;
  l.style.overflow.y = Overflow::Hidden;
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

// root(1) > scroller(2, clips) > column(3) > blocks(4, 5)
fn scroller_tree() -> (RenderTree, PlatformContext) {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, attached());
  tree.create_node(4, Rectangle::default().with_layout());
  tree.create_node(5, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.insert_node(3, 4, None);
  tree.insert_node(3, 5, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 2, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 3, 0.0, 0.0, 400.0, 2000.0);
  place(&mut tree, 4, 0.0, 0.0, 400.0, 1000.0);
  place(&mut tree, 5, 0.0, 1000.0, 400.0, 1000.0);
  clip(&mut tree, 2);
  (tree, PlatformContext::new(Vec::new()))
}

#[test]
fn clipping_node_envelope_is_its_box() {
  let (tree, platform) = scroller_tree();
  let frame = Size::new(400.0, 300.0);
  // The column reaches 2000 px; the scroller cuts that to its own box.
  assert!(close(bounded(envelope(&tree, 3, &platform, frame)), rect(-1.0, -1.0, 402.0, 2002.0)));
  assert!(close(bounded(envelope(&tree, 2, &platform, frame)), rect(0.0, 0.0, 400.0, 300.0)));
}

#[test]
fn overflowing_child_grows_parent_envelope() {
  let (mut tree, platform) = scroller_tree();
  // Block 5 pokes out of the column's box on the right.
  place(&mut tree, 5, 350.0, 1000.0, 400.0, 1000.0);
  let env = bounded(envelope(&tree, 3, &platform, Size::new(400.0, 300.0)));
  assert!(close(env, rect(-1.0, -1.0, 752.0, 2002.0)), "{env:?}");
}

#[test]
fn unbounded_extent_propagates_up_to_the_clipper() {
  let (mut tree, platform) = scroller_tree();
  let mut v = View::default();
  v.set_rotate_y(Some(0.5));
  v.set_perspective(Some(400.0));
  tree.create_node(6, v.with_layout());
  tree.create_node(7, Rectangle::default().with_layout());
  tree.insert_node(4, 6, None);
  tree.insert_node(6, 7, None);
  place(&mut tree, 6, 0.0, 0.0, 10.0, 10.0);
  place(&mut tree, 7, 0.0, 0.0, 10.0, 10.0);
  let frame = Size::new(400.0, 300.0);
  assert_eq!(envelope(&tree, 4, &platform, frame), Extent::Unbounded);
  assert_eq!(envelope(&tree, 3, &platform, frame), Extent::Unbounded);
  // Sibling stays bounded, the scroller stays its box.
  assert!(matches!(envelope(&tree, 5, &platform, frame), Extent::Bounded(_)));
  assert!(close(bounded(envelope(&tree, 2, &platform, frame)), rect(0.0, 0.0, 400.0, 300.0)));
}

#[test]
fn envelope_passes_through_own_matrix() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_scale_x(Some(0.5));
  v.set_scale_y(Some(0.5));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 50.0, 50.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 100.0);
  let platform = PlatformContext::new(Vec::new());
  // Scale 0.5 around the box center: the 100x100 box (with 1px AA outset from
  // the rect child) shrinks to ~51x51 centered at (50, 50) in the slot frame.
  let env = bounded(envelope(&tree, 2, &platform, Size::new(200.0, 200.0)));
  assert!(close(env, rect(24.5, 24.5, 51.0, 51.0)), "{env:?}");
}

#[test]
fn perspective_makes_envelope_unbounded() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut v = View::default();
  v.set_rotate_y(Some(0.5));
  v.set_perspective(Some(400.0));
  tree.create_node(2, v.with_layout());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 50.0, 50.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 100.0);
  let platform = PlatformContext::new(Vec::new());
  assert_eq!(envelope(&tree, 2, &platform, Size::new(200.0, 200.0)), Extent::Unbounded);
}

#[test]
fn envelope_cache_follows_damage() {
  let (mut tree, platform) = scroller_tree();
  let frame = Size::new(400.0, 300.0);
  let before = bounded(envelope(&tree, 3, &platform, frame));
  // A property write on a leaf walks invalidate_paint up to the root.
  place(&mut tree, 5, 0.0, 1000.0, 400.0, 3000.0);
  tree.apply_damage(5, Damage::Layout);
  let after = bounded(envelope(&tree, 3, &platform, frame));
  assert!(close(before, rect(-1.0, -1.0, 402.0, 2002.0)));
  assert!(close(after, rect(-1.0, -1.0, 402.0, 4002.0)), "{after:?}");
}

#[test]
fn compose_damage_clears_the_nodes_own_envelope() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 200.0, 200.0);
  place(&mut tree, 2, 0.0, 0.0, 100.0, 100.0);
  place(&mut tree, 3, 0.0, 0.0, 100.0, 100.0);
  let platform = PlatformContext::new(Vec::new());
  let frame = Size::new(200.0, 200.0);
  let before = bounded(envelope(&tree, 2, &platform, frame));
  let damage = match &mut tree.node_mut(2).kind {
    ElementKind::View(v) => v.set_x(Some(40.0)),
    _ => unreachable!(),
  };
  tree.apply_damage(2, damage);
  let after = bounded(envelope(&tree, 2, &platform, frame));
  assert!(close(before, rect(-1.0, -1.0, 102.0, 102.0)));
  assert!(close(after, rect(39.0, -1.0, 102.0, 102.0)), "{after:?}");
}

#[test]
fn cull_rect_walks_the_record_order() {
  let cull: Option<Rect> = Some(rect(0.0, 0.0, 400.0, 300.0));
  // Into a child at (10, 20).
  assert!(close(cull.into_child(Point::new(10.0, 20.0)).unwrap(), rect(-10.0, -20.0, 400.0, 300.0)));
  // Clip on y only, then scroll by 1000: the visible window in the child frame
  // is y in [1000, 1300]; x keeps the incoming bound.
  let c = cull.clipped(Size::new(400.0, 300.0), false, true).scrolled(Vector::new(0.0, 1000.0)).unwrap();
  assert!(close(c, rect(0.0, 1000.0, 400.0, 300.0)), "{c:?}");
  // A y-only clip of an unknown rect leaves x unbounded.
  let c = None::<Rect>.clipped(Size::new(400.0, 300.0), false, true).unwrap();
  assert!(c.origin.x < -1.0e6 && c.max_x() > 1.0e6 && (c.size.height - 300.0).abs() < 1e-3);
  // Through a scale-by-2 matrix the window halves.
  let m = Matrix::scale(2.0, 2.0, 1.0);
  assert!(close(cull.through(&m).unwrap(), rect(0.0, 0.0, 200.0, 150.0)));
  // A perspective matrix gives up.
  let mut p = Matrix::identity();
  p.m34 = -1.0 / 400.0;
  assert!(cull.through(&p).is_none());
  // None stays None through everything but a clip, which bounds it.
  let none: Option<Rect> = None;
  assert!(none.into_child(Point::new(1.0, 1.0)).is_none());
  assert!(none.through(&m).is_none());
  assert!(close(none.clipped(Size::new(10.0, 10.0), true, true).unwrap(), rect(0.0, 0.0, 10.0, 10.0)));
}

#[test]
fn extent_intersection_semantics() {
  let cull = rect(0.0, 0.0, 100.0, 100.0);
  assert!(!Extent::Empty.may_intersect(&cull));
  assert!(Extent::Unbounded.may_intersect(&cull));
  assert!(Extent::Bounded(rect(90.0, 90.0, 50.0, 50.0)).may_intersect(&cull));
  assert!(!Extent::Bounded(rect(100.0, 0.0, 50.0, 50.0)).may_intersect(&cull));
}

#[test]
fn text_painted_extent_starts_at_the_content_origin() {
  // Text paints from the content box origin at the content width; the
  // extent must move with the inset exactly as build() does
  // (okf/done/padding-box-divergence.md).
  let platform = PlatformContext::new(Vec::new());
  let mut t = Text::default();
  t.set_plain_text("hello".to_string());
  let full = t.painted_extent(&platform, rect(0.0, 0.0, 100.0, 50.0)).expect("owned layout has an extent");
  let inset = t.painted_extent(&platform, rect(10.0, 5.0, 80.0, 40.0)).expect("owned layout has an extent");
  assert!((inset.origin.x - full.origin.x - 10.0).abs() < 1e-3);
  assert!((inset.origin.y - full.origin.y - 5.0).abs() < 1e-3);
  assert!((full.size.width - inset.size.width - 20.0).abs() < 1e-3);
}

// A display none subtree is left out of the envelope
// (okf/done/display-none-subtree.md): a huge stale box under the hidden
// pane does not grow the parent's envelope past its own box.
#[test]
fn hidden_subtree_leaves_the_envelope() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  place(&mut tree, 2, 0.0, 0.0, 0.0, 0.0);
  place(&mut tree, 3, 0.0, 0.0, 5000.0, 5000.0);
  tree.node_mut(2).style_mut().expect("pane").display = taffy::style::Display::None;

  let platform = PlatformContext::new(Vec::new());
  let env = bounded(envelope(&tree, 1, &platform, Size::new(400.0, 300.0)));
  assert!(close(env, rect(0.0, 0.0, 400.0, 300.0)), "{env:?}");
}

// A detached line or path is bounded by what it paints, so one entirely
// outside the cull rect is culled and one crossing it is not.
#[test]
fn detached_line_and_path_extents_are_their_painted_boxes() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  let mut far = Line::default();
  far.set_points(Some(vec![500.0, 500.0, 600.0, 650.0]));
  far.paint.stroke_width = 4.0;
  tree.create_node(2, far.no_layout());
  let mut crossing = Line::default();
  crossing.set_x1(Some(390.0));
  crossing.set_y1(Some(290.0));
  crossing.set_x2(Some(600.0));
  crossing.set_y2(Some(650.0));
  tree.create_node(3, crossing.no_layout());
  let mut far_path = Path::default();
  far_path.set_d("M500 500 L600 650".into());
  far_path.paint.draw_style = DrawStyle::Stroke;
  far_path.paint.stroke_width = 4.0;
  tree.create_node(4, far_path.no_layout());
  tree.insert_node(1, 2, None);
  tree.insert_node(1, 3, None);
  tree.insert_node(1, 4, None);
  tree.root = Some(1);
  place(&mut tree, 1, 0.0, 0.0, 400.0, 300.0);
  let platform = PlatformContext::new(Vec::new());
  let frame = Size::new(400.0, 300.0);
  let cull = rect(0.0, 0.0, 400.0, 300.0);

  let far = envelope(&tree, 2, &platform, frame);
  assert!(close(bounded(far), rect(497.0, 497.0, 106.0, 156.0)), "{far:?}");
  assert!(!far.may_intersect(&cull));
  assert!(envelope(&tree, 3, &platform, frame).may_intersect(&cull));
  let far_path = envelope(&tree, 4, &platform, frame);
  assert!(close(bounded(far_path), rect(497.0, 497.0, 106.0, 156.0)), "{far_path:?}");
  assert!(!far_path.may_intersect(&cull));
}
