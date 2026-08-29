use crate::impellers::{Point, Size};
use crate::rendertree::composite::layout_phase;
use crate::rendertree::hit::{DefaultHitTester, HitTester};
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;

fn attached() -> Element {
  View::default().with_layout()
}

// A Context with no raster thread behind it: laying out views and
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

// A display write as the property layer makes it: layout damage, so the
// node's cache chain is invalidated like any other style change.
fn display(tree: &mut RenderTree, id: u64, display: Display) {
  tree.edit(id, |e| {
    e.style_mut().expect("laid-out node").display = display;
    Damage::Layout
  });
}

fn box_of(tree: &RenderTree, id: u64) -> Size {
  tree.node(id).layout_data().size()
}

fn layout(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) {
  platform.set_window_size(400.0, 300.0);
  layout_phase(tree, platform, alloy);
}

// A 400x300 row holding a list pane (2) with a header (3) and a body (4),
// next to a detail pane (5) that takes the rest.
//
// root(1, row) > pane(2, 320 wide) > header(3), body(4)
//              > detail(5, flex 1)
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
  size(&mut tree, 4, 320.0, 637.0);
  // The body overflows the pane like a scroll region does; keep the stated
  // heights instead of letting flex shrink them.
  for id in [3, 4] {
    tree.node_mut(id).style_mut().expect("pane child").flex_shrink = 0.0;
  }
  tree.node_mut(5).style_mut().expect("detail").flex_grow = 1.0;
  tree
}

// display none runs taffy's hidden pass over the whole subtree
// (okf/done/display-none-subtree.md): the pane, its view child and its
// measured leaf all report zero, and the hidden pass clears their caches so
// showing the pane again relays out from scratch.
#[test]
fn hidden_subtree_lays_out_to_zero_and_comes_back() {
  let mut tree = split();
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();

  layout(&mut tree, &platform, &alloy);
  assert_eq!(box_of(&tree, 3), Size::new(320.0, 51.0));
  assert_eq!(box_of(&tree, 4), Size::new(320.0, 637.0));
  assert_eq!(box_of(&tree, 5), Size::new(80.0, 300.0));

  display(&mut tree, 2, Display::None);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(box_of(&tree, 2), Size::zero());
  assert_eq!(box_of(&tree, 3), Size::zero());
  assert_eq!(box_of(&tree, 4), Size::zero());
  assert_eq!(box_of(&tree, 5), Size::new(400.0, 300.0));

  display(&mut tree, 2, Display::Flex);
  layout(&mut tree, &platform, &alloy);
  assert_eq!(box_of(&tree, 3), Size::new(320.0, 51.0));
  assert_eq!(box_of(&tree, 4), Size::new(320.0, 637.0));
  assert_eq!(box_of(&tree, 5), Size::new(80.0, 300.0));
}

// A design-size view for the layout-space tests: its children lay out at w x h.
fn design_size(w: f32, h: f32) -> Element {
  let mut v = View::default();
  v.set_design_size(Some((w, h)));
  v.with_layout()
}

fn layout_in(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context, w: f32, h: f32) {
  platform.set_window_size(w, h);
  layout_phase(tree, platform, alloy);
}

fn assert_xy(got: Point, x: f32, y: f32) {
  let eps = 1e-3;
  assert!((got.x - x).abs() < eps && (got.y - y).abs() < eps, "expected ({x}, {y}), got ({}, {})", got.x, got.y);
}

// Children of a design-size view lay out at the design size, not the box: flex
// and percentages resolve against the space the fit then maps onto the box,
// the same space paint and hit testing already hand them
// (okf/done/viewbox-layout-space.md). Pinned in both fit directions, like
// the clip and scroll rules before it.
#[test]
fn design_size_children_lay_out_in_design_space() {
  // root(1, 400x300) > minifying(2, box 100x100, design 200x200) > filler(3, flex 1)
  //                  > magnifying(4, box 100x100, design 50x50) > half(5, 50%)
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, design_size(200.0, 200.0));
  tree.create_node(3, attached());
  tree.create_node(4, design_size(50.0, 50.0));
  tree.create_node(5, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.insert_node(1, 4, None).expect("insert");
  tree.insert_node(4, 5, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 100.0, 100.0);
  size(&mut tree, 4, 100.0, 100.0);
  tree.node_mut(3).style_mut().expect("filler").flex_grow = 1.0;
  tree.node_mut(5).style_mut().expect("half").size = taffy::Size { width: percent(0.5), height: percent(0.5) };
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();

  layout(&mut tree, &platform, &alloy);
  // The views' own boxes are their layout boxes ...
  assert_eq!(box_of(&tree, 2), Size::new(100.0, 100.0));
  assert_eq!(box_of(&tree, 4), Size::new(100.0, 100.0));
  // ... and their children fill and halve the DESIGN boxes.
  assert_eq!(box_of(&tree, 3), Size::new(200.0, 200.0));
  assert_eq!(box_of(&tree, 5), Size::new(25.0, 25.0));
}

// From the outside a design-size view is a replaced element - the texture's <img>
// rules with the design size as intrinsic size. Unsized in a column it takes
// the column's width and the height the design aspect gives; one sized axis
// derives the other.
#[test]
fn design_size_view_sizes_like_a_replaced_element() {
  // root(1, 400x300 column) > unsized(2, design 200x100), sized(3, width 100, design 200x100)
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, design_size(200.0, 100.0));
  tree.create_node(3, design_size(200.0, 100.0));
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(1, 3, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  tree.node_mut(3).style_mut().expect("sized").size.width = length(100.0);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();

  layout(&mut tree, &platform, &alloy);
  assert_eq!(box_of(&tree, 2), Size::new(400.0, 200.0));
  assert_eq!(box_of(&tree, 3), Size::new(100.0, 50.0));
}

// Unlike a texture, a design-size view compresses: its min-content size is zero,
// since a design has no size it cannot scale below. A flex={1} view whose
// design is taller than the window fits the window instead of overflowing it
// (the canonical `<view flex={1} designSize>` on a phone).
#[test]
fn design_size_view_compresses_below_its_design() {
  // root(1, 400x300) > view(2, flex 1, design 800x1280)
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, design_size(800.0, 1280.0));
  tree.insert_node(1, 2, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  tree.node_mut(2).style_mut().expect("view").flex_grow = 1.0;
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();

  layout(&mut tree, &platform, &alloy);
  assert_eq!(box_of(&tree, 2), Size::new(400.0, 300.0));
}

// The inner layout input is constant, so a resize re-solves nothing below a
// design-size view: the child keeps its design-space box and its cache answers.
#[test]
fn design_size_children_survive_a_resize_from_cache() {
  // root(1, window-sized) > view(2, flex 1, design 200x200) > filler(3, flex 1)
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, design_size(200.0, 200.0));
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.root = Some(1);
  // The window root's own sizing (Window::initial_style).
  tree.node_mut(1).style_mut().expect("root").size = taffy::Size { width: percent(1.0), height: percent(1.0) };
  tree.node_mut(2).style_mut().expect("view").flex_grow = 1.0;
  tree.node_mut(3).style_mut().expect("filler").flex_grow = 1.0;
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();

  layout_in(&mut tree, &platform, &alloy, 400.0, 300.0);
  assert_eq!(box_of(&tree, 2), Size::new(400.0, 300.0));
  assert_eq!(box_of(&tree, 3), Size::new(200.0, 200.0));

  counters::take();
  layout_in(&mut tree, &platform, &alloy, 200.0, 150.0);
  assert_eq!(box_of(&tree, 2), Size::new(200.0, 150.0));
  assert_eq!(box_of(&tree, 3), Size::new(200.0, 200.0));
  let after = counters::take();
  assert!(after.cache_hits > 0, "the filler's layout must come from its cache");
}

// The placements hit testing sees are the design-space ones: a laid-out child
// under a design-size view is hit where the fit paints it, in both directions.
#[test]
fn design_size_laid_out_children_hit_in_design_space() {
  // root(1, 400x300 column) > minifying(2, box 100x100, design 200x200) > a(3, 100x100), b(4, 100x100)
  //                         > magnifying(5, box 100x100, design 50x50) > c(6, 25x25), d(7, 25x25)
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, design_size(200.0, 200.0));
  tree.create_node(3, attached());
  tree.create_node(4, attached());
  tree.create_node(5, design_size(50.0, 50.0));
  tree.create_node(6, attached());
  tree.create_node(7, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.insert_node(2, 4, None).expect("insert");
  tree.insert_node(1, 5, None).expect("insert");
  tree.insert_node(5, 6, None).expect("insert");
  tree.insert_node(5, 7, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 100.0, 100.0);
  size(&mut tree, 3, 100.0, 100.0);
  size(&mut tree, 4, 100.0, 100.0);
  size(&mut tree, 5, 100.0, 100.0);
  size(&mut tree, 6, 25.0, 25.0);
  size(&mut tree, 7, 25.0, 25.0);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  layout(&mut tree, &platform, &alloy);

  // b sits at design (0,100)-(100,200): box (0,50)-(50,100) under the 0.5 fit.
  let path = DefaultHitTester.hit_test(&tree, Point::new(25.0, 75.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 2, 4]);
  assert_xy(path[2].2, 50.0, 50.0);

  // d sits at design (0,25)-(25,50): box (0,50)-(50,100) under the 2x fit,
  // in view 5, which the column places below view 2.
  let path = DefaultHitTester.hit_test(&tree, Point::new(25.0, 175.0));
  let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
  assert_eq!(ids, vec![1, 5, 7]);
  assert_xy(path[2].2, 12.5, 12.5);
}

// In a flex ROW a one-axis-sized design-size view is a flex item like any other:
// with the default stretch alignment the line's cross size wins over the
// design aspect (CSS's rule for a replaced element too - an <img> with a width
// stretches in a flex row), a style aspect ratio does not change that, and a
// non-stretch alignment lets the aspect height through.
#[test]
fn design_size_view_in_a_row_stretches_unless_aligned() {
  // root(1, 400x300 row) > plain(2, width 100), aspect(3, width 100, aspectRatio 2), start(4, width 100, alignSelf start)
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  for id in [2, 3, 4] {
    tree.create_node(id, design_size(200.0, 100.0));
    tree.insert_node(1, id, None).expect("insert");
    tree.node_mut(id).style_mut().expect("tile").size.width = length(100.0);
  }
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  tree.node_mut(1).style_mut().expect("root").flex_direction = FlexDirection::Row;
  tree.node_mut(3).style_mut().expect("aspect").aspect_ratio = Some(2.0);
  tree.node_mut(4).style_mut().expect("start").align_self = Some(AlignSelf::FLEX_START);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();

  layout(&mut tree, &platform, &alloy);
  assert_eq!(box_of(&tree, 2), Size::new(100.0, 300.0));
  assert_eq!(box_of(&tree, 3), Size::new(100.0, 300.0));
  assert_eq!(box_of(&tree, 4), Size::new(100.0, 50.0));
}

// A span has no layout of its own but feeds its text's measure: inserting or
// detaching one invalidates the text's cache like a text write does. (A text
// laid out empty, then given a span, otherwise keeps its 0-wide box.)
#[test]
fn span_insert_and_detach_invalidate_the_text_layout() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, Text::default().with_layout());
  tree.create_node(3, Span { text: "NITRO!".into(), ..Default::default() }.no_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  let cached = |tree: &RenderTree| !tree.node(2).layout_data().cache.is_empty();

  layout(&mut tree, &platform, &alloy);
  assert!(cached(&tree));
  tree.insert_node(2, 3, None).expect("insert");
  assert!(!cached(&tree));

  layout(&mut tree, &platform, &alloy);
  assert!(cached(&tree));
  tree.detach_node(2, 3);
  assert!(!cached(&tree));
}
