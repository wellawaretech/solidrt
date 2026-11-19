// A `<text>` drawn straight into the frame (okf/plans/own-renderer.md,
// stage 2): its glyph runs recorded as `draw_glyphs` ops under the painted
// box's origin and density, against the text atlas's texture, and what
// the tree does when the atlas moves its cells beneath those recordings.
// No GL: a stand-in raster thread answers texture creates with handles of
// no renderer (`renderer::test_texture`) and refuses everything else, so
// the atlas has a texture and the walk reads back what the paint walk drew
// (`DisplayList::ops`, `renderer::oplog::placed`, and anvil's own walk for
// the texture a run samples), and which runs a text recorded by reference
// from its record (anvil's `DisplayList::glyph_runs`). The pixels are
// alloy/examples/glyph_run.rs.
use crate::gpu::GlyphQuad;
use crate::raster::RasterCmd;
use crate::renderer::anvil::walk::{walk, KIND_GLYPH};
use crate::renderer::oplog::{placed, Op};
use crate::renderer::prelude::*;
use crate::renderer::{test_texture, DisplayList, DisplayListBuilder, ISize, Rect, Size};
use crate::rendertree::composite::{apply_content_changes, paint_phase, PaintStats};
use crate::rendertree::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use taffy::prelude::*;

const ROOT: u64 = 1;
const HOLDER: u64 = 2;
const FIRST: u64 = 3;
const SECOND: u64 = 4;

// The window, logical px.
const WINDOW_W: f32 = 400.0;
const WINDOW_H: f32 = 300.0;
// A display that is not 1x, so the run's 1/scale is visible.
const HIDPI: f32 = 2.0;
// How far a run's device bounds may reach past the ink box: a mask cell's
// one texel of padding per side, plus the pixel its quad snaps to.
const CELL_SLOP: f32 = 2.0;
// Float noise on a device coordinate.
const EPS: f32 = 1e-3;
// How long a test gives the glyph worker to make a style's warm-up.
const WORKER_WAIT: Duration = Duration::from_secs(10);
// Frames a test paints for the warm-up to land without moving the atlas:
// a style warms once, so a move is followed by at most one more.
const REST_ROUNDS: u32 = 3;
// Frames the holder's recording replays without building its texts: past
// the 120 after which a text's record used to be released.
const STANDING_FRAMES: u32 = 150;

// A context whose raster thread is a stand-in: a texture create gets a
// handle with a fresh GL name and the asked size, every other command is
// refused (which answers a blocked rpc). The thread ends with the context.
pub(super) fn stand_in() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, rx) = std::sync::mpsc::channel::<RasterCmd>();
  std::thread::spawn(move || {
    let mut name = 0;
    for cmd in rx {
      match cmd {
        RasterCmd::CreateTexture { width, height, reply, .. } => {
          name += 1;
          reply.send(Ok(test_texture(name, ISize::new(width as i64, height as i64)))).ok();
        }
        other => other.refuse("no raster thread in a lib test"),
      }
    }
  });
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
}

fn size(tree: &mut RenderTree, id: u64, w: f32, h: f32) {
  let style = tree.node_mut(id).style_mut().expect("laid-out node");
  style.size = taffy::Size { width: length(w), height: length(h) };
}

fn label(text: &str) -> Text {
  let mut label = Text::default();
  label.set_plain_text(text.to_string());
  label
}

// root(1) > holder(2, a Recording boundary, a column) > first(3), second(4)
fn tree_with_two_texts() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  let mut holder = View::default().with_layout();
  holder.repaint_boundary = BoundaryMode::Recording;
  tree.create_node(HOLDER, holder);
  tree.create_node(FIRST, label("Hello").with_layout());
  tree.create_node(SECOND, label("world").with_layout());
  tree.insert_node(ROOT, HOLDER, None).expect("insert");
  tree.insert_node(HOLDER, FIRST, None).expect("insert");
  tree.insert_node(HOLDER, SECOND, None).expect("insert");
  tree.root = Some(ROOT);
  size(&mut tree, ROOT, WINDOW_W, WINDOW_H);
  size(&mut tree, HOLDER, WINDOW_W, WINDOW_H);
  tree.node_mut(HOLDER).style_mut().expect("laid-out node").flex_direction = FlexDirection::Column;
  tree
}

struct Stage {
  platform: PlatformContext,
  alloy: crate::Context,
  tree: RenderTree,
}

impl Stage {
  fn new() -> Self {
    let platform = crate::tests::text_platform();
    platform.set_window_size(WINDOW_W, WINDOW_H);
    platform.set_display_scale(HIDPI);
    Stage { platform, alloy: stand_in(), tree: tree_with_two_texts() }
  }

  // One frame as composite::render records it: the display scale first,
  // then the walk in logical px.
  fn frame(&mut self) -> (DisplayList, PaintStats) {
    apply_content_changes(&mut self.tree, &self.platform, &self.alloy);
    let mut builder = DisplayListBuilder::new(None);
    builder.scale(HIDPI, HIDPI);
    let stats = paint_phase(&mut builder, &mut self.tree, &self.platform, &self.alloy);
    (builder.build().expect("a builder always builds a list"), stats)
  }

  fn settle_warm(&self) -> bool {
    settle_warm(&self.platform)
  }

  // Frames until the warm-up has landed without moving the atlas: the
  // last frame's list, whose runs the next frame keeps unless something
  // changes.
  fn rest(&mut self) -> DisplayList {
    for _ in 0..REST_ROUNDS {
      let (dl, _) = self.frame();
      if !self.settle_warm() {
        return dl;
      }
    }
    panic!("the warm-up kept moving the atlas");
  }

  fn text(&self, id: u64) -> &Text {
    match &self.tree.node(id).kind {
      ElementKind::Text(text) => text,
      _ => panic!("node {id} is a text"),
    }
  }

  // The atlas's GL name now, from the registry.
  fn atlas_name(&self) -> glow::NativeTexture {
    let id = self.platform.text_atlas_texture_id().expect("a text flushed the atlas");
    let entry = self.alloy.textures.get(id).expect("the atlas is registered");
    entry.handle.as_ref().and_then(|h| h.gl_name()).expect("the atlas has a handle")
  }
}

// Land the glyph worker's warm-up cells until none is queued: a text at
// the display scale warms its style on first sight, and a landing that
// grows the atlas is a move like any other. Returns whether the atlas
// moved while landing them.
pub(super) fn settle_warm(platform: &PlatformContext) -> bool {
  let generation = platform.text_atlas_generation();
  let started = Instant::now();
  loop {
    let mut atlas = platform.text_atlas();
    atlas.land();
    if atlas.queued_cells() == 0 {
      break;
    }
    assert!(started.elapsed() < WORKER_WAIT, "the worker did not finish: {} cells queued", atlas.queued_cells());
    drop(atlas);
    std::thread::sleep(Duration::from_millis(1));
  }
  platform.text_atlas_generation() != generation
}

// The shared quads of the frame's glyph runs, in recording order, the
// holder's recording flattened in place.
pub(super) fn runs(dl: &DisplayList) -> Vec<Arc<[GlyphQuad]>> {
  dl.glyph_runs().into_iter().map(|(quads, _)| quads).collect()
}

// Whether two frames hold the very same runs, by reference.
pub(super) fn same_runs(a: &[Arc<[GlyphQuad]>], b: &[Arc<[GlyphQuad]>]) -> bool {
  a.len() == b.len() && a.iter().zip(b).all(|(a, b)| Arc::ptr_eq(a, b))
}

// The GL names the frame's glyph runs sample, in batch order.
fn glyph_names(dl: &DisplayList) -> Vec<glow::NativeTexture> {
  let walked = walk(dl, ISize::new((WINDOW_W * HIDPI) as i64, (WINDOW_H * HIDPI) as i64));
  walked
    .batches
    .iter()
    .filter(|b| walked.instances[b.start..b.start + b.count].iter().any(|i| i.offset_kind_stroke[2] == KIND_GLYPH))
    .map(|b| b.texture.expect("a glyph batch samples the atlas").0)
    .collect()
}

// The ops of `ops` with every nested list's ops in its place (the
// holder's recording replays as one).
fn flat(ops: &[Op]) -> Vec<&Op> {
  let mut out = Vec::new();
  for op in ops {
    match op {
      Op::DrawDisplayList { ops, .. } => out.extend(flat(ops)),
      op => out.push(op),
    }
  }
  out
}

#[test]
fn a_text_records_its_glyph_runs_under_its_box_origin_and_density() {
  let mut stage = Stage::new();
  let (dl, _) = stage.frame();
  let ops = flat(dl.ops());
  // Each text: save, translate to its box's origin, scale by 1/density,
  // its runs, restore.
  let runs: Vec<usize> =
    ops.iter().enumerate().filter(|(_, op)| matches!(op, Op::DrawGlyphs { .. })).map(|(at, _)| at).collect();
  assert_eq!(runs.len(), 2, "one solid run per text: {ops:?}");
  for &at in &runs {
    assert!(matches!(ops[at - 3], Op::Save), "{:?}", &ops[at - 3..=at + 1]);
    assert!(matches!(ops[at - 2], Op::Translate { .. }), "{:?}", ops[at - 2]);
    assert_eq!(*ops[at - 1], Op::Scale { x: 1.0 / HIDPI, y: 1.0 / HIDPI });
    assert!(matches!(ops[at + 1], Op::Restore), "{:?}", ops[at + 1]);
    let Op::DrawGlyphs { quads, gradient, .. } = ops[at] else { unreachable!() };
    assert_eq!((*quads, *gradient), (5, false), "five letters, solid");
  }
  // In device pixels a quad's pixel is one device pixel, and the box's
  // origin lands on a whole one, so every quad sits on the device grid.
  let draws: Vec<_> = placed(dl.ops()).into_iter().filter(|p| matches!(p.op, Op::DrawGlyphs { .. })).collect();
  for draw in &draws {
    let m = draw.to_device;
    assert!((m.m11 - 1.0).abs() < EPS && (m.m22 - 1.0).abs() < EPS, "{m:?}");
    assert!((m.m41.round() - m.m41).abs() < EPS && (m.m42.round() - m.m42).abs() < EPS, "{m:?}");
  }
  // The run's device bounds lie on the text's ink: across, inside its ink
  // box (from the pen, before the first glyph's bearing, to the last
  // glyph's extent), a cell's padding and its snapped pixel aside; down,
  // inside its line box.
  let mut top = 0.0;
  for (id, draw) in [FIRST, SECOND].into_iter().zip(&draws) {
    let run: Rect = draw.device_rect().expect("an upright run");
    let ink = stage.text(id).detached_bounds(Size::new(WINDOW_W, WINDOW_H));
    let (left, right) = (ink.min_x() * HIDPI, ink.max_x() * HIDPI);
    let (line_top, line_bottom) = (top * HIDPI, (top + ink.size.height) * HIDPI);
    top += ink.size.height;
    assert!(run.size.width > 0.0 && run.size.height > 0.0, "node {id}: {run:?}");
    assert!(run.min_x() >= left - CELL_SLOP && run.max_x() <= right + CELL_SLOP, "node {id}: {run:?} vs {ink:?}");
    assert!(run.min_y() >= line_top && run.max_y() <= line_bottom, "node {id}: {run:?} in {line_top}..{line_bottom}");
  }
}

#[test]
fn texts_in_one_frame_record_against_one_atlas_texture() {
  let mut stage = Stage::new();
  let (dl, _) = stage.frame();
  // Both texts' runs share one batch over the atlas as the registry has it.
  assert_eq!(glyph_names(&dl), vec![stage.atlas_name()]);
  // Each recorded glyph placements, and the tree holds both for the next
  // move of the atlas.
  assert!(stage.text(FIRST).has_raster() && stage.text(SECOND).has_raster());
  let mut held = stage.tree.text_raster_nodes();
  held.sort_unstable();
  assert_eq!(held, vec![FIRST, SECOND]);
}

#[test]
fn a_move_of_the_atlas_re_records_the_texts_on_a_new_texture() {
  let mut stage = Stage::new();
  let (before, _) = stage.frame();
  let old = stage.atlas_name();
  // At rest nothing re-records, the holder's recording replays, unless the
  // warm-up's landing moved the cells.
  let warm_moved = stage.settle_warm();
  let (_, rest) = stage.frame();
  assert_eq!(rest.boundaries_recorded, u32::from(warm_moved), "warm-up moved the atlas: {warm_moved}");
  let old = if warm_moved { stage.atlas_name() } else { old };
  // A font reset moves every cell (the same fonts, re-registered): the
  // generation moves, and the next frame re-records the texts' boundary
  // against the texture the flush made, at a new GL name.
  let generation = stage.platform.text_atlas_generation();
  stage.platform.reset_fonts(vec![FontPayload {
    alias: Some("sans".to_string()),
    bytes: std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/NotoSans.ttf")),
  }]);
  assert!(stage.platform.text_atlas_generation() > generation);
  let (after, moved) = stage.frame();
  assert_eq!(moved.boundaries_recorded, 1, "the damage re-records the holder");
  let new = stage.atlas_name();
  assert_ne!(new, old, "a whole upload takes a new name");
  assert_eq!(glyph_names(&after), vec![new]);
  // The frame recorded before still names the texture it was made
  // against, whose handle it holds: it draws the old placements from the
  // old texels.
  let first = glyph_names(&before);
  assert_eq!(first.len(), 1);
  assert_ne!(first[0], new);
  // Seen once: the frame after re-records only if the re-made style's
  // warm-up moved the cells again.
  let warm_moved = stage.settle_warm();
  let (_, settled) = stage.frame();
  assert_eq!(settled.boundaries_recorded, u32::from(warm_moved), "warm-up moved the atlas: {warm_moved}");
}

#[test]
fn a_text_at_rest_re_records_the_same_runs() {
  // No recording boundary: every frame builds both texts.
  let mut stage = Stage::new();
  stage.tree.node_mut(HOLDER).repaint_boundary = BoundaryMode::None;
  let first = stage.rest();
  let generation = stage.platform.text_atlas_generation();
  let (second, stats) = stage.frame();
  assert_eq!(stage.platform.text_atlas_generation(), generation, "nothing moved the atlas");
  assert_eq!((stats.boundaries_recorded, stats.boundaries_reused), (0, 0), "nothing replays");
  // Built again, recorded by reference: the runs are the ones the record
  // kept, not remade.
  let (before, after) = (runs(&first), runs(&second));
  assert_eq!(after.len(), 2, "one run per text");
  assert!(same_runs(&before, &after), "the runs were remade");
}

#[test]
fn an_atlas_move_refreshes_the_runs() {
  let mut stage = Stage::new();
  let before = stage.rest();
  let generation = stage.platform.text_atlas_generation();
  stage.platform.reset_fonts(vec![FontPayload {
    alias: Some("sans".to_string()),
    bytes: std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/NotoSans.ttf")),
  }]);
  assert!(stage.platform.text_atlas_generation() > generation);
  let (after, moved) = stage.frame();
  assert_eq!(moved.boundaries_recorded, 1, "the damage re-records the holder");
  // Both texts read their placements again, into the new texture.
  let (old, new) = (runs(&before), runs(&after));
  assert_eq!((old.len(), new.len()), (2, 2));
  assert!(old.iter().zip(&new).all(|(a, b)| !Arc::ptr_eq(a, b)), "a run outlived the move");
  assert_eq!(glyph_names(&after), vec![stage.atlas_name()]);
}

// The colors of the frame's glyph runs, in recording order, the holder's
// recording flattened in place.
fn run_colors(dl: &DisplayList) -> Vec<[f32; 4]> {
  flat(dl.ops())
    .into_iter()
    .filter_map(|op| match op {
      Op::DrawGlyphs { color, .. } => Some(*color),
      _ => None,
    })
    .collect()
}

// Whether the frame's glyph runs carry a gradient, in recording order.
fn run_gradients(dl: &DisplayList) -> Vec<bool> {
  flat(dl.ops())
    .into_iter()
    .filter_map(|op| match op {
      Op::DrawGlyphs { gradient, .. } => Some(*gradient),
      _ => None,
    })
    .collect()
}

#[test]
fn a_color_change_recolors_the_kept_runs() {
  let mut stage = Stage::new();
  let before = runs(&stage.rest());
  let red = crate::renderer::Color::new_srgba(1.0, 0.0, 0.0, 1.0);
  stage.tree.edit(FIRST, |el| el.kind.paint_mut().expect("a text paints").set_color(Some(red)));
  let (dl, changed) = stage.frame();
  assert_eq!(changed.boundaries_recorded, 1, "the paint damage re-records the holder");
  let after = runs(&dl);
  assert_eq!(after.len(), 2);
  // Both texts re-recorded their kept quads by reference; the recolored
  // one carries its new color on the op.
  assert!(Arc::ptr_eq(&before[0], &after[0]), "the recolored text remade its quads");
  assert!(Arc::ptr_eq(&before[1], &after[1]), "the unchanged text remade its quads");
  assert_eq!(run_colors(&dl)[0], [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn a_color_change_does_not_reshape() {
  let mut stage = Stage::new();
  stage.rest();
  let generation = stage.text(FIRST).shaping_generation();
  let red = crate::renderer::Color::new_srgba(1.0, 0.0, 0.0, 1.0);
  stage.tree.edit(FIRST, |el| el.kind.paint_mut().expect("a text paints").set_color(Some(red)));
  stage.frame();
  assert_eq!(stage.text(FIRST).shaping_generation(), generation, "a recolor reshaped the text");
  // A text change does reshape.
  stage.tree.edit(FIRST, |el| match &mut el.kind {
    ElementKind::Text(text) => {
      text.set_plain_text("Goodbye".to_string());
      Damage::Layout
    }
    _ => panic!("node {FIRST} is a text"),
  });
  stage.frame();
  assert!(stage.text(FIRST).shaping_generation() > generation, "a text change kept the shaping");
}

#[test]
fn a_gradient_change_remakes_the_runs() {
  let mut stage = Stage::new();
  let before = runs(&stage.rest());
  let stops = vec![
    GradientStop { offset: 0.0, color: crate::renderer::Color::new_srgba(1.0, 0.0, 0.0, 1.0) },
    GradientStop { offset: 1.0, color: crate::renderer::Color::new_srgba(0.0, 0.0, 1.0, 1.0) },
  ];
  let gradient =
    Gradient::linear_box(crate::renderer::Point::new(0.0, 0.0), crate::renderer::Point::new(1.0, 0.0), stops);
  stage.tree.edit(FIRST, |el| el.kind.paint_mut().expect("a text paints").set_gradient(gradient));
  let (dl, _) = stage.frame();
  let after = runs(&dl);
  assert_eq!(after.len(), 2);
  // The gradient's ramp is resolved into the run: its quads are remade,
  // and the op carries the gradient. The other text keeps its run.
  assert!(!Arc::ptr_eq(&before[0], &after[0]), "the graded text kept its quads");
  assert!(Arc::ptr_eq(&before[1], &after[1]), "the unchanged text remade its quads");
  assert_eq!(run_gradients(&dl), vec![true, false]);
}

#[test]
fn a_released_boundary_text_is_still_damaged_by_a_move() {
  let mut stage = Stage::new();
  stage.rest();
  // The holder replays for longer than a text's record used to live
  // unbuilt: its texts are never entered.
  for frame in 0..STANDING_FRAMES {
    let (_, standing) = stage.frame();
    assert_eq!((standing.boundaries_recorded, standing.boundaries_reused), (0, 1), "frame {frame}");
  }
  let mut held = stage.tree.text_raster_nodes();
  held.sort_unstable();
  assert_eq!(held, vec![FIRST, SECOND], "the texts keep their records");
  // A move still reaches them: the holder re-records against the new
  // texture, so the standing recording lets go of the old one.
  let old = stage.atlas_name();
  stage.platform.reset_fonts(vec![FontPayload {
    alias: Some("sans".to_string()),
    bytes: std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/NotoSans.ttf")),
  }]);
  let (after, moved) = stage.frame();
  assert_eq!(moved.boundaries_recorded, 1, "the damage re-records the holder");
  let new = stage.atlas_name();
  assert_ne!(new, old);
  assert_eq!(glyph_names(&after), vec![new]);
}
