//! Frame-damage resolution (okf/plans/partial-repaint.md, stage 1): the
//! damaged-id set plus the last_extent cells resolve into a FrameDamage
//! rect that covers a change's old and new pixels, degrades to Full on
//! anything uncertain, and reports None on an unchanged frame.

use crate::impellers::{Color, DisplayListBuilder, Point, Rect, Size};
use crate::rendertree::composite::paint_phase;
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;
use taffy::style::Overflow;

fn attached() -> Element {
  View::default().with_layout()
}

// A Context with no raster thread behind it (see tests/composite.rs).
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
// The same split as tests/composite.rs, so boxes are known: pane (0,0,320,300),
// header (0,0,320,51), body (0,51,320,100), detail (320,0,80,300).
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
  size(&mut tree, 4, 320.0, 100.0);
  tree.node_mut(5).style_mut().expect("detail").flex_grow = 1.0;
  tree
}

fn paint(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> FrameDamage {
  paint_phase(&mut DisplayListBuilder::new(None), tree, platform, alloy);
  tree.frame_damage()
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

// The damage covers `r`: trivially for Full, by containment for a rect.
// Containment tolerates the window clamp: `r` is intersected with the window
// first.
fn covers(damage: FrameDamage, r: Rect, window: Size) -> bool {
  let clamped = match r.intersection(&Rect::new(Point::zero(), window)) {
    Some(c) => c,
    None => return true,
  };
  match damage {
    FrameDamage::None => false,
    FrameDamage::Full => true,
    FrameDamage::Rect(d) => d.contains_rect(&clamped),
  }
}

fn setup() -> (RenderTree, PlatformContext, crate::Context) {
  let mut tree = split();
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);
  // First paint: everything is new, so the frame resolves Full.
  let first = paint(&mut tree, &platform, &alloy);
  assert_eq!(first, FrameDamage::Full, "first frame is fully damaged");
  (tree, platform, alloy)
}

#[test]
fn unchanged_frame_reports_no_damage() {
  let (mut tree, platform, alloy) = setup();
  assert_eq!(paint(&mut tree, &platform, &alloy), FrameDamage::None);
}

#[test]
fn paint_damage_is_the_nodes_rect() {
  let (mut tree, platform, alloy) = setup();
  tree.edit(4, |el| match &mut el.kind {
    ElementKind::Rectangle(r) => r.paint.set_color(Some(Color::new_srgba(1.0, 0.0, 0.0, 1.0))),
    _ => unreachable!(),
  });
  let damage = paint(&mut tree, &platform, &alloy);
  let window = Size::new(400.0, 300.0);
  // Covers the body's box, but stays a rect well inside the window: the
  // point of the whole exercise.
  assert!(covers(damage, rect(0.0, 51.0, 320.0, 100.0), window), "{damage:?}");
  match damage {
    FrameDamage::Rect(d) => {
      assert!(d.size.height < 110.0, "{damage:?}");
      assert!(d.origin.y > 45.0, "{damage:?}");
    }
    other => panic!("expected a rect, got {other:?}"),
  }
}

#[test]
fn compose_move_damages_old_and_new_place() {
  let (mut tree, platform, alloy) = setup();
  // A transform write: content caches survive (Compose), but the subtree
  // shows up somewhere else, so both places are damage.
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::View(v) => {
      v.translate = Some(Vector::new(30.0, 40.0));
      Damage::Compose
    }
    _ => unreachable!(),
  });
  let damage = paint(&mut tree, &platform, &alloy);
  let window = Size::new(400.0, 300.0);
  assert!(covers(damage, rect(0.0, 0.0, 320.0, 300.0), window), "old place: {damage:?}");
  assert!(covers(damage, rect(30.0, 40.0, 320.0, 260.0), window), "new place: {damage:?}");
  // The detail column (x 350..400) never changed.
  match damage {
    FrameDamage::Rect(d) => assert!(d.max_x() < 360.0, "{damage:?}"),
    other => panic!("expected a rect, got {other:?}"),
  }
}

#[test]
fn relayout_shift_damages_the_moved_sibling() {
  let (mut tree, platform, alloy) = setup();
  // Shrinking the pane moves the detail column left: the detail's old and
  // new places must be in the damage even though only the pane was edited.
  tree.edit(2, |el| {
    el.style_mut().expect("pane").size.width = length(100.0);
    Damage::Layout
  });
  let damage = paint(&mut tree, &platform, &alloy);
  let window = Size::new(400.0, 300.0);
  assert!(covers(damage, rect(320.0, 0.0, 80.0, 300.0), window), "detail's old place: {damage:?}");
  assert!(covers(damage, rect(100.0, 0.0, 300.0, 300.0), window), "detail's new place: {damage:?}");
  assert_ne!(damage, FrameDamage::None);
}

#[test]
fn removal_damages_the_parent_region() {
  let (mut tree, platform, alloy) = setup();
  tree.delete_node(2, 4);
  let damage = paint(&mut tree, &platform, &alloy);
  let window = Size::new(400.0, 300.0);
  assert!(covers(damage, rect(0.0, 51.0, 320.0, 100.0), window), "{damage:?}");
  // The pane's region bounds it; the detail column stays clean.
  match damage {
    FrameDamage::Rect(d) => assert!(d.max_x() < 330.0, "{damage:?}"),
    other => panic!("expected a rect, got {other:?}"),
  }
}

#[test]
fn resize_is_full_damage() {
  let (mut tree, platform, alloy) = setup();
  platform.set_window_size(500.0, 400.0);
  assert_eq!(paint(&mut tree, &platform, &alloy), FrameDamage::Full);
}

#[test]
fn scroll_damage_is_the_viewport_not_the_content() {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, Rectangle::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 200.0, 150.0);
  size(&mut tree, 3, 200.0, 1000.0);
  let style = tree.node_mut(2).style_mut().expect("scroller");
  style.overflow = taffy::Point { x: Overflow::Hidden, y: Overflow::Hidden };

  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);
  assert_eq!(paint(&mut tree, &platform, &alloy), FrameDamage::Full);

  tree.edit(2, |el| match &mut el.kind {
    ElementKind::View(v) => v.set_scroll_y(Some(120.0)),
    _ => unreachable!(),
  });
  let damage = paint(&mut tree, &platform, &alloy);
  let window = Size::new(400.0, 300.0);
  assert!(covers(damage, rect(0.0, 0.0, 200.0, 150.0), window), "{damage:?}");
  match damage {
    // The clipped envelope bounds the damage to the viewport box; the
    // content's 1000 px never leak out.
    FrameDamage::Rect(d) => {
      assert!(d.size.height < 160.0, "{damage:?}");
      assert!(d.size.width < 210.0, "{damage:?}");
    }
    other => panic!("expected a rect, got {other:?}"),
  }
}
