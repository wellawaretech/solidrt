use crate::impellers::{Matrix, Point, Rect, Size};
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
fn unbounded_kind_propagates_up_to_the_clipper() {
  let (mut tree, platform) = scroller_tree();
  tree.create_node(6, Path::default().with_layout());
  tree.insert_node(4, 6, None);
  place(&mut tree, 6, 0.0, 0.0, 10.0, 10.0);
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
  v.set_scale_x(0.5);
  v.set_scale_y(0.5);
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
  v.set_rotate_y(0.5);
  v.set_perspective(400.0);
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
    ElementKind::View(v) => v.set_x(40.0),
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
