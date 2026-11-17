//! Validates a snapshot boundary switched off and on again
//! (Element::set_repaint_boundary): the mode write is a Compose damage, so
//! the node's own snapshot survives the switch. Snapshot, off, snapshot with
//! nothing changed inside reuses the texture; a change inside while off
//! re-renders once, in place, at the switch back; an off boundary releases
//! its snapshot storage after the release age, also when it is culled or
//! under a recording that replays. Off, the subtree paints inline. A texture
//! whose content changes while off marks the kept snapshot stale once, not at
//! every frame, and a boundary shader written while off runs its pass once
//! over the kept source at the switch back. Recording to snapshot and back,
//! and Recording to off and back, keep their pictures. A switch back under
//! another scale follows the snapshot rules. A vended snapshot texture id
//! keeps its last raster while off and past the release.
//!
//! Scene, at display scale 1 and 1.5 (geometry stated in device px, so it
//! rounds the same on both grids): root > holder (absolute at the origin, the
//! window's size, scaled and translated about its top-left corner) > [outer,
//! a Recording boundary filling it >] tile (the boundary switched,
//! absolute) > fill (a detached rect filling it, blue), and in some cases a
//! texture element at the tile's top-left, or a boundary shader on the tile
//! that mixes the source with magenta by a uniform.
//!
//! Counters are (reused, rerendered, rasterized, deferred) snapshots and
//! (recorded, reused) Recording boundaries. Every case sets the snapshot
//! raster budget itself (none), never the default.
//!
//! Run: cargo run -p alloy --example boundary_switch

use alloy::renderer::prelude::*;
use alloy::renderer::{Color, DisplayListBuilder, ISize};
use alloy::rendertree::composite::{paint_phase, PaintStats};
use alloy::rendertree::{
  BoundaryMode, Damage, ElementKind, OriginCoord, PaintCache, PlatformContext, Rectangle, RenderTree, Texture, View,
  Window,
};
use alloy::{NodeShader, ParamValue, SamplerState, ShaderStage, TextureFormat};
use taffy::prelude::{length, Size, Style};
use taffy::style::Position;

const W: u32 = 400;
const H: u32 = 300;
const DISPLAY_SCALES: [f32; 2] = [1.0, 1.5];

// The engine's rules this example checks, restated (the constants are
// private to alloy): the paint walks an off boundary keeps its snapshot
// storage (SNAPSHOT_RELEASE_FRAMES), the looks a scale must hold to be at
// rest (SCALE_REST_LOOKS), and the ratio between the composite scale and the
// raster's past which a raster is re-made in flight
// (LAYER_SCALE_MOTION_BOUND).
const RELEASE_FRAMES: u32 = 120;
const REST_LOOKS: u32 = 3;
const MOTION_BOUND: f32 = 1.5;

// The tile, in device px: its place and its box.
const TILE_LEFT: f32 = 20.0;
const TILE_TOP: f32 = 20.0;
const TILE_W: f32 = 100.0;
const TILE_H: f32 = 80.0;
// The texture element inside the tile, in device px from the tile's corner,
// and the texture's own size in texels.
const TEX_AT: f32 = 10.0;
const TEX_SIDE: f32 = 30.0;
const TEX_TEXELS: u32 = 4;
// Frames painted after a change, each expected to repeat it.
const STILL_FRAMES: u32 = 3;
// Frames a texture's content changes while the tile is off, one change each.
const VIDEO_FRAMES: u32 = 6;
// The holder's scale while off: within the motion bound of the raster's
// (1.5), and past it.
const WITHIN_SCALE: f32 = 1.2;
const PAST_SCALE: f32 = 2.0;
// Off frames under the new scale before the switch back: past the rest looks.
const SCALED_OFF_FRAMES: u32 = 5;
// How far right the holder moves to take the tile out of view: past the
// window.
const OUT_OF_VIEW: f32 = 2.0 * W as f32;
// How far a channel of a readback may be off where two pictures are drawn
// two ways (inline and from a texture).
const CHANNEL_TOLERANCE: i32 = 2;

const ROOT: u64 = 1;
const HOLDER: u64 = 2;
const OUTER: u64 = 3;
const TILE: u64 = 4;
const FILL: u64 = 5;
const TEX: u64 = 6;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
// The texture's frames while it changes every frame.
const VIDEO: [[u8; 4]; 2] = [YELLOW, CYAN];

// The boundary shader: a fullscreen triangle, and the source mixed with
// magenta by uMix.
const VERTEX: &str = "
out vec2 vUV;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUV = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";
const MIX: &str = "
uniform sampler2D uSource;
uniform float uMix;
in vec2 vUV;
void main() {
  fragColor = mix(texture(uSource, vUV), vec4(1.0, 0.0, 1.0, 1.0), uMix);
}
";

fn rgba(px: [u8; 4]) -> Color {
  let unit = |c: u8| f32::from(c) / f32::from(u8::MAX);
  Color::new_srgba(unit(px[0]), unit(px[1]), unit(px[2]), unit(px[3]))
}

// A frame's snapshot counters: (reused, rerendered, rasterized, deferred).
type Counts = (u32, u32, u32, u32);

fn counts(stats: &PaintStats) -> Counts {
  (stats.snapshots_reused, stats.snapshots_rerendered, stats.snapshots_rasterized, stats.snapshots_deferred)
}

const REUSED: Counts = (1, 0, 0, 0);
const RERENDERED: Counts = (0, 1, 0, 0);
const RASTERIZED: Counts = (0, 0, 1, 0);
const NONE: Counts = (0, 0, 0, 0);

// A frame's Recording boundaries: (recorded, reused).
fn recordings(stats: &PaintStats) -> (u32, u32) {
  (stats.boundaries_recorded, stats.boundaries_reused)
}

// A frame's pixels and stats, and what its content change did: whether
// `texture_content_changed` invalidated anything and whether the revision
// moved; None without a content change.
struct Frame {
  pixels: Vec<u8>,
  width: u32,
  stats: PaintStats,
  content: Option<(bool, bool)>,
}

impl Frame {
  fn at(&self, x: f32, y: f32) -> [u8; 4] {
    let i = ((y as u32 * self.width + x as u32) * 4) as usize;
    [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
  }
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
  a.iter().zip(b).all(|(x, y)| (i32::from(*x) - i32::from(y)).abs() <= CHANNEL_TOLERANCE)
}

// The first pixel where two frames differ past the tolerance.
fn differs(a: &Frame, b: &Frame) -> Option<(usize, [u8; 4], [u8; 4])> {
  let px = |f: &Frame, i: usize| [f.pixels[i], f.pixels[i + 1], f.pixels[i + 2], f.pixels[i + 3]];
  (0..a.pixels.len()).step_by(4).map(|i| (i / 4, px(a, i), px(b, i))).find(|(_, x, y)| !near(*x, *y))
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
  fn counts(&mut self, frame: &str, f: &Frame, want: Counts) {
    let got = counts(&f.stats);
    println!("  {frame}: snapshots {got:?}, recordings {:?}", recordings(&f.stats));
    self.expect(got == want, || format!("{frame}: {got:?}, expected {want:?}"));
  }

  // The same, unprinted, for a long run of frames.
  fn quiet(&mut self, frame: &str, f: &Frame, want: Counts) {
    let got = counts(&f.stats);
    self.expect(got == want, || format!("{frame}: {got:?}, expected {want:?}"));
  }

  fn recordings(&mut self, frame: &str, f: &Frame, want: (u32, u32)) {
    let got = recordings(&f.stats);
    self.expect(got == want, || format!("{frame}: recordings {got:?}, expected {want:?}"));
  }

  // The device pixel at (x, y) against `want`.
  fn pixel(&mut self, f: &Frame, x: f32, y: f32, want: [u8; 4], what: &str) {
    let got = f.at(x, y);
    self.expect(near(got, want), || format!("{what} at ({x},{y}): {got:?}, expected {want:?}"));
  }

  // Two frames show the same picture, within the tolerance; `exact` asks
  // for the same pixels.
  fn same(&mut self, a: &Frame, b: &Frame, exact: bool, what: &str) {
    if exact {
      let at = a.pixels.iter().zip(&b.pixels).position(|(x, y)| x != y);
      self.expect(at.is_none(), || format!("{what}: the pixels differ at byte {at:?}"));
    } else if let Some((i, x, y)) = differs(a, b) {
      self.expect(false, || format!("{what}: pixel {i} is {x:?}, expected {y:?}"));
    }
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

fn styled(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut Style)) {
  tree.edit(id, |el| {
    f(el.style_mut().expect("layout"));
    Damage::Layout
  });
}

fn placed(style: &mut Style, left: f32, top: f32, w: f32, h: f32) {
  style.position = Position::Absolute;
  style.inset.left = length(left);
  style.inset.top = length(top);
  style.size = Size { width: length(w), height: length(h) };
}

fn write_color(tree: &mut RenderTree, id: u64, color: [u8; 4]) {
  tree.edit(id, |el| el.kind.paint_mut().expect("paintable kind").set_color(Some(rgba(color))));
}

fn write_view(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut View) -> Damage) {
  tree.edit(id, |el| match &mut el.kind {
    ElementKind::View(v) => f(v),
    _ => panic!("node {id} is not a view"),
  });
}

// The mode write as the props arm performs it.
fn switch(tree: &mut RenderTree, mode: BoundaryMode) {
  tree.edit(TILE, |el| el.set_repaint_boundary(mode));
}

fn holder_scale(tree: &mut RenderTree, scale: f32) {
  write_view(tree, HOLDER, |v| {
    v.set_scale_x(Some(scale));
    v.set_scale_y(Some(scale))
  });
}

// What the tile keeps: whether it holds a snapshot, and whether it is valid.
fn kept(tree: &RenderTree) -> Option<bool> {
  match &*tree.try_node(TILE).expect("the tile").paint_cache.borrow() {
    Some(PaintCache::Snapshot(snap)) => Some(snap.valid),
    _ => None,
  }
}

// What a case's scene holds besides the tile and its fill.
#[derive(Default)]
struct Scene {
  outer: bool,
  // A texture element showing this registry id.
  texture: Option<u64>,
  // A boundary shader over this program.
  shader: Option<u64>,
}

fn mix_shader(program: u64, amount: f32) -> NodeShader {
  NodeShader {
    program,
    params: vec![("uMix".to_string(), ParamValue::Scalar(amount))],
    textures: vec![],
    outset: 0.0,
    previous: false,
  }
}

// The scene with the tile a snapshot boundary, geometry in device px.
fn build(display_scale: f32, scene: &Scene) -> RenderTree {
  let logical = |device: f32| device / display_scale;
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, Window::default().with_layout());
  tree.root = Some(ROOT);
  tree.create_node(HOLDER, View::default().with_layout());
  tree.insert_node(ROOT, HOLDER, None).expect("insert");
  styled(&mut tree, HOLDER, |s| placed(s, 0.0, 0.0, W as f32, H as f32));
  write_view(&mut tree, HOLDER, |v| {
    v.set_origin_x(Some(OriginCoord::Px(0.0)));
    v.set_origin_y(Some(OriginCoord::Px(0.0)))
  });
  let parent = if scene.outer {
    tree.create_node(OUTER, View::default().with_layout());
    tree.insert_node(HOLDER, OUTER, None).expect("insert");
    styled(&mut tree, OUTER, |s| placed(s, 0.0, 0.0, W as f32, H as f32));
    tree.edit(OUTER, |el| el.set_repaint_boundary(BoundaryMode::Recording));
    OUTER
  } else {
    HOLDER
  };
  tree.create_node(TILE, View::default().with_layout());
  tree.insert_node(parent, TILE, None).expect("insert");
  styled(&mut tree, TILE, |s| placed(s, logical(TILE_LEFT), logical(TILE_TOP), logical(TILE_W), logical(TILE_H)));
  tree.create_node(FILL, Rectangle::default().no_layout());
  tree.insert_node(TILE, FILL, None).expect("insert");
  write_color(&mut tree, FILL, BLUE);
  if let Some(id) = scene.texture {
    tree.create_node(TEX, Texture::default().with_layout());
    tree.insert_node(TILE, TEX, None).expect("insert");
    styled(&mut tree, TEX, |s| placed(s, logical(TEX_AT), logical(TEX_AT), logical(TEX_SIDE), logical(TEX_SIDE)));
    tree.edit(TEX, |el| match &mut el.kind {
      ElementKind::Texture(t) => t.set_src(Some(id)),
      _ => panic!("a texture"),
    });
  }
  if let Some(program) = scene.shader {
    write_view(&mut tree, TILE, |v| v.set_shader(Some(mix_shader(program, 0.0))));
  }
  switch(&mut tree, BoundaryMode::Snapshot);
  tree
}

// A frame as a frame producer makes one (composite::render): the GPU
// content changes since the last frame and the looks noted last frame (a
// Compose damage each) land first, then the paint, recorded under the
// display scale, read back in device px.
fn frame(tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> Frame {
  let changes = ctx.take_content_changes();
  let content = (!changes.is_empty()).then(|| {
    let revision = tree.revision();
    let hit = tree.texture_content_changed(&changes);
    (hit, tree.revision() != revision)
  });
  for id in tree.take_scale_waits() {
    if tree.try_node(id).is_some() {
      tree.apply_damage(id, Damage::Compose);
    }
  }
  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, ctx);
  let dl = builder.build().expect("frame display list");
  let (w, h) = ((W as f32 * scale).round() as u32, (H as f32 * scale).round() as u32);
  let texture = ctx.render_display_list_to_texture(&dl, w, h, true).expect("frame rasterization");
  let pixels = ctx.read_texture(&texture, w, h).expect("texture readback");
  Frame { pixels, width: w, stats, content }
}

fn platform_at(display_scale: f32) -> PlatformContext {
  let platform = PlatformContext::new(Vec::new());
  platform.set_window_size(W as f32, H as f32);
  platform.set_display_scale(display_scale);
  platform.set_snapshot_raster_budget(None);
  platform
}

// The tile's center and the texture's, in device px of the holder at scale
// 1.
const CENTER: (f32, f32) = (TILE_LEFT + TILE_W / 2.0, TILE_TOP + TILE_H / 2.0);
const TEX_CENTER: (f32, f32) = (TILE_LEFT + TEX_AT + TEX_SIDE / 2.0, TILE_TOP + TEX_AT + TEX_SIDE / 2.0);

// What a frame that changes nothing counts of a snapshot tile: reused, or
// nothing under an outer recording that replays, which the walk does not
// enter.
fn at_rest(outer: bool) -> Counts {
  if outer {
    NONE
  } else {
    REUSED
  }
}

// A rasterized tile and one frame after it: the picture before a switch.
fn rastered(
  case: &mut Case,
  tree: &mut RenderTree,
  platform: &PlatformContext,
  ctx: &alloy::Context,
  outer: bool,
) -> Frame {
  let first = frame(tree, platform, ctx);
  case.counts("first", &first, RASTERIZED);
  let reused = frame(tree, platform, ctx);
  case.counts("reused", &reused, at_rest(outer));
  reused
}

// Frames after a change that changes nothing more.
fn still(case: &mut Case, tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context, want: Counts) {
  for look in 1..=STILL_FRAMES {
    let f = frame(tree, platform, ctx);
    case.quiet(&format!("still {look}"), &f, want);
  }
}

fn unchanged_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("snapshot, off, snapshot with nothing changed reuses the texture, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  switch(&mut tree, BoundaryMode::None);
  let off = frame(&mut tree, &platform, ctx);
  case.counts("off", &off, NONE);
  // Inline: the walk enters the fill, which the reused snapshot covered.
  case.expect(off.stats.nodes_painted > before.stats.nodes_painted, || {
    format!("off painted {} nodes, the reused snapshot {}", off.stats.nodes_painted, before.stats.nodes_painted)
  });
  case.same(&off, &before, false, "off, the same picture inline");
  case.expect(kept(&tree) == Some(true), || format!("off keeps a valid snapshot: {:?}", kept(&tree)));
  still(&mut case, &mut tree, &platform, ctx, NONE);
  switch(&mut tree, BoundaryMode::Snapshot);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, REUSED);
  case.same(&back, &before, true, "back, the frame before the switch");
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  case.finish(all);
}

// A paint change inside while off, back after `off_frames` off frames
// within the release age: one re-render in place, of the new content.
fn changed_case(ctx: &alloy::Context, display_scale: f32, off_frames: u32, all: &mut Vec<String>) {
  let mut case = Case::new(format!(
    "a change inside while off for {off_frames} frames re-renders once in place, at {display_scale}x"
  ));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  switch(&mut tree, BoundaryMode::None);
  write_color(&mut tree, FILL, RED);
  let off = frame(&mut tree, &platform, ctx);
  case.counts("off and changed", &off, NONE);
  case.pixel(&off, CENTER.0, CENTER.1, RED, "off, the change inline");
  case.expect(kept(&tree) == Some(false), || format!("the kept snapshot is stale: {:?}", kept(&tree)));
  for k in 2..=off_frames {
    frame(&mut tree, &platform, ctx);
    case.expect(kept(&tree).is_some(), || format!("off frame {k}: the stale storage is kept"));
  }
  switch(&mut tree, BoundaryMode::Snapshot);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, RERENDERED);
  case.pixel(&back, CENTER.0, CENTER.1, RED, "back, the new content");
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  case.finish(all);
}

// Where the off tile spends the release age.
#[derive(Clone, Copy)]
enum Away {
  // In view, painted inline every frame.
  Shown,
  // Out of view: the walk does not visit it.
  Culled,
  // Under a Recording boundary that replays: the walk does not visit it.
  Replayed,
}

impl Away {
  fn name(self) -> &'static str {
    match self {
      Away::Shown => "in view",
      Away::Culled => "culled",
      Away::Replayed => "under a replayed recording",
    }
  }
}

// Off past the release age, then back: the storage is gone by the walk
// after the age, and the switch back rasterizes into new storage.
fn released_case(ctx: &alloy::Context, display_scale: f32, away: Away, all: &mut Vec<String>) {
  let mut case = Case::new(format!(
    "an off tile {} past the release age is released and rasterized again, at {display_scale}x",
    away.name()
  ));
  let platform = platform_at(display_scale);
  let outer = matches!(away, Away::Replayed);
  let mut tree = build(display_scale, &Scene { outer, ..Scene::default() });
  let before = rastered(&mut case, &mut tree, &platform, ctx, outer);
  switch(&mut tree, BoundaryMode::None);
  let off = frame(&mut tree, &platform, ctx);
  case.counts("off", &off, NONE);
  if outer {
    case.recordings("off", &off, (1, 0));
  }
  if matches!(away, Away::Culled) {
    write_view(&mut tree, HOLDER, |v| v.set_x(Some(OUT_OF_VIEW)));
  }
  // The walk of the mode write was the first: the age counts from there.
  for k in 2..=RELEASE_FRAMES + 2 {
    let f = frame(&mut tree, &platform, ctx);
    let label = format!("off frame {k}");
    case.quiet(&label, &f, NONE);
    match away {
      Away::Shown => {}
      Away::Culled => case.expect(f.stats.nodes_painted < off.stats.nodes_painted, || {
        format!("{label}: {} nodes painted, the tile is not culled", f.stats.nodes_painted)
      }),
      Away::Replayed => case.recordings(&label, &f, (0, 1)),
    }
    let held = kept(&tree).is_some();
    let want = k <= RELEASE_FRAMES + 1;
    case.expect(held == want, || format!("{label}: the snapshot is held {held}, expected {want}"));
  }
  if matches!(away, Away::Culled) {
    write_view(&mut tree, HOLDER, |v| v.set_x(None));
  }
  switch(&mut tree, BoundaryMode::Snapshot);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, RASTERIZED);
  case.same(&back, &before, true, "back, the picture before the switch");
  still(&mut case, &mut tree, &platform, ctx, at_rest(outer));
  case.finish(all);
}

// A texture inside whose content changes at every frame while off, under a
// Recording boundary: the first change marks the kept snapshot stale (one
// bake hit, one revision bump, the outer recorded once), the next ones hit
// nothing (no bake, no revision, the outer replays) and show live; the
// switch back re-renders once with the last content.
fn texture_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!(
    "a texture changing every frame inside an off tile invalidates its snapshot once, at {display_scale}x"
  ));
  let platform = platform_at(display_scale);
  let texels = |px: [u8; 4]| px.repeat((TEX_TEXELS * TEX_TEXELS) as usize);
  let id = ctx
    .create_texture_from_pixels(
      TEX_TEXELS,
      TEX_TEXELS,
      &texels(GREEN),
      SamplerState::default(),
      TextureFormat::Rgba8,
      None,
    )
    .expect("the texture");
  let mut tree = build(display_scale, &Scene { outer: true, texture: Some(id), ..Scene::default() });
  let before = rastered(&mut case, &mut tree, &platform, ctx, true);
  case.pixel(&before, TEX_CENTER.0, TEX_CENTER.1, GREEN, "the texture in the snapshot");
  switch(&mut tree, BoundaryMode::None);
  let off = frame(&mut tree, &platform, ctx);
  case.counts("off", &off, NONE);
  case.recordings("off", &off, (1, 0));
  let mut last = GREEN;
  for k in 1..=VIDEO_FRAMES {
    last = VIDEO[k as usize % VIDEO.len()];
    ctx.update_texture(id, &texels(last), 0).expect("the upload");
    let f = frame(&mut tree, &platform, ctx);
    let label = format!("content frame {k}");
    case.quiet(&label, &f, NONE);
    let first = k == 1;
    case.expect(f.content == Some((first, first)), || {
      format!("{label}: (invalidated, revision moved) {:?}, expected {:?}", f.content, Some((first, first)))
    });
    case.recordings(&label, &f, if first { (1, 0) } else { (0, 1) });
    case.pixel(&f, TEX_CENTER.0, TEX_CENTER.1, last, &format!("{label}: the live texture"));
    case.expect(kept(&tree) == Some(false), || format!("{label}: the kept snapshot is stale: {:?}", kept(&tree)));
  }
  println!("  off with a changing texture: one bake hit over {VIDEO_FRAMES} content frames");
  switch(&mut tree, BoundaryMode::Snapshot);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, RERENDERED);
  case.pixel(&back, TEX_CENTER.0, TEX_CENTER.1, last, "back, the last content");
  still(&mut case, &mut tree, &platform, ctx, at_rest(true));
  ctx.destroy_texture(id);
  case.finish(all);
}

// A boundary shader whose params are written while off: the switch back
// runs the pass over the kept source, no raster of it.
fn shader_case(ctx: &alloy::Context, display_scale: f32, program: u64, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("a shader written while off runs its pass once over the kept source, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene { shader: Some(program), ..Scene::default() });
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  case.pixel(&before, CENTER.0, CENTER.1, BLUE, "the identity pass");
  switch(&mut tree, BoundaryMode::None);
  let off = frame(&mut tree, &platform, ctx);
  case.counts("off", &off, NONE);
  write_view(&mut tree, TILE, |v| v.set_shader(Some(mix_shader(program, 1.0))));
  let written = frame(&mut tree, &platform, ctx);
  case.counts("off, the shader written", &written, NONE);
  case.pixel(&written, CENTER.0, CENTER.1, BLUE, "off, no pass");
  switch(&mut tree, BoundaryMode::Snapshot);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, REUSED);
  case.pixel(&back, CENTER.0, CENTER.1, MAGENTA, "back, the pass at the new params");
  for look in 1..=STILL_FRAMES {
    let f = frame(&mut tree, &platform, ctx);
    case.quiet(&format!("still {look}"), &f, REUSED);
    case.pixel(&f, CENTER.0, CENTER.1, MAGENTA, &format!("still {look}"));
  }
  case.finish(all);
}

// Snapshot to Recording and back, then Recording to off and back.
fn recording_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("switches through Recording keep the picture, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  // Each step: the mode, the frame's snapshot counters and its recordings
  // (recorded, reused), then the same for the frames after it.
  let steps = [
    ("snapshot to Recording", BoundaryMode::Recording, NONE, (1, 0), NONE, (0, 1)),
    ("Recording to snapshot", BoundaryMode::Snapshot, RASTERIZED, (0, 0), REUSED, (0, 0)),
    ("snapshot to Recording again", BoundaryMode::Recording, NONE, (1, 0), NONE, (0, 1)),
    ("Recording to off", BoundaryMode::None, NONE, (0, 0), NONE, (0, 0)),
    ("off to Recording", BoundaryMode::Recording, NONE, (1, 0), NONE, (0, 1)),
  ];
  for (name, mode, snaps, recs, still_snaps, still_recs) in steps {
    switch(&mut tree, mode);
    let f = frame(&mut tree, &platform, ctx);
    case.counts(name, &f, snaps);
    case.recordings(name, &f, recs);
    case.same(&f, &before, false, name);
    for look in 1..=STILL_FRAMES {
      let f = frame(&mut tree, &platform, ctx);
      let label = format!("{name}, still {look}");
      case.quiet(&label, &f, still_snaps);
      case.recordings(&label, &f, still_recs);
    }
  }
  case.finish(all);
}

// A switch back under a scale written while off: within the motion bound
// the kept raster is stretched (reused) until the scale has held for the
// rest looks, then rasterized once; past it, rasterized at once.
fn scale_case(ctx: &alloy::Context, display_scale: f32, scale: f32, all: &mut Vec<String>) {
  // The raster is made at the holder's scale of 1.
  let past = scale > MOTION_BOUND;
  let mut case =
    Case::new(format!("a switch back under a scale of {scale} follows the snapshot rules, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  switch(&mut tree, BoundaryMode::None);
  holder_scale(&mut tree, scale);
  for k in 1..=SCALED_OFF_FRAMES {
    let f = frame(&mut tree, &platform, ctx);
    case.quiet(&format!("off frame {k}"), &f, NONE);
  }
  switch(&mut tree, BoundaryMode::Snapshot);
  let (cx, cy) = (CENTER.0 * scale, CENTER.1 * scale);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, if past { RASTERIZED } else { REUSED });
  case.pixel(&back, cx, cy, BLUE, "back");
  if !past {
    // The switch back is the first look: the scale is at rest at the rest
    // looks after it.
    for look in 1..=REST_LOOKS {
      let f = frame(&mut tree, &platform, ctx);
      let want = if look == REST_LOOKS { RASTERIZED } else { REUSED };
      case.counts(&format!("look {look}"), &f, want);
      case.pixel(&f, cx, cy, BLUE, &format!("look {look}"));
    }
  }
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  case.finish(all);
}

// A vended snapshot texture id across an off period and the release: the
// id is no new one while off, and the one already vended keeps the last
// raster, readable, past the release; the raster after the switch back
// publishes under it again.
fn vended_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a vended snapshot texture id survives the off state, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  let id = tree.snapshot_texture(TILE, ctx).expect("a snapshot boundary vends its texture");
  let center = |pixels: &[u8], w: u32| {
    let (x, y) = ((w / 2) as usize, (pixels.len() / 4 / w as usize) / 2);
    let i = (y * w as usize + x) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
  };
  let read = |what: &str, case: &mut Case, want: [u8; 4]| match ctx.read_texture_by_id(id) {
    Ok((w, _, pixels)) => {
      let got = center(&pixels, w);
      case.expect(near(got, want), || format!("{what}: the vended texture shows {got:?}, expected {want:?}"));
    }
    Err(e) => case.expect(false, || format!("{what}: the vended texture does not read: {e}")),
  };
  read("snapshot", &mut case, BLUE);
  switch(&mut tree, BoundaryMode::None);
  write_color(&mut tree, FILL, RED);
  case.expect(tree.snapshot_texture(TILE, ctx).is_err(), || "off: the tile is no snapshot boundary".to_string());
  for _ in 1..=RELEASE_FRAMES + 2 {
    frame(&mut tree, &platform, ctx);
  }
  case.expect(kept(&tree).is_none(), || "the snapshot is released".to_string());
  read("off, past the release", &mut case, BLUE);
  switch(&mut tree, BoundaryMode::Snapshot);
  let back = frame(&mut tree, &platform, ctx);
  case.counts("back", &back, RASTERIZED);
  case.expect(tree.snapshot_texture(TILE, ctx) == Ok(id), || "back: the same id".to_string());
  read("back", &mut case, RED);
  for id in tree.drain_snapshot_textures() {
    ctx.release_borrowed(id);
  }
  case.finish(all);
}

fn mix_program(ctx: &alloy::Context) -> u64 {
  let vs = ctx.compile_shader_stage(ShaderStage::Vertex, VERTEX, true).expect("the vertex stage");
  let fs = ctx.compile_shader_stage(ShaderStage::Fragment, MIX, true).expect("the fragment stage");
  let program = ctx.link_shader_program(vs, fs, Some("boundary-switch-mix".to_string())).expect("the program");
  ctx.destroy_shader_stage(vs);
  ctx.destroy_shader_stage(fs);
  program
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "boundary switch", ISize::new(600, 450), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let mut all = Vec::new();
    let program = mix_program(&ctx);
    for display_scale in DISPLAY_SCALES {
      unchanged_case(&ctx, display_scale, &mut all);
      changed_case(&ctx, display_scale, 1, &mut all);
      changed_case(&ctx, display_scale, RELEASE_FRAMES, &mut all);
      for away in [Away::Shown, Away::Culled, Away::Replayed] {
        released_case(&ctx, display_scale, away, &mut all);
      }
      texture_case(&ctx, display_scale, &mut all);
      shader_case(&ctx, display_scale, program, &mut all);
      recording_case(&ctx, display_scale, &mut all);
      for scale in [WITHIN_SCALE, PAST_SCALE] {
        scale_case(&ctx, display_scale, scale, &mut all);
      }
      vended_case(&ctx, display_scale, &mut all);
    }
    ctx.destroy_shader_program(program);
    assert!(all.is_empty(), "{} boundary switch checks failed", all.len());
    println!("all boundary switch checks passed");
  });
}
