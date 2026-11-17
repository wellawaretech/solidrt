// A repaint boundary switched between modes (Element::set_repaint_boundary):
// the damage of the write (Compose, so the node's own snapshot survives, and
// none for the mode in force), the damage of an `antialias` write (Paint on a
// snapshot, none where it is inert), the ancestors that hold its old composited
// result and repaint, the caches a write drops (leaving Recording drops the
// recording, a write to Recording drops a snapshot), the declared shader a
// switch to a snapshot marks dirty, and the release registry of the nodes
// switched off with a snapshot. The headless raster here refuses every
// raster, so no snapshot cache forms: the kept snapshot, its reuse at the
// switch back, its stale mark and its release are the GL example's
// (alloy/examples/boundary_switch.rs).
use crate::gpu::{DirtySet, NodeShader};
use crate::renderer::prelude::*;
use crate::renderer::{Color, DisplayListBuilder};
use crate::rendertree::composite::{apply_content_changes, paint_phase, PaintStats};
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;

const ROOT: u64 = 1;
const OUTER: u64 = 2;
const TILE: u64 = 3;
const FILL: u64 = 4;
const TEX: u64 = 5;
// The texture registry id the texture element shows.
const TEXTURE_ID: u64 = 7;
// A shader program id; the lib tests never run its pass.
const PROGRAM: u64 = 1;

const WINDOW_W: f32 = 400.0;
const WINDOW_H: f32 = 300.0;
const TILE_W: f32 = 100.0;
const TILE_H: f32 = 80.0;
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
}

impl Stage {
  fn new() -> Self {
    let platform = PlatformContext::new(Vec::new());
    platform.set_window_size(WINDOW_W, WINDOW_H);
    Self { platform, alloy: headless() }
  }

  fn frame(&self, tree: &mut RenderTree) -> PaintStats {
    apply_content_changes(tree, &self.platform, &self.alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &self.platform, &self.alloy)
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

// The mode write as the props arm performs it.
fn switch(tree: &mut RenderTree, id: u64, mode: BoundaryMode) {
  tree.edit(id, |el| el.set_repaint_boundary(mode));
}

// root > [outer, a Recording boundary >] tile (`mode`) > fill, a rect.
fn scene(outer: bool, mode: BoundaryMode) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.root = Some(ROOT);
  styled(&mut tree, ROOT, |s| sized(s, WINDOW_W, WINDOW_H));
  let parent = if outer {
    tree.create_node(OUTER, View::default().with_layout());
    tree.insert_node(ROOT, OUTER, None).expect("insert");
    styled(&mut tree, OUTER, |s| sized(s, WINDOW_W, WINDOW_H));
    switch(&mut tree, OUTER, BoundaryMode::Recording);
    OUTER
  } else {
    ROOT
  };
  tree.create_node(TILE, View::default().with_layout());
  tree.insert_node(parent, TILE, None).expect("insert");
  styled(&mut tree, TILE, |s| sized(s, TILE_W, TILE_H));
  tree.create_node(FILL, Rectangle::default().no_layout());
  tree.insert_node(TILE, FILL, None).expect("insert");
  tree
    .edit(FILL, |el| el.kind.paint_mut().expect("a rect paints").set_color(Some(Color::new_srgba(1.0, 0.0, 0.0, 1.0))));
  switch(&mut tree, TILE, mode);
  tree
}

fn shader() -> NodeShader {
  NodeShader { program: PROGRAM, params: vec![], textures: vec![], outset: 0.0, previous: false }
}

#[test]
fn a_mode_write_is_compose_and_the_mode_in_force_is_no_change() {
  use BoundaryMode::*;
  let mut el = View::default().with_layout();
  let writes = [
    (Snapshot, Damage::Compose),
    (Snapshot, Damage::None),
    (None, Damage::Compose),
    (None, Damage::None),
    (Recording, Damage::Compose),
    (Recording, Damage::None),
    (Snapshot, Damage::Compose),
    (Recording, Damage::Compose),
    (None, Damage::Compose),
  ];
  for (step, (mode, damage)) in writes.into_iter().enumerate() {
    assert_eq!(el.set_repaint_boundary(mode), damage, "write {step}");
    assert!(el.repaint_boundary == mode, "write {step}: the mode is written");
  }
}

#[test]
fn an_antialias_write_is_paint_on_a_snapshot_and_inert_elsewhere() {
  // Element::set_antialias: on a snapshot boundary the storage stays and
  // the raster changes, Paint, whose walk marks the node's own snapshot
  // stale for an in-place re-render. Off holding no snapshot, or a
  // recording, the field is inert: no damage. The value in force is none
  // anywhere. Off holding a snapshot is Paint too, which needs a raster
  // (the GL example's).
  let mut el = View::default().with_layout();
  assert!(el.antialias, "antialiased by default");
  assert_eq!(el.set_antialias(false), Damage::None, "off, nothing kept");
  assert!(!el.antialias, "the value is written");
  assert_eq!(el.set_antialias(false), Damage::None, "off, the value in force");
  el.set_repaint_boundary(BoundaryMode::Recording);
  assert_eq!(el.set_antialias(true), Damage::None, "a recording");
  el.set_repaint_boundary(BoundaryMode::Snapshot);
  assert_eq!(el.set_antialias(true), Damage::None, "a snapshot, the value in force");
  assert_eq!(el.set_antialias(false), Damage::Paint, "a snapshot, without");
  assert_eq!(el.set_antialias(true), Damage::Paint, "a snapshot, with again");
  assert!(el.repaint_boundary == BoundaryMode::Snapshot, "the mode is not written");
}

#[test]
fn an_antialias_write_repaints_only_under_a_snapshot() {
  let stage = Stage::new();
  let mut tree = scene(true, BoundaryMode::Snapshot);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "first");
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "still");
  // A snapshot boundary: the outer holds the tile's old composited result
  // and records again, and the tile is noted for the screen damage.
  tree.edit(TILE, |el| el.set_antialias(false));
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "a snapshot's antialias write");
  assert!(tree.frame_damage() != FrameDamage::None, "the tile is noted for the screen damage");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "still {look}");
  }
  // Off, and the refused raster formed no snapshot to keep: no damage, no
  // revision, the outer replays.
  switch(&mut tree, TILE, BoundaryMode::None);
  stage.frame(&mut tree);
  assert!(!tree.node(TILE).holds_off_snapshot(), "nothing kept");
  let revision = tree.revision();
  tree.edit(TILE, |el| el.set_antialias(true));
  assert_eq!(tree.revision(), revision, "an inert write, no revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "an off node's antialias write");
  assert_eq!(tree.frame_damage(), FrameDamage::None, "an inert write damages nothing");
}

#[test]
fn a_mode_write_repaints_the_ancestors_and_damages_the_node() {
  let stage = Stage::new();
  let mut tree = scene(true, BoundaryMode::Snapshot);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "first");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "still {look}");
  }
  // Off: the outer holds the tile's old composited result and records again.
  switch(&mut tree, TILE, BoundaryMode::None);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "off");
  assert!(tree.frame_damage() != FrameDamage::None, "the tile is noted for the screen damage");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "off, still {look}");
  }
  // The mode in force is no write: no revision, the outer replays.
  let revision = tree.revision();
  switch(&mut tree, TILE, BoundaryMode::None);
  assert_eq!(tree.revision(), revision, "no change, no revision");
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "the same mode again");
  assert_eq!(tree.frame_damage(), FrameDamage::None, "the same mode again damages nothing");
  // Back: the outer records again once.
  switch(&mut tree, TILE, BoundaryMode::Snapshot);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "back");
  for look in 1..=STILL_FRAMES {
    assert_eq!(counts(&stage.frame(&mut tree)), (0, 1), "back, still {look}");
  }
}

#[test]
fn leaving_recording_drops_the_recording_and_joins_no_registry() {
  let stage = Stage::new();
  let mut tree = scene(false, BoundaryMode::Recording);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "first");
  let recorded = |tree: &RenderTree| matches!(&*tree.node(TILE).paint_cache.borrow(), Some(PaintCache::Recording(_)));
  assert!(recorded(&tree), "the tile holds its recording");
  switch(&mut tree, TILE, BoundaryMode::None);
  assert!(tree.node(TILE).paint_cache.borrow().is_none(), "off drops the recording");
  assert!(tree.off_snapshot_nodes().is_empty(), "a recording is no snapshot kept");
  assert_eq!(counts(&stage.frame(&mut tree)), (0, 0), "off paints inline");

  switch(&mut tree, TILE, BoundaryMode::Recording);
  assert_eq!(counts(&stage.frame(&mut tree)), (1, 0), "recording again");
  assert!(recorded(&tree), "recorded again");
  switch(&mut tree, TILE, BoundaryMode::Snapshot);
  assert!(tree.node(TILE).paint_cache.borrow().is_none(), "a snapshot drops the recording");
  // The refused raster paints inline and forms no snapshot: switched off,
  // the tile holds nothing to keep.
  stage.frame(&mut tree);
  switch(&mut tree, TILE, BoundaryMode::None);
  assert!(tree.off_snapshot_nodes().is_empty(), "nothing kept, nothing registered");
  for _ in 0..STILL_FRAMES {
    stage.frame(&mut tree);
  }
  assert!(tree.off_snapshot_nodes().is_empty(), "the sweeps register nothing");
}

#[test]
fn a_switch_to_a_snapshot_marks_a_declared_shader_dirty() {
  let dirty = |el: &Element| match &el.kind {
    ElementKind::View(v) => v.take_shader_dirty(),
    _ => panic!("a view"),
  };
  let mut el = View::default().with_layout();
  // No shader: nothing to mark.
  el.set_repaint_boundary(BoundaryMode::Snapshot);
  assert!(!dirty(&el), "no shader declared");
  el.set_repaint_boundary(BoundaryMode::None);
  if let ElementKind::View(v) = &mut el.kind {
    v.set_shader(Some(shader()));
  }
  assert!(dirty(&el), "the declaration write itself");
  // A switch off marks nothing; one to a snapshot, from off or from a
  // recording, marks the shader once.
  el.set_repaint_boundary(BoundaryMode::Recording);
  assert!(!dirty(&el), "to Recording");
  el.set_repaint_boundary(BoundaryMode::Snapshot);
  assert!(dirty(&el), "Recording to snapshot");
  el.set_repaint_boundary(BoundaryMode::None);
  assert!(!dirty(&el), "snapshot to off");
  el.set_repaint_boundary(BoundaryMode::Snapshot);
  assert!(dirty(&el), "off to snapshot");
  el.set_repaint_boundary(BoundaryMode::Snapshot);
  assert!(!dirty(&el), "the mode in force marks nothing");
}

#[test]
fn a_texture_change_inside_an_off_node_without_a_snapshot_bakes_nothing() {
  // Only a VALID kept snapshot counts as baked (RenderTree::content_reach):
  // an off node that never rastered has none, and a plain displayed
  // texture keeps the present-only reuse path.
  let stage = Stage::new();
  let mut tree = scene(false, BoundaryMode::Snapshot);
  tree.create_node(TEX, Texture::default().with_layout());
  tree.insert_node(TILE, TEX, None).expect("insert");
  tree.edit(TEX, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(TEXTURE_ID)),
    _ => unreachable!(),
  });
  stage.frame(&mut tree);
  switch(&mut tree, TILE, BoundaryMode::None);
  stage.frame(&mut tree);
  let mut changes = DirtySet::new();
  changes.extend([TEXTURE_ID]);
  let revision = tree.revision();
  assert!(!tree.texture_content_changed(&changes));
  assert_eq!(tree.revision(), revision);
}
