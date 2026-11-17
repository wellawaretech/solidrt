//! Validates that a snapshot boundary a relayout only moves keeps its
//! raster: the texture holds the subtree relative to the boundary's own
//! origin, which is on a whole device pixel wherever it lands, so a move
//! with the same snapped box composites the same texture at the new place.
//! A move that changes the snapped box (its far edge rounds to another
//! device pixel) rasterizes once, and a size change rasterizes again.
//!
//! Scenes, each at display scale 1 and 1.5 (geometry stated in device px,
//! so a scene rounds the same on both grids):
//!   push: root (column) > sibling (green fill) > card (Snapshot boundary,
//!     blue fill). The sibling grows by whole device pixels: the card moves
//!     down with its box unchanged.
//!   nudge: the same with a card whose far edge sits a fraction off the
//!     grid; the sibling grows by a fraction of a device pixel, so the
//!     card's snapped box takes one more device pixel.
//!   resize: the card's own height changes.
//!   list: root (column) > cards (Snapshot boundaries, red and blue in
//!     turn); a green card is inserted at the top, and every card below
//!     moves down.
//!
//! Run: cargo run -p alloy --example boundary_layout_move

use alloy::renderer::prelude::*;
use alloy::renderer::{Color, DisplayListBuilder, ISize};
use alloy::rendertree::composite::{paint_phase, PaintStats};
use alloy::rendertree::{BoundaryMode, Damage, PlatformContext, Rectangle, RenderTree, View, Window};
use taffy::prelude::{length, FlexDirection, Size, Style};

const W: u32 = 400;
const H: u32 = 300;
const DISPLAY_SCALES: [f32; 2] = [1.0, 1.5];

// The push scene, in device px: the sibling's height, the card's box, and
// how much the sibling grows.
const SIBLING_DEVICE: f32 = 30.0;
const CARD_W_DEVICE: f32 = 150.0;
const CARD_H_DEVICE: f32 = 75.0;
const GROW_DEVICE: f32 = 30.0;
// The nudge scene: a card whose far edge is a fraction above a whole device
// pixel (rounds down), and a growth of the sibling that takes the far edge
// past the half (rounds up) while the near edge still rounds down.
const NUDGE_CARD_H_DEVICE: f32 = 75.4;
const NUDGE_DEVICE: f32 = 0.4;
// The resize scene: the card's new height.
const RESIZED_H_DEVICE: f32 = 90.0;
// The list scene: its cards' box, how many there are before the insert.
const LIST_CARD_W_DEVICE: f32 = 150.0;
const LIST_CARD_H_DEVICE: f32 = 50.0;
const LIST_CARDS: u64 = 4;
// Frames painted after a change, each expected to reuse every snapshot.
const STILL_FRAMES: u32 = 3;
// How far a channel of a solid color's readback may be off.
const CHANNEL_TOLERANCE: i32 = 2;

const ROOT: u64 = 1;
const SIBLING: u64 = 2;
const SIBLING_FILL: u64 = 3;
const CARD: u64 = 4;
const CARD_FILL: u64 = 5;
// List card i is LIST_ID_BASE + 2 * i, its fill the id after it; the
// inserted card is card LIST_CARDS.
const LIST_ID_BASE: u64 = 10;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn rgba(px: [u8; 4]) -> Color {
  let unit = |c: u8| f32::from(c) / f32::from(u8::MAX);
  Color::new_srgba(unit(px[0]), unit(px[1]), unit(px[2]), unit(px[3]))
}

// The failures of one case, printed when it ends.
struct Case {
  name: String,
  failures: Vec<String>,
}

impl Case {
  fn new(name: String) -> Self {
    Self { name, failures: Vec::new() }
  }

  fn expect(&mut self, ok: bool, what: impl FnOnce() -> String) {
    if !ok {
      self.failures.push(what());
    }
  }

  // A frame's snapshot counters against `want`, printed either way.
  fn counts(&mut self, frame: &str, stats: &PaintStats, want: (u32, u32, u32)) {
    let got = counts(stats);
    println!("  {}: {frame}: (reused, rerendered, rasterized) = {got:?}", self.name);
    self.expect(got == want, || format!("{frame}: {got:?}, expected {want:?}"));
  }

  // The device pixel at (x, y) of a frame's readback against `want`.
  fn pixel(&mut self, frame: &Frame, x: f32, y: f32, want: [u8; 4], what: &str) {
    let (x, y) = (x as u32, y as u32);
    let i = ((y * frame.width + x) * 4) as usize;
    let got = [frame.pixels[i], frame.pixels[i + 1], frame.pixels[i + 2], frame.pixels[i + 3]];
    let near = got.iter().zip(want).all(|(g, w)| (i32::from(*g) - i32::from(w)).abs() <= CHANNEL_TOLERANCE);
    self.expect(near, || format!("{what} at ({x},{y}): {got:?}, expected {want:?}"));
  }

  // Report the case; its failures join `all`.
  fn finish(self, all: &mut Vec<String>) {
    if self.failures.is_empty() {
      println!("ok: {}", self.name);
      return;
    }
    println!("FAIL: {} ({} failed checks)", self.name, self.failures.len());
    for failure in &self.failures {
      println!("  {failure}");
    }
    all.extend(self.failures.into_iter().map(|f| format!("{}: {f}", self.name)));
  }
}

fn counts(stats: &PaintStats) -> (u32, u32, u32) {
  (stats.snapshots_reused, stats.snapshots_rerendered, stats.snapshots_rasterized)
}

fn styled(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut Style)) {
  tree.edit(id, |el| {
    f(el.style_mut().expect("layout"));
    Damage::Layout
  });
}

fn sized(style: &mut Style, w: f32, h: f32) {
  style.size = Size { width: length(w), height: length(h) };
}

fn write_color(tree: &mut RenderTree, id: u64, color: Color) {
  tree.edit(id, |el| el.kind.paint_mut().expect("paintable kind").set_color(Some(color)));
}

fn snapshot(tree: &mut RenderTree, id: u64) {
  tree.edit(id, |el| {
    el.repaint_boundary = BoundaryMode::Snapshot;
    Damage::Paint
  });
}

// A laid-out view of `w` x `h` with a detached rect of `color` filling it.
fn filled(tree: &mut RenderTree, id: u64, fill: u64, w: f32, h: f32, color: [u8; 4]) {
  tree.create_node(id, View::default().with_layout());
  tree.create_node(fill, Rectangle::default().no_layout());
  tree.insert_node(id, fill, None).expect("insert");
  styled(tree, id, |s| {
    sized(s, w, h);
    s.flex_shrink = 0.0;
  });
  write_color(tree, fill, rgba(color));
}

fn build_root() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, Window::default().with_layout());
  tree.root = Some(ROOT);
  styled(&mut tree, ROOT, |s| s.flex_direction = FlexDirection::Column);
  tree
}

// root > sibling (green) > card (a snapshot, blue), sizes in device px.
fn build_push(display_scale: f32, card_h_device: f32) -> RenderTree {
  let mut tree = build_root();
  let logical = |device: f32| device / display_scale;
  filled(&mut tree, SIBLING, SIBLING_FILL, W as f32, logical(SIBLING_DEVICE), GREEN);
  filled(&mut tree, CARD, CARD_FILL, logical(CARD_W_DEVICE), logical(card_h_device), BLUE);
  tree.insert_node(ROOT, SIBLING, None).expect("insert");
  tree.insert_node(ROOT, CARD, None).expect("insert");
  snapshot(&mut tree, CARD);
  tree
}

fn list_card(i: u64) -> (u64, u64) {
  (LIST_ID_BASE + 2 * i, LIST_ID_BASE + 2 * i + 1)
}

fn list_color(i: u64) -> [u8; 4] {
  match i {
    i if i == LIST_CARDS => GREEN,
    i if i % 2 == 0 => RED,
    _ => BLUE,
  }
}

fn add_list_card(tree: &mut RenderTree, display_scale: f32, i: u64, before: Option<u64>) {
  let (id, fill) = list_card(i);
  let logical = |device: f32| device / display_scale;
  filled(tree, id, fill, logical(LIST_CARD_W_DEVICE), logical(LIST_CARD_H_DEVICE), list_color(i));
  tree.insert_node(ROOT, id, before).expect("insert");
  snapshot(tree, id);
}

// A frame's pixels and stats.
struct Frame {
  pixels: Vec<u8>,
  width: u32,
  stats: PaintStats,
}

// A frame recorded as `composite::render` records it, under the display
// scale, and read back in device px.
fn frame(tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> Frame {
  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, ctx);
  let dl = builder.build().expect("frame display list");
  let (w, h) = ((W as f32 * scale).round() as u32, (H as f32 * scale).round() as u32);
  let texture = ctx.render_display_list_to_texture(&dl, w, h, true).expect("frame rasterization");
  let pixels = ctx.read_texture(&texture, w, h).expect("texture readback");
  Frame { pixels, width: w, stats }
}

fn platform_at(display_scale: f32) -> PlatformContext {
  let platform = PlatformContext::new(Vec::new());
  platform.set_window_size(W as f32, H as f32);
  platform.set_display_scale(display_scale);
  platform
}

// Frames after a change that changes nothing more: every snapshot reused.
fn still(case: &mut Case, tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context, cards: u32) {
  for look in 1..=STILL_FRAMES {
    let f = frame(tree, platform, ctx);
    case.counts(&format!("still {look}"), &f.stats, (cards, 0, 0));
  }
}

fn push_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a card pushed down by whole device px at display scale {display_scale}"));
  let platform = platform_at(display_scale);
  let mut tree = build_push(display_scale, CARD_H_DEVICE);
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first.stats, (0, 0, 1));
  still(&mut case, &mut tree, &platform, ctx, 1);
  let x = CARD_W_DEVICE / 2.0;
  case.pixel(&first, x, SIBLING_DEVICE + CARD_H_DEVICE / 2.0, BLUE, "the card");
  styled(&mut tree, SIBLING, |s| sized(s, W as f32, (SIBLING_DEVICE + GROW_DEVICE) / display_scale));
  let moved = frame(&mut tree, &platform, ctx);
  case.counts("moved", &moved.stats, (1, 0, 0));
  // The card's old top, not under its new box: the sibling grew over it.
  case.pixel(&moved, x, SIBLING_DEVICE + GROW_DEVICE / 2.0, GREEN, "the card's old place");
  // The card's new bottom, past its old box.
  let new_bottom = SIBLING_DEVICE + GROW_DEVICE + CARD_H_DEVICE - GROW_DEVICE / 2.0;
  case.pixel(&moved, x, new_bottom, BLUE, "the card's new place");
  still(&mut case, &mut tree, &platform, ctx, 1);
  case.finish(all);
}

fn nudge_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a card nudged onto another snapped box at display scale {display_scale}"));
  let platform = platform_at(display_scale);
  let (near, far) = (SIBLING_DEVICE + NUDGE_DEVICE, SIBLING_DEVICE + NUDGE_CARD_H_DEVICE + NUDGE_DEVICE);
  let before = (SIBLING_DEVICE + NUDGE_CARD_H_DEVICE).round() - SIBLING_DEVICE.round();
  case.expect(far.round() - near.round() != before, || "the nudge leaves the snapped box as it was".to_string());
  let mut tree = build_push(display_scale, NUDGE_CARD_H_DEVICE);
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first.stats, (0, 0, 1));
  still(&mut case, &mut tree, &platform, ctx, 1);
  styled(&mut tree, SIBLING, |s| sized(s, W as f32, (SIBLING_DEVICE + NUDGE_DEVICE) / display_scale));
  let nudged = frame(&mut tree, &platform, ctx);
  // A box a device pixel taller is a texture a texel taller: new storage.
  case.counts("nudged", &nudged.stats, (0, 0, 1));
  // The pixel the snapped box took on.
  case.pixel(&nudged, CARD_W_DEVICE / 2.0, far.round() - 1.0, BLUE, "the card's new last row");
  still(&mut case, &mut tree, &platform, ctx, 1);
  case.finish(all);
}

fn resize_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a resized card at display scale {display_scale}"));
  let platform = platform_at(display_scale);
  let mut tree = build_push(display_scale, CARD_H_DEVICE);
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first.stats, (0, 0, 1));
  styled(&mut tree, CARD, |s| sized(s, CARD_W_DEVICE / display_scale, RESIZED_H_DEVICE / display_scale));
  let resized = frame(&mut tree, &platform, ctx);
  case.counts("resized", &resized.stats, (0, 0, 1));
  case.pixel(&resized, CARD_W_DEVICE / 2.0, SIBLING_DEVICE + RESIZED_H_DEVICE - 1.0, BLUE, "the card's last row");
  still(&mut case, &mut tree, &platform, ctx, 1);
  case.finish(all);
}

fn list_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a card inserted above a list at display scale {display_scale}"));
  let platform = platform_at(display_scale);
  let mut tree = build_root();
  for i in 0..LIST_CARDS {
    add_list_card(&mut tree, display_scale, i, None);
  }
  let cards = LIST_CARDS as u32;
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first.stats, (0, 0, cards));
  still(&mut case, &mut tree, &platform, ctx, cards);
  add_list_card(&mut tree, display_scale, LIST_CARDS, Some(list_card(0).0));
  let inserted = frame(&mut tree, &platform, ctx);
  // The cards below are reused; the new one is rasterized.
  case.counts("inserted", &inserted.stats, (cards, 0, 1));
  let x = LIST_CARD_W_DEVICE / 2.0;
  let row = |slot: u64| (slot as f32 + 0.5) * LIST_CARD_H_DEVICE;
  case.pixel(&inserted, x, row(0), list_color(LIST_CARDS), "the inserted card");
  for i in 0..LIST_CARDS {
    case.pixel(&inserted, x, row(i + 1), list_color(i), &format!("card {i} one slot down"));
  }
  still(&mut case, &mut tree, &platform, ctx, cards + 1);
  case.finish(all);
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "boundary layout move", ISize::new(600, 450), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let mut all = Vec::new();
    for display_scale in DISPLAY_SCALES {
      push_case(&ctx, display_scale, &mut all);
      nudge_case(&ctx, display_scale, &mut all);
      resize_case(&ctx, display_scale, &mut all);
      list_case(&ctx, display_scale, &mut all);
    }
    assert!(all.is_empty(), "{} boundary layout move checks failed", all.len());
    println!("all boundary layout move checks passed");
  });
}
