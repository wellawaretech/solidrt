// The glyph engine (rendertree/text/glyphs): the font set, the caret path
// over the cluster map, mask and distance-field cells, the atlas packer
// and the text atlas. The layout contract itself is pinned by
// text_baseline.rs.
use crate::impellers::{FontStyle, FontWeight};
use crate::gpu::{CoverageMode, CoveragePolicy};
use crate::rendertree::text::glyphs::{
  split_phase, AtlasPacker, Cell, CellKind, CellRequest, Dirty, FontSet, HoldSource, InsertOutcome, Rasterizer,
  ShapeStyle, ShapedGlyphs, StyleKey, TextAtlas, TextRendering, WarmRequest, BYTES_PER_TEXEL, PHASES, WARM_CHUNK,
};
use crate::rendertree::text::{prepare_units, Fallback, RunStyle};
use crate::rendertree::{FontPayload, PaintState, PlatformContext};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans.ttf");
const NOTO_SANS_MONO: &[u8] = include_bytes!("../../assets/fonts/NotoSansMono.ttf");
// A code point no font has: the last private-use character.
const UNCOVERED: char = '\u{10FFFD}';

// The size every shaping check shapes at.
const SIZE: f32 = 16.0;

fn noto() -> FontPayload {
  FontPayload { alias: Some("sans".to_string()), bytes: Cow::Borrowed(NOTO_SANS) }
}

fn mono() -> FontPayload {
  FontPayload { alias: Some("mono".to_string()), bytes: Cow::Borrowed(NOTO_SANS_MONO) }
}

fn fonts(payloads: &[FontPayload]) -> FontSet {
  FontSet::from_payloads(payloads, |alias, e| panic!("{alias}: {e}"))
}

// A regular, normal-width style at `size` with `letter_spacing`.
fn shaping(size: f32, letter_spacing: f32) -> ShapeStyle {
  ShapeStyle { weight: 400, stretch: 100.0, size, line_height: 0.0, letter_spacing }
}

fn shape_one(fonts: &FontSet, text: &str, size: f32) -> ShapedGlyphs {
  ShapedGlyphs::shape(fonts, 0, text, &shaping(size, 0.0), Fallback::None).expect("shaped")
}

fn style(weight: FontWeight) -> RunStyle {
  RunStyle {
    font_family: "sans".to_string(),
    font_size: SIZE,
    font_style: FontStyle::Normal,
    font_weight: weight,
    font_stretch: 100.0,
    line_height: 0.0,
    letter_spacing: 0.0,
    paint: PaintState::default(),
  }
}

#[test]
fn font_set_resolves_alias_family_and_fallback() {
  let fonts = fonts(&[noto()]);
  assert_eq!(fonts.resolve("sans"), Some(0));
  assert_eq!(fonts.resolve("Noto Sans"), Some(0));
  // An unknown family falls back to the sans role.
  assert_eq!(fonts.resolve("No Such Font"), Some(0));
  assert!(FontSet::default().resolve("sans").is_none());
}

#[test]
fn shipped_noto_is_variable_in_weight() {
  let fonts = fonts(&[noto()]);
  let face = fonts.face(0).expect("face registered");
  assert_eq!(face.weight_setting(700), Some(700.0));
  assert!(!face.synthetic_bold(700), "a weight axis replaces synthetic bold");
  // Bold advances differ from regular ones on a variable font.
  let regular = shape_one(&fonts, "Hamburg", SIZE);
  let bold =
    ShapedGlyphs::shape(&fonts, 0, "Hamburg", &ShapeStyle { weight: 700, ..shaping(SIZE, 0.0) }, Fallback::None)
      .expect("shaped");
  assert!(bold.metrics.advance > regular.metrics.advance);
}

#[test]
fn engine_carets_follow_the_cluster_map() {
  let platform = PlatformContext::new(vec![noto()]);
  let units = prepare_units(&platform, "ab", &style(FontWeight::Regular), &[], true, Fallback::Registered);
  let unit = &units[0];
  let stops = unit.carets.as_ref().expect("carets asked for");
  let glyphs = &unit.glyphs.glyphs;
  assert_eq!(stops.len(), 3, "start, after a, after b");
  assert_eq!(stops[0].x, 0.0);
  assert_eq!(stops[1].x, glyphs[1].x, "the stop after a is b's pen position");
  assert_eq!(stops[2].x, unit.metrics.advance, "the last stop is the advance");
}

#[test]
fn carets_cost_no_shaping_beyond_the_word() {
  let platform = PlatformContext::new(vec![noto()]);
  let before = platform.words().len();
  let units = prepare_units(&platform, "Hamburg", &style(FontWeight::Regular), &[], true, Fallback::Registered);
  assert_eq!(units[0].carets.as_ref().expect("carets").len(), "Hamburg".len() + 1);
  // The word itself; no grapheme prefixes.
  assert_eq!(platform.words().len() - before, 1);
}

#[test]
fn mask_cell_holds_the_glyph_coverage() {
  let fonts = fonts(&[noto()]);
  let face = fonts.face(0).expect("face");
  let shaped = shape_one(&fonts, "H", 32.0);
  let glyph = shaped.glyphs[0].id;
  let request = CellRequest {
    kind: CellKind::Mask { ppem: 32.0 },
    weight: face.weight_setting(400),
    width: None,
    synthetic_bold: false,
    synthetic_italic: false,
    phase: 0.0,
    darken: 0.0,
    hint: false,
    glyphs: vec![glyph],
  };
  let cells = Rasterizer::default().rasterize(NOTO_SANS, &request).expect("a font");
  assert_eq!(cells.len(), 1);
  let cell = &cells[0];
  assert!(cell.width > 0 && cell.height > 0);
  assert_eq!(cell.pixels.len(), (cell.width * cell.height) as usize * BYTES_PER_TEXEL);
  // Premultiplied white: every channel is the coverage, and an H has ink.
  let ink: u32 = cell.pixels.chunks(BYTES_PER_TEXEL).map(|p| p[3] as u32).sum();
  assert!(ink > 0);
  assert!(cell.pixels.chunks(BYTES_PER_TEXEL).all(|p| p[0] == p[3] && p[1] == p[3] && p[2] == p[3]));
  // The cell sits on the baseline: its top is above it by its height.
  assert!(cell.top > 0 && cell.top as u32 <= cell.height);
}

fn solid(glyph: u16, side: u32) -> Cell {
  Cell {
    key: glyph,
    width: side,
    height: side,
    left: 0,
    top: side as i32,
    pixels: vec![255; (side * side) as usize * BYTES_PER_TEXEL],
  }
}

#[test]
fn packer_places_and_pads_and_reports_dirty_rects() {
  let mut packer = AtlasPacker::new(CellKind::Mask { ppem: 16.0 }, 2048);
  assert_eq!(packer.insert(solid(1, 8)), InsertOutcome::Placed);
  let placed = packer.placement(1).expect("placed");
  assert_eq!((placed.width, placed.height), (8, 8));
  // The padding ring stays transparent around the cell.
  let (w, _) = packer.size();
  let stride = w as usize * BYTES_PER_TEXEL;
  let left_of = ((placed.y as usize) * stride + (placed.x as usize - 1) * BYTES_PER_TEXEL) + 3;
  assert_eq!(packer.mirror()[left_of], 0);
  let inside = ((placed.y as usize) * stride + (placed.x as usize) * BYTES_PER_TEXEL) + 3;
  assert_eq!(packer.mirror()[inside], 255);
  let Some(Dirty::Rects(dirty)) = packer.take_dirty() else { panic!("rects, not a growth") };
  assert_eq!(dirty.len(), 1);
  assert_eq!(dirty[0], (placed.x - 1, placed.y - 1, 10, 10));
  assert!(packer.take_dirty().is_none(), "nothing changed since");
  // A second insert of the same glyph changes nothing.
  assert_eq!(packer.insert(solid(1, 8)), InsertOutcome::Placed);
  assert!(packer.take_dirty().is_none());
}

#[test]
fn packer_grows_by_repacking_and_then_fills() {
  let mut packer = AtlasPacker::new(CellKind::Mask { ppem: 16.0 }, 1024);
  let (w0, h0) = packer.size();
  // Enough 100-texel cells to overflow the initial side; the first growth
  // moves everything.
  let mut grew = false;
  for glyph in 0..60u16 {
    match packer.insert(solid(glyph, 100)) {
      InsertOutcome::Moved => grew = true,
      InsertOutcome::Full => panic!("full before the cap"),
      InsertOutcome::Placed => {}
    }
  }
  assert!(grew);
  assert_ne!(packer.size(), (w0, h0));
  assert_eq!(packer.len(), 60);
  assert_eq!(packer.take_dirty(), Some(Dirty::Whole { resized: true }), "a growth is the whole mirror");
  // Every cell survived the repack with its pixels.
  for glyph in 0..60u16 {
    let p = packer.placement(glyph).expect("kept");
    let (w, _) = packer.size();
    let at = (p.y as usize) * w as usize * BYTES_PER_TEXEL + (p.x as usize) * BYTES_PER_TEXEL;
    assert_eq!(packer.mirror()[at + 3], 255, "glyph {glyph} pixels moved with it");
  }
  // Past the cap the packer says Full and keeps what it has.
  let mut big = 1000u16;
  loop {
    match packer.insert(solid(big, 100)) {
      InsertOutcome::Full => break,
      _ => big += 1,
    }
    assert!(big < 2000, "an atlas capped at 1024 cannot hold that many");
  }
  assert_eq!(packer.size(), (1024, 1024));
  assert!(packer.placement(big).is_none());
}

#[test]
fn packer_insert_in_place_hands_back_what_does_not_fit_and_moves_nothing() {
  let mut packer = AtlasPacker::new(CellKind::Mask { ppem: 16.0 }, 1024);
  let side = packer.size();
  // 100-texel cells until the initial square refuses one: the refused cell
  // comes back, and nothing placed before it moved or grew.
  let mut glyph = 0u16;
  let refused = loop {
    match packer.insert_in_place(solid(glyph, 100)) {
      Ok(()) => glyph += 1,
      Err(cell) => break cell,
    }
    assert!(glyph < 100, "a 512 square holds fewer than a hundred 100-texel cells");
  };
  assert_eq!(refused.key, glyph);
  assert!(packer.placement(glyph).is_none());
  assert_eq!(packer.size(), side);
  let before: Vec<_> = (0..glyph).map(|g| packer.placement(g).expect("placed")).collect();
  assert!(matches!(packer.take_dirty(), Some(Dirty::Rects(_))), "placing in place is rects, not a repack");
  // A key already present is a no-op in place too.
  assert_eq!(packer.insert_in_place(solid(0, 100)), Ok(()));
  assert!(packer.take_dirty().is_none());
  for (g, placement) in before.iter().enumerate() {
    assert_eq!(packer.placement(g as u16).expect("kept"), *placement, "glyph {g} stayed put");
  }
  // The owner's frame start inserts the refused cell with growth allowed:
  // that is the one place cells move.
  assert_eq!(packer.insert(refused), InsertOutcome::Moved);
  assert_ne!(packer.size(), side);
  assert!(packer.placement(glyph).is_some());
}

// The distance-field cell checks: "H" at 48 texels per em with 8 texels of
// range, the sprite font's defaults.
const MSDF_PPEM: f32 = 48.0;
const MSDF_RANGE: f32 = 8.0;
// The field's byte value at an edge; inside reads above it.
const EDGE: u8 = 128;
// A stem's centre reads at least a texel of distance past the edge
// (255 / range above it, with room for the quantization).
const STEM_CENTRE_MIN: u8 = 160;
// The outermost texel of a cell is at least range / 2 - 1 texels from any
// ink, so at most 16 up the field, plus slack for the error correction.
const OUTSIDE_MAX: u8 = 24;
// The true field (alpha) against the median at an edge texel: both are
// 8-bit roundings of the same distance along a straight edge.
const FIELD_SLACK: i32 = 4;

/// The consumer's decode: the median of the three channels.
fn median(texel: &[u8]) -> u8 {
  let (r, g, b) = (texel[0], texel[1], texel[2]);
  r.max(g).min(r.min(g).max(b))
}

#[test]
fn msdf_cell_holds_the_glyph_as_a_field() {
  let fonts = fonts(&[noto()]);
  let face = fonts.face(0).expect("face");
  let shaped = shape_one(&fonts, "H", MSDF_PPEM);
  let request = CellRequest {
    kind: CellKind::Msdf { ppem: MSDF_PPEM, range: MSDF_RANGE },
    weight: face.weight_setting(400),
    width: None,
    synthetic_bold: false,
    synthetic_italic: false,
    phase: 0.0,
    darken: 0.0,
    hint: false,
    glyphs: vec![shaped.glyphs[0].id],
  };
  let cells = Rasterizer::default().rasterize(NOTO_SANS, &request).expect("a font");
  assert_eq!(cells.len(), 1);
  let cell = &cells[0];
  let (w, h) = (cell.width as usize, cell.height as usize);
  assert!(w > 0 && h > 0);
  assert_eq!(cell.pixels.len(), w * h * BYTES_PER_TEXEL);
  // The baseline runs through the cell: the box is padded below it.
  assert!(cell.top > 0 && (cell.top as usize) < h, "top {} in {h}", cell.top);
  let texel = |x: usize, y: usize| &cell.pixels[(y * w + x) * BYTES_PER_TEXEL..][..BYTES_PER_TEXEL];
  // The rim of the box is outside: the padding is half the range.
  for x in 0..w {
    assert!(median(texel(x, 0)) <= OUTSIDE_MAX && median(texel(x, h - 1)) <= OUTSIDE_MAX, "rim at x {x}");
  }
  for y in 0..h {
    assert!(median(texel(0, y)) <= OUTSIDE_MAX && median(texel(w - 1, y)) <= OUTSIDE_MAX, "rim at y {y}");
  }
  // A row above the crossbar crosses the two stems and nothing else: the
  // median rises through the edge twice, and along those straight edges the
  // true field agrees with it.
  let row = h / 4;
  let inside: Vec<bool> = (0..w).map(|x| median(texel(x, row)) > EDGE).collect();
  let stems = inside.windows(2).filter(|pair| !pair[0] && pair[1]).count();
  assert_eq!(stems, 2, "row {row}: {inside:?}");
  for x in 1..w {
    if inside[x] != inside[x - 1] {
      for t in [texel(x - 1, row), texel(x, row)] {
        assert!((t[3] as i32 - median(t) as i32).abs() <= FIELD_SLACK, "edge at x {x}: {t:?}");
      }
    }
  }
  let peak = (0..w).map(|x| median(texel(x, row))).max().expect("a row");
  assert!(peak >= STEM_CENTRE_MIN, "stem centre reads {peak}");
}

#[test]
fn blank_glyphs_are_empty_cells_that_take_no_atlas_space() {
  let fonts = fonts(&[noto()]);
  let face = fonts.face(0).expect("face");
  let space = shape_one(&fonts, " ", SIZE).glyphs[0].id;
  for kind in [CellKind::Mask { ppem: SIZE }, CellKind::Msdf { ppem: MSDF_PPEM, range: MSDF_RANGE }] {
    let request = CellRequest {
      kind,
      weight: face.weight_setting(400),
      width: None,
      synthetic_bold: false,
      synthetic_italic: false,
      phase: 0.0,
      darken: 0.0,
      hint: false,
      glyphs: vec![space],
    };
    let cells = Rasterizer::default().rasterize(NOTO_SANS, &request).expect("a font");
    assert_eq!(cells.len(), 1, "{kind:?}: a space is a cell, not a failure");
    assert_eq!((cells[0].width, cells[0].height), (0, 0), "{kind:?}");
    let mut packer = AtlasPacker::new(kind, 1024);
    assert_eq!(packer.insert(cells[0].clone()), InsertOutcome::Placed);
    assert_eq!(packer.placement(space).map(|p| (p.width, p.height)), Some((0, 0)));
    assert!(packer.take_dirty().is_none(), "{kind:?}: nothing to upload for a blank cell");
  }
}

#[test]
fn packer_evicts_what_a_full_atlas_stopped_using() {
  // A cap at the initial side, so there is no room to grow: cells of 100
  // texels until the 512 square refuses one. Nothing is older than the
  // eviction age yet, so that refusal is a real Full.
  let mut packer = AtlasPacker::new(CellKind::Mask { ppem: 16.0 }, 512).with_eviction(2);
  packer.begin_frame(1);
  let mut glyph = 0u16;
  while packer.insert(solid(glyph, 100)) != InsertOutcome::Full {
    glyph += 1;
    assert!(glyph < 100, "a 512 square holds fewer than a hundred 100-texel cells");
  }
  let held = glyph;
  assert!(held >= 4, "the square holds a few: {held}");
  let kept = held / 2;
  // The first half keeps being used; the rest were last used at frame 1.
  for frame in [2, 4] {
    packer.begin_frame(frame);
    for glyph in 0..kept {
      assert!(packer.touch(glyph));
    }
  }
  // The unused cells are older than two frames: they go, the ones in use
  // stay, and the repack leaves the whole mirror new at the same size.
  assert_eq!(packer.insert(solid(held, 100)), InsertOutcome::Moved);
  for glyph in 0..kept {
    assert!(packer.placement(glyph).is_some(), "glyph {glyph} in use stays");
  }
  for glyph in kept..held {
    assert!(packer.placement(glyph).is_none(), "unused glyph {glyph} went");
  }
  assert!(packer.placement(held).is_some());
  assert_eq!(packer.size(), (512, 512));
  assert_eq!(packer.take_dirty(), Some(Dirty::Whole { resized: false }));
}

#[test]
fn phases_split_a_pen_position_into_a_pixel_and_a_third() {
  assert_eq!(split_phase(10.0), (10, 0));
  assert_eq!(split_phase(10.34), (10, 1));
  assert_eq!(split_phase(10.67), (10, 2));
  assert_eq!(split_phase(10.999), (10, PHASES - 1));
  assert_eq!(split_phase(-0.5), (-1, 1));
}

#[test]
fn a_mask_at_a_subpixel_phase_shifts_its_coverage() {
  let fonts = fonts(&[noto()]);
  let face = fonts.face(0).expect("face");
  let stem = shape_one(&fonts, "l", SIZE).glyphs[0].id;
  let request = |phase: f32| CellRequest {
    kind: CellKind::Mask { ppem: SIZE },
    weight: face.weight_setting(400),
    width: None,
    synthetic_bold: false,
    synthetic_italic: false,
    phase,
    darken: 0.0,
    hint: false,
    glyphs: vec![stem],
  };
  let mut rasterizer = Rasterizer::default();
  let whole = rasterizer.rasterize(NOTO_SANS, &request(0.0)).expect("a font").remove(0);
  let half = rasterizer.rasterize(NOTO_SANS, &request(0.5)).expect("a font").remove(0);
  // Half a pixel over, the stem's coverage spreads across its edge
  // columns instead of filling one: a different mask for the same glyph.
  assert_ne!(whole.pixels, half.pixels);
  let ink = |cell: &Cell| cell.pixels.chunks(BYTES_PER_TEXEL).map(|p| p[3] as u32).sum::<u32>();
  assert!((ink(&whole) as i32 - ink(&half) as i32).abs() < ink(&whole) as i32 / 10, "the ink is the same");
}

// A hold source that counts: how many holds were taken, how many are
// still held. What an embedder's work-in-flight ledger sees of the atlas.
struct CountingHolds {
  taken: AtomicU32,
  held: Arc<AtomicU32>,
}

struct CountedHold(Arc<AtomicU32>);

impl Drop for CountedHold {
  fn drop(&mut self) {
    self.0.fetch_sub(1, Ordering::SeqCst);
  }
}

fn counting_holds() -> (Arc<CountingHolds>, HoldSource) {
  let counts = Arc::new(CountingHolds { taken: AtomicU32::new(0), held: Arc::new(AtomicU32::new(0)) });
  let source = counts.clone();
  let hold_source: HoldSource = Arc::new(move || {
    source.taken.fetch_add(1, Ordering::SeqCst);
    source.held.fetch_add(1, Ordering::SeqCst);
    Box::new(CountedHold(source.held.clone()))
  });
  (counts, hold_source)
}

// How long a unit test gives the worker to make a warm-up's cells.
const WORKER_WAIT: Duration = Duration::from_secs(10);

#[test]
fn warming_a_style_queues_its_ascii_at_every_phase() {
  let fonts = fonts(&[noto()]);
  let latch = Arc::new(AtomicBool::new(false));
  let mut atlas = TextAtlas::new(latch.clone());
  let (holds, source) = counting_holds();
  atlas.set_hold_source(source);
  let style = StyleKey::new(0, SIZE, 500, 100.0, false, 0.0, false);
  atlas.warm(&fonts, style);
  // Printable ASCII is 95 code points and the shipped face covers them:
  // 95 cells per phase in jobs of a chunk each, each job a hold on the
  // embedder.
  let jobs = PHASES as u32 * 95u32.div_ceil(WARM_CHUNK as u32);
  assert_eq!(holds.taken.load(Ordering::SeqCst), jobs);
  assert_eq!(atlas.queued_cells(), 95 * PHASES as usize);
  // Warming again queues nothing more.
  atlas.warm(&fonts, style);
  assert_eq!(holds.taken.load(Ordering::SeqCst), jobs);
  assert_eq!(atlas.queued_cells(), 95 * PHASES as usize);
  // The worker ends each hold when the job's cells are made, after it
  // requested the frame that lands them.
  let started = Instant::now();
  while holds.held.load(Ordering::SeqCst) > 0 {
    assert!(started.elapsed() < WORKER_WAIT, "the worker did not finish the warm-up");
    std::thread::sleep(Duration::from_millis(1));
  }
  assert!(latch.load(Ordering::Relaxed), "a finished job requests the frame that lands its cells");
}

#[test]
fn a_warm_request_asks_for_the_frame_that_submits_it() {
  let latch = Arc::new(AtomicBool::new(false));
  let mut atlas = TextAtlas::new(latch.clone());
  let request = WarmRequest { face: 0, size: SIZE, weight: 500, stretch: 100.0, italic: false };
  atlas.request_warm(request);
  assert!(latch.load(Ordering::Relaxed));
  // The same request again changes nothing.
  latch.store(false, Ordering::Relaxed);
  atlas.request_warm(request);
  assert!(!latch.load(Ordering::Relaxed));
}

#[test]
fn fallback_borrows_a_missing_glyph_from_the_next_covering_face() {
  let fonts = fonts(&[mono(), noto()]);
  let sans = fonts.face(1).expect("sans");
  // A character the mono face shapes to its notdef (its character map
  // lacks it and the shaper cannot compose it from parts either), that
  // the sans face covers and shapes as a cluster of its own after a letter
  // (a combining mark would merge into the cluster before it); found
  // rather than assumed, so a font update cannot rot the test.
  let sans_alone = self::fonts(&[noto()]);
  let own_cluster = |c: char| {
    ShapedGlyphs::shape(&sans_alone, 0, &format!("a{c}"), &shaping(SIZE, 0.0), Fallback::None)
      .is_some_and(|shaped| shaped.glyphs.iter().any(|g| g.cluster == 1 && g.id != 0))
  };
  let ch = (0x80u32..0x3000)
    .filter_map(char::from_u32)
    .find(|&c| {
      sans.glyph_id(c).is_some()
        && own_cluster(c)
        && ShapedGlyphs::shape(&fonts, 0, &c.to_string(), &shaping(SIZE, 0.0), Fallback::None)
          .is_some_and(|shaped| shaped.glyphs.iter().any(|g| g.id == 0))
    })
    .expect("the sans face covers something the mono face does not");
  let text = format!("a{ch}b");
  let with = ShapedGlyphs::shape(&fonts, 0, &text, &shaping(SIZE, 0.0), Fallback::Registered).expect("shaped");
  let without = ShapedGlyphs::shape(&fonts, 0, &text, &shaping(SIZE, 0.0), Fallback::None).expect("shaped");
  // The middle cluster (one glyph, or a base and its marks) comes from the
  // covering face; the letters around it stay on the primary.
  let middle = |shaped: &ShapedGlyphs| shaped.glyphs.iter().filter(|g| g.cluster == 1).copied().collect::<Vec<_>>();
  let borrowed = middle(&with);
  assert!(!borrowed.is_empty(), "{ch:?} shapes to something");
  assert!(borrowed.iter().all(|g| g.face == 1 && g.id != 0), "{ch:?} comes from the sans face: {borrowed:?}");
  assert!(with.glyphs.iter().filter(|g| g.cluster != 1).all(|g| g.face == 0 && g.id != 0));
  assert!(middle(&without).iter().all(|g| g.face == 0 && g.id == 0), "a font handle keeps the notdef");
  // The cluster map survives the splice: a caret stop per letter, the
  // third on b's pen, the last on the advance.
  let stops = with.caret_stops(&text);
  assert_eq!(stops.len(), 4);
  let b = with.glyphs.iter().find(|g| g.cluster as usize == 1 + ch.len_utf8()).expect("b's glyph");
  assert_eq!(stops[2].x, b.x);
  assert_eq!(stops[3].x, with.metrics.advance);
}

#[test]
fn whitespace_the_face_lacks_takes_the_space_advance() {
  let fonts = fonts(&[noto()]);
  let tab = shape_one(&fonts, "a\tb", SIZE);
  let space = shape_one(&fonts, "a b", SIZE);
  assert_eq!(tab.glyphs[1].id, space.glyphs[1].id, "a tab is the space glyph: blank, never a box");
  assert_eq!(tab.metrics.advance, space.metrics.advance);
  assert_eq!(tab.glyphs[2].x, space.glyphs[2].x);
}

#[test]
fn text_no_face_covers_keeps_the_notdef_box() {
  let fonts = fonts(&[noto(), mono()]);
  let text = format!("a{UNCOVERED}");
  let shaped = ShapedGlyphs::shape(&fonts, 0, &text, &shaping(SIZE, 0.0), Fallback::Registered).expect("shaped");
  assert_eq!(shaped.glyphs.len(), 2);
  assert_eq!((shaped.glyphs[1].face, shaped.glyphs[1].id), (0, 0));
  assert!(shaped.glyphs[1].advance > 0.0, "the notdef box has the font's notdef advance");
}

// Letter spacing on a three-letter word: px after every cluster.
const SPACING: f32 = 2.0;
// A condensed stretch inside the shipped Noto's width axis (62.5 to 100).
const CONDENSED: f32 = 75.0;
// How close a computed gamma ratio must come to the published float.
const RATIO_TOLERANCE: f32 = 1e-6;
// A stem darkening strength, em per side, for the key checks.
const DARKEN: f32 = 0.02;

#[test]
fn letter_spacing_adds_after_every_cluster() {
  let fonts = fonts(&[noto()]);
  let plain = shape_one(&fonts, "abc", SIZE);
  let spaced = ShapedGlyphs::shape(&fonts, 0, "abc", &shaping(SIZE, SPACING), Fallback::None).expect("shaped");
  assert_eq!(spaced.glyphs[0].x, plain.glyphs[0].x, "the first letter does not move");
  assert_eq!(spaced.glyphs[1].x, plain.glyphs[1].x + SPACING);
  assert_eq!(spaced.glyphs[2].x, plain.glyphs[2].x + 2.0 * SPACING);
  assert_eq!(spaced.metrics.advance, plain.metrics.advance + 3.0 * SPACING, "the last letter is spaced too");
  let stops = spaced.caret_stops("abc");
  assert_eq!(stops[1].x, spaced.glyphs[1].x, "carets follow the spaced pens");
  assert_eq!(stops[3].x, spaced.metrics.advance);
}

#[test]
fn the_width_axis_condenses_and_clamps() {
  let fonts = fonts(&[noto()]);
  let face = fonts.face(0).expect("face");
  assert_eq!(face.width_setting(CONDENSED), Some(CONDENSED), "the shipped Noto Sans has a width axis");
  assert_eq!(face.width_setting(150.0), Some(100.0), "a stretch past the axis clamps to it");
  let normal = shape_one(&fonts, "Hamburg", SIZE);
  let condensed =
    ShapedGlyphs::shape(&fonts, 0, "Hamburg", &ShapeStyle { stretch: CONDENSED, ..shaping(SIZE, 0.0) }, Fallback::None)
      .expect("shaped");
  assert!(condensed.metrics.advance < normal.metrics.advance, "condensed advances are narrower");
  let widest =
    ShapedGlyphs::shape(&fonts, 0, "Hamburg", &ShapeStyle { stretch: 150.0, ..shaping(SIZE, 0.0) }, Fallback::None)
      .expect("shaped");
  assert_eq!(widest.metrics.advance, normal.metrics.advance, "clamped to normal");
}

#[test]
fn directwrite_gamma_ratios_match_the_published_defaults() {
  // The 1.8 ratios dwrite_helpers.hlsl documents, and the table's clamp.
  let policy = CoveragePolicy { mode: CoverageMode::DirectWrite, gamma: 1.8, ..CoveragePolicy::default() };
  let expected = [0.148054421, -0.894594550, 1.47590804, -0.324668258];
  for (got, want) in policy.gamma_ratios().iter().zip(expected) {
    assert!((got - want).abs() < RATIO_TOLERANCE, "{got} vs {want}");
  }
  let low = CoveragePolicy { gamma: 0.5, ..policy };
  assert_eq!(low.gamma_ratios(), [0.0; 4], "below the table is gamma 1.0: no correction");
  let high = CoveragePolicy { gamma: 3.0, ..policy };
  let top = CoveragePolicy { gamma: 2.2, ..policy };
  assert_eq!(high.gamma_ratios(), top.gamma_ratios(), "above the table is its last row");
}

#[test]
fn text_rendering_darkening_follows_the_display_scale_unless_set() {
  let rendering = TextRendering::default();
  assert_eq!(rendering.darken_em(2.0), 0.0, "no darkening at 2x");
  let set = TextRendering { darken: Some(DARKEN), ..rendering };
  assert_eq!(set.darken_em(2.0), DARKEN, "a set strength applies at any scale");
  assert_eq!(set.darken_em(1.0), DARKEN);
  let plain = StyleKey::new(0, SIZE, 400, 100.0, false, 0.0, false);
  let darkened = StyleKey::new(0, SIZE, 400, 100.0, false, DARKEN, false);
  assert_ne!(plain, darkened, "darkening is part of the cell's identity");
  assert_eq!(darkened.darken_em(), DARKEN);
  let hinted = StyleKey::new(0, SIZE, 400, 100.0, false, 0.0, true);
  assert_ne!(plain, hinted, "hinting is part of the cell's identity");
  assert!(!rendering.hint_at(1.0), "unhinted by default until judged");
  assert!(TextRendering { hint: Some(true), ..rendering }.hint_at(2.0), "a set hinting applies at any scale");
}
