// A frozen snapshot boundary (Element::freeze), the damage half: the damage
// of a `freeze` write, and what a change inside a frozen node does. Its paint
// invalidation walk stops at the frozen node (an ancestor Recording boundary
// replays), and it notes no screen damage and moves no revision: a paint, a
// compose and a scroll write, an insert, a removal and a destroy of a
// detached child, a texture content change. A change that dirties layout (a
// layout write, an insert or a destroy of a laid-out child) still notes no
// damage, but moves the revision, so the next frame runs the layout pass,
// whose paint walk records nothing. The frozen node's own box change and its
// own compose write pass it and damage; its own paint write is the stale
// mark only. A `freeze` on a node that is off or a Recording boundary stops
// nothing, and neither does one the snapshot arm drew inline. Slides,
// reparenting, nested frozen nodes and the writes that pass a frozen node
// from its parent (its own layout and `pixelSnap` writes, an inline atom's
// margin) are here too.
//
// The headless raster here refuses every raster, so no snapshot forms: the
// held picture, its stale mark, the unfreeze that repaints it and the pixels
// are the GL example's (alloy/examples/boundary_freeze.rs). A refused raster
// draws the snapshot inline, which makes the node not frozen
// (Element::snapshot_inline); the stage stands in for a raster that drew a
// texture, unless a case asks for the harness as it is (`Stage::inline`).
use crate::gpu::DirtySet;
use crate::renderer::prelude::*;
use crate::renderer::{Color, DisplayListBuilder, Point, Rect, Size};
use crate::rendertree::composite::{apply_content_changes, paint_phase, PaintStats};
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;
use taffy::style::Position;

const ROOT: u64 = 1;
const OUTER: u64 = 2;
const TILE: u64 = 3;
const INNER: u64 = 4;
const FILL: u64 = 5;
const FILL2: u64 = 6;
const BESIDE: u64 = 7;
const BESIDE_FILL: u64 = 8;
const SIBLING: u64 = 9;
// Nodes a case adds under the tile.
const ADDED: u64 = 10;
const TEX: u64 = 11;
// The texture registry id the texture element shows.
const TEXTURE_ID: u64 = 7;
// A view under the inner one, filling it; a text, its spans and an inline
// atom; a node moved in and out of the tile; a frozen snapshot boundary
// inside the tile and the view between the two.
const KID: u64 = 12;
const TEXT: u64 = 13;
const SPAN_A: u64 = 14;
const ATOM: u64 = 15;
const SPAN_B: u64 = 16;
const MOVER: u64 = 17;
const MID: u64 = 18;
const NESTED: u64 = 19;
// The highest node id a scene here uses: the stand-in visits each.
const LAST_ID: u64 = NESTED;

const WINDOW_W: f32 = 400.0;
const WINDOW_H: f32 = 300.0;
const TILE_W: f32 = 100.0;
const TILE_H: f32 = 80.0;
// The view inside the tile, and the size a layout write gives it.
const INNER_W: f32 = 40.0;
const INNER_H: f32 = 30.0;
const INNER_GROWN: f32 = 60.0;
// A node outside the tile and the outer boundary, far from both.
const BESIDE_AT: f32 = 300.0;
const BESIDE_SIDE: f32 = 40.0;
// How far a written transform moves a node.
const MOVE_BY: f32 = 10.0;
// The size of an inline atom, and the margin a write gives it.
const ATOM_SIDE: f32 = 16.0;
// A layout slide: its length, the clock it starts at, the frames it is
// looked at in, and how far it grows a box.
const SLIDE_MS: f32 = 100.0;
const SLIDE_START: f64 = 1000.0;
const SLIDE_STEPS: u32 = 4;
const GROW_BY: f32 = 60.0;
// Frames painted after a change, every boundary replaying.
const STILL_FRAMES: u32 = 3;

fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
}

struct Stage {
  platform: PlatformContext,
  alloy: crate::Context,
  // Stand in for a raster that drew a texture at every snapshot paint.
  textured: bool,
}

impl Stage {
  fn new() -> Self {
    Self::on(PlatformContext::new(Vec::new()), true)
  }

  // The headless raster as it is: a refused raster draws the snapshot
  // inline.
  fn inline() -> Self {
    Self::on(PlatformContext::new(Vec::new()), false)
  }

  // A platform that shapes text.
  fn with_text() -> Self {
    Self::on(crate::tests::text_platform(), true)
  }

  fn on(platform: PlatformContext, textured: bool) -> Self {
    platform.set_window_size(WINDOW_W, WINDOW_H);
    Self { platform, alloy: headless(), textured }
  }

  fn frame(&self, tree: &mut RenderTree) -> PaintStats {
    apply_content_changes(tree, &self.platform, &self.alloy);
    let stats = paint_phase(&mut DisplayListBuilder::new(None), tree, &self.platform, &self.alloy);
    if self.textured {
      for id in 1..=LAST_ID {
        if let Some(el) = tree.try_node(id) {
          el.snapshot_inline.set(false);
        }
      }
    }
    stats
  }
}

// A frame's (recorded, reused) Recording boundaries.
fn counts(stats: &PaintStats) -> (u32, u32) {
  (stats.boundaries_recorded, stats.boundaries_reused)
}

fn styled(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut Style)) {
  tree.edit(id, |el| {
    f(el.style_mut().expect("laid-out node"));
    Damage::Layout
  });
}

fn sized(style: &mut Style, w: f32, h: f32) {
  style.size = taffy::Size { width: length(w), height: length(h) };
}

fn color(tree: &mut RenderTree, id: u64, rgb: (f32, f32, f32)) {
  tree.edit(id, |el| {
    el.kind.paint_mut().expect("a rect paints").set_color(Some(Color::new_srgba(rgb.0, rgb.1, rgb.2, 1.0)))
  });
}

fn write_view(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut View) -> Damage) {
  tree.edit(id, |el| match &mut el.kind {
    ElementKind::View(v) => f(v),
    _ => panic!("node {id} is not a view"),
  });
}

fn rect(tree: &mut RenderTree, id: u64, parent: u64) {
  tree.create_node(id, Rectangle::default().no_layout());
  tree.insert_node(parent, id, None).expect("insert");
}

// root > [outer, a Recording boundary filling the window >] tile (a snapshot
// boundary in `mode`, with freeze set, `TILE_W` wide, `TILE_H` tall or sized
// by its content) > inner (a view) > fill, fill2 (rects); [sibling, a view
// below the tile;] beside (a view far from both, under the root) > its fill.
struct Scene {
  outer: bool,
  mode: BoundaryMode,
  // The tile takes its height from its content.
  content_sized: bool,
}

impl Default for Scene {
  fn default() -> Self {
    Self { outer: true, mode: BoundaryMode::Snapshot, content_sized: false }
  }
}

fn build(scene: Scene) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.root = Some(ROOT);
  styled(&mut tree, ROOT, |s| sized(s, WINDOW_W, WINDOW_H));
  let parent = if scene.outer {
    tree.create_node(OUTER, View::default().with_layout());
    tree.insert_node(ROOT, OUTER, None).expect("insert");
    styled(&mut tree, OUTER, |s| sized(s, WINDOW_W, WINDOW_H));
    tree.edit(OUTER, |el| el.set_repaint_boundary(BoundaryMode::Recording));
    OUTER
  } else {
    ROOT
  };
  tree.create_node(TILE, View::default().with_layout());
  tree.insert_node(parent, TILE, None).expect("insert");
  let content_sized = scene.content_sized;
  styled(&mut tree, TILE, |s| {
    s.size.width = length(TILE_W);
    if !content_sized {
      s.size.height = length(TILE_H);
    }
  });
  tree.create_node(INNER, View::default().with_layout());
  tree.insert_node(TILE, INNER, None).expect("insert");
  styled(&mut tree, INNER, |s| sized(s, INNER_W, INNER_H));
  rect(&mut tree, FILL, INNER);
  color(&mut tree, FILL, (1.0, 0.0, 0.0));
  rect(&mut tree, FILL2, INNER);
  color(&mut tree, FILL2, (0.0, 1.0, 0.0));
  if content_sized {
    tree.create_node(SIBLING, View::default().with_layout());
    tree.insert_node(parent, SIBLING, None).expect("insert");
    styled(&mut tree, SIBLING, |s| sized(s, TILE_W, TILE_H));
  }
  tree.create_node(BESIDE, View::default().with_layout());
  tree.insert_node(ROOT, BESIDE, None).expect("insert");
  styled(&mut tree, BESIDE, |s| {
    s.position = Position::Absolute;
    s.inset.left = length(BESIDE_AT);
    s.inset.top = length(BESIDE_AT - TILE_W);
    sized(s, BESIDE_SIDE, BESIDE_SIDE);
  });
  rect(&mut tree, BESIDE_FILL, BESIDE);
  color(&mut tree, BESIDE_FILL, (0.0, 0.0, 1.0));
  tree.edit(TILE, |el| el.set_repaint_boundary(scene.mode));
  tree.edit(TILE, |el| el.set_freeze(true));
  tree
}

// The first frame and the still frames after it: the outer records once,
// then replays.
fn settled(stage: &Stage, tree: &mut RenderTree) {
  assert_eq!(counts(&stage.frame(tree)), (1, 0), "first");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(tree)), (0, 1), "still {look}");
  }
}

// A change under the frozen tile: `change` runs, and the revision moves
// exactly when `layout` says (the change dirtied layout). The frame after it
// damages nothing and records nothing: the outer replays.
fn unseen(stage: &Stage, tree: &mut RenderTree, what: &str, layout: bool, change: impl FnOnce(&mut RenderTree)) {
  let revision = tree.revision();
  change(tree);
  assert_eq!(tree.revision() != revision, layout, "{what}: the revision moves only for layout");
  assert_eq!(counts(&stage.frame(tree)), (0, 1), "{what}: the outer replays");
  assert_eq!(tree.frame_damage(), FrameDamage::None, "{what}: no damage");
}

#[test]
fn a_freeze_write_reports_its_damage_in_each_state() {
  use BoundaryMode::*;
  // Off and on a Recording boundary the field is inert, but a changing write
  // is Compose, so the node is painted and the warning can fire there. On a
  // snapshot boundary a freeze is Compose (the paint that takes the picture),
  // and so is an unfreeze where no stale snapshot is held (here none: the
  // stale one is the GL example's, Paint). The value in force is none.
  for mode in [None, Recording, Snapshot] {
    let mut el = View::default().with_layout();
    el.set_repaint_boundary(mode);
    assert!(!el.freeze, "live by default");
    assert_eq!(el.set_freeze(true), Damage::Compose, "a freeze");
    assert!(el.freeze, "the value is written");
    assert_eq!(el.frozen(), mode == Snapshot, "frozen only on a snapshot boundary");
    assert_eq!(el.set_freeze(true), Damage::None, "the value in force");
    assert_eq!(el.set_freeze(false), Damage::Compose, "an unfreeze holding no stale snapshot");
    assert_eq!(el.set_freeze(false), Damage::None, "the value in force again");
    assert!(el.repaint_boundary == mode, "the mode is not written");
  }
}

#[test]
fn a_write_inside_a_frozen_node_stops_at_it_and_shows_nothing() {
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  unseen(&stage, &mut tree, "a paint write", false, |tree| color(tree, FILL, (1.0, 1.0, 0.0)));
  unseen(&stage, &mut tree, "a compose write", false, |tree| write_view(tree, INNER, |v| v.set_x(Some(MOVE_BY))));
  unseen(&stage, &mut tree, "a scroll write", false, |tree| write_view(tree, INNER, |v| v.set_scroll_y(Some(MOVE_BY))));
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "after the writes, still {look}");
  }
}

#[test]
fn a_layout_write_inside_moves_the_revision_and_lays_out_unseen() {
  // The layout write dirties layout: the revision moves, so a frame driver
  // rebuilds, and the layout pass of that frame lays the inner view out at
  // its new box. Its box change stops at the frozen tile: no damage, and
  // the paint walk records nothing.
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  unseen(&stage, &mut tree, "a layout write and its relayout", true, |tree| {
    styled(tree, INNER, |s| sized(s, INNER_GROWN, INNER_GROWN))
  });
  let laid_out = tree.layout_box(INNER).expect("inner is laid out").size;
  assert_eq!(laid_out, Size::new(INNER_GROWN, INNER_GROWN), "the layout pass ran inside the frozen tile");
  assert_eq!(tree.layout_box(TILE).expect("the tile is laid out").size, Size::new(TILE_W, TILE_H), "same box");
}

#[test]
fn structure_changes_under_a_frozen_node_show_nothing() {
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  // A detached child: no layout, no revision.
  tree.create_node(ADDED, Rectangle::default().no_layout());
  unseen(&stage, &mut tree, "an insert of a detached child", false, |tree| {
    tree.insert_node(TILE, ADDED, None).expect("insert")
  });
  unseen(&stage, &mut tree, "a detach of a detached child", false, |tree| tree.detach_node(TILE, ADDED));
  unseen(&stage, &mut tree, "the insert again", false, |tree| tree.insert_node(TILE, ADDED, None).expect("insert"));
  unseen(&stage, &mut tree, "a destroy of a linked detached child", false, |tree| tree.destroy_node(ADDED));
  assert!(tree.try_node(ADDED).is_none(), "destroyed");
  // A laid-out child dirties the tile's layout: the revision moves.
  tree.create_node(ADDED, View::default().with_layout());
  unseen(&stage, &mut tree, "an insert of a laid-out child", true, |tree| {
    tree.insert_node(TILE, ADDED, None).expect("insert")
  });
  assert!(tree.layout_box(ADDED).is_some(), "the inserted child is laid out");
  unseen(&stage, &mut tree, "a destroy of a laid-out child", true, |tree| tree.destroy_node(ADDED));
  assert!(tree.try_node(ADDED).is_none(), "destroyed");
}

#[test]
fn a_texture_change_inside_a_frozen_node_shows_nothing() {
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  tree.create_node(TEX, Texture::default().with_layout());
  tree.insert_node(TILE, TEX, None).expect("insert");
  tree.edit(TEX, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(TEXTURE_ID)),
    _ => unreachable!(),
  });
  settled(&stage, &mut tree);
  for k in 1..=STILL_FRAMES {
    let mut changes = DirtySet::new();
    changes.extend([TEXTURE_ID]);
    let revision = tree.revision();
    assert!(!tree.texture_content_changed(&changes), "content change {k}: nothing invalidated");
    assert_eq!(tree.revision(), revision, "content change {k}: no revision");
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "content change {k}: the outer replays");
    assert_eq!(tree.frame_damage(), FrameDamage::None, "content change {k}: no damage");
  }
}

#[test]
fn the_frozen_nodes_own_box_change_clears_the_ancestors() {
  // Its own box written: the layout write's walk stops at the tile (the
  // revision moves for layout), and the layout pass's clear of the new box
  // passes it: the outer records again and the tile is damaged.
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  let revision = tree.revision();
  styled(&mut tree, TILE, |s| s.size.width = length(TILE_W + MOVE_BY));
  assert_ne!(tree.revision(), revision, "the layout write moves the revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "a resize: the outer records again");
  assert!(tree.frame_damage() != FrameDamage::None, "a resize damages the tile");
  // A move by a layout write (only placed): the clear starts at the parent.
  styled(&mut tree, TILE, |s| s.margin.left = length(MOVE_BY));
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "a move: the outer records again");
  assert!(tree.frame_damage() != FrameDamage::None, "a move damages the tile");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "still {look}");
  }
}

#[test]
fn a_frozen_node_sized_by_its_content_grows_and_moves_its_sibling() {
  // A layout write inside grows the tile, whose height is its content's:
  // the inner view's own box change stops at the tile, the tile's passes
  // it, and the sibling below moves, in the same frame.
  let stage = Stage::new();
  let mut tree = build(Scene { content_sized: true, ..Scene::default() });
  settled(&stage, &mut tree);
  assert_eq!(tree.layout_box(SIBLING).expect("the sibling").origin.y, INNER_H, "below the tile");
  let revision = tree.revision();
  styled(&mut tree, INNER, |s| sized(s, INNER_W, INNER_GROWN));
  assert_ne!(tree.revision(), revision, "the layout write moves the revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "the outer records again");
  assert_eq!(tree.layout_box(TILE).expect("the tile").size.height, INNER_GROWN, "the tile grew");
  assert_eq!(tree.layout_box(SIBLING).expect("the sibling").origin.y, INNER_GROWN, "the sibling moved");
  // The outer, the window's size, is noted too (its own content's
  // overflow changed), so the damage may be the whole window.
  match tree.frame_damage() {
    FrameDamage::Rect(r) => {
      assert!(r.max_y() >= INNER_GROWN + TILE_H, "the damage holds the sibling's new place: {r:?}")
    }
    FrameDamage::Full => {}
    FrameDamage::None => panic!("the growth damages nothing"),
  }
}

#[test]
fn the_frozen_nodes_own_writes() {
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  // A compose write moves the quad: the walk starts at the parent.
  let revision = tree.revision();
  write_view(&mut tree, TILE, |v| v.set_x(Some(MOVE_BY)));
  assert_ne!(tree.revision(), revision, "a compose write on it moves the revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "a compose write: the outer records again");
  assert!(tree.frame_damage() != FrameDamage::None, "a compose write damages the tile");
  // A paint write on it (`antialias`) and a scroll write are the stale mark
  // only.
  unseen(&stage, &mut tree, "its own paint write", false, |tree| tree.edit(TILE, |el| el.set_antialias(false)));
  unseen(&stage, &mut tree, "its own scroll write", false, |tree| {
    write_view(tree, TILE, |v| v.set_scroll_y(Some(MOVE_BY)))
  });
  // Unfrozen, with no stale snapshot held: Compose, the outer records again.
  tree.edit(TILE, |el| el.set_freeze(false));
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "the unfreeze: the outer records again");
  assert!(tree.frame_damage() != FrameDamage::None, "the unfreeze damages the tile");
  // Live again: a write inside reaches the outer.
  color(&mut tree, FILL, (0.0, 1.0, 1.0));
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "live: a write inside records the outer");
}

#[test]
fn a_freeze_off_a_snapshot_stops_nothing() {
  for (mode, recorded) in [(BoundaryMode::None, 1), (BoundaryMode::Recording, 2)] {
    let stage = Stage::new();
    let mut tree = build(Scene { mode, ..Scene::default() });
    assert_eq!(counts(&stage.frame(&mut tree)), (recorded, 0), "first");
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "still");
    let revision = tree.revision();
    color(&mut tree, FILL, (1.0, 1.0, 0.0));
    assert_ne!(tree.revision(), revision, "a write inside moves the revision");
    assert_eq!(counts(&stage.frame(&mut tree)), (recorded, 0), "a write inside records again");
    assert!(tree.frame_damage() != FrameDamage::None, "a write inside damages");
  }
}

#[test]
fn a_batch_takes_each_walks_answer() {
  // Two writes under the frozen tile and one outside it, in one batch, in
  // either order: only the one outside is damaged, and the revision moves
  // once. The second write inside ends its walk at the inner view the first
  // one cleared, and takes its answer from there: had it not, its damage
  // would reach into the tile.
  let tile = Rect::new(Point::zero(), Size::new(TILE_W, TILE_H));
  let orders: [[u64; 3]; 2] = [[FILL, FILL2, BESIDE_FILL], [BESIDE_FILL, FILL2, FILL]];
  for order in orders {
    let stage = Stage::new();
    let mut tree = build(Scene::default());
    settled(&stage, &mut tree);
    let paint = Color::new_srgba(1.0, 0.0, 1.0, 1.0);
    for id in order {
      if let Some(p) = tree.node_mut(id).kind.paint_mut() {
        p.set_color(Some(paint));
      }
    }
    let items: Vec<(u64, Damage)> = order.iter().map(|&id| (id, Damage::Paint)).collect();
    let revision = tree.revision();
    tree.apply_damage_batch(&items);
    assert_eq!(tree.revision(), revision.wrapping_add(1), "{order:?}: one revision for the batch");
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "{order:?}: the outer replays");
    match tree.frame_damage() {
      FrameDamage::Rect(r) => assert!(!r.intersects(&tile), "{order:?}: the damage {r:?} reaches into the tile"),
      damage => panic!("{order:?}: the frame damage is {damage:?}, expected the outside node's rect"),
    }
  }
  // A batch of writes inside alone moves nothing.
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  let revision = tree.revision();
  tree.apply_damage_batch(&[(FILL, Damage::Paint), (INNER, Damage::Compose), (FILL2, Damage::Paint)]);
  assert_eq!(tree.revision(), revision, "a batch inside: no revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "a batch inside: the outer replays");
  assert_eq!(tree.frame_damage(), FrameDamage::None, "a batch inside: no damage");
  // With layout damage inside, it moves the revision and still damages
  // nothing.
  tree.apply_damage_batch(&[(FILL, Damage::Paint), (INNER, Damage::Layout)]);
  assert_eq!(tree.revision(), revision.wrapping_add(1), "a layout batch inside: one revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "a layout batch inside: the outer replays");
  assert_eq!(tree.frame_damage(), FrameDamage::None, "a layout batch inside: no damage");
}

fn declare_layout(tree: &mut RenderTree, id: u64) {
  let spec = TransitionSpec::Tween { duration_ms: SLIDE_MS, curve: Curve::Linear };
  tree.edit(id, |el| {
    el.transitions = Some(Box::new(TransitionConfig {
      layout: Some(TransitionEntry { spec, delay_ms: 0.0, from: None, exit: None }),
      ..Default::default()
    }));
    Damage::None
  });
}

fn painted(tree: &RenderTree, id: u64) -> Size {
  tree.node(id).painted_size().expect("laid out")
}

// The slide's clock at look `step` of SLIDE_STEPS.
fn slide_at(step: u32) -> f64 {
  SLIDE_START + f64::from(SLIDE_MS) * f64::from(step) / f64::from(SLIDE_STEPS)
}

#[test]
fn a_size_slide_on_the_frozen_node_stretches_its_quad_every_frame() {
  // The slide's size change is the quad stretching: the snapshot is stale
  // and the clear passes the tile from its parent, at every frame of it.
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  declare_layout(&mut tree, TILE);
  tree.set_transition_now(0.0);
  settled(&stage, &mut tree);
  styled(&mut tree, TILE, |s| s.size.width = length(TILE_W + GROW_BY));
  tree.set_transition_now(SLIDE_START);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "the reflow: the outer records again");
  assert_eq!(painted(&tree, TILE).width, TILE_W, "the slide starts from the painted box");
  let mut last = TILE_W;
  for step in 1..SLIDE_STEPS {
    tree.set_transition_now(slide_at(step));
    assert!(tree.advance_transitions(), "step {step}: the slide runs");
    let width = painted(&tree, TILE).width;
    assert!(width > last && width < TILE_W + GROW_BY, "step {step}: the quad follows the slide: {width}");
    last = width;
    assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "step {step}: the outer records again");
    assert!(tree.frame_damage() != FrameDamage::None, "step {step}: the tile is damaged");
  }
  tree.set_transition_now(slide_at(SLIDE_STEPS));
  assert!(!tree.advance_transitions(), "settled");
  assert_eq!(painted(&tree, TILE).width, TILE_W + GROW_BY, "on the solved box");
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "the settle: the outer records again");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "still {look}");
  }
}

#[test]
fn a_size_slide_inside_a_frozen_node_lays_out_unseen() {
  // The slide's walk stops at the tile, but its size change dirties layout:
  // the revision moves at every frame of it, and the layout pass lays the
  // children out against the slide's box. Nothing is recorded or damaged.
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  tree.create_node(KID, View::default().with_layout());
  tree.insert_node(INNER, KID, None).expect("insert");
  styled(&mut tree, KID, |s| s.size = taffy::Size { width: percent(1.0), height: percent(1.0) });
  declare_layout(&mut tree, INNER);
  tree.set_transition_now(0.0);
  settled(&stage, &mut tree);
  unseen(&stage, &mut tree, "the size write", true, |tree| {
    styled(tree, INNER, |s| sized(s, INNER_W, INNER_GROWN));
    tree.set_transition_now(SLIDE_START);
  });
  assert_eq!(painted(&tree, INNER).height, INNER_H, "the slide starts from the painted box");
  for step in 1..=SLIDE_STEPS {
    unseen(&stage, &mut tree, &format!("slide step {step}"), true, |tree| {
      tree.set_transition_now(slide_at(step));
      tree.advance_transitions();
    });
    let slid = painted(&tree, INNER).height;
    let kid = tree.layout_box(KID).expect("the kid is laid out").size.height;
    assert_eq!(kid, slid, "step {step}: the kid is laid out against the slide's box");
  }
  assert_eq!(painted(&tree, INNER).height, INNER_GROWN, "settled on the solved box");
}

#[test]
fn a_frozen_node_drawn_inline_is_not_frozen() {
  // The harness as it is: the refused raster draws the tile inline, and so
  // does a box with no texels. Its content is then in the outer's recording,
  // so a write inside reaches the outer and is damaged; it holds nothing.
  for zero_box in [false, true] {
    let what = if zero_box { "a box with no texels" } else { "a refused raster" };
    let stage = Stage::inline();
    let mut tree = build(Scene::default());
    if zero_box {
      styled(&mut tree, TILE, |s| sized(s, 0.0, 0.0));
    }
    assert!(tree.node(TILE).frozen(), "{what}: frozen before its first paint");
    settled(&stage, &mut tree);
    assert!(!tree.node(TILE).frozen(), "{what}: drawn inline, not frozen");
    assert!(tree.node(TILE).paint_cache.borrow().is_none(), "{what}: nothing held");
    let revision = tree.revision();
    color(&mut tree, FILL, (1.0, 1.0, 0.0));
    assert_ne!(tree.revision(), revision, "{what}: a write inside moves the revision");
    assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "{what}: a write inside records the outer");
    assert!(tree.frame_damage() != FrameDamage::None, "{what}: a write inside is damaged");
  }
}

#[test]
fn a_frozen_inline_atoms_own_margin_write_reflows_its_text() {
  // A frozen view inside a text: its own layout write moves the words
  // around it, which the text and its enclosing recording hold. The walk
  // passes it from the text.
  let stage = Stage::with_text();
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.root = Some(ROOT);
  styled(&mut tree, ROOT, |s| sized(s, WINDOW_W, WINDOW_H));
  tree.create_node(OUTER, View::default().with_layout());
  tree.insert_node(ROOT, OUTER, None).expect("insert");
  styled(&mut tree, OUTER, |s| sized(s, WINDOW_W, WINDOW_H));
  tree.edit(OUTER, |el| el.set_repaint_boundary(BoundaryMode::Recording));
  tree.create_node(TEXT, Text::default().with_layout());
  tree.insert_node(OUTER, TEXT, None).expect("insert");
  tree.create_node(SPAN_A, Span { text: "Rated ".into(), ..Default::default() }.no_layout());
  tree.create_node(ATOM, View::default().with_layout());
  tree.create_node(SPAN_B, Span { text: " stars".into(), ..Default::default() }.no_layout());
  for id in [SPAN_A, ATOM, SPAN_B] {
    tree.insert_node(TEXT, id, None).expect("insert");
  }
  styled(&mut tree, ATOM, |s| sized(s, ATOM_SIDE, ATOM_SIDE));
  tree.edit(ATOM, |el| el.set_repaint_boundary(BoundaryMode::Snapshot));
  tree.edit(ATOM, |el| el.set_freeze(true));
  settled(&stage, &mut tree);
  assert!(tree.node(ATOM).frozen(), "the atom is frozen");
  let revision = tree.revision();
  styled(&mut tree, ATOM, |s| s.margin.right = length(ATOM_SIDE));
  assert_ne!(tree.revision(), revision, "a layout write moves the revision");
  // The write's own walk, ahead of any layout pass (the text's measured
  // content changes too, and the pass would clear from the text itself).
  assert!(tree.node(OUTER).paint_cache.borrow().is_none(), "the write's walk drops the outer's recording");
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "the text's enclosing recording records again");
  assert!(tree.frame_damage() != FrameDamage::None, "the atom's write is damaged");
}

#[test]
fn pixel_snap_on_a_frozen_node_is_compose() {
  // The setting decides how the quad rounds in its parent's slot: Compose,
  // whose walk passes the node. Paint elsewhere, none for the value in
  // force.
  let mut el = View::default().with_layout();
  assert_eq!(el.set_pixel_snap_off(true), Damage::Paint, "a view that is no boundary");
  assert_eq!(el.set_pixel_snap_off(true), Damage::None, "the value in force");
  el.set_repaint_boundary(BoundaryMode::Snapshot);
  el.set_freeze(true);
  assert!(el.frozen());
  assert_eq!(el.set_pixel_snap_off(false), Damage::Compose, "a frozen node");
  el.set_freeze(false);
  assert_eq!(el.set_pixel_snap_off(true), Damage::Paint, "a snapshot boundary, live");

  let stage = Stage::new();
  let mut tree = build(Scene::default());
  settled(&stage, &mut tree);
  let revision = tree.revision();
  tree.edit(TILE, |el| el.set_pixel_snap_off(true));
  assert_ne!(tree.revision(), revision, "the write moves the revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "the outer records again");
  assert!(tree.frame_damage() != FrameDamage::None, "the tile is damaged");
}

#[test]
fn a_span_destroyed_under_a_frozen_node_moves_the_revision() {
  // Its text feeds the paragraph's measure, as at a detach: layout, so the
  // revision moves; the walk stops at the tile, so nothing is damaged.
  let stage = Stage::with_text();
  let mut tree = build(Scene::default());
  tree.create_node(TEXT, Text::default().with_layout());
  tree.insert_node(TILE, TEXT, None).expect("insert");
  tree.create_node(SPAN_A, Span { text: "Rated".into(), ..Default::default() }.no_layout());
  tree.insert_node(TEXT, SPAN_A, None).expect("insert");
  settled(&stage, &mut tree);
  unseen(&stage, &mut tree, "a span destroyed in place", true, |tree| tree.destroy_node(SPAN_A));
  assert!(tree.try_node(SPAN_A).is_none(), "destroyed");
}

#[test]
fn a_write_between_two_frozen_nodes_stops_at_the_nearer() {
  // tile (frozen) > mid > nested (frozen): a write on mid stops at the tile,
  // and the outer replays. The nested node is below the write: no walk
  // reaches it (its validity is the GL example's).
  let stage = Stage::new();
  let mut tree = build(Scene::default());
  tree.create_node(MID, View::default().with_layout());
  tree.insert_node(TILE, MID, None).expect("insert");
  styled(&mut tree, MID, |s| sized(s, INNER_W, INNER_H));
  tree.create_node(NESTED, View::default().with_layout());
  tree.insert_node(MID, NESTED, None).expect("insert");
  styled(&mut tree, NESTED, |s| sized(s, INNER_W, INNER_H));
  tree.edit(NESTED, |el| el.set_repaint_boundary(BoundaryMode::Snapshot));
  tree.edit(NESTED, |el| el.set_freeze(true));
  rect(&mut tree, MOVER, NESTED);
  settled(&stage, &mut tree);
  unseen(&stage, &mut tree, "a write on the view between", false, |tree| {
    write_view(tree, MID, |v| v.set_x(Some(MOVE_BY)))
  });
  unseen(&stage, &mut tree, "a write inside the nested one", false, |tree| color(tree, MOVER, (1.0, 1.0, 0.0)));
}

#[test]
fn a_node_moved_out_of_or_into_a_frozen_node_damages_only_outside() {
  // Out: the old place is inside the held picture, the new one is damaged
  // and painted there. In: the old place is damaged and repainted, nothing
  // inside. The tile stays out of the damage either way, and the outer
  // replays.
  let tile = Rect::new(Point::zero(), Size::new(TILE_W, TILE_H));
  let beside = Rect::new(Point::new(BESIDE_AT, BESIDE_AT - TILE_W), Size::new(BESIDE_SIDE, BESIDE_SIDE));
  for into in [false, true] {
    let what = if into { "into the tile" } else { "out of the tile" };
    let stage = Stage::new();
    let mut tree = build(Scene::default());
    let (from, to) = if into { (BESIDE, INNER) } else { (INNER, BESIDE) };
    rect(&mut tree, MOVER, from);
    settled(&stage, &mut tree);
    tree.insert_node(to, MOVER, None).expect("the move");
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "{what}: the outer replays");
    match tree.frame_damage() {
      FrameDamage::Rect(r) => {
        assert!(r.intersects(&beside), "{what}: the place outside is damaged: {r:?}");
        assert!(!r.intersects(&tile), "{what}: the damage {r:?} reaches into the tile");
      }
      damage => panic!("{what}: the frame damage is {damage:?}, expected a rect"),
    }
    if !into {
      let shown = tree.node(MOVER).last_extent.get();
      let there = matches!(shown, cull::Extent::Bounded(r) if r.intersects(&beside));
      assert!(there, "{what}: painted at its new place: {shown:?}");
    }
  }
}
