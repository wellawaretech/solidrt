//! Frame-damage resolution (okf/done/partial-repaint.md, stage 1): the
//! damaged-id set plus the last_extent cells resolve into a FrameDamage
//! rect that covers a change's old and new pixels, degrades to Full on
//! anything uncertain, and reports None on an unchanged frame.

use crate::renderer::prelude::*;
use crate::renderer::{Color, DisplayListBuilder, Point, Rect, Size};
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
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
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

// --- content damage: a change inside rectangles of a texture -----------------
//
// A target whose change stayed inside rectangles (a draw list's write)
// damages only what the node showing it shows of them
// (RenderTree::texture_content_changed), through how the last walk drew the
// node (rendertree::Shown). The frames here are present-only reuse frames:
// nothing in the tree changed.

use crate::gpu::{DirtySet, PxRect, SamplerState, TextureEntry, TextureFormat};
use crate::rendertree::composite::resolve_reuse_damage;

const TEX: u64 = 7;
const WINDOW: Size = Size::new(400.0, 300.0);

// root(1, 400x300) > holder(2, 200x100) > texture(3, 200x100) showing
// texture TEX, a 100x50 target: every texel is 2x2 window pixels.
fn shown_texture() -> (RenderTree, PlatformContext, crate::Context) {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(3, Texture::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 3, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 200.0, 100.0);
  size(&mut tree, 3, 200.0, 100.0);
  // Top-left, so the boxes are known: holder and texture at the origin.
  let root = tree.node_mut(1).style_mut().expect("root");
  root.align_items = AlignItems::FLEX_START;
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(TEX)),
    _ => unreachable!(),
  });
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  alloy.textures.insert(TEX, TextureEntry::d2_sampler_only(100, 50, SamplerState::default(), TextureFormat::Rgba8));
  platform.set_window_size(WINDOW.width, WINDOW.height);
  assert_eq!(paint(&mut tree, &platform, &alloy), FrameDamage::Full);
  (tree, platform, alloy)
}

fn texture_of(tree: &mut RenderTree, f: impl FnOnce(&mut Texture) -> Damage) {
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::Texture(t) => f(t),
    _ => unreachable!(),
  });
}

fn holder_of(tree: &mut RenderTree, f: impl FnOnce(&mut View)) {
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::View(v) => {
      f(v);
      Damage::Compose
    }
    _ => unreachable!(),
  });
}

// A change of TEX inside one rectangle of its pixels.
fn inside(x: i32, y: i32, w: u32, h: u32) -> DirtySet {
  let mut changes = DirtySet::new();
  changes.damage(TEX, &[PxRect { x, y, w, h }]);
  changes
}

fn all_of() -> DirtySet {
  let mut changes = DirtySet::new();
  changes.insert(TEX);
  changes
}

// The damage of a reuse frame after `changes`.
fn content_damage(tree: &mut RenderTree, changes: &DirtySet) -> FrameDamage {
  assert!(!tree.texture_content_changed(changes), "no snapshot boundary bakes the texture");
  resolve_reuse_damage(tree, WINDOW)
}

#[test]
fn content_rect_maps_through_the_fill() {
  let (mut tree, _platform, _alloy) = shown_texture();
  // Texels 10..15 grow by the filter's texel to 9..16, land on window
  // 18..32, and grow by the edge pixel.
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(17.0, 17.0, 16.0, 16.0)));
  // Nothing accumulates: the next frame is clean.
  assert_eq!(resolve_reuse_damage(&mut tree, WINDOW), FrameDamage::None);
}

#[test]
fn content_rects_union_and_stop_at_the_node() {
  let (mut tree, _platform, _alloy) = shown_texture();
  let mut changes = inside(0, 0, 2, 2);
  changes.damage(TEX, &[PxRect { x: 96, y: 46, w: 4, h: 4 }]);
  // Two far corners: the union is the node's own extent (its box and the
  // antialiasing outset), no more.
  let FrameDamage::Rect(d) = content_damage(&mut tree, &changes) else { panic!("expected a rect") };
  assert!(d.contains_rect(&rect(0.0, 0.0, 200.0, 100.0)), "{d:?}");
  assert!(rect(-2.0, -2.0, 204.0, 104.0).contains_rect(&d), "{d:?}");
}

#[test]
fn full_content_change_is_the_node() {
  let (mut tree, _platform, _alloy) = shown_texture();
  let FrameDamage::Rect(d) = content_damage(&mut tree, &all_of()) else { panic!("expected a rect") };
  assert!(d.contains_rect(&rect(0.0, 0.0, 200.0, 100.0)), "{d:?}");
}

#[test]
fn content_rect_follows_a_moved_and_scaled_ancestor() {
  let (mut tree, platform, alloy) = shown_texture();
  holder_of(&mut tree, |v| {
    v.translate = Some(Vector::new(30.0, 40.0));
    v.scale_x = Some(0.5);
    v.scale_y = Some(0.5);
    v.origin_x = Some(OriginCoord::Px(0.0));
    v.origin_y = Some(OriginCoord::Px(0.0));
  });
  paint(&mut tree, &platform, &alloy);
  // The fill's 18..32 halves to 9..16 and moves by (30, 40).
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(38.0, 48.0, 9.0, 9.0)));
}

#[test]
fn content_rect_under_a_rotation_is_its_bounding_box() {
  let (mut tree, platform, alloy) = shown_texture();
  holder_of(&mut tree, |v| {
    v.translate = Some(Vector::new(150.0, 50.0));
    v.rotate = Some(std::f32::consts::FRAC_PI_2);
    v.origin_x = Some(OriginCoord::Px(0.0));
    v.origin_y = Some(OriginCoord::Px(0.0));
  });
  paint(&mut tree, &platform, &alloy);
  // A quarter turn about the origin takes (x, y) to (-y, x): local 18..32
  // on both axes lands on x -32..-18, y 18..32, then moves by (150, 50).
  let FrameDamage::Rect(d) = content_damage(&mut tree, &inside(10, 10, 5, 5)) else { panic!("expected a rect") };
  assert!(d.contains_rect(&rect(118.0, 68.0, 14.0, 14.0)), "{d:?}");
  assert!(rect(116.0, 66.0, 18.0, 18.0).contains_rect(&d), "{d:?}");
}

#[test]
fn content_rect_maps_through_a_crop_and_misses_outside_it() {
  let (mut tree, platform, alloy) = shown_texture();
  // The right half of the texture, 50x50 texels over the 200x100 box: 4
  // window pixels a texel across, 2 down.
  texture_of(&mut tree, |t| {
    t.set_src_x(Some(50.0));
    t.set_src_w(Some(50.0))
  });
  paint(&mut tree, &platform, &alloy);
  // Texels x 60..62 grow to 59..63, 9 texels into the crop: window 36..52.
  assert_eq!(content_damage(&mut tree, &inside(60, 10, 2, 2)), FrameDamage::Rect(rect(35.0, 17.0, 18.0, 10.0)));
  // A change in the half the node does not show damages nothing.
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::None);
}

#[test]
fn content_rect_maps_through_a_letterbox() {
  let (mut tree, platform, alloy) = shown_texture();
  // The left 50x50 texels in the 200x100 box: contain scales them by 2
  // and centers the 100x100 picture, 50 px in.
  texture_of(&mut tree, |t| {
    t.set_fit(Some(TextureFit::Contain));
    t.set_src_w(Some(50.0))
  });
  paint(&mut tree, &platform, &alloy);
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(67.0, 17.0, 16.0, 16.0)));
}

#[test]
fn content_rect_under_a_repaint_boundary_maps_through_it() {
  let (mut tree, platform, alloy) = shown_texture();
  // A recording the texture was recorded in, where it has not moved: the
  // same rectangle as with no boundary (content_rect_maps_through_the_fill).
  boundary(&mut tree, 2, BoundaryMode::Recording);
  paint(&mut tree, &platform, &alloy);
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(17.0, 17.0, 16.0, 16.0)));
}

// A sample blur radius for the holder's filter, in logical px: any radius
// with a reach shows the spread.
const HOLDER_BLUR_RADIUS: f32 = 4.0;

// Blurs the holder by HOLDER_BLUR_RADIUS and repaints; returns the blur's
// reach past the holder's box, per side.
fn blur_holder(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> f32 {
  let filter = FilterState { blur: Some(HOLDER_BLUR_RADIUS), ..Default::default() };
  holder_of(tree, |v| {
    v.set_filter(Some(filter));
  });
  paint(tree, platform, alloy);
  filter.blur_outset()
}

#[test]
fn content_rect_under_a_blur_is_the_blurred_view() {
  let (mut tree, platform, alloy) = shown_texture();
  let reach = blur_holder(&mut tree, &platform, &alloy);
  let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
  // The blur spreads the mapped 17..33 by its reach on every side.
  assert!(covers(damage, rect(17.0, 17.0, 16.0, 16.0).inflate(reach, reach), WINDOW), "{damage:?}");
  // Only the holder's extent holds that reach, so the change damages the
  // whole holder: its 200x100 box and the reach past it.
  assert!(covers(damage, rect(0.0, 0.0, 200.0 + reach, 100.0 + reach), WINDOW), "{damage:?}");
}

#[test]
fn full_content_change_under_a_blur_reaches_past_the_node() {
  let (mut tree, platform, alloy) = shown_texture();
  let reach = blur_holder(&mut tree, &platform, &alloy);
  // The texture's own extent stops at its 200x100 box; the blur paints the
  // reach past it.
  let damage = content_damage(&mut tree, &all_of());
  assert!(covers(damage, rect(0.0, 0.0, 200.0 + reach, 100.0 + reach), WINDOW), "{damage:?}");
}

#[test]
fn content_rect_under_a_color_filter_keeps_the_rect() {
  let (mut tree, platform, alloy) = shown_texture();
  // Full grayscale: each pixel maps onto itself, nothing spreads.
  holder_of(&mut tree, |v| {
    v.set_filter(Some(FilterState { grayscale: Some(1.0), ..Default::default() }));
  });
  paint(&mut tree, &platform, &alloy);
  // The same rectangle as with no filter (content_rect_maps_through_the_fill).
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(17.0, 17.0, 16.0, 16.0)));
}

#[test]
fn content_rect_before_any_walk_is_the_node() {
  let (mut tree, platform, alloy) = shown_texture();
  // A texture node born after the last walk has no drawn placement.
  tree.create_node(4, Texture::default().with_layout());
  tree.insert_node(2, 4, None).expect("insert");
  tree.edit(4, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(TEX)),
    _ => unreachable!(),
  });
  tree.texture_content_changed(&inside(10, 10, 5, 5));
  // The walk that follows resolves the new node's own damage: more than
  // the rectangle node 3 shows.
  let damage = paint(&mut tree, &platform, &alloy);
  assert!(covers(damage, rect(17.0, 17.0, 16.0, 16.0), WINDOW), "{damage:?}");
  assert_ne!(damage, FrameDamage::Rect(rect(17.0, 17.0, 16.0, 16.0)));
}

#[test]
fn content_rect_and_a_move_in_one_frame_cover_both_places() {
  let (mut tree, platform, alloy) = shown_texture();
  // The rectangle resolves against the last walk; the move carries the
  // holder's own damage, old extent and new, which holds the rectangle
  // where it shows now.
  tree.texture_content_changed(&inside(10, 10, 5, 5));
  holder_of(&mut tree, |v| v.translate = Some(Vector::new(100.0, 100.0)));
  let damage = paint(&mut tree, &platform, &alloy);
  assert!(covers(damage, rect(17.0, 17.0, 16.0, 16.0), WINDOW), "{damage:?}");
  assert!(covers(damage, rect(117.0, 117.0, 16.0, 16.0), WINDOW), "{damage:?}");
}

// Node `id`'s repaint boundary, written as the prop is (Paint damage).
fn boundary(tree: &mut RenderTree, id: u64, mode: BoundaryMode) {
  tree.edit(id, |el| {
    el.repaint_boundary = mode;
    Damage::Paint
  });
}

// A paint, and how many boundaries it replayed and recorded.
fn paint_counts(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> (u32, u32) {
  let stats = paint_phase(&mut DisplayListBuilder::new(None), tree, platform, alloy);
  (stats.boundaries_reused, stats.boundaries_recorded)
}

// A view's write, as Compose damage, like holder_of for node `id`.
fn view_of(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut View)) {
  tree.edit(id, |el| match &mut el.kind {
    ElementKind::View(v) => {
      f(v);
      Damage::Compose
    }
    _ => unreachable!(),
  });
}

// Where a moved holder takes the texture: a sample offset, whole pixels.
const MOVED: Vector = Vector::new(150.0, 150.0);

#[test]
fn content_change_under_a_moved_recording_damages_where_it_shows() {
  let (mut tree, platform, alloy) = shown_texture();
  // A holder larger than the texture (300x200 around its 200x100), so the
  // texture's box and the holder's differ.
  tree.edit(2, |el| {
    el.style_mut().expect("holder").size = taffy::Size { width: length(300.0), height: length(200.0) };
    Damage::Layout
  });
  boundary(&mut tree, 2, BoundaryMode::Recording);
  paint(&mut tree, &platform, &alloy);
  // The move replays the recording: the walk does not enter it, and the
  // texture's draw is where the recording holds it, moved with the holder.
  holder_of(&mut tree, |v| v.translate = Some(MOVED));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the move replays");
  // The fill's 17..33 (content_rect_maps_through_the_fill) moved by 150.
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(167.0, 167.0, 16.0, 16.0)));
  // A full change: the texture's 200x100 box at 150, grown by the edge
  // pixel, not the holder's 300x200.
  assert_eq!(content_damage(&mut tree, &all_of()), FrameDamage::Rect(rect(149.0, 149.0, 202.0, 102.0)));
}

// How far the scrolled holder scrolls, px: past the changed rectangle's
// top, so the cut shows.
const SCROLL_Y: f32 = 20.0;

#[test]
fn content_rect_under_a_scrolled_recording_is_cut_to_what_it_shows() {
  let (mut tree, platform, alloy) = shown_texture();
  tree.edit(2, |el| {
    el.style_mut().expect("holder").overflow = taffy::Point { x: Overflow::Hidden, y: Overflow::Hidden };
    Damage::Layout
  });
  boundary(&mut tree, 2, BoundaryMode::Recording);
  paint(&mut tree, &platform, &alloy);
  // A scroll keeps a recording's cache: the composite scrolls the replay.
  tree.edit(2, |el| match &mut el.kind {
    ElementKind::View(v) => v.set_scroll_y(Some(SCROLL_Y)),
    _ => unreachable!(),
  });
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the scroll replays");
  // The fill's 17..33 scrolls up by 20 to -3..13, cut to the holder's
  // clipped 0..100: rows 0..13.
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(17.0, 0.0, 16.0, 13.0)));
}

// root(1, 400x300) > outer(2, 200x100) > inner(4, 200x100) > texture(3,
// 200x100) showing TEX, a 100x50 target, the boxes at the origin as in
// shown_texture.
fn nested_texture(outer: BoundaryMode, inner: BoundaryMode) -> (RenderTree, PlatformContext, crate::Context) {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(4, attached());
  tree.create_node(3, Texture::default().with_layout());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 4, None).expect("insert");
  tree.insert_node(4, 3, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 200.0, 100.0);
  size(&mut tree, 4, 200.0, 100.0);
  size(&mut tree, 3, 200.0, 100.0);
  tree.node_mut(1).style_mut().expect("root").align_items = AlignItems::FLEX_START;
  tree.edit(3, |el| match &mut el.kind {
    ElementKind::Texture(t) => t.set_src(Some(TEX)),
    _ => unreachable!(),
  });
  boundary(&mut tree, 2, outer);
  boundary(&mut tree, 4, inner);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  alloy.textures.insert(TEX, TextureEntry::d2_sampler_only(100, 50, SamplerState::default(), TextureFormat::Rgba8));
  platform.set_window_size(WINDOW.width, WINDOW.height);
  assert_eq!(paint(&mut tree, &platform, &alloy), FrameDamage::Full);
  (tree, platform, alloy)
}

// Sample moves for the nested case, whole pixels: the inner boundary's,
// then the outer one's.
const INNER_MOVE: Vector = Vector::new(30.0, 20.0);
const OUTER_MOVE: Vector = Vector::new(100.0, 100.0);

#[test]
fn content_rect_under_nested_recordings_follows_both_moves() {
  let (mut tree, platform, alloy) = nested_texture(BoundaryMode::Recording, BoundaryMode::Recording);
  // The inner move re-records the outer recording, which replays the
  // inner one inside it.
  view_of(&mut tree, 4, |v| v.translate = Some(INNER_MOVE));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 1), "the inner replays, the outer records");
  // The fill's 17..33 moved by (30, 20).
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(47.0, 37.0, 16.0, 16.0)));
  // The outer move replays the outer recording, the inner one inside it
  // unvisited: the rectangle moves by both, to (130, 120).
  view_of(&mut tree, 2, |v| v.translate = Some(OUTER_MOVE));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the outer replays");
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(147.0, 137.0, 16.0, 16.0)));
}

// A blur for the nested case's views, with a reach.
fn blur(tree: &mut RenderTree, id: u64) {
  view_of(tree, id, |v| {
    v.set_filter(Some(FilterState { blur: Some(HOLDER_BLUR_RADIUS), ..Default::default() }));
  });
}

#[test]
fn content_rect_under_a_snapshot_or_a_blur_on_a_recording_chain_is_the_outermost_node() {
  // Each chain, outer node over inner node over the texture, and which of
  // the two blurs. A snapshot is resampled at composite and a blur spreads
  // the change, inside a recording or outside it: the outer node is the
  // outermost on every chain, damaged whole.
  let chains = [
    (BoundaryMode::Snapshot, BoundaryMode::Recording, None),
    (BoundaryMode::Recording, BoundaryMode::Snapshot, None),
    (BoundaryMode::None, BoundaryMode::Recording, Some(2)),
    (BoundaryMode::Recording, BoundaryMode::None, Some(4)),
  ];
  for (n, (outer, inner, blurred)) in chains.into_iter().enumerate() {
    let (mut tree, platform, alloy) = nested_texture(outer, inner);
    if let Some(id) = blurred {
      blur(&mut tree, id);
      paint(&mut tree, &platform, &alloy);
    }
    tree.texture_content_changed(&inside(10, 10, 5, 5));
    // The outer node's whole 200x100 box, not the 16x16 rectangle, and
    // not the whole window either.
    let FrameDamage::Rect(d) = resolve_reuse_damage(&mut tree, WINDOW) else { panic!("chain {n}: expected a rect") };
    assert!(d.contains_rect(&rect(0.0, 0.0, 200.0, 100.0)), "chain {n}: {d:?}");
  }
}

// How far inside the scaled holder its padding puts the texture, px: an
// offset the holder's scale multiplies, so the frame's map carries a
// translation besides the scale.
const INSET: f32 = 50.0;

// A scale far under the frame scale floor (cull::map_relative) but
// not zero: its inverse exists, with a large f32 error.
const NEARLY_SINGULAR_SCALE: f32 = 1.0e-6;

// The holder of shown_texture as a 300x200 Recording boundary with the
// texture INSET in, moved by MOVED and recorded at `scale_x`, then
// scaled back to 1 by a Compose, which replays the recording: the
// texture's draw is stated relative to a frame recorded at that scale.
// Returns the window rectangle the texels 10..15 show on now.
fn scaled_back(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context, scale_x: f32) -> Rect {
  scaled_back_xy(tree, platform, alloy, scale_x, 1.0)
}

// scaled_back, recorded at `scale_x` by `scale_y`.
fn scaled_back_xy(
  tree: &mut RenderTree,
  platform: &PlatformContext,
  alloy: &crate::Context,
  scale_x: f32,
  scale_y: f32,
) -> Rect {
  tree.edit(2, |el| {
    let style = el.style_mut().expect("holder");
    style.size = taffy::Size { width: length(300.0), height: length(200.0) };
    style.padding = taffy::Rect { left: length(INSET), right: zero(), top: length(INSET), bottom: zero() };
    Damage::Layout
  });
  boundary(tree, 2, BoundaryMode::Recording);
  holder_of(tree, |v| {
    v.translate = Some(MOVED);
    v.scale_x = Some(scale_x);
    v.scale_y = Some(scale_y);
    v.origin_x = Some(OriginCoord::Px(0.0));
    v.origin_y = Some(OriginCoord::Px(0.0));
  });
  assert_eq!(paint_counts(tree, platform, alloy).1, 1, "the scaled holder records");
  holder_of(tree, |v| {
    v.scale_x = Some(1.0);
    v.scale_y = Some(1.0);
  });
  assert_eq!(paint_counts(tree, platform, alloy), (1, 0), "the scale back replays");
  // The fill's 17..33 (content_rect_maps_through_the_fill), INSET into
  // the holder at MOVED.
  rect(MOVED.x + INSET + 17.0, MOVED.y + INSET + 17.0, 16.0, 16.0)
}

#[test]
fn content_change_under_a_zero_scaled_recording_falls_back_to_the_boundary() {
  let (mut tree, platform, alloy) = shown_texture();
  // Recorded at a zero scale: the boundary's frame has no inverse, so the
  // texture's draw cannot be stated relative to it, and the holder stands
  // for it, damaged whole where it shows now.
  let real = scaled_back(&mut tree, &platform, &alloy, 0.0);
  let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
  assert!(covers(damage, real, WINDOW), "{damage:?}");
  assert_ne!(damage, FrameDamage::Rect(real.inflate(1.0, 1.0)));
}

#[test]
fn content_change_under_a_nearly_singular_recording_falls_back_to_the_boundary() {
  let (mut tree, platform, alloy) = shown_texture();
  // A tiny scale inverts, with an error that a scale back to 1 replays
  // unchanged: the frame counts as singular, like a zero scale.
  let real = scaled_back(&mut tree, &platform, &alloy, NEARLY_SINGULAR_SCALE);
  let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
  assert!(covers(damage, real, WINDOW), "{damage:?}");
}

// A large scale on the other axis, so the frame's determinant is not
// small while its x axis is nearly singular.
const LARGE_SCALE: f32 = 100.0;

#[test]
fn content_change_under_a_recording_singular_on_one_axis_falls_back_to_the_boundary() {
  let (mut tree, platform, alloy) = shown_texture();
  // The inverse's error is per axis: a large y scale does not make up
  // for a nearly zero x scale.
  let real = scaled_back_xy(&mut tree, &platform, &alloy, NEARLY_SINGULAR_SCALE, LARGE_SCALE);
  let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
  assert!(covers(damage, real, WINDOW), "{damage:?}");
}

#[test]
fn full_content_change_under_nested_recordings_follows_both_moves() {
  let (mut tree, platform, alloy) = nested_texture(BoundaryMode::Recording, BoundaryMode::Recording);
  view_of(&mut tree, 4, |v| v.translate = Some(INNER_MOVE));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 1), "the inner replays, the outer records");
  view_of(&mut tree, 2, |v| v.translate = Some(OUTER_MOVE));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the outer replays");
  // The texture's 200x100 box moved by both, to (130, 120), grown by the
  // edge pixel: not either boundary's box.
  assert_eq!(content_damage(&mut tree, &all_of()), FrameDamage::Rect(rect(129.0, 119.0, 202.0, 102.0)));
}

// A display scale with two device pixels to a logical one.
const HIDPI: f32 = 2.0;

#[test]
fn content_rect_at_display_scale_two_is_logical_and_covers_the_device_pixels() {
  let (mut tree, platform, alloy) = shown_texture();
  platform.set_display_scale(HIDPI);
  paint(&mut tree, &platform, &alloy);
  // The same logical rectangle as at scale 1 (content_rect_maps_through_the_fill).
  let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
  assert_eq!(damage, FrameDamage::Rect(rect(17.0, 17.0, 16.0, 16.0)));
  // Handed to the raster side in device pixels, it holds the texels 9..16
  // the change shows on through the filter: 4 device pixels a texel.
  let crate::PresentDamage::Rect(d) = crate::PresentDamage::from_frame(damage, HIDPI) else {
    panic!("expected a device rect")
  };
  assert!(d.x <= 36 && d.y <= 36 && d.x + d.width >= 64 && d.y + d.height >= 64, "{d:?}");
}

// A corner radius for the rounded texture, px.
const CORNER: f32 = 8.0;

#[test]
fn content_rect_of_a_rounded_texture_maps_like_the_plain_draw() {
  let (mut tree, platform, alloy) = shown_texture();
  texture_of(&mut tree, |t| t.set_radius(Some([CORNER; 4])));
  paint(&mut tree, &platform, &alloy);
  assert_eq!(content_damage(&mut tree, &inside(10, 10, 5, 5)), FrameDamage::Rect(rect(17.0, 17.0, 16.0, 16.0)));
}

#[test]
fn content_rect_while_a_boundary_comes_and_goes_is_never_too_small() {
  let (mut tree, platform, alloy) = shown_texture();
  boundary(&mut tree, 2, BoundaryMode::Recording);
  paint(&mut tree, &platform, &alloy);
  holder_of(&mut tree, |v| v.translate = Some(MOVED));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the move replays");
  // Where the texels 10..15 show, at MOVED.
  let real = rect(167.0, 167.0, 16.0, 16.0);
  // Each mode write, then a change before the walk and one after it.
  for (step, mode) in [BoundaryMode::None, BoundaryMode::Recording, BoundaryMode::None].into_iter().enumerate() {
    boundary(&mut tree, 2, mode);
    let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
    assert!(covers(damage, real, WINDOW), "step {step} before the walk: {damage:?}");
    paint(&mut tree, &platform, &alloy);
    let damage = content_damage(&mut tree, &inside(10, 10, 5, 5));
    assert_eq!(damage, FrameDamage::Rect(real), "step {step} after the walk");
  }
}

// --- tree damage under a Recording boundary that moved since it recorded ----
//
// A replayed recording is not walked, so the cells of the nodes inside it
// keep the walk that recorded it. A change of one of them resolves its old
// half where it truly showed: through the outermost Recording boundary
// above it where one moved since it recorded, its own cell otherwise.

// Where the child sits in its holder, px from the left.
const CHILD_X: f32 = 100.0;
const CHILD: Size = Size::new(100.0, 50.0);

// root(1, 400x300) > holder(2, 300x200, `mode`) > wrapper(4, 300x200) >
// backdrop(5, 300x200, absolute), child(3, a 100x50 view CHILD_X in): the
// holder larger than its child. The backdrop fills the wrapper, so its
// content size, and every layout above, holds whatever the child's layout
// does: only the child is damaged.
fn held_child(mode: BoundaryMode) -> (RenderTree, PlatformContext, crate::Context) {
  let mut tree = RenderTree::new();
  tree.create_node(1, attached());
  tree.create_node(2, attached());
  tree.create_node(4, attached());
  tree.create_node(5, attached());
  tree.create_node(3, attached());
  tree.insert_node(1, 2, None).expect("insert");
  tree.insert_node(2, 4, None).expect("insert");
  tree.insert_node(4, 5, None).expect("insert");
  tree.insert_node(4, 3, None).expect("insert");
  tree.root = Some(1);
  size(&mut tree, 1, 400.0, 300.0);
  size(&mut tree, 2, 300.0, 200.0);
  size(&mut tree, 4, 300.0, 200.0);
  size(&mut tree, 5, 300.0, 200.0);
  tree.node_mut(5).style_mut().expect("backdrop").position = taffy::style::Position::Absolute;
  size(&mut tree, 3, CHILD.width, CHILD.height);
  tree.node_mut(1).style_mut().expect("root").align_items = AlignItems::FLEX_START;
  tree.node_mut(3).style_mut().expect("child").margin.left = length(CHILD_X);
  boundary(&mut tree, 2, mode);
  let platform = PlatformContext::new(Vec::new());
  let alloy = headless();
  platform.set_window_size(WINDOW.width, WINDOW.height);
  assert_eq!(paint(&mut tree, &platform, &alloy), FrameDamage::Full);
  (tree, platform, alloy)
}

// The child moved back to its holder's left edge, a Compose write: no
// layout changes.
fn child_to_the_left(tree: &mut RenderTree) {
  view_of(tree, 3, |v| v.translate = Some(Vector::new(-CHILD_X, 0.0)));
}

// The holder moved by MOVED, a move that replays its recording.
fn moved_holder(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) {
  holder_of(tree, |v| v.translate = Some(MOVED));
  assert_eq!(paint_counts(tree, platform, alloy), (1, 0), "the move replays");
}

// Where the child showed after the move: CHILD_X into the holder at MOVED.
fn true_old_place() -> Rect {
  Rect::new(Point::new(MOVED.x + CHILD_X, MOVED.y), CHILD)
}

#[test]
fn child_moved_under_a_moved_recording_damages_its_true_old_place() {
  let (mut tree, platform, alloy) = held_child(BoundaryMode::Recording);
  moved_holder(&mut tree, &platform, &alloy);
  // Left, so its stale cell and its new place do not span the old one.
  child_to_the_left(&mut tree);
  let damage = paint(&mut tree, &platform, &alloy);
  assert!(covers(damage, true_old_place(), WINDOW), "old place: {damage:?}");
  assert!(covers(damage, Rect::new(Point::new(MOVED.x, MOVED.y), CHILD), WINDOW), "new place: {damage:?}");
}

// The child hidden, a layout write; the child loop skips it before its
// cell is written, so its cell keeps the last walk that reached it.
fn hide_child(tree: &mut RenderTree) {
  tree.edit(3, |el| {
    el.style_mut().expect("child").display = Display::None;
    Damage::Layout
  });
}

#[test]
fn child_hidden_under_a_moved_recording_damages_its_true_old_place() {
  let (mut tree, platform, alloy) = held_child(BoundaryMode::Recording);
  moved_holder(&mut tree, &platform, &alloy);
  hide_child(&mut tree);
  let damage = paint(&mut tree, &platform, &alloy);
  assert!(covers(damage, true_old_place(), WINDOW), "{damage:?}");
}

// How far past CHILD_X the removed child is translated, px: half of it
// past the holder's right edge, so the wrapper's extent, which the removal
// damages, does not hold it once it is gone.
const OVERHANG: f32 = 150.0;
// A move of the holder small enough that the overhanging child stays in
// the window, past the right edge of every extent the removal damages
// without the stand-in.
const NUDGE: Vector = Vector::new(30.0, 0.0);

#[test]
fn child_removed_under_a_moved_recording_damages_its_true_old_place() {
  let (mut tree, platform, alloy) = held_child(BoundaryMode::Recording);
  view_of(&mut tree, 3, |v| v.translate = Some(Vector::new(OVERHANG, 0.0)));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy).1, 1, "the child's move records");
  holder_of(&mut tree, |v| v.translate = Some(NUDGE));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the move replays");
  // The removal damages the wrapper, whose cell before the walk is where
  // the recording walk left it, and after it is the wrapper without the
  // child: neither reaches the child's right half where it showed.
  tree.delete_node(4, 3);
  let damage = paint(&mut tree, &platform, &alloy);
  let shown = Rect::new(Point::new(NUDGE.x + CHILD_X + OVERHANG, NUDGE.y), CHILD);
  assert!(covers(damage, shown, WINDOW), "{damage:?}");
}

#[test]
fn child_hidden_after_a_capture_walk_under_a_recording_moved_and_back_damages_its_true_place() {
  let (mut tree, platform, alloy) = held_child(BoundaryMode::Recording);
  // Moved with a capture inside: the capture's walk enters the replayed
  // recording and writes the cells inside it at MOVED.
  holder_of(&mut tree, |v| v.translate = Some(MOVED));
  alloy.request_capture(3, Box::new(|_| {}));
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the move replays");
  // Back where it recorded: the replay's frame is the recorded one, and
  // the child's cell is still at MOVED.
  holder_of(&mut tree, |v| v.translate = None);
  assert_eq!(paint_counts(&mut tree, &platform, &alloy), (1, 0), "the move back replays");
  hide_child(&mut tree);
  let damage = paint(&mut tree, &platform, &alloy);
  assert!(covers(damage, Rect::new(Point::new(CHILD_X, 0.0), CHILD), WINDOW), "{damage:?}");
}

#[test]
fn a_node_under_a_snapshot_resolves_through_the_outermost_boundary() {
  // A reused snapshot is not walked and keeps no frame, so the cells
  // under it may be those of the walk that last rasterized it, wherever
  // the snapshot moved since: the outermost repaint boundary on the chain
  // stands in, of either kind. Headless, a snapshot's rasterization fails
  // and it paints inline on every walk, so its cells are fresh here and
  // the stand-in is read directly.
  let (tree, _platform, _alloy) = held_child(BoundaryMode::Snapshot);
  assert_eq!(tree.boundary_stand_in(3), Some(2), "a snapshot holder");
  // A Recording boundary inside a snapshot, or a snapshot inside one: the
  // inner Recording boundary's frame says nothing of a snapshot that moved.
  let chains = [(BoundaryMode::Snapshot, BoundaryMode::Recording), (BoundaryMode::Recording, BoundaryMode::Snapshot)];
  for (n, (outer, inner)) in chains.into_iter().enumerate() {
    let (tree, _platform, _alloy) = nested_texture(outer, inner);
    assert_eq!(tree.boundary_stand_in(3), Some(2), "chain {n}");
  }
  // Under Recording boundaries alone that have not moved, the node's own
  // cell.
  let (tree, _platform, _alloy) = nested_texture(BoundaryMode::Recording, BoundaryMode::Recording);
  assert_eq!(tree.boundary_stand_in(3), None, "unmoved recordings");
}

#[test]
fn child_moved_under_an_unmoved_recording_damages_the_child_not_the_holder() {
  let damage_of = |mode: BoundaryMode| {
    let (mut tree, platform, alloy) = held_child(mode);
    child_to_the_left(&mut tree);
    paint(&mut tree, &platform, &alloy)
  };
  // The same damage as with no boundary: the child's old and new boxes.
  let recorded = damage_of(BoundaryMode::Recording);
  assert_eq!(recorded, damage_of(BoundaryMode::None));
  let FrameDamage::Rect(d) = recorded else { panic!("expected a rect, got {recorded:?}") };
  assert!(d.contains_rect(&Rect::new(Point::new(0.0, 0.0), Size::new(CHILD_X + CHILD.width, CHILD.height))), "{d:?}");
  assert!(d.size.width < 300.0 && d.size.height < 200.0, "not the holder: {d:?}");
}
