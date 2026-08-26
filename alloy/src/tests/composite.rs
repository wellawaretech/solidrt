use crate::impellers::DisplayListBuilder;
use crate::rendertree::composite::paint_phase;
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
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats)
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
  tree.insert_node(1, 2, None);
  tree.insert_node(2, 3, None);
  tree.insert_node(2, 4, None);
  tree.insert_node(1, 5, None);
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
