//! Validates a frozen snapshot boundary (Element::freeze), the texture half:
//! the picture is taken at the tile's first snapshot paint after the freeze
//! write (the state at the end of that batch, whatever the order of its
//! writes; when first painted if it froze out of view; never deferred by the
//! raster budget), and held from then on. A write inside, every frame, marks
//! the snapshot stale and changes nothing on screen: the held texture is
//! reused, the pixels stay, nothing is rasterized, nothing is damaged and the
//! revision does not move. Held, the texture is never rasterized again under
//! an ancestor scale (past the motion bound and at rest) nor under its own
//! resize: it is stretched to the box, and a frozen tile sized by its content
//! that grows from a write inside is drawn at its new box while the sibling
//! it pushes moves, in the same frame. A texture inside that changes every
//! frame costs nothing. The tile's own boundary shader stays live: a params
//! write re-runs the pass over the held source. A context loss
//! (drop_paint_caches) drops the held picture, and the next paint takes the
//! current content and holds it. The unfreeze repaints once where something
//! changed inside (Damage::Paint, one re-render in place) and rasterizes
//! nothing where nothing did (Damage::Compose, reused); after a scale change
//! the snapshot rules re-raster it as they say, and an unfreeze and freeze
//! again makes it sharp. The switch pattern (off while live, then a snapshot
//! with freeze, in either order) re-renders the kept storage in place, then
//! holds it.
//!
//! Scene, at display scale 1 and 1.5 (geometry stated in device px, so it
//! rounds the same on both grids): root > holder (absolute at the origin, the
//! window's size, scaled and translated about its top-left corner) > [outer,
//! a Recording boundary filling it >] [a second snapshot tile, ahead of the
//! tile in the walk,] tile (the boundary frozen, absolute) > fill (a detached
//! rect filling it, blue), and in some cases a texture element at the tile's
//! top-left, a boundary shader on the tile that mixes the source with magenta
//! by a uniform, or a red oval (a detached oval filling it) over the fill,
//! whose curved edge shows the raster's density. The grow case has its own
//! scene: holder > column (absolute, a column) > tile (frozen, its height its
//! content's) > child (a laid-out view, blue) ; sibling (a laid-out view
//! below it, green).
//!
//! Counters are (reused, rerendered, rasterized, deferred) snapshots and
//! (recorded, reused) Recording boundaries. Every case sets the snapshot
//! raster budget itself, never the default.
//!
//! Run: cargo run -p alloy --example boundary_freeze

use alloy::renderer::prelude::*;
use alloy::renderer::{Color, DisplayListBuilder, ISize};
use alloy::rendertree::composite::{paint_phase, PaintStats};
use alloy::rendertree::{
  BoundaryMode, Curve, Damage, ElementKind, FrameDamage, OriginCoord, Oval, PaintCache, PlatformContext, Rectangle,
  RenderTree, Texture, TransitionConfig, TransitionEntry, TransitionSpec, View, Window,
};
use alloy::{NodeShader, ParamValue, SamplerState, ShaderStage, TextureFormat};
use std::time::Duration;
use taffy::prelude::{length, Size, Style};
use taffy::style::{Display, FlexDirection, Position};

const W: u32 = 400;
const H: u32 = 300;
const DISPLAY_SCALES: [f32; 2] = [1.0, 1.5];

// The engine's rules this example checks, restated (the constants are
// private to alloy): the looks a scale must hold to be at rest
// (SCALE_REST_LOOKS), and the ratio between the composite scale and the
// raster's past which a raster is re-made in flight
// (LAYER_SCALE_MOTION_BOUND).
const REST_LOOKS: u32 = 3;
const MOTION_BOUND: f32 = 1.5;

// The tile, in device px: its place and its box.
const TILE_LEFT: f32 = 20.0;
const TILE_TOP: f32 = 20.0;
const TILE_W: f32 = 100.0;
const TILE_H: f32 = 80.0;
// The second tile's place, right of the tile.
const SECOND_LEFT: f32 = 200.0;
// The texture element inside the tile, in device px from the tile's corner,
// and the texture's own size in texels.
const TEX_AT: f32 = 10.0;
const TEX_SIDE: f32 = 30.0;
const TEX_TEXELS: u32 = 4;
// The grow case, in device px: the child's height before and after the
// write inside, and the sibling's height.
const CHILD_H: f32 = 40.0;
const CHILD_GROWN: f32 = 80.0;
const SIBLING_H: f32 = 40.0;
// Frames painted after a change, each expected to repeat it.
const STILL_FRAMES: u32 = 3;
// The side of the view inside the tile in the nested and the zero-box
// cases, in device px.
const NEST_SIDE: f32 = 40.0;
// How far inside or outside a box's edge a pixel is read, in device px.
const EDGE_INSET: f32 = 2.0;
// A layout slide: its length, the clock it starts at, and the frames it is
// looked at in.
const SLIDE_MS: f32 = 100.0;
const SLIDE_START: f64 = 1000.0;
const SLIDE_STEPS: u32 = 4;
// Frames a held tile takes a write inside, one write each.
const WRITE_FRAMES: u32 = 6;
// Frames a texture's content changes inside a held tile, one change each.
const VIDEO_FRAMES: u32 = 6;
// The holder's scale while held: within the motion bound of the raster's
// (1.5), and past it.
const WITHIN_SCALE: f32 = 1.2;
const PAST_SCALE: f32 = 2.0;
// How far right the holder moves to take the tile out of view: past the
// window.
const OUT_OF_VIEW: f32 = 2.0 * W as f32;
// The boundary shader's mix toward magenta: none, and half way.
const MIX_NONE: f32 = 0.0;
const MIX_HALF: f32 = 0.5;
// How far a channel of a readback may be off where two pictures are drawn
// two ways (a pass's rounding of a half mix).
const CHANNEL_TOLERANCE: i32 = 2;
// How far a raster's density may be off the product of the display scale
// and the holder's: the grid composes the two in its own order.
const DENSITY_EPSILON: f32 = 1e-4;

const ROOT: u64 = 1;
const HOLDER: u64 = 2;
const OUTER: u64 = 3;
const TILE: u64 = 4;
const FILL: u64 = 5;
const TEX: u64 = 6;
const OVAL: u64 = 7;
const SECOND: u64 = 8;
const SECOND_FILL: u64 = 9;
const COLUMN: u64 = 10;
const CHILD: u64 = 11;
const SIBLING: u64 = 12;
const SIBLING_FILL: u64 = 13;
const MID: u64 = 14;
const NESTED: u64 = 15;
const NEST_FILL: u64 = 16;
const OVER: u64 = 17;
const OVER_FILL: u64 = 18;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
// Blue mixed half way toward magenta.
const BLUE_HALF_MAGENTA: [u8; 4] = [128, 0, 255, 255];
// The colors a write inside alternates between, and the texture's frames.
const WRITES: [[u8; 4]; 2] = [RED, GREEN];
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

// A frame's pixels, stats and resolved damage, whether its walk asked for
// another frame, and what its content change did: whether
// `texture_content_changed` invalidated anything and whether the revision
// moved; None without a content change.
struct Frame {
  pixels: Vec<u8>,
  width: u32,
  stats: PaintStats,
  damage: FrameDamage,
  requested: bool,
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

  // A frame that shows nothing new: no damage, and no frame asked for.
  fn unseen(&mut self, frame: &str, f: &Frame) {
    self.expect(f.damage == FrameDamage::None, || format!("{frame}: damage {:?}, expected none", f.damage));
    self.expect(!f.requested, || format!("{frame}: the walk asked for another frame"));
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

// The freeze write as the props arm performs it, on the tile: the damage
// the rendertree reported.
fn write_freeze(tree: &mut RenderTree, freeze: bool) -> Damage {
  let mut damage = Damage::None;
  tree.edit(TILE, |el| {
    damage = el.set_freeze(freeze);
    damage
  });
  damage
}

fn holder_scale(tree: &mut RenderTree, scale: f32) {
  write_view(tree, HOLDER, |v| {
    v.set_scale_x(Some(scale));
    v.set_scale_y(Some(scale))
  });
}

// What the tile keeps: whether it holds a snapshot, and whether it is valid.
fn kept(tree: &RenderTree) -> Option<bool> {
  kept_of(tree, TILE)
}

fn kept_of(tree: &RenderTree, id: u64) -> Option<bool> {
  match &*tree.try_node(id).expect("the node").paint_cache.borrow() {
    Some(PaintCache::Snapshot(snap)) => Some(snap.valid),
    _ => None,
  }
}

// Whether the tile holds its picture, and the density of that picture.
fn held(tree: &RenderTree) -> Option<(bool, f32)> {
  held_of(tree, TILE)
}

fn held_of(tree: &RenderTree, id: u64) -> Option<(bool, f32)> {
  match &*tree.try_node(id).expect("the node").paint_cache.borrow() {
    Some(PaintCache::Snapshot(snap)) => Some((snap.held, snap.scale)),
    _ => None,
  }
}

// Whether the tile holds its picture at about density `want`.
fn held_at(tree: &RenderTree, want: f32) -> bool {
  held(tree).is_some_and(|(h, scale)| h && (scale - want).abs() < DENSITY_EPSILON)
}

// What a case's scene holds besides the tile and its fill.
#[derive(Default)]
struct Scene {
  outer: bool,
  // A texture element showing this registry id.
  texture: Option<u64>,
  // A boundary shader over this program.
  shader: Option<u64>,
  // A red oval filling the tile, over the fill.
  oval: bool,
  // A second snapshot tile ahead of the tile in the walk.
  second: bool,
  // A view filling the tile with a snapshot boundary inside it, at the
  // tile's corner, over the fill.
  nested: bool,
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

// The window and the holder, geometry in device px.
fn holder_tree() -> RenderTree {
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
  tree
}

// A detached rect filling `parent`, in `color`.
fn fill(tree: &mut RenderTree, id: u64, parent: u64, color: [u8; 4]) {
  tree.create_node(id, Rectangle::default().no_layout());
  tree.insert_node(parent, id, None).expect("insert");
  write_color(tree, id, color);
}

// The scene with the tile a snapshot boundary, not frozen yet.
fn build(display_scale: f32, scene: &Scene) -> RenderTree {
  let logical = |device: f32| device / display_scale;
  let mut tree = holder_tree();
  let parent = if scene.outer {
    tree.create_node(OUTER, View::default().with_layout());
    tree.insert_node(HOLDER, OUTER, None).expect("insert");
    styled(&mut tree, OUTER, |s| placed(s, 0.0, 0.0, W as f32, H as f32));
    tree.edit(OUTER, |el| el.set_repaint_boundary(BoundaryMode::Recording));
    OUTER
  } else {
    HOLDER
  };
  if scene.second {
    tree.create_node(SECOND, View::default().with_layout());
    tree.insert_node(parent, SECOND, None).expect("insert");
    styled(&mut tree, SECOND, |s| placed(s, logical(SECOND_LEFT), logical(TILE_TOP), logical(TILE_W), logical(TILE_H)));
    fill(&mut tree, SECOND_FILL, SECOND, GREEN);
    tree.edit(SECOND, |el| el.set_repaint_boundary(BoundaryMode::Snapshot));
  }
  tree.create_node(TILE, View::default().with_layout());
  tree.insert_node(parent, TILE, None).expect("insert");
  styled(&mut tree, TILE, |s| placed(s, logical(TILE_LEFT), logical(TILE_TOP), logical(TILE_W), logical(TILE_H)));
  fill(&mut tree, FILL, TILE, BLUE);
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
    write_view(&mut tree, TILE, |v| v.set_shader(Some(mix_shader(program, MIX_NONE))));
  }
  if scene.oval {
    tree.create_node(OVAL, Oval::default().no_layout());
    tree.insert_node(TILE, OVAL, None).expect("insert");
    write_color(&mut tree, OVAL, RED);
  }
  if scene.nested {
    tree.create_node(MID, View::default().with_layout());
    tree.insert_node(TILE, MID, None).expect("insert");
    styled(&mut tree, MID, |s| placed(s, 0.0, 0.0, logical(TILE_W), logical(TILE_H)));
    tree.create_node(NESTED, View::default().with_layout());
    tree.insert_node(MID, NESTED, None).expect("insert");
    styled(&mut tree, NESTED, |s| placed(s, 0.0, 0.0, logical(NEST_SIDE), logical(NEST_SIDE)));
    fill(&mut tree, NEST_FILL, NESTED, GREEN);
    tree.edit(NESTED, |el| el.set_repaint_boundary(BoundaryMode::Snapshot));
  }
  switch(&mut tree, BoundaryMode::Snapshot);
  tree
}

// The grow case's scene: a column at the tile's place holding the tile, a
// snapshot boundary whose height is its child's, and a sibling below it.
fn build_column(display_scale: f32) -> RenderTree {
  let logical = |device: f32| device / display_scale;
  let mut tree = holder_tree();
  tree.create_node(COLUMN, View::default().with_layout());
  tree.insert_node(HOLDER, COLUMN, None).expect("insert");
  styled(&mut tree, COLUMN, |s| {
    s.position = Position::Absolute;
    s.inset.left = length(logical(TILE_LEFT));
    s.inset.top = length(logical(TILE_TOP));
    s.size.width = length(logical(TILE_W));
    s.flex_direction = FlexDirection::Column;
  });
  tree.create_node(TILE, View::default().with_layout());
  tree.insert_node(COLUMN, TILE, None).expect("insert");
  styled(&mut tree, TILE, |s| s.size.width = length(logical(TILE_W)));
  tree.create_node(CHILD, View::default().with_layout());
  tree.insert_node(TILE, CHILD, None).expect("insert");
  styled(&mut tree, CHILD, |s| s.size = Size { width: length(logical(TILE_W)), height: length(logical(CHILD_H)) });
  fill(&mut tree, FILL, CHILD, BLUE);
  tree.create_node(SIBLING, View::default().with_layout());
  tree.insert_node(COLUMN, SIBLING, None).expect("insert");
  styled(&mut tree, SIBLING, |s| s.size = Size { width: length(logical(TILE_W)), height: length(logical(SIBLING_H)) });
  fill(&mut tree, SIBLING_FILL, SIBLING, GREEN);
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
  // Only what the walk asks for counts.
  platform.take_frame_requested();
  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, ctx);
  let requested = platform.take_frame_requested();
  let dl = builder.build().expect("frame display list");
  let (w, h) = ((W as f32 * scale).round() as u32, (H as f32 * scale).round() as u32);
  let texture = ctx.render_display_list_to_texture(&dl, w, h, true).expect("frame rasterization");
  let pixels = ctx.read_texture(&texture, w, h).expect("texture readback");
  Frame { pixels, width: w, stats, damage: tree.frame_damage(), requested, content }
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

// A rasterized tile and one frame after it: the picture before the freeze.
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

// The freeze write and its frame: Compose, and the tile reuses its valid
// snapshot, which it holds from then on.
fn frozen(case: &mut Case, tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> Frame {
  let damage = write_freeze(tree, true);
  case.expect(damage == Damage::Compose, || format!("the freeze reported {damage:?}, expected Compose"));
  let f = frame(tree, platform, ctx);
  case.counts("frozen", &f, REUSED);
  case.expect(held(tree).is_some_and(|(h, _)| h), || "the frozen tile holds its picture".to_string());
  f
}

// Frames after a change that changes nothing more.
fn still(case: &mut Case, tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context, want: Counts) {
  for look in 1..=STILL_FRAMES {
    let f = frame(tree, platform, ctx);
    case.quiet(&format!("still {look}"), &f, want);
  }
}

// A write inside at every frame, held: the stale mark only. Then the
// unfreeze repaints the tile once, in place, with the last write.
fn hold_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!(
    "a held tile shows none of the writes inside, and the unfreeze repaints it once, at {display_scale}x"
  ));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  let held_frame = frozen(&mut case, &mut tree, &platform, ctx);
  case.same(&held_frame, &before, true, "frozen, the same picture");
  let mut last = BLUE;
  for k in 1..=WRITE_FRAMES {
    last = WRITES[k as usize % WRITES.len()];
    let revision = tree.revision();
    write_color(&mut tree, FILL, last);
    let label = format!("write {k}");
    case.expect(tree.revision() == revision, || format!("{label}: the revision moved"));
    let f = frame(&mut tree, &platform, ctx);
    case.quiet(&label, &f, REUSED);
    case.unseen(&label, &f);
    case.same(&f, &before, true, &label);
    case.expect(kept(&tree) == Some(false), || format!("{label}: the stale mark is not set: {:?}", kept(&tree)));
    let stale = tree.try_node(TILE).expect("the tile").frozen_stale();
    case.expect(stale, || format!("{label}: the tree read shows no stale mark"));
  }
  println!("  {WRITE_FRAMES} writes inside, held: reused, no damage, no revision");
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Paint, || format!("the unfreeze reported {damage:?}, expected Paint"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, RERENDERED);
  case.pixel(&back, CENTER.0, CENTER.1, last, "unfrozen, the last write");
  case.expect(back.damage != FrameDamage::None, || "the unfreeze damages nothing".to_string());
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  case.finish(all);
}

// Frozen with nothing changing inside: the unfreeze is Compose and
// rasterizes nothing.
fn unchanged_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("an unfreeze with nothing changed inside rasterizes nothing, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Compose, || format!("the unfreeze reported {damage:?}, expected Compose"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, REUSED);
  case.same(&back, &before, true, "unfrozen, the same picture");
  case.finish(all);
}

// A content write and the freeze write in one batch, in either order: the
// picture is of the end of the batch.
fn batch_case(ctx: &alloy::Context, display_scale: f32, freeze_first: bool, all: &mut Vec<String>) {
  let order = if freeze_first { "the freeze, then a write" } else { "a write, then the freeze" };
  let mut case = Case::new(format!("{order} in one batch: the picture holds the write, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  if freeze_first {
    write_freeze(&mut tree, true);
    write_color(&mut tree, FILL, RED);
  } else {
    write_color(&mut tree, FILL, RED);
    write_freeze(&mut tree, true);
  }
  let f = frame(&mut tree, &platform, ctx);
  case.counts("the batch", &f, RERENDERED);
  case.pixel(&f, CENTER.0, CENTER.1, RED, "the batch, its write");
  write_color(&mut tree, FILL, GREEN);
  let after = frame(&mut tree, &platform, ctx);
  case.counts("a write after it", &after, REUSED);
  case.pixel(&after, CENTER.0, CENTER.1, RED, "a write after it, held");
  case.finish(all);
}

// Frozen out of view, with a write inside: the picture is taken when the
// tile is first painted, of that moment.
fn culled_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a tile frozen out of view takes its picture when shown, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  write_view(&mut tree, HOLDER, |v| v.set_x(Some(OUT_OF_VIEW)));
  let away = frame(&mut tree, &platform, ctx);
  case.counts("out of view", &away, NONE);
  write_freeze(&mut tree, true);
  write_color(&mut tree, FILL, RED);
  still(&mut case, &mut tree, &platform, ctx, NONE);
  case.expect(held(&tree).is_some_and(|(h, _)| !h), || "out of view, no picture taken yet".to_string());
  write_view(&mut tree, HOLDER, |v| v.set_x(None));
  let shown = frame(&mut tree, &platform, ctx);
  case.counts("shown", &shown, RERENDERED);
  case.pixel(&shown, CENTER.0, CENTER.1, RED, "shown, the content of that moment");
  write_color(&mut tree, FILL, GREEN);
  let after = frame(&mut tree, &platform, ctx);
  case.counts("a write after it", &after, REUSED);
  case.pixel(&after, CENTER.0, CENTER.1, RED, "a write after it, held");
  case.finish(all);
}

// Held under an ancestor scale: never rasterized again, at rest or not,
// stretched, and nothing waits. The scale is observed all the same, so the
// unfreeze, a Compose (nothing changed inside), rasters at once at the new
// density: past the motion bound by the rule, within it because the scale
// held through the freeze and is at rest.
fn scale_case(ctx: &alloy::Context, display_scale: f32, scale: f32, all: &mut Vec<String>) {
  let past = scale > MOTION_BOUND;
  let mut case = Case::new(format!(
    "held under a scale of {scale} ({}), never rasterized, then the snapshot rules, at {display_scale}x",
    if past { "past the motion bound" } else { "within it" }
  ));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  holder_scale(&mut tree, scale);
  let (cx, cy) = (CENTER.0 * scale, CENTER.1 * scale);
  for look in 1..=REST_LOOKS + STILL_FRAMES {
    let f = frame(&mut tree, &platform, ctx);
    let label = format!("scaled look {look}");
    case.quiet(&label, &f, REUSED);
    case.expect(!f.requested, || format!("{label}: the held tile asked for another frame"));
    case.pixel(&f, cx, cy, BLUE, &label);
  }
  let made = held(&tree);
  case.expect(held_at(&tree, display_scale), || format!("held {made:?}, expected at density {display_scale}"));
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Compose, || format!("the unfreeze reported {damage:?}, expected Compose"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, RASTERIZED);
  case.pixel(&back, cx, cy, BLUE, "unfrozen");
  let made = held(&tree).map(|(_, s)| s);
  let want = display_scale * scale;
  let rastered_at = made.is_some_and(|s| (s - want).abs() < DENSITY_EPSILON);
  case.expect(rastered_at, || format!("rastered at density {made:?}, expected {want}"));
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  case.finish(all);
}

// Held under a scale, stretched; an unfreeze and a freeze again in one
// batch takes a new picture at the density of the moment: the picture of
// a tile rasterized there from the start.
fn refreeze_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("an unfreeze and a freeze again makes the tile sharp, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut reference = build(display_scale, &Scene { oval: true, ..Scene::default() });
  holder_scale(&mut reference, WITHIN_SCALE);
  frame(&mut reference, &platform, ctx);
  let sharp = frame(&mut reference, &platform, ctx);
  let mut tree = build(display_scale, &Scene { oval: true, ..Scene::default() });
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  holder_scale(&mut tree, WITHIN_SCALE);
  let mut stretched = frame(&mut tree, &platform, ctx);
  for _ in 1..=REST_LOOKS {
    stretched = frame(&mut tree, &platform, ctx);
  }
  case.counts("held, stretched", &stretched, REUSED);
  case.expect(differs(&stretched, &sharp).is_some(), || "the stretched picture is the sharp one".to_string());
  write_freeze(&mut tree, false);
  write_freeze(&mut tree, true);
  let again = frame(&mut tree, &platform, ctx);
  case.counts("frozen again", &again, RASTERIZED);
  case.same(&again, &sharp, true, "frozen again, the sharp picture");
  case.expect(held(&tree).is_some_and(|(h, _)| h), || "frozen again, held".to_string());
  write_color(&mut tree, FILL, GREEN);
  let after = frame(&mut tree, &platform, ctx);
  case.counts("a write after it", &after, REUSED);
  case.same(&after, &sharp, true, "a write after it, held");
  case.finish(all);
}

// Held under its own resize: stretched to the new box, never rasterized;
// the resize clears the ancestors and is damaged. The unfreeze rasters the
// new box.
fn resize_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a held tile is stretched to its own new box, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let logical = |device: f32| device / display_scale;
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  let revision = tree.revision();
  styled(&mut tree, TILE, |s| s.size.width = length(logical(2.0 * TILE_W)));
  case.expect(tree.revision() != revision, || "the resize did not move the revision".to_string());
  let f = frame(&mut tree, &platform, ctx);
  case.counts("resized", &f, REUSED);
  let wide = TILE_LEFT + 1.5 * TILE_W;
  case.pixel(&f, wide, CENTER.1, BLUE, "resized, the stretched picture in the new half");
  case.expect(f.damage != FrameDamage::None, || "the resize damages nothing".to_string());
  case.expect(kept(&tree) == Some(false), || format!("the resize marks the picture stale: {:?}", kept(&tree)));
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Paint, || format!("the unfreeze reported {damage:?}, expected Paint"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, RASTERIZED);
  case.pixel(&back, wide, CENTER.1, BLUE, "unfrozen, the new box");
  case.finish(all);
}

// A frozen tile sized by its content grows from a write inside: drawn at
// its new box, stretched, and the sibling it pushes moves, in the same frame;
// nothing is rasterized. The write moves the revision (it dirties layout),
// so a frame driver rebuilds and lays it out.
fn grow_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("a frozen tile that grows from a write inside moves its sibling, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let logical = |device: f32| device / display_scale;
  let mut tree = build_column(display_scale);
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  let band = TILE_TOP + (CHILD_H + CHILD_GROWN) / 2.0;
  let below = TILE_TOP + CHILD_GROWN + SIBLING_H / 2.0;
  case.pixel(&before, CENTER.0, band, GREEN, "before, the sibling below the tile");
  let empty = before.at(CENTER.0, below);
  case.expect(!near(empty, GREEN), || format!("before, the sibling reaches down to {below}: {empty:?}"));
  frozen(&mut case, &mut tree, &platform, ctx);
  let revision = tree.revision();
  styled(&mut tree, CHILD, |s| s.size.height = length(logical(CHILD_GROWN)));
  case.expect(tree.revision() != revision, || "the write inside did not move the revision".to_string());
  let f = frame(&mut tree, &platform, ctx);
  case.counts("grown", &f, REUSED);
  case.pixel(&f, CENTER.0, band, BLUE, "grown, the tile stretched over the band");
  case.pixel(&f, CENTER.0, below, GREEN, "grown, the sibling moved down");
  case.expect(f.damage != FrameDamage::None, || "the growth damages nothing".to_string());
  still(&mut case, &mut tree, &platform, ctx, REUSED);
  case.finish(all);
}

// The tile's own boundary shader stays live while held: a params write
// re-runs the pass over the held source, not over the content written
// inside since, and rasterizes nothing.
fn shader_case(ctx: &alloy::Context, display_scale: f32, program: u64, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a shader params write re-runs the pass over the held source, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene { shader: Some(program), ..Scene::default() });
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  case.pixel(&before, CENTER.0, CENTER.1, BLUE, "the identity pass");
  frozen(&mut case, &mut tree, &platform, ctx);
  write_color(&mut tree, FILL, RED);
  let f = frame(&mut tree, &platform, ctx);
  case.counts("a write inside", &f, REUSED);
  case.pixel(&f, CENTER.0, CENTER.1, BLUE, "a write inside, held");
  write_view(&mut tree, TILE, |v| v.set_shader(Some(mix_shader(program, MIX_HALF))));
  let mixed = frame(&mut tree, &platform, ctx);
  case.counts("the params write", &mixed, REUSED);
  case.pixel(&mixed, CENTER.0, CENTER.1, BLUE_HALF_MAGENTA, "the pass over the held source");
  for look in 1..=STILL_FRAMES {
    let f = frame(&mut tree, &platform, ctx);
    case.quiet(&format!("still {look}"), &f, REUSED);
    case.pixel(&f, CENTER.0, CENTER.1, BLUE_HALF_MAGENTA, &format!("still {look}"));
  }
  case.finish(all);
}

// A context loss while held drops the picture with every cache: the next
// paint rasterizes the current content and holds it.
fn dropped_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a context loss while held takes a new picture, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  write_color(&mut tree, FILL, RED);
  let f = frame(&mut tree, &platform, ctx);
  case.pixel(&f, CENTER.0, CENTER.1, BLUE, "a write inside, held");
  tree.drop_paint_caches();
  let lost = frame(&mut tree, &platform, ctx);
  case.counts("after the loss", &lost, RASTERIZED);
  case.pixel(&lost, CENTER.0, CENTER.1, RED, "after the loss, the current content");
  case.expect(held(&tree).is_some_and(|(h, _)| h), || "after the loss, held".to_string());
  write_color(&mut tree, FILL, GREEN);
  let after = frame(&mut tree, &platform, ctx);
  case.counts("a write after it", &after, REUSED);
  case.pixel(&after, CENTER.0, CENTER.1, RED, "a write after it, held");
  case.finish(all);
}

// The switch pattern: off while live (a write inside, shown inline), then a
// snapshot with freeze in one batch, in either order: the kept storage is
// re-rendered in place with the live content, then held; the unfreeze
// repaints once.
fn switch_case(ctx: &alloy::Context, display_scale: f32, freeze_first: bool, all: &mut Vec<String>) {
  let order = if freeze_first { "freeze, then the mode" } else { "the mode, then freeze" };
  let mut case = Case::new(format!("live, then {order}: re-rendered in place, then held, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  switch(&mut tree, BoundaryMode::None);
  write_color(&mut tree, FILL, RED);
  let live = frame(&mut tree, &platform, ctx);
  case.counts("live", &live, NONE);
  case.pixel(&live, CENTER.0, CENTER.1, RED, "live, inline");
  if freeze_first {
    write_freeze(&mut tree, true);
    switch(&mut tree, BoundaryMode::Snapshot);
  } else {
    switch(&mut tree, BoundaryMode::Snapshot);
    write_freeze(&mut tree, true);
  }
  let f = frame(&mut tree, &platform, ctx);
  case.counts("frozen", &f, RERENDERED);
  case.pixel(&f, CENTER.0, CENTER.1, RED, "frozen, the live content");
  write_color(&mut tree, FILL, GREEN);
  let after = frame(&mut tree, &platform, ctx);
  case.counts("a write after it", &after, REUSED);
  case.pixel(&after, CENTER.0, CENTER.1, RED, "a write after it, held");
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Paint, || format!("the unfreeze reported {damage:?}, expected Paint"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, RERENDERED);
  case.pixel(&back, CENTER.0, CENTER.1, GREEN, "unfrozen, the last write");
  case.finish(all);
}

// A texture inside a held tile, under a Recording boundary, whose content
// changes at every frame: nothing invalidated, no revision, no damage,
// nothing rasterized, the outer replays and the held picture stays. The
// unfreeze re-renders once with the last content.
fn texture_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("a texture changing every frame inside a held tile costs nothing, at {display_scale}x"));
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
  rastered(&mut case, &mut tree, &platform, ctx, true);
  let held_frame = frozen(&mut case, &mut tree, &platform, ctx);
  case.recordings("frozen", &held_frame, (1, 0));
  case.pixel(&held_frame, TEX_CENTER.0, TEX_CENTER.1, GREEN, "frozen, the texture in the picture");
  let mut last = GREEN;
  for k in 1..=VIDEO_FRAMES {
    last = VIDEO[k as usize % VIDEO.len()];
    ctx.update_texture(id, &texels(last), 0).expect("the upload");
    let f = frame(&mut tree, &platform, ctx);
    let label = format!("content frame {k}");
    case.quiet(&label, &f, NONE);
    case.expect(f.content == Some((false, false)), || {
      format!("{label}: (invalidated, revision moved) {:?}, expected {:?}", f.content, Some((false, false)))
    });
    case.recordings(&label, &f, (0, 1));
    case.unseen(&label, &f);
    case.pixel(&f, TEX_CENTER.0, TEX_CENTER.1, GREEN, &format!("{label}: the held picture"));
  }
  println!("  held with a changing texture: nothing over {VIDEO_FRAMES} content frames");
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Paint, || format!("the unfreeze reported {damage:?}, expected Paint"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, RERENDERED);
  case.pixel(&back, TEX_CENTER.0, TEX_CENTER.1, last, "unfrozen, the last content");
  ctx.destroy_texture(id);
  case.finish(all);
}

// Two tiles under a zero raster budget and a scale past the motion bound,
// so each would raster for density alone: the second tile in the walk is
// deferred, stretched, unless it freezes in that frame, which takes its
// picture then (the budget never defers a freeze).
fn budget_case(ctx: &alloy::Context, display_scale: f32, freeze: bool, all: &mut Vec<String>) {
  let mut case = Case::new(format!(
    "a zero budget {} the second tile, at {display_scale}x",
    if freeze { "never defers a freeze of" } else { "defers" }
  ));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene { second: true, ..Scene::default() });
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first, (0, 0, 2, 0));
  let reused = frame(&mut tree, &platform, ctx);
  case.counts("reused", &reused, (2, 0, 0, 0));
  platform.set_snapshot_raster_budget(Some(Duration::ZERO));
  holder_scale(&mut tree, PAST_SCALE);
  if freeze {
    write_freeze(&mut tree, true);
  }
  let f = frame(&mut tree, &platform, ctx);
  case.counts("scaled", &f, if freeze { (0, 0, 2, 0) } else { (1, 0, 1, 1) });
  if freeze {
    let made = held(&tree);
    let want = display_scale * PAST_SCALE;
    case.expect(held_at(&tree, want), || format!("the frozen tile holds {made:?}, expected at density {want}"));
  }
  case.finish(all);
}

// The slide's clock at look `step` of SLIDE_STEPS.
fn slide_at(step: u32) -> f64 {
  SLIDE_START + f64::from(SLIDE_MS) * f64::from(step) / f64::from(SLIDE_STEPS)
}

// A `layout` transition's size slide on a held tile: the held picture is
// stretched over the box at every frame of it, never rasterized, and every
// frame is damaged.
fn slide_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("a layout slide on a held tile stretches its picture every frame, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let logical = |device: f32| device / display_scale;
  let mut tree = build(display_scale, &Scene::default());
  let spec = TransitionSpec::Tween { duration_ms: SLIDE_MS, curve: Curve::Linear };
  tree.edit(TILE, |el| {
    el.transitions = Some(Box::new(TransitionConfig {
      layout: Some(TransitionEntry { spec, delay_ms: 0.0, from: None, exit: None }),
      ..Default::default()
    }));
    Damage::None
  });
  tree.set_transition_now(0.0);
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  write_color(&mut tree, FILL, RED);
  styled(&mut tree, TILE, |s| s.size.width = length(logical(2.0 * TILE_W)));
  tree.set_transition_now(SLIDE_START);
  let reflow = frame(&mut tree, &platform, ctx);
  case.counts("the reflow", &reflow, REUSED);
  case.pixel(&reflow, CENTER.0, CENTER.1, BLUE, "the reflow, the held picture");
  for step in 1..=SLIDE_STEPS {
    tree.set_transition_now(slide_at(step));
    tree.advance_transitions();
    let painted = tree.try_node(TILE).and_then(|e| e.painted_size()).map_or(0.0, |s| s.width);
    let edge = TILE_LEFT + painted * display_scale;
    let f = frame(&mut tree, &platform, ctx);
    let label = format!("slide step {step}");
    case.quiet(&label, &f, REUSED);
    case.expect(f.damage != FrameDamage::None, || format!("{label}: the slide damages nothing"));
    case.pixel(&f, edge - EDGE_INSET, CENTER.1, BLUE, &format!("{label}: the held picture at the box's edge"));
    if step < SLIDE_STEPS {
      let past = f.at(edge + EDGE_INSET, CENTER.1);
      case.expect(!near(past, BLUE), || format!("{label}: the picture reaches past the box's edge: {past:?}"));
    }
  }
  println!("  {SLIDE_STEPS} slide frames, held: reused, damaged, stretched to the box");
  case.finish(all);
}

// A texture id first asked of a held tile whose subtree changed: the held
// picture, which is what is on screen, though it never rasterizes again.
fn vended_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("a texture id first asked of a held stale tile shows its picture, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  write_color(&mut tree, FILL, RED);
  let f = frame(&mut tree, &platform, ctx);
  case.counts("a write inside", &f, REUSED);
  case.expect(kept(&tree) == Some(false), || format!("the held picture is stale: {:?}", kept(&tree)));
  let id = tree.snapshot_texture(TILE, ctx).expect("a snapshot boundary vends its texture");
  match ctx.read_texture_by_id(id) {
    Ok((w, _, pixels)) => {
      let (x, y) = ((w / 2) as usize, (pixels.len() / 4 / w as usize) / 2);
      let i = (y * w as usize + x) * 4;
      let got = [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]];
      case.expect(near(got, BLUE), || format!("the vended texture shows {got:?}, expected the held {BLUE:?}"));
    }
    Err(e) => case.expect(false, || format!("the vended texture does not read: {e}")),
  }
  for id in tree.drain_snapshot_textures() {
    ctx.release_borrowed(id);
  }
  case.finish(all);
}

// A frozen tile whose box has no texels is drawn inline: it holds nothing,
// is not frozen, and a write inside shows and is damaged.
fn zero_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case =
    Case::new(format!("a frozen tile with no texels holds nothing, a write inside shows, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let logical = |device: f32| device / display_scale;
  let mut tree = build(display_scale, &Scene::default());
  styled(&mut tree, TILE, |s| s.size = Size { width: length(0.0), height: length(0.0) });
  tree.create_node(OVER, View::default().with_layout());
  tree.insert_node(TILE, OVER, None).expect("insert");
  styled(&mut tree, OVER, |s| placed(s, 0.0, 0.0, logical(NEST_SIDE), logical(NEST_SIDE)));
  fill(&mut tree, OVER_FILL, OVER, BLUE);
  let over = (TILE_LEFT + NEST_SIDE / 2.0, TILE_TOP + NEST_SIDE / 2.0);
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first, NONE);
  case.pixel(&first, over.0, over.1, BLUE, "first, inline");
  write_freeze(&mut tree, true);
  let f = frame(&mut tree, &platform, ctx);
  case.counts("frozen", &f, NONE);
  let node = tree.try_node(TILE).expect("the tile");
  case.expect(!node.frozen(), || "drawn inline, the tile is frozen".to_string());
  case.expect(kept(&tree).is_none(), || format!("drawn inline, the tile holds {:?}", held(&tree)));
  let revision = tree.revision();
  write_color(&mut tree, OVER_FILL, RED);
  case.expect(tree.revision() != revision, || "a write inside did not move the revision".to_string());
  let written = frame(&mut tree, &platform, ctx);
  case.counts("a write inside", &written, NONE);
  case.pixel(&written, over.0, over.1, RED, "a write inside shows");
  case.expect(written.damage != FrameDamage::None, || "a write inside damages nothing".to_string());
  case.expect(kept(&tree).is_none(), || format!("never held: {:?}", held(&tree)));
  case.finish(all);
}

// Nested frozen tiles under a Recording boundary: a write stops at the
// nearest frozen node above it, the outer replays, and what is below the
// write is untouched. The unfreeze of the tile repaints it, and the nested
// one, still frozen, keeps its picture inside the new one.
fn nested_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a write stops at the nearest frozen node above it, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let logical = |device: f32| device / display_scale;
  let mut tree = build(display_scale, &Scene { outer: true, nested: true, ..Scene::default() });
  let first = frame(&mut tree, &platform, ctx);
  case.counts("first", &first, (0, 0, 2, 0));
  let again = frame(&mut tree, &platform, ctx);
  case.counts("again", &again, NONE);
  // The nested freeze is a change inside the tile, which takes its picture
  // anew with the nested one's inside it.
  tree.edit(NESTED, |el| el.set_freeze(true));
  write_freeze(&mut tree, true);
  let f = frame(&mut tree, &platform, ctx);
  case.counts("both frozen", &f, (1, 1, 0, 0));
  let both = held(&tree).is_some_and(|(h, _)| h) && held_of(&tree, NESTED).is_some_and(|(h, _)| h);
  case.expect(both, || format!("both held: {:?}, {:?}", held(&tree), held_of(&tree, NESTED)));
  let revision = tree.revision();
  write_view(&mut tree, MID, |v| v.set_x(Some(logical(TEX_AT))));
  let between = frame(&mut tree, &platform, ctx);
  case.counts("a write between the two", &between, NONE);
  case.recordings("a write between the two", &between, (0, 1));
  case.unseen("a write between the two", &between);
  case.expect(tree.revision() == revision, || "a write between the two moved the revision".to_string());
  let marks = (kept(&tree), kept_of(&tree, NESTED));
  case
    .expect(marks == (Some(false), Some(true)), || format!("(tile, nested) valid {marks:?}, expected the tile stale"));
  write_color(&mut tree, NEST_FILL, RED);
  let inside = frame(&mut tree, &platform, ctx);
  case.counts("a write inside the nested one", &inside, NONE);
  case.unseen("a write inside the nested one", &inside);
  case.expect(kept_of(&tree, NESTED) == Some(false), || "the nested one is not stale".to_string());
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Paint, || format!("the unfreeze reported {damage:?}, expected Paint"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("the tile unfrozen", &back, (1, 1, 0, 0));
  case.recordings("the tile unfrozen", &back, (1, 0));
  let nested = (TILE_LEFT + TEX_AT + NEST_SIDE / 2.0, TILE_TOP + NEST_SIDE / 2.0);
  case.pixel(&back, nested.0, nested.1, GREEN, "the tile unfrozen, the nested one's held picture, moved");
  case.finish(all);
}

// A held tile hidden (`display: none`) disappears, and shown again shows its
// held picture, never rasterized.
fn hidden_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a held tile hidden and shown shows its held picture again, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let mut tree = build(display_scale, &Scene::default());
  rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  write_color(&mut tree, FILL, RED);
  styled(&mut tree, TILE, |s| s.display = Display::None);
  let hidden = frame(&mut tree, &platform, ctx);
  case.counts("hidden", &hidden, NONE);
  let gone = hidden.at(CENTER.0, CENTER.1);
  case.expect(!near(gone, BLUE) && !near(gone, RED), || format!("hidden, the tile still shows: {gone:?}"));
  case.expect(hidden.damage != FrameDamage::None, || "hiding it damages nothing".to_string());
  styled(&mut tree, TILE, |s| s.display = Display::Flex);
  let shown = frame(&mut tree, &platform, ctx);
  case.counts("shown", &shown, REUSED);
  case.pixel(&shown, CENTER.0, CENTER.1, BLUE, "shown, the held picture");
  case.expect(shown.damage != FrameDamage::None, || "showing it damages nothing".to_string());
  case.finish(all);
}

// A held tile's own scroll write is inside the held raster: the stale mark
// only, and the unfreeze repaints it.
fn scroll_case(ctx: &alloy::Context, display_scale: f32, all: &mut Vec<String>) {
  let mut case = Case::new(format!("a held tile's own scroll write is the stale mark only, at {display_scale}x"));
  let platform = platform_at(display_scale);
  let logical = |device: f32| device / display_scale;
  let mut tree = build(display_scale, &Scene::default());
  let before = rastered(&mut case, &mut tree, &platform, ctx, false);
  frozen(&mut case, &mut tree, &platform, ctx);
  let revision = tree.revision();
  write_view(&mut tree, TILE, |v| v.set_scroll_y(Some(logical(TEX_AT))));
  case.expect(tree.revision() == revision, || "the scroll write moved the revision".to_string());
  let f = frame(&mut tree, &platform, ctx);
  case.counts("scrolled", &f, REUSED);
  case.unseen("scrolled", &f);
  case.same(&f, &before, true, "scrolled, the held picture");
  case.expect(kept(&tree) == Some(false), || format!("the scroll write marks it stale: {:?}", kept(&tree)));
  let damage = write_freeze(&mut tree, false);
  case.expect(damage == Damage::Paint, || format!("the unfreeze reported {damage:?}, expected Paint"));
  let back = frame(&mut tree, &platform, ctx);
  case.counts("unfrozen", &back, RERENDERED);
  case.finish(all);
}

fn mix_program(ctx: &alloy::Context) -> u64 {
  let vs = ctx.compile_shader_stage(ShaderStage::Vertex, VERTEX, true).expect("the vertex stage");
  let fs = ctx.compile_shader_stage(ShaderStage::Fragment, MIX, true).expect("the fragment stage");
  let program = ctx.link_shader_program(vs, fs, Some("boundary-freeze-mix".to_string())).expect("the program");
  ctx.destroy_shader_stage(vs);
  ctx.destroy_shader_stage(fs);
  program
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "boundary freeze", ISize::new(600, 450), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let mut all = Vec::new();
    let program = mix_program(&ctx);
    for display_scale in DISPLAY_SCALES {
      hold_case(&ctx, display_scale, &mut all);
      unchanged_case(&ctx, display_scale, &mut all);
      for freeze_first in [false, true] {
        batch_case(&ctx, display_scale, freeze_first, &mut all);
      }
      culled_case(&ctx, display_scale, &mut all);
      for scale in [WITHIN_SCALE, PAST_SCALE] {
        scale_case(&ctx, display_scale, scale, &mut all);
      }
      refreeze_case(&ctx, display_scale, &mut all);
      resize_case(&ctx, display_scale, &mut all);
      grow_case(&ctx, display_scale, &mut all);
      shader_case(&ctx, display_scale, program, &mut all);
      dropped_case(&ctx, display_scale, &mut all);
      for freeze_first in [false, true] {
        switch_case(&ctx, display_scale, freeze_first, &mut all);
      }
      texture_case(&ctx, display_scale, &mut all);
      for freeze in [false, true] {
        budget_case(&ctx, display_scale, freeze, &mut all);
      }
      slide_case(&ctx, display_scale, &mut all);
      vended_case(&ctx, display_scale, &mut all);
      zero_case(&ctx, display_scale, &mut all);
      nested_case(&ctx, display_scale, &mut all);
      hidden_case(&ctx, display_scale, &mut all);
      scroll_case(&ctx, display_scale, &mut all);
    }
    ctx.destroy_shader_program(program);
    assert!(all.is_empty(), "{} boundary freeze checks failed", all.len());
    println!("all boundary freeze checks passed");
  });
}
