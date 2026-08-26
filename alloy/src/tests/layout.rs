use crate::impellers::Size;
use crate::rendertree::composite::layout_phase;
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
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.insert_node(2, 4, None);
  tree.insert_node(1, 5, None);
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
