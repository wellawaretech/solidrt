//! Validates snapshot boundaries under an ancestor scale written every
//! frame, as an app's per-frame write does (no transition track): the
//! snapped box of a boundary is a new logical size at every scale, so the
//! snapshot key keeps the box its texture was made for while the raster is
//! composited stretched, and while this frame's box takes the same texels.
//! The snapshot is then reused while the scale moves, re-rasterizes only
//! where the density rule says so (past the motion bound, and at rest at a
//! scale other than the raster's), and never allocates a texture per frame.
//! The quad draws at this frame's box, so snapshot boundaries that touch
//! meet without a seam, under the written scale and under a native scale
//! track up and down.
//!
//! Scenes, each at display scale 1 and 1.5:
//!   tiles: root > container (its scale written per frame, origin 0,0) >
//!     two tiles at fractional left/top, each holding a snapshot boundary
//!     of laid-out cells with a detached rect each. Read through PaintStats,
//!     under a sine sweep, an exponential ease and a slow drift.
//!   text tiles: the same with a `<text>` in each boundary: a raster made
//!     mid-motion holds unhinted text, which the snapshot re-renders once,
//!     hinted, at rest.
//!   pair: root > [a green detached background, the container > three
//!     snapshot boundaries: A red, B blue at A's right, C blue below A, no
//!     gaps, each a detached rect filling it], in two geometries. Read back
//!     along a row across A|B and a column across A|C while the container's
//!     scale moves, written per frame (with a content change in flight) and
//!     under a track.
//! The written-scale cases run again with `pixelSnap={false}` on the
//! container, counters only: there the boxes are fractional layout boxes,
//! mapped 1:1, and a seam between them is what that prop asks for.
//!
//! Run: cargo run -p alloy --example boundary_scale

use alloy::renderer::prelude::*;
use alloy::renderer::{Color, DisplayListBuilder, ISize};
use alloy::rendertree::composite::{paint_phase, render, PaintStats};
use alloy::rendertree::{
  AnimProp, AnimValue, BoundaryMode, Curve, Damage, ElementKind, FontPayload, OriginCoord, PlatformContext, Rectangle,
  RenderTree, Text, TransitionConfig, TransitionSpec, View, Window,
};
use std::borrow::Cow;
use taffy::prelude::{length, Size, Style};
use taffy::style::{FlexDirection, FlexWrap, Position};

const W: u32 = 400;
const H: u32 = 300;
const DISPLAY_SCALES: [f32; 2] = [1.0, 1.5];

// The written scale: REST_SCALE * 2^(SWEEP_OCTAVES * sin(SWEEP_RATE * t)),
// one write per frame at FRAME_HZ, for one period of the sine. A sweep of
// one octave either way passes the motion bound a few times per period.
const REST_SCALE: f32 = 0.5;
const SWEEP_OCTAVES: f64 = 1.0;
// Radians of the sine per second.
const SWEEP_RATE: f64 = 2.0;
const FRAME_HZ: f64 = 60.0;
// Frames at rest before the motion starts.
const REST_FRAMES: u32 = 5;
// Frames painted after the last write.
const HOLD_FRAMES: u32 = 8;

// An exponential ease of the written scale, s += (target - s) * EASE_K per
// frame, from REST_SCALE to EASE_TO times it, for EASE_FRAMES frames: long
// past the frame its steps fall below a held step and the f32 fixed point.
const EASE_K: f32 = 0.2;
const EASE_TO: f32 = 3.0;
const EASE_FRAMES: u32 = 200;
// A slow drift of the written scale, s *= 1 + DRIFT per frame, for
// DRIFT_FRAMES frames: each step a fifth of a held step (the engine's
// SCALE_HELD_EPSILON).
const DRIFT: f32 = 2e-6;
const DRIFT_FRAMES: u32 = 300;
// A one-shot write: the scale jumps from REST_SCALE to ONE_SHOT times it in
// one frame, past the motion bound, and holds. Not to the display scale,
// where a raster is hinted anyway.
const ONE_SHOT: f32 = 1.8;

// The engine's rules this example checks, restated (the constants are
// private to alloy's text module): the ratio between the composite scale
// and the raster's scale past which a raster is re-made in flight
// (LAYER_SCALE_MOTION_BOUND), the looks a scale must hold to be at rest
// (SCALE_REST_LOOKS), and the fraction within which two scales are the
// same scale (SCALE_HELD_EPSILON).
const MOTION_BOUND: f64 = 1.5;
const REST_LOOKS: u32 = 3;
const SCALE_HELD_EPSILON: f64 = 1e-5;
// A ratio this close to the motion bound is too close to call from here:
// the engine composes the scale from matrices, the example multiplies.
const BOUND_AMBIGUITY: f64 = 1e-4;
// Two scales whose difference is this close to a held step (as a fraction
// of the scale) are too close to call from here, for the same reason.
const HELD_AMBIGUITY: f64 = 1e-6;
// The earliest moving frame of the content change inside a tile.
const CONTENT_CHANGE_FRAME: u32 = 10;

// The tiles scene: two tiles at fractional places, cells inside.
const TILE_W: f32 = 150.3;
const TILE_H: f32 = 100.7;
const TILE_AT: [(f32, f32); 2] = [(10.4, 20.6), (180.7, 30.2)];
const CELL_W: f32 = 30.0;
const CELL_H: f32 = 20.0;
const CELL_GAP: f32 = 6.0;
const CELLS: u64 = 6;

// The text tiles: a snapshot boundary each, at a fractional place, with a
// label inside.
const TEXT_TILE_W: f32 = 150.3;
const TEXT_TILE_H: f32 = 40.7;
const TEXT_TILE_AT: [(f32, f32); 2] = [(10.4, 20.6), (180.7, 30.2)];
const TEXT_LABEL: &str = "Snapshot text";
const TEXT_SIZE: f32 = 18.0;
const NOTO_SANS: &[u8] = include_bytes!("../assets/fonts/NotoSans.ttf");

// The pair scene, in device px at the resting scale (REST_SCALE times the
// display scale): A's left edge, A's width and B's width, A's top edge,
// A's height and C's height, once per way a box snapped at rest can miss
// its layout box. In the first, A's left and top edges are a fraction above
// a half and its right and bottom edges a fraction below one, so A's box
// snapped at rest is most of a device px narrower and shorter than its
// layout box; in the second the other way round. Once the rounding is off,
// a quad drawn at a cached box leaves a gap or an overlap, and a texture
// laid out for one box and stretched onto another misses the edge.
struct PairDevice {
  name: &'static str,
  left: f32,
  a: f32,
  b: f32,
  top: f32,
  h: f32,
  c: f32,
}
const PAIRS_DEVICE: [PairDevice; 2] = [
  PairDevice { name: "A snapped narrower", left: 20.6, a: 60.8, b: 50.4, top: 20.6, h: 40.8, c: 30.4 },
  PairDevice { name: "A snapped wider", left: 20.4, a: 60.2, b: 50.4, top: 20.4, h: 40.2, c: 30.4 },
];
// The native tracks: the container zooms from REST_SCALE up past the
// motion bound (a raster for the end at once, minified in flight), and
// down within it (the end too, magnified in flight).
const TRACKS: [(&str, f32); 2] = [("up", 0.9), ("down", 0.35)];
const TRACK: TransitionSpec = TransitionSpec::Tween { duration_ms: 400.0, curve: Curve::Linear };
// Frames painted once the track has ended.
const SETTLE_FRAMES: u32 = 5;
// Device pixels read on each side of a shared edge.
const EDGE_SPAN: u32 = 2;
// How far a channel of a solid color's readback may be off.
const CHANNEL_TOLERANCE: i32 = 2;
// A device edge this close to a half is too close to call which pixel it
// rounds to.
const EDGE_AMBIGUITY: f64 = 1e-3;
// Failures printed per case; the rest are counted.
const FAILURES_SHOWN: usize = 8;

const ROOT: u64 = 1;
const CONTAINER: u64 = 2;
const BACKGROUND: u64 = 3;
const PAIR_A: u64 = 4;
const PAIR_A_FILL: u64 = 5;
const PAIR_B: u64 = 6;
const PAIR_B_FILL: u64 = 7;
const PAIR_C: u64 = 8;
const PAIR_C_FILL: u64 = 9;
const PAIR_BOUNDARIES: u32 = 3;
// Tile i takes TILE_ID_STRIDE ids from tile_id(i): the tile view, its
// boundary, then a view and a rect per cell; text tile i its boundary and
// its label.
const TILE_ID_BASE: u64 = 100;
const TILE_ID_STRIDE: u64 = 100;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const GREY: Color = Color::new_srgba(0.5, 0.5, 0.5, 1.0);
const WHITE: Color = Color::new_srgba(1.0, 1.0, 1.0, 1.0);

fn rgba(px: [u8; 4]) -> Color {
  let unit = |c: u8| f32::from(c) / f32::from(u8::MAX);
  Color::new_srgba(unit(px[0]), unit(px[1]), unit(px[2]), unit(px[3]))
}

fn tile_id(i: u64) -> u64 {
  TILE_ID_BASE + i * TILE_ID_STRIDE
}

fn tile_boundary_id(i: u64) -> u64 {
  tile_id(i) + 1
}

fn cell_id(i: u64, c: u64) -> u64 {
  tile_id(i) + 2 + 2 * c
}

fn cell_rect_id(i: u64, c: u64) -> u64 {
  cell_id(i, c) + 1
}

fn text_label_id(i: u64) -> u64 {
  tile_boundary_id(i) + 1
}

fn text_recording_id(i: u64) -> u64 {
  tile_boundary_id(i) + 2
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

  // Report the case; its failures join `all`.
  fn finish(self, all: &mut Vec<String>) {
    if self.failures.is_empty() {
      println!("ok: {}", self.name);
      return;
    }
    println!("FAIL: {} ({} failed checks)", self.name, self.failures.len());
    for failure in self.failures.iter().take(FAILURES_SHOWN) {
      println!("  {failure}");
    }
    if self.failures.len() > FAILURES_SHOWN {
      println!("  ... and {} more", self.failures.len() - FAILURES_SHOWN);
    }
    all.extend(self.failures.into_iter().map(|f| format!("{}: {f}", self.name)));
  }
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

fn placed(style: &mut Style, left: f32, top: f32) {
  style.position = Position::Absolute;
  style.inset.left = length(left);
  style.inset.top = length(top);
}

fn write_color(tree: &mut RenderTree, id: u64, color: Color) {
  tree.edit(id, |el| el.kind.paint_mut().expect("paintable kind").set_color(Some(color)));
}

fn boundary(tree: &mut RenderTree, id: u64) {
  tree.edit(id, |el| {
    el.repaint_boundary = BoundaryMode::Snapshot;
    Damage::Paint
  });
}

// The container's scale as an app writes it: a Compose damage, no track.
fn write_scale(tree: &mut RenderTree, scale: f32) {
  tree.edit(CONTAINER, |el| match &mut el.kind {
    ElementKind::View(v) => {
      v.set_scale_x(Some(scale));
      v.set_scale_y(Some(scale))
    }
    _ => panic!("the container is a view"),
  });
}

// `pixelSnap={false}` on the container: its subtree is off the device grid
// at rest too, so the boxes are layout boxes and the snapshots' grid is
// unrounded.
fn unround(tree: &mut RenderTree) {
  tree.edit(CONTAINER, |el| {
    el.pixel_snap_off = true;
    Damage::Paint
  });
}

// The case-name suffix of a run with the rounding off.
fn rounding(snap_off: bool) -> &'static str {
  if snap_off {
    " under pixelSnap={false}"
  } else {
    ""
  }
}

fn container_scale(tree: &RenderTree) -> f32 {
  match &tree.try_node(CONTAINER).expect("the container is in the tree").kind {
    ElementKind::View(v) => v.scale_x.unwrap_or(1.0),
    _ => panic!("the container is a view"),
  }
}

// root > container: absolute at the origin, the window's size, scaled about
// its top-left corner from REST_SCALE.
fn build_root() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, Window::default().with_layout());
  tree.root = Some(ROOT);
  tree.create_node(CONTAINER, View::default().with_layout());
  tree.insert_node(ROOT, CONTAINER, None).expect("insert");
  styled(&mut tree, CONTAINER, |s| {
    placed(s, 0.0, 0.0);
    sized(s, W as f32, H as f32);
  });
  tree.edit(CONTAINER, |el| match &mut el.kind {
    ElementKind::View(v) => {
      v.set_origin_x(Some(OriginCoord::Px(0.0)));
      v.set_origin_y(Some(OriginCoord::Px(0.0)))
    }
    _ => panic!("the container is a view"),
  });
  write_scale(&mut tree, REST_SCALE);
  tree
}

fn build_tiles() -> RenderTree {
  let mut tree = build_root();
  for (i, &(left, top)) in (0u64..).zip(TILE_AT.iter()) {
    tree.create_node(tile_id(i), View::default().with_layout());
    tree.create_node(tile_boundary_id(i), View::default().with_layout());
    tree.insert_node(CONTAINER, tile_id(i), None).expect("insert");
    tree.insert_node(tile_id(i), tile_boundary_id(i), None).expect("insert");
    styled(&mut tree, tile_id(i), |s| {
      placed(s, left, top);
      sized(s, TILE_W, TILE_H);
    });
    styled(&mut tree, tile_boundary_id(i), |s| {
      sized(s, TILE_W, TILE_H);
      s.flex_direction = FlexDirection::Row;
      s.flex_wrap = FlexWrap::Wrap;
      s.gap = Size { width: length(CELL_GAP), height: length(CELL_GAP) };
    });
    for c in 0..CELLS {
      tree.create_node(cell_id(i, c), View::default().with_layout());
      tree.create_node(cell_rect_id(i, c), Rectangle::default().no_layout());
      tree.insert_node(tile_boundary_id(i), cell_id(i, c), None).expect("insert");
      tree.insert_node(cell_id(i, c), cell_rect_id(i, c), None).expect("insert");
      styled(&mut tree, cell_id(i, c), |s| sized(s, CELL_W, CELL_H));
      write_color(&mut tree, cell_rect_id(i, c), GREY);
    }
    boundary(&mut tree, tile_boundary_id(i));
  }
  tree
}

// The text tiles; `nested` puts a Recording boundary between each
// snapshot and its label.
fn build_text_tiles(nested: bool) -> RenderTree {
  let mut tree = build_root();
  for (i, &(left, top)) in (0u64..).zip(TEXT_TILE_AT.iter()) {
    let (tile, label) = (tile_boundary_id(i), text_label_id(i));
    tree.create_node(tile, View::default().with_layout());
    let mut text = Text::default();
    text.set_plain_text(TEXT_LABEL.to_string());
    tree.create_node(label, text.with_layout());
    tree.insert_node(CONTAINER, tile, None).expect("insert");
    let holder = if nested {
      let recording = text_recording_id(i);
      tree.create_node(recording, View::default().with_layout());
      tree.insert_node(tile, recording, None).expect("insert");
      styled(&mut tree, recording, |s| sized(s, TEXT_TILE_W, TEXT_TILE_H));
      tree.edit(recording, |el| {
        el.repaint_boundary = BoundaryMode::Recording;
        Damage::Paint
      });
      recording
    } else {
      tile
    };
    tree.insert_node(holder, label, None).expect("insert");
    tree.edit(label, |el| match &mut el.kind {
      ElementKind::Text(t) => t.set_font_size(Some(TEXT_SIZE)),
      _ => panic!("the label is a text"),
    });
    styled(&mut tree, tile, |s| {
      placed(s, left, top);
      sized(s, TEXT_TILE_W, TEXT_TILE_H);
    });
    boundary(&mut tree, tile);
  }
  tree
}

// A pair's geometry in logical px of the container, for a display scale.
struct Pair {
  name: &'static str,
  left: f32,
  a: f32,
  b: f32,
  top: f32,
  h: f32,
  c: f32,
}

impl Pair {
  // The pairs of PAIRS_DEVICE at a display scale.
  fn all(display_scale: f32) -> Vec<Self> {
    let device = REST_SCALE * display_scale;
    let pair = |d: &PairDevice| Pair {
      name: d.name,
      left: d.left / device,
      a: d.a / device,
      b: d.b / device,
      top: d.top / device,
      h: d.h / device,
      c: d.c / device,
    };
    PAIRS_DEVICE.iter().map(pair).collect()
  }

  // Where B starts, and A ends: the vertical layout edge they share.
  fn shared_x(&self) -> f32 {
    self.left + self.a
  }

  // Where C starts, and A ends: the horizontal layout edge they share.
  fn shared_y(&self) -> f32 {
    self.top + self.h
  }

  // The three boundaries' layout boxes: (left, top, width, height).
  fn boxes(&self) -> [(f32, f32, f32, f32); 3] {
    [
      (self.left, self.top, self.a, self.h),
      (self.shared_x(), self.top, self.b, self.h),
      (self.left, self.shared_y(), self.a, self.c),
    ]
  }

  // The row through A and B, and the column through A and C.
  fn lines(&self) -> [Line; 2] {
    [
      Line {
        axis: Axis::Row,
        across: self.top + self.h / 2.0,
        start: self.left,
        shared: self.shared_x(),
        size: self.b,
      },
      Line {
        axis: Axis::Column,
        across: self.left + self.a / 2.0,
        start: self.top,
        shared: self.shared_y(),
        size: self.c,
      },
    ]
  }
}

fn build_pair(pair: &Pair) -> RenderTree {
  let mut tree = build_root();
  tree.create_node(BACKGROUND, Rectangle::default().no_layout());
  tree.insert_node(ROOT, BACKGROUND, Some(CONTAINER)).expect("insert");
  write_color(&mut tree, BACKGROUND, rgba(GREEN));
  let [a, b, c] = pair.boxes();
  let sides = [(PAIR_A, PAIR_A_FILL, a, RED), (PAIR_B, PAIR_B_FILL, b, BLUE), (PAIR_C, PAIR_C_FILL, c, BLUE)];
  for (id, fill, (left, top, w, h), color) in sides {
    tree.create_node(id, View::default().with_layout());
    tree.create_node(fill, Rectangle::default().no_layout());
    tree.insert_node(CONTAINER, id, None).expect("insert");
    tree.insert_node(id, fill, None).expect("insert");
    styled(&mut tree, id, |s| {
      placed(s, left, top);
      sized(s, w, h);
    });
    write_color(&mut tree, fill, rgba(color));
    boundary(&mut tree, id);
  }
  tree
}

// Rewrite the pair's fills as they are: a content change inside all three
// boundaries.
fn touch_pair(tree: &mut RenderTree) {
  for (fill, color) in [(PAIR_A_FILL, RED), (PAIR_B_FILL, BLUE), (PAIR_C_FILL, BLUE)] {
    write_color(tree, fill, rgba(color));
  }
}

// A frame as `composite::render` records it, under the display scale.
fn paint(tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> PaintStats {
  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, ctx);
  builder.build().expect("frame display list");
  stats
}

// A frame as the runtime produces it (`composite::render`): the frame's
// GPU content changes and the looks noted last frame land first, a scale
// wait as a Compose damage on its node, then the paint, and the frame goes
// to the window. The text case runs on it: a text's wait is a damage that
// reaches the enclosing snapshot.
struct Producer {
  frame: u64,
}

impl Producer {
  fn produce(&mut self, tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> PaintStats {
    self.frame += 1;
    render(tree, platform, ctx, self.frame)
  }
}

// The frame's pixels: (rgba, width in device px, stats).
fn paint_read(tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> (Vec<u8>, u32, PaintStats) {
  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, ctx);
  let dl = builder.build().expect("frame display list");
  let (w, h) = ((W as f32 * scale).round() as u32, (H as f32 * scale).round() as u32);
  let texture = ctx.render_display_list_to_texture(&dl, w, h, true).expect("frame rasterization");
  let pixels = ctx.read_texture(&texture, w, h).expect("texture readback");
  (pixels, w, stats)
}

fn platform_with(fonts: Vec<FontPayload>, display_scale: f32) -> PlatformContext {
  let platform = PlatformContext::new(fonts);
  platform.set_window_size(W as f32, H as f32);
  platform.set_display_scale(display_scale);
  platform
}

fn platform_at(display_scale: f32) -> PlatformContext {
  platform_with(Vec::new(), display_scale)
}

// The shipped Noto Sans as "sans", the family a text resolves by default.
fn text_platform_at(display_scale: f32) -> PlatformContext {
  platform_with(vec![FontPayload { alias: Some("sans".to_string()), bytes: Cow::Borrowed(NOTO_SANS) }], display_scale)
}

// A frame's snapshot counters: (reused, rerendered, rasterized).
fn counts(stats: &PaintStats) -> (u32, u32, u32) {
  (stats.snapshots_reused, stats.snapshots_rerendered, stats.snapshots_rasterized)
}

fn written_scale(frame: u32) -> f32 {
  let t = f64::from(frame) / FRAME_HZ;
  (f64::from(REST_SCALE) * (SWEEP_OCTAVES * (SWEEP_RATE * t).sin()).exp2()) as f32
}

// One period of the sine, in frames.
fn moving_frames() -> u32 {
  (std::f64::consts::TAU / SWEEP_RATE * FRAME_HZ).ceil() as u32
}

// The written scale of one sine period.
fn sweep() -> Vec<f32> {
  (1..=moving_frames()).map(written_scale).collect()
}

// The written scale of an exponential ease from REST_SCALE to EASE_TO
// times it.
fn ease() -> Vec<f32> {
  let target = REST_SCALE * EASE_TO;
  let mut s = REST_SCALE;
  (0..EASE_FRAMES)
    .map(|_| {
      s += (target - s) * EASE_K;
      s
    })
    .collect()
}

// The written scale of a slow drift up from REST_SCALE.
fn drift() -> Vec<f32> {
  let mut s = REST_SCALE;
  (0..DRIFT_FRAMES)
    .map(|_| {
      s *= 1.0 + DRIFT;
      s
    })
    .collect()
}

fn ratio(a: f64, b: f64) -> f64 {
  (a / b).max(b / a)
}

// What the motion bound says of the container's `scale` against the scale
// the rasters were made at (`have`; the display scale cancels in the
// ratio): the ratio, whether it passes the bound, and whether it is too
// close to the bound to call.
fn bound(scale: f32, have: f64) -> (f64, bool, bool) {
  let r = ratio(f64::from(scale), have);
  (r, r > MOTION_BOUND, (r - MOTION_BOUND).abs() < BOUND_AMBIGUITY)
}

// Whether a scale `now` is the same scale as `then` (text::scale_held), and
// whether that is too close to call from here.
fn held(then: f64, now: f64) -> (bool, bool) {
  let (gap, step) = ((then - now).abs(), now.abs() * SCALE_HELD_EPSILON);
  (gap <= step, (gap - step).abs() <= now.abs() * HELD_AMBIGUITY)
}

// The engine's density rule for a scale written with no track
// (text::raster_density and text::scale_observed), run beside the engine
// on the container's scale (the display scale cancels): whether a frame
// re-rasterizes, past the motion bound or at rest at a scale other than
// the raster's. It is told what the engine did, so a call made the other
// way near a threshold does not fail every frame after it.
struct Rule {
  // The scale the rasters were made at.
  have: f64,
  // The last frame's scale, and how many looks it has held for.
  last: f64,
  looks: u32,
  // Frames left in which rest is too close to call.
  unsure: u32,
}

impl Rule {
  // At rest at `scale`, the raster made there.
  fn at_rest(scale: f32) -> Self {
    Rule { have: f64::from(scale), last: f64::from(scale), looks: REST_LOOKS, unsure: 0 }
  }

  // A frame at `scale`: (whether it re-rasterizes, whether it is at rest,
  // whether either is too close to call).
  fn look(&mut self, scale: f32) -> (bool, bool, bool) {
    let now = f64::from(scale);
    let (step_held, step_close) = held(self.last, now);
    self.looks = if step_held { self.looks + 1 } else { 0 };
    self.last = now;
    self.unsure = if step_close { REST_LOOKS + 1 } else { self.unsure.saturating_sub(1) };
    let at_rest = self.looks >= REST_LOOKS;
    let (_, past, bound_close) = bound(scale, self.have);
    let (kept, kept_close) = held(self.have, now);
    (past || (at_rest && !kept), at_rest, bound_close || kept_close || self.unsure > 0)
  }

  fn rastered(&mut self, scale: f32) {
    self.have = f64::from(scale);
  }
}

// How many frames showed each counter triple, for the summary.
fn tally(seen: &mut Vec<((u32, u32, u32), u32)>, got: (u32, u32, u32)) {
  match seen.iter_mut().find(|(c, _)| *c == got) {
    Some((_, n)) => *n += 1,
    None => seen.push((got, 1)),
  }
}

fn tally_text(seen: &[((u32, u32, u32), u32)]) -> String {
  let parts: Vec<String> =
    seen.iter().map(|((r, e, a), n)| format!("{n}x reused {r} rerendered {e} rasterized {a}")).collect();
  parts.join(", ")
}

fn tiles_case(ctx: &alloy::Context, display_scale: f32, snap_off: bool, all: &mut Vec<String>) {
  let platform = platform_at(display_scale);
  let mut tree = build_tiles();
  if snap_off {
    unround(&mut tree);
  }
  let off = rounding(snap_off);
  let tiles = TILE_AT.len() as u32;
  let reuse_all = (tiles, 0, 0);
  let raster_all = (0, 0, tiles);

  let mut rest = Case::new(format!("tiles at rest are rasterized once, then reused{off}, at {display_scale}x"));
  let first = counts(&paint(&mut tree, &platform, ctx));
  rest.expect(first == raster_all, || format!("first frame: {first:?}, expected {raster_all:?}"));
  for frame in 1..REST_FRAMES {
    let got = counts(&paint(&mut tree, &platform, ctx));
    rest.expect(got == reuse_all, || format!("rest frame {frame}: {got:?}, expected {reuse_all:?}"));
  }
  rest.finish(all);

  // The motion: the scale written once per frame. The rasters' scale is
  // the container's (the display scale cancels in the ratio).
  let mut moving = Case::new(format!(
    "tiles under a scale written per frame are reused, re-rasterized only past the motion bound{off}, at {display_scale}x"
  ));
  let mut change =
    Case::new(format!("a content change under the written scale re-renders in place{off}, at {display_scale}x"));
  let mut have = f64::from(REST_SCALE);
  let mut crossings = 0;
  let mut changed_at = None;
  let mut seen = Vec::new();
  let mut last = REST_SCALE;
  for frame in 1..=moving_frames() {
    let scale = written_scale(frame);
    last = scale;
    write_scale(&mut tree, scale);
    let (r, crosses, ambiguous) = bound(scale, have);
    let content = changed_at.is_none() && frame >= CONTENT_CHANGE_FRAME && !crosses && !ambiguous;
    if content {
      write_color(&mut tree, cell_rect_id(0, 0), WHITE);
      changed_at = Some(frame);
    }
    let got = counts(&paint(&mut tree, &platform, ctx));
    tally(&mut seen, got);
    if content {
      let want = (tiles - 1, 1, 0);
      change.expect(got == want, || format!("frame {frame} (scale {scale}): {got:?}, expected {want:?}"));
    } else if ambiguous {
      moving.expect(got == reuse_all || got == raster_all, || {
        format!("frame {frame} (ratio {r:.5}, at the bound): {got:?}, expected {reuse_all:?} or {raster_all:?}")
      });
    } else {
      let want = if crosses { raster_all } else { reuse_all };
      moving.expect(got == want, || format!("frame {frame} (scale {scale}, ratio {r:.4}): {got:?}, expected {want:?}"));
    }
    if crosses {
      crossings += 1;
    }
    // Follow the engine's raster scale from what it did, so one miss does
    // not fail every frame after it.
    if got.2 > 0 {
      have = f64::from(scale);
    }
  }
  moving.expect(crossings > 0, || "the sweep never passed the motion bound".to_string());
  change.expect(changed_at.is_some(), || "no frame took the content change".to_string());
  println!("  moving frames{off} at {display_scale}x: {}; {crossings} crossings predicted", tally_text(&seen));
  moving.finish(all);
  change.finish(all);

  // The scale holds: a look per frame, the rest looks, one re-raster for
  // the resting scale (unless the last crossing made the raster there),
  // then reuse.
  let mut hold = Case::new(format!("tiles re-rasterize once when the written scale holds{off}, at {display_scale}x"));
  let (kept, _) = held(have, f64::from(last));
  let mut seen = Vec::new();
  for look in 1..=HOLD_FRAMES {
    let got = counts(&paint(&mut tree, &platform, ctx));
    tally(&mut seen, got);
    let want = if look == REST_LOOKS && !kept { raster_all } else { reuse_all };
    hold.expect(got == want, || format!("look {look}: {got:?}, expected {want:?}"));
  }
  println!("  hold frames{off} at {display_scale}x: {}", tally_text(&seen));
  hold.finish(all);
}

// The tiles under a written scale `scales`, then held for HOLD_FRAMES:
// each frame rasterizes both tiles exactly where the density rule says
// (Rule), and reuses both otherwise; never a re-render, never a raster per
// frame. Returns the frames that rasterized.
fn rule_case(ctx: &alloy::Context, display_scale: f32, motion: &str, scales: &[f32], all: &mut Vec<String>) {
  let platform = platform_at(display_scale);
  let mut tree = build_tiles();
  let tiles = TILE_AT.len() as u32;
  let (reuse_all, raster_all) = ((tiles, 0, 0), (0, 0, tiles));
  let mut case =
    Case::new(format!("tiles under {motion} rasterize only where the density rule says, at {display_scale}x"));
  for _ in 0..REST_FRAMES {
    paint(&mut tree, &platform, ctx);
  }
  let mut rule = Rule::at_rest(REST_SCALE);
  let held_frames = std::iter::repeat_n(*scales.last().expect("a motion"), HOLD_FRAMES as usize);
  let (mut past, mut rest) = (0, 0);
  let mut seen = Vec::new();
  for (frame, scale) in (1u32..).zip(scales.iter().copied().chain(held_frames)) {
    write_scale(&mut tree, scale);
    let (rasters, at_rest, close) = rule.look(scale);
    let got = counts(&paint(&mut tree, &platform, ctx));
    tally(&mut seen, got);
    let want = if rasters { raster_all } else { reuse_all };
    let ok = got == want || (close && (got == reuse_all || got == raster_all));
    case.expect(ok, || format!("frame {frame} (scale {scale}): {got:?}, expected {want:?}"));
    if got.2 > 0 {
      rule.rastered(scale);
      if at_rest {
        rest += 1;
      } else {
        past += 1;
      }
    }
  }
  println!(
    "  {motion} at {display_scale}x: {} ({past} past the bound, {rest} at rest, over {} frames)",
    tally_text(&seen),
    scales.len() as u32 + HOLD_FRAMES
  );
  case.finish(all);
}

// The text tiles under the written sweep and its hold, and under a one-shot
// write and its hold, with the labels straight in the snapshots or in a
// Recording boundary inside each (`nested`). A raster made mid-motion (a
// crossing) holds unhinted text: it is reused after the crossing, never
// re-rendered while the scale moves, and made again once for rest, a raster
// where the density re-makes it, a re-render in place where the density is
// kept. A nested recording is re-made exactly when its snapshot is, the
// re-render for rest included, never replayed with its unhinted text. The
// text ends hinted: the frame at rest is the frame of a tree made at that
// scale from the start, and differs from the frame the mid-motion raster
// drew at the same density.
fn text_case(ctx: &alloy::Context, display_scale: f32, nested: bool, all: &mut Vec<String>) {
  let platform = text_platform_at(display_scale);
  let mut producer = Producer { frame: 0 };
  let tiles = TEXT_TILE_AT.len() as u32;
  let (reuse_all, raster_all, rerender_all) = ((tiles, 0, 0), (0, 0, tiles), (0, tiles, 0));
  let scene = if nested { "text tiles in a recording" } else { "text tiles" };
  // The nested recordings are re-made with every raster and re-render of
  // their snapshots, and with nothing else.
  let follows = |stats: &PaintStats| {
    !nested || stats.boundaries_recorded == stats.snapshots_rerendered + stats.snapshots_rasterized
  };
  // A frame read back from a tree made at `scale` from its first frame: its
  // texts are steady there, so hinted.
  let made_at = |scale: f32| {
    let mut fresh = build_text_tiles(nested);
    write_scale(&mut fresh, scale);
    paint_read(&mut fresh, &platform, ctx).0
  };

  let mut tree = build_text_tiles(nested);
  let mut sweep_case = Case::new(format!(
    "{scene} under the written sweep rasterize once per crossing and re-render nothing in flight, at {display_scale}x"
  ));
  let first = counts(&producer.produce(&mut tree, &platform, ctx));
  sweep_case.expect(first == raster_all, || format!("first frame: {first:?}, expected {raster_all:?}"));
  for frame in 1..REST_FRAMES {
    let got = counts(&producer.produce(&mut tree, &platform, ctx));
    sweep_case.expect(got == reuse_all, || format!("rest frame {frame}: {got:?}, expected {reuse_all:?}"));
  }
  let mut rule = Rule::at_rest(REST_SCALE);
  let mut crossings = 0;
  let mut seen = Vec::new();
  let scales = sweep();
  for (frame, &scale) in (1u32..).zip(scales.iter()) {
    write_scale(&mut tree, scale);
    let (rasters, _, close) = rule.look(scale);
    let stats = producer.produce(&mut tree, &platform, ctx);
    let got = counts(&stats);
    tally(&mut seen, got);
    let want = if rasters { raster_all } else { reuse_all };
    let ok = got == want || (close && (got == reuse_all || got == raster_all));
    sweep_case.expect(ok, || format!("frame {frame} (scale {scale}): {got:?}, expected {want:?}"));
    sweep_case.expect(follows(&stats), || format!("frame {frame}: {} recorded", stats.boundaries_recorded));
    if got.2 > 0 {
      rule.rastered(scale);
      crossings += 1;
    }
  }
  sweep_case.expect(crossings > 0, || "the sweep never passed the motion bound".to_string());
  println!("  {scene} sweep at {display_scale}x: {}; {crossings} crossings", tally_text(&seen));
  sweep_case.finish(all);

  // The hold: the rest looks, then one raster (the density re-makes it) or
  // one re-render in place (it keeps it) for the hinted text, then reuse.
  let mut hold =
    Case::new(format!("{scene} are made again once for rest, hinted, after the sweep, at {display_scale}x"));
  let last = *scales.last().expect("a sweep");
  let mut seen = Vec::new();
  for look in 1..=HOLD_FRAMES {
    let (rasters, _, _) = rule.look(last);
    let stats = producer.produce(&mut tree, &platform, ctx);
    let got = counts(&stats);
    tally(&mut seen, got);
    let want = match (look == REST_LOOKS, rasters) {
      (true, true) => raster_all,
      (true, false) => rerender_all,
      (false, _) => reuse_all,
    };
    hold.expect(got == want, || format!("look {look}: {got:?}, expected {want:?}"));
    hold.expect(follows(&stats), || format!("look {look}: {} recorded", stats.boundaries_recorded));
  }
  let (rested, _, stats) = paint_read(&mut tree, &platform, ctx);
  hold.expect(counts(&stats) == reuse_all, || format!("after the hold: {:?}", counts(&stats)));
  hold.expect(rested == made_at(last), || "the frame at rest is not the frame of a tree made there".to_string());
  println!("  {scene} hold at {display_scale}x: {}", tally_text(&seen));
  hold.finish(all);

  // A one-shot write past the bound, held: 1 raster (unhinted text, made
  // mid-motion) and 1 re-render in place at rest, at the same density, and
  // nothing after it.
  let mut tree = build_text_tiles(nested);
  let mut one_shot = Case::new(format!(
    "a one-shot scale write over {scene} costs one raster and one re-render at rest, hinted, at {display_scale}x"
  ));
  for _ in 0..REST_FRAMES {
    producer.produce(&mut tree, &platform, ctx);
  }
  let target = REST_SCALE * ONE_SHOT;
  write_scale(&mut tree, target);
  let mut seen = Vec::new();
  let (moving, _, stats) = paint_read(&mut tree, &platform, ctx);
  let got = counts(&stats);
  tally(&mut seen, got);
  one_shot.expect(got == raster_all, || format!("the write: {got:?}, expected {raster_all:?}"));
  one_shot.expect(follows(&stats), || format!("the write: {} recorded", stats.boundaries_recorded));
  for look in 1..=HOLD_FRAMES {
    let stats = producer.produce(&mut tree, &platform, ctx);
    let got = counts(&stats);
    tally(&mut seen, got);
    let want = if look == REST_LOOKS { rerender_all } else { reuse_all };
    one_shot.expect(got == want, || format!("look {look}: {got:?}, expected {want:?}"));
    one_shot.expect(follows(&stats), || format!("look {look}: {} recorded", stats.boundaries_recorded));
  }
  let (rested, _, stats) = paint_read(&mut tree, &platform, ctx);
  one_shot.expect(counts(&stats) == reuse_all, || format!("after the hold: {:?}", counts(&stats)));
  one_shot.expect(rested == made_at(target), || "the frame at rest is not the frame of a tree made there".to_string());
  one_shot.expect(rested != moving, || "the mid-motion frame is the frame at rest: no hinting told apart".to_string());
  println!("  {scene} one-shot at {display_scale}x: {}", tally_text(&seen));
  one_shot.finish(all);
}

fn pixel(pixels: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
  let i = ((y * w + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn near(px: [u8; 4], want: [u8; 4]) -> bool {
  px.iter().zip(want.iter()).all(|(&a, &b)| (i32::from(a) - i32::from(b)).abs() <= CHANNEL_TOLERANCE)
}

// Opaque, and nothing of the background in it: red, blue, or a mix of the
// two.
fn pair_colored(px: [u8; 4]) -> bool {
  i32::from(px[1]) <= CHANNEL_TOLERANCE && i32::from(px[3]) >= i32::from(GREEN[3]) - CHANNEL_TOLERANCE
}

#[derive(Clone, Copy, Debug)]
enum Axis {
  Row,
  Column,
}

// A line of the readback across a shared edge, in logical px of the
// container: along `axis` at `across` on the other axis, from `start`
// (red) over `shared` to `shared + size` (blue).
struct Line {
  axis: Axis,
  across: f32,
  start: f32,
  shared: f32,
  size: f32,
}

// Every pixel of the line from the red box's start to the blue box's end
// is red, blue or a mix of the two (no seam, and the blue box's far edge
// not short), the background shows right past the far edge (not long),
// and the colors change where the boxes meet. `rounded` says the edges are
// on whole device pixels (a rounded grid: the snapped boxes, which meet at
// the shared edge rounded) or not (a running track: the layout boxes,
// which meet at the fractional edge, whose pixel mixes the two). An edge
// too close to a half to call which pixel it rounds to is read a pixel
// inward.
fn check_line(case: &mut Case, pixels: &[u8], w: u32, line: &Line, device: f64, rounded: bool, what: &str) {
  let at = |x: f32| f64::from(x) * device;
  let callable = |v: f64| rounded && (v.fract() - 0.5).abs() >= EDGE_AMBIGUITY;
  let across = at(line.across) as u32;
  let read = |along: u32| match line.axis {
    Axis::Row => pixel(pixels, w, along, across),
    Axis::Column => pixel(pixels, w, across, along),
  };
  let axis = line.axis;
  let (start, end) = (at(line.start), at(line.shared + line.size));
  let from = if callable(start) { start.round() } else { start.ceil() } as u32;
  let to = if callable(end) { end.round() } else { end.floor() } as u32;
  for i in from..to {
    let px = read(i);
    case
      .expect(pair_colored(px), || format!("{what}: {axis:?} {across}: background at {i}: {px:?}, spans {from}..{to}"));
  }
  if !rounded || callable(end) {
    let past = if rounded { end.round() } else { end.ceil() } as u32;
    let px = read(past);
    case.expect(near(px, GREEN), || format!("{what}: {axis:?} {across}: {past} past the far edge {end:.3} is {px:?}"));
  }
  let edge = at(line.shared);
  if rounded && !callable(edge) {
    return;
  }
  let (last_red, first_blue) =
    if rounded { (edge.round() as u32, edge.round() as u32) } else { (edge.floor() as u32, edge.floor() as u32 + 1) };
  for i in last_red - EDGE_SPAN..last_red {
    let px = read(i);
    case
      .expect(near(px, RED), || format!("{what}: {axis:?} {across}: {i} before the edge {edge:.3} is {px:?}, not red"));
  }
  for i in first_blue..first_blue + EDGE_SPAN {
    let px = read(i);
    case.expect(near(px, BLUE), || {
      format!("{what}: {axis:?} {across}: {i} after the edge {edge:.3} is {px:?}, not blue")
    });
  }
}

fn check_pair(case: &mut Case, pixels: &[u8], w: u32, pair: &Pair, device: f64, rounded: bool, what: &str) {
  for line in pair.lines() {
    check_line(case, pixels, w, &line, device, rounded, what);
  }
}

fn pair_written_case(ctx: &alloy::Context, display_scale: f32, pair: &Pair, snap_off: bool, all: &mut Vec<String>) {
  let platform = platform_at(display_scale);
  let mut tree = build_pair(pair);
  if snap_off {
    unround(&mut tree);
  }
  // Under pixelSnap={false} the boxes are fractional layout boxes, mapped
  // 1:1, and a seam between them is what that prop asks for: there only
  // the counters are asserted.
  let seams = !snap_off;
  let what_of = if seams { "meet without a seam" } else { "keep their counters" };
  let mut case = Case::new(format!(
    "touching snapshots ({}) {what_of} under a written scale, a content change in flight too{}, at {display_scale}x",
    pair.name,
    rounding(snap_off)
  ));
  let device = |scale: f32| f64::from(scale) * f64::from(display_scale);
  for frame in 0..REST_FRAMES {
    let (pixels, w, _) = paint_read(&mut tree, &platform, ctx);
    if seams {
      check_pair(&mut case, &pixels, w, pair, device(REST_SCALE), true, &format!("rest frame {frame}"));
    }
    let corner = pixel(&pixels, w, 0, 0);
    case.expect(near(corner, GREEN), || format!("rest frame {frame}: the background is {corner:?}, not green"));
  }
  // The motion, with one content change in flight inside the boundaries:
  // the in-place re-render holds the cached box while stretched, laid out
  // at that box, so the lines stay seamless from that frame on too.
  let mut have = f64::from(REST_SCALE);
  let mut changed_at = None;
  let mut seen = Vec::new();
  for frame in 1..=moving_frames() {
    let scale = written_scale(frame);
    write_scale(&mut tree, scale);
    let (_, crosses, ambiguous) = bound(scale, have);
    let content = changed_at.is_none() && frame >= CONTENT_CHANGE_FRAME && !crosses && !ambiguous;
    if content {
      touch_pair(&mut tree);
      changed_at = Some(frame);
    }
    let (pixels, w, stats) = paint_read(&mut tree, &platform, ctx);
    let got = counts(&stats);
    tally(&mut seen, got);
    if content {
      let want = (0, PAIR_BOUNDARIES, 0);
      case.expect(got == want, || format!("content change at frame {frame}: {got:?}, expected {want:?}"));
    }
    if got.2 > 0 {
      have = f64::from(scale);
    }
    if seams {
      check_pair(&mut case, &pixels, w, pair, device(scale), true, &format!("frame {frame} (scale {scale})"));
    }
  }
  case.expect(changed_at.is_some(), || "no frame took the content change".to_string());
  println!("  pair ({}) moving frames{} at {display_scale}x: {}", pair.name, rounding(snap_off), tally_text(&seen));
  case.finish(all);
}

// Which of the pair's boundaries rasterize again on the frame a track to
// `end` settles: the raster made for the track's end holds each layout box
// in whole texels of the end's density, and the settled frame keeps it
// where its snapped box takes the same texels. (fewest, most), the
// boundaries too close to call counted in the most only.
fn settle_rasters(pair: &Pair, end: f32, display_scale: f32) -> (u32, u32) {
  let device = f64::from(end) * f64::from(display_scale);
  let close = |v: f64| (v.fract() - 0.5).abs() < EDGE_AMBIGUITY;
  let (mut fewest, mut most) = (0, 0);
  for (x, y, w, h) in pair.boxes() {
    let mut differs = false;
    let mut unsure = false;
    for (at, size) in [(x, w), (y, h)] {
      let (near, far, made) = (f64::from(at) * device, f64::from(at + size) * device, f64::from(size) * device);
      unsure |= close(near) || close(far) || close(made);
      differs |= far.round() - near.round() != made.round();
    }
    if differs {
      fewest += u32::from(!unsure);
      most += 1;
    } else if unsure {
      most += 1;
    }
  }
  (fewest, most)
}

fn pair_track_case(
  ctx: &alloy::Context,
  display_scale: f32,
  pair: &Pair,
  (direction, end): (&str, f32),
  all: &mut Vec<String>,
) {
  let platform = platform_at(display_scale);
  let mut tree = build_pair(pair);
  tree.edit(CONTAINER, |el| {
    el.transitions = Some(Box::new(TransitionConfig { all: Some(TRACK.into()), ..Default::default() }));
    Damage::None
  });
  let mut case = Case::new(format!(
    "touching snapshots ({}) meet without a seam under a scale track {direction} at {display_scale}x",
    pair.name
  ));
  let device = |scale: f32| f64::from(scale) * f64::from(display_scale);
  let frame_ms = 1000.0 / FRAME_HZ;
  let mut now_ms = 0.0;
  let mut step = |tree: &mut RenderTree| {
    now_ms += frame_ms;
    tree.set_transition_now(now_ms);
    tree.advance_transitions();
    paint_read(tree, &platform, ctx)
  };
  for frame in 0..REST_FRAMES {
    let (pixels, w, _) = step(&mut tree);
    check_pair(&mut case, &pixels, w, pair, device(REST_SCALE), true, &format!("rest frame {frame}"));
  }
  let started = tree.transition_write(CONTAINER, AnimProp::Scale, Some(AnimValue::Scalar(end)));
  case.expect(started, || "the scale track did not start".to_string());
  // The pair is rasterized once for the track's end on its first frame and
  // reused on every later one; on the settled frame it is rasterized again
  // only where its snapped box takes other texels than the box made for
  // the end.
  let mut rasters = Case::new(format!(
    "a scale track {direction} rasterizes the pair ({}) once at its start, again at rest only for other texels, at {display_scale}x",
    pair.name
  ));
  let (reuse_all, raster_all) = ((PAIR_BOUNDARIES, 0, 0), (0, 0, PAIR_BOUNDARIES));
  let (fewest, most) = settle_rasters(pair, end, display_scale);
  let mut track_frames = 0;
  let mut settled = 0;
  let mut seen = Vec::new();
  while settled < SETTLE_FRAMES {
    let (pixels, w, stats) = step(&mut tree);
    let got = counts(&stats);
    tally(&mut seen, got);
    let scale = container_scale(&tree);
    let running = tree.unrounded(CONTAINER);
    if running {
      track_frames += 1;
      let want = if track_frames == 1 { raster_all } else { reuse_all };
      rasters.expect(got == want, || format!("track frame {track_frames} (scale {scale}): {got:?}, expected {want:?}"));
    } else {
      settled += 1;
      let ok = if settled == 1 {
        got.1 == 0 && got.0 + got.2 == PAIR_BOUNDARIES && (fewest..=most).contains(&got.2)
      } else {
        got == reuse_all
      };
      rasters.expect(ok, || format!("settled frame {settled}: {got:?}, expected {fewest}..={most} rasterized"));
    }
    let what = format!("{} (scale {scale})", if running { "track frame" } else { "settled frame" });
    check_pair(&mut case, &pixels, w, pair, device(scale), !running, &what);
  }
  case.expect(track_frames > 0, || "the track never ran".to_string());
  println!(
    "  pair ({}) track {direction} frames at {display_scale}x ({track_frames} running, {fewest}..={most} at rest): {}",
    pair.name,
    tally_text(&seen)
  );
  case.finish(all);
  rasters.finish(all);
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "boundary scale", ISize::new(600, 450), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let mut all = Vec::new();
    for display_scale in DISPLAY_SCALES {
      for snap_off in [false, true] {
        tiles_case(&ctx, display_scale, snap_off, &mut all);
        for pair in Pair::all(display_scale) {
          pair_written_case(&ctx, display_scale, &pair, snap_off, &mut all);
        }
      }
      rule_case(&ctx, display_scale, "an exponential ease", &ease(), &mut all);
      rule_case(&ctx, display_scale, "a drift below a held step", &drift(), &mut all);
      for nested in [false, true] {
        text_case(&ctx, display_scale, nested, &mut all);
      }
      for pair in Pair::all(display_scale) {
        for track in TRACKS {
          pair_track_case(&ctx, display_scale, &pair, track, &mut all);
        }
      }
    }
    assert!(all.is_empty(), "{} boundary scale checks failed", all.len());
    println!("all boundary scale checks passed");
  });
}
