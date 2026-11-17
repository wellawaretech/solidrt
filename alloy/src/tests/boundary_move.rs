// A repaint boundary that a relayout only moves (a sibling above it grew):
// its own cache is stated relative to its own origin, so it is kept, and
// the clear starts at the parent (set_unrounded_layout). The ancestors'
// caches go, the damage holds the old and the new place, and a move that
// takes the snapped box onto other device pixels re-records once
// (composite.rs, the Recording arm). A write of the boundary's own inset is
// a layout write and clears its own cache, as any layout write does. Read
// through PaintStats, the frame damage and the ops the walk drew
// (`DisplayList::ops`, `renderer::oplog::placed`). The snapshot half needs
// GL rasters: alloy/examples/boundary_layout_move.rs.
use crate::renderer::oplog::{placed, Op};
use crate::renderer::prelude::*;
use crate::renderer::{Color, DisplayList, DisplayListBuilder, DrawStyle, Rect};
use crate::rendertree::composite::{apply_content_changes, paint_phase, PaintStats};
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;

const ROOT: u64 = 1;
const OUTER: u64 = 2;
const SIBLING: u64 = 3;
const CARD: u64 = 4;
const FILL: u64 = 5;
const ANCHOR: u64 = 6;

const WINDOW_W: f32 = 400.0;
const WINDOW_H: f32 = 300.0;
const DISPLAY_SCALES: [f32; 2] = [1.0, 1.5];
// The card's box and the sibling above it, before and after it grows:
// whole device pixels at every display scale above.
const CARD_W: f32 = 100.0;
const CARD_H: f32 = 50.0;
const SIBLING_H: f32 = 20.0;
const GROWN_H: f32 = 40.0;
// A card placed by its own insets: where it starts, how far each write
// moves it, how many writes, and a jump clear of its old box.
const LEFT: f32 = 10.0;
const TOP: f32 = 10.0;
const STEP: f32 = 20.0;
const STEPS: u32 = 4;
const JUMP: f32 = 100.0;
// A node at the window's bottom-right corner, past every place of the card.
const ANCHOR_SIDE: f32 = 10.0;
// Frames painted after a change.
const STILL_FRAMES: u32 = 3;
// The snapped-box case, in device px: the sibling's height, a card whose far
// edge is a fraction above a whole device pixel (it rounds down), and a
// growth of the sibling that takes that edge past the half (it rounds up)
// while the near edge still rounds down.
const SNAP_SIBLING_DEVICE: f32 = 30.0;
const SNAP_CARD_H_DEVICE: f32 = 75.4;
const NUDGE_DEVICE: f32 = 0.4;
// How far a device coordinate read back from the ops may be off.
const EPS: f32 = 1e-3;

fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
}

// A style write as the runtime performs it: a Layout damage.
fn styled(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut Style)) {
  tree.edit(id, |el| {
    f(el.style_mut().expect("laid-out node"));
    Damage::Layout
  });
}

fn sized(style: &mut Style, w: f32, h: f32) {
  style.size = taffy::Size { width: length(w), height: length(h) };
}

// A move only the layout pass sees: the inset is written with no damage of
// its own, so what the frame clears is what set_unrounded_layout clears for
// the box it moved. (A style write's Layout damage clears the node's own
// paint before any layout runs, RenderTree::apply_damage.)
fn relayout_top(tree: &mut RenderTree, id: u64, top: f32) {
  tree.node_mut(id).style_mut().expect("laid-out node").inset.top = length(top);
  tree.invalidate_cache(id);
}

fn recording(tree: &mut RenderTree, id: u64) {
  tree.edit(id, |el| {
    el.repaint_boundary = BoundaryMode::Recording;
    Damage::Paint
  });
}

// The card's fill: a laid-out rect filling it.
fn fill(tree: &mut RenderTree, w: f32, h: f32) {
  tree.create_node(FILL, Rectangle::default().with_layout());
  tree.insert_node(CARD, FILL, None).expect("insert");
  styled(tree, FILL, |s| sized(s, w, h));
  tree
    .edit(FILL, |el| el.kind.paint_mut().expect("a rect paints").set_color(Some(Color::new_srgba(1.0, 1.0, 1.0, 1.0))));
}

fn root() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.root = Some(ROOT);
  styled(&mut tree, ROOT, |s| {
    sized(s, WINDOW_W, WINDOW_H);
    s.flex_direction = FlexDirection::Column;
  });
  tree
}

// The parent the card and its sibling go in: the root, or a Recording
// boundary filling it.
fn holder(tree: &mut RenderTree, outer: bool) -> u64 {
  if !outer {
    return ROOT;
  }
  tree.create_node(OUTER, View::default().with_layout());
  tree.insert_node(ROOT, OUTER, None).expect("insert");
  styled(tree, OUTER, |s| {
    sized(s, WINDOW_W, WINDOW_H);
    s.flex_direction = FlexDirection::Column;
  });
  recording(tree, OUTER);
  OUTER
}

// root (column) > [outer, a Recording boundary >] sibling > card (a
// Recording boundary) > fill.
fn pushed(outer: bool, sibling_h: f32, card_h: f32) -> RenderTree {
  let mut tree = root();
  let parent = holder(&mut tree, outer);
  tree.create_node(SIBLING, View::default().with_layout());
  tree.create_node(CARD, View::default().with_layout());
  tree.insert_node(parent, SIBLING, None).expect("insert");
  tree.insert_node(parent, CARD, None).expect("insert");
  styled(&mut tree, SIBLING, |s| {
    sized(s, WINDOW_W, sibling_h);
    s.flex_shrink = 0.0;
  });
  styled(&mut tree, CARD, |s| {
    sized(s, CARD_W, card_h);
    s.flex_shrink = 0.0;
  });
  fill(&mut tree, CARD_W, card_h);
  recording(&mut tree, CARD);
  tree
}

// root > [outer >] card (absolute at LEFT, TOP) > fill; `card` is the
// card's kind and `boundary` its mode.
fn placed_card(outer: bool, card: Element, boundary: BoundaryMode) -> RenderTree {
  let mut tree = root();
  let parent = holder(&mut tree, outer);
  tree.create_node(CARD, card);
  tree.insert_node(parent, CARD, None).expect("insert");
  styled(&mut tree, CARD, |s| {
    sized(s, CARD_W, CARD_H);
    s.position = Position::Absolute;
    s.inset.left = length(LEFT);
    s.inset.top = length(TOP);
  });
  if matches!(tree.node(CARD).kind, ElementKind::View(_)) {
    fill(&mut tree, CARD_W, CARD_H);
  }
  tree.edit(CARD, |el| {
    el.repaint_boundary = boundary;
    Damage::Paint
  });
  tree
}

struct Stage {
  platform: PlatformContext,
  alloy: crate::Context,
}

impl Stage {
  fn at_scale(display_scale: f32) -> Self {
    let platform = PlatformContext::new(Vec::new());
    platform.set_window_size(WINDOW_W, WINDOW_H);
    platform.set_display_scale(display_scale);
    Self { platform, alloy: headless() }
  }

  fn frame(&self, tree: &mut RenderTree) -> PaintStats {
    apply_content_changes(tree, &self.platform, &self.alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &self.platform, &self.alloy)
  }

  // The frame recorded the way `composite::render` records one, under the
  // display scale, so the list's root space is device pixels.
  fn drawn(&self, tree: &mut RenderTree) -> (PaintStats, DisplayList) {
    apply_content_changes(tree, &self.platform, &self.alloy);
    let mut builder = DisplayListBuilder::new(None);
    let scale = self.platform.display_scale();
    builder.scale(scale, scale);
    let stats = paint_phase(&mut builder, tree, &self.platform, &self.alloy);
    (stats, builder.build().expect("the frame builds"))
  }

  // Frames after a change: every boundary replays.
  fn still(&self, tree: &mut RenderTree, boundaries: u32, what: &str) {
    for look in 1..=STILL_FRAMES {
      let stats = self.frame(tree);
      assert_eq!(counts(&stats), (0, boundaries), "{what}: still {look}");
    }
  }
}

// A frame's (recorded, reused) boundaries.
fn counts(stats: &PaintStats) -> (u32, u32) {
  (stats.boundaries_recorded, stats.boundaries_reused)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), crate::renderer::Size::new(w, h))
}

// The frame damage holds `r`.
fn covers(damage: FrameDamage, r: Rect) -> bool {
  match damage {
    FrameDamage::None => false,
    FrameDamage::Full => true,
    FrameDamage::Rect(d) => d.contains_rect(&r),
  }
}

// The device box of the one filled rect the frame drew.
fn filled_rect(list: &DisplayList) -> Rect {
  let mut found = placed(list.ops())
    .into_iter()
    .filter(|p| matches!(p.op, Op::DrawRect { paint, .. } if paint.draw_style == DrawStyle::Fill));
  let drawn = found.next().expect("a filled rect is drawn");
  assert!(found.next().is_none(), "one filled rect");
  drawn.device_rect().expect("an upright frame has a device box")
}

fn near(a: Rect, b: Rect) -> bool {
  [a.min_x() - b.min_x(), a.min_y() - b.min_y(), a.width() - b.width(), a.height() - b.height()]
    .iter()
    .all(|d| d.abs() < EPS)
}

#[test]
fn a_recording_pushed_down_by_a_sibling_replays_and_its_ancestor_re_records() {
  for display_scale in DISPLAY_SCALES {
    let stage = Stage::at_scale(display_scale);
    let mut tree = pushed(true, SIBLING_H, CARD_H);
    assert_eq!(counts(&stage.frame(&mut tree)), (2, 0), "at {display_scale}: first");
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "at {display_scale}: the outer replays whole");
    styled(&mut tree, SIBLING, |s| sized(s, WINDOW_W, GROWN_H));
    let moved = stage.frame(&mut tree);
    // The outer holds the card's old place and re-records; the card,
    // the same box one place down, replays inside it.
    assert_eq!(counts(&moved), (1, 1), "at {display_scale}: moved");
    let damage = tree.frame_damage();
    assert!(covers(damage, rect(0.0, SIBLING_H, CARD_W, CARD_H)), "at {display_scale}: old place {damage:?}");
    assert!(covers(damage, rect(0.0, GROWN_H, CARD_W, CARD_H)), "at {display_scale}: new place {damage:?}");
    stage.still(&mut tree, 1, &format!("at {display_scale}"));
  }
}

// Layout props are not for animation (a transform write is, and keeps the
// cache): a write of a boundary's own inset reports Damage::Layout, which
// clears the node's own cache by design, so the card re-records at every
// such write.
#[test]
fn a_recording_whose_own_top_is_written_re_records() {
  let stage = Stage::at_scale(1.0);
  let mut tree = placed_card(false, View::default().with_layout(), BoundaryMode::Recording);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "first");
  stage.still(&mut tree, 1, "placed");
  for step in 1..=STEPS {
    styled(&mut tree, CARD, |s| s.inset.top = length(TOP + STEP * step as f32));
    assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "step {step}");
  }
}

#[test]
fn the_damage_of_a_moved_recording_holds_its_old_and_new_place() {
  let stage = Stage::at_scale(1.0);
  let mut tree = placed_card(false, View::default().with_layout(), BoundaryMode::Recording);
  // A child moving changes its parent's scrollable overflow, a layout
  // change that damages the parent whole (the root: the window). An anchor
  // past the card's places holds the root's overflow, so the frame's damage
  // is the card's own.
  tree.create_node(ANCHOR, View::default().with_layout());
  tree.insert_node(ROOT, ANCHOR, None).expect("insert");
  styled(&mut tree, ANCHOR, |s| {
    sized(s, ANCHOR_SIDE, ANCHOR_SIDE);
    s.position = Position::Absolute;
    s.inset.left = length(WINDOW_W - ANCHOR_SIDE);
    s.inset.top = length(WINDOW_H - ANCHOR_SIDE);
  });
  stage.frame(&mut tree);
  stage.frame(&mut tree);
  relayout_top(&mut tree, CARD, TOP + JUMP);
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "the card replays");
  let damage = tree.frame_damage();
  assert!(matches!(damage, FrameDamage::Rect(_)), "a partial frame: {damage:?}");
  assert!(covers(damage, rect(LEFT, TOP, CARD_W, CARD_H)), "old place {damage:?}");
  assert!(covers(damage, rect(LEFT, TOP + JUMP, CARD_W, CARD_H)), "new place {damage:?}");
  let anchor = rect(WINDOW_W - ANCHOR_SIDE, WINDOW_H - ANCHOR_SIDE, ANCHOR_SIDE, ANCHOR_SIDE);
  assert!(!covers(damage, anchor), "the anchor is not damaged: {damage:?}");
}

#[test]
fn a_replayed_recording_draws_at_its_new_place() {
  for display_scale in DISPLAY_SCALES {
    let stage = Stage::at_scale(display_scale);
    let mut tree = pushed(false, SIBLING_H, CARD_H);
    let (_, first) = stage.drawn(&mut tree);
    let device = |y: f32| rect(0.0, y * display_scale, CARD_W * display_scale, CARD_H * display_scale);
    assert!(near(filled_rect(&first), device(SIBLING_H)), "at {display_scale}: {:?}", filled_rect(&first));
    styled(&mut tree, SIBLING, |s| sized(s, WINDOW_W, GROWN_H));
    let (moved, list) = stage.drawn(&mut tree);
    assert_eq!(counts(&moved), (0, 1), "at {display_scale}: the card replays");
    assert!(near(filled_rect(&list), device(GROWN_H)), "at {display_scale}: {:?}", filled_rect(&list));
  }
}

#[test]
fn a_resized_recording_re_records() {
  let stage = Stage::at_scale(1.0);
  let mut tree = pushed(false, SIBLING_H, CARD_H);
  stage.frame(&mut tree);
  // The card takes what the sibling leaves (its own style unwritten from
  // here): the sibling's growth moves it and shrinks it.
  styled(&mut tree, CARD, |s| {
    s.size.height = Dimension::auto();
    s.flex_grow = 1.0;
  });
  stage.frame(&mut tree);
  stage.still(&mut tree, 1, "grown");
  styled(&mut tree, SIBLING, |s| sized(s, WINDOW_W, GROWN_H));
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "shrunk");
  stage.still(&mut tree, 1, "shrunk");
}

#[test]
fn a_move_onto_another_snapped_box_re_records_once() {
  for display_scale in DISPLAY_SCALES {
    let logical = |device: f32| device / display_scale;
    let (near, far) = (SNAP_SIBLING_DEVICE + NUDGE_DEVICE, SNAP_SIBLING_DEVICE + SNAP_CARD_H_DEVICE + NUDGE_DEVICE);
    let before = (SNAP_SIBLING_DEVICE + SNAP_CARD_H_DEVICE).round() - SNAP_SIBLING_DEVICE.round();
    assert_ne!(far.round() - near.round(), before, "the nudge takes the snapped box onto another device pixel");
    let stage = Stage::at_scale(display_scale);
    let mut tree = pushed(false, logical(SNAP_SIBLING_DEVICE), logical(SNAP_CARD_H_DEVICE));
    stage.frame(&mut tree);
    stage.still(&mut tree, 1, &format!("at {display_scale}"));
    styled(&mut tree, SIBLING, |s| sized(s, WINDOW_W, logical(SNAP_SIBLING_DEVICE + NUDGE_DEVICE)));
    assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "at {display_scale}: nudged");
    stage.still(&mut tree, 1, &format!("at {display_scale}: nudged"));
    // Nudged back: the box it was first recorded at, re-recorded once more.
    styled(&mut tree, SIBLING, |s| sized(s, WINDOW_W, logical(SNAP_SIBLING_DEVICE)));
    assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "at {display_scale}: back");
    stage.still(&mut tree, 1, &format!("at {display_scale}: back"));
  }
}

#[test]
fn a_fractional_move_that_keeps_the_snapped_box_replays() {
  let display_scale = 1.5;
  let stage = Stage::at_scale(display_scale);
  let mut tree = pushed(false, SIBLING_H, CARD_H);
  stage.frame(&mut tree);
  // A fraction of a device pixel down: the origin snaps back onto the same
  // pixel and the far edge with it.
  styled(&mut tree, SIBLING, |s| sized(s, WINDOW_W, SIBLING_H + NUDGE_DEVICE / display_scale));
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1));
}

#[test]
fn a_moved_plain_node_re_records_its_enclosing_recording() {
  let stage = Stage::at_scale(1.0);
  let mut tree = placed_card(true, View::default().with_layout(), BoundaryMode::None);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "first");
  stage.still(&mut tree, 1, "placed");
  relayout_top(&mut tree, CARD, TOP + JUMP);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "moved");
}

#[test]
fn a_recording_of_another_kind_re_records_when_moved() {
  // A rect boundary records its own box offset, the fraction of a device
  // pixel where it sits, so a move clears its own cache as before.
  let stage = Stage::at_scale(1.0);
  let mut tree = placed_card(false, Rectangle::default().with_layout(), BoundaryMode::Recording);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "first");
  stage.still(&mut tree, 1, "placed");
  relayout_top(&mut tree, CARD, TOP + JUMP);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "moved");
}
