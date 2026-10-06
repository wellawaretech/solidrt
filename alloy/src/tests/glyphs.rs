// The glyph engine (rendertree/text/glyphs): the seam between the two
// shapers, the caret path over the cluster map, mask cells, and the atlas
// packer. The seam check is what gates switching prepareText's default
// shaper later (okf/plans/text-own-rasterizer.md): a word shaped by the
// engine and by Impeller must agree on advance.
use crate::impellers::{FontStyle, FontWeight};
use crate::rendertree::text::glyphs::{
  AtlasPacker, Cell, CellKind, CellRequest, FontSet, InsertOutcome, Rasterizer, ShapedGlyphs, BYTES_PER_TEXEL,
};
use crate::rendertree::text::{prepare_units, RunStyle, ShaperKind};
use crate::rendertree::{FontPayload, PaintState, PlatformContext};
use std::borrow::Cow;

const NOTO_SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSans.ttf");

// A shaper's advance against Impeller's for the same word: within a texel
// at 16 px (both are HarfBuzz over the same file; the rounding differs).
const ADVANCE_TOLERANCE_PX: f32 = 1.0;
// The size every seam check shapes at.
const SIZE: f32 = 16.0;

fn noto() -> FontPayload {
  FontPayload { alias: Some("sans".to_string()), bytes: Cow::Borrowed(NOTO_SANS) }
}

fn style(weight: FontWeight) -> RunStyle {
  RunStyle {
    font_family: "sans".to_string(),
    font_size: SIZE,
    font_style: FontStyle::Normal,
    font_weight: weight,
    line_height: 0.0,
    paint: PaintState::default(),
  }
}

#[test]
fn font_set_resolves_alias_family_and_fallback() {
  let fonts = FontSet::from_payloads(&[noto()]);
  assert_eq!(fonts.resolve("sans"), Some(0));
  assert_eq!(fonts.resolve("Noto Sans"), Some(0));
  // An unknown family falls back to the sans role, as Impeller falls back
  // to its default family.
  assert_eq!(fonts.resolve("No Such Font"), Some(0));
  assert!(FontSet::default().resolve("sans").is_none());
}

#[test]
fn shipped_noto_is_variable_in_weight() {
  let fonts = FontSet::from_payloads(&[noto()]);
  let face = fonts.face(0).expect("face registered");
  assert_eq!(face.weight_setting(700), Some(700.0));
  assert!(!face.synthetic_bold(700), "a weight axis replaces synthetic bold");
  // Bold advances differ from regular ones on a variable font.
  let regular = ShapedGlyphs::shape(face, 400, "Hamburg", SIZE, 0.0).expect("shaped");
  let bold = ShapedGlyphs::shape(face, 700, "Hamburg", SIZE, 0.0).expect("shaped");
  assert!(bold.metrics.advance > regular.metrics.advance);
}

#[test]
fn engine_and_impeller_agree_on_advance() {
  let platform = PlatformContext::new(vec![noto()]);
  for word in ["Hello", "world,", "AVAST", "fi", "kerning"] {
    for weight in [FontWeight::Regular, FontWeight::Bold] {
      let impeller = prepare_units(&platform, ShaperKind::Impeller, word, &style(weight), &[], false);
      let engine = prepare_units(&platform, ShaperKind::Engine, word, &style(weight), &[], false);
      assert_eq!(engine.len(), 1, "one unit for one word");
      let (a, b) = (impeller[0].metrics, engine[0].metrics);
      assert!(
        (a.advance - b.advance).abs() <= ADVANCE_TOLERANCE_PX,
        "{word} at {weight:?}: Impeller advance {} vs engine {}",
        a.advance,
        b.advance
      );
      assert!((a.ascent - b.ascent).abs() <= ADVANCE_TOLERANCE_PX, "{word}: ascent {} vs {}", a.ascent, b.ascent);
      assert!((a.descent - b.descent).abs() <= ADVANCE_TOLERANCE_PX, "{word}: descent {} vs {}", a.descent, b.descent);
      assert!(engine[0].glyphs.is_some(), "the engine's unit carries its glyphs");
      assert!(impeller[0].glyphs.is_none(), "Impeller exposes none");
    }
  }
}

#[test]
fn engine_carets_follow_the_cluster_map() {
  let platform = PlatformContext::new(vec![noto()]);
  let units = prepare_units(&platform, ShaperKind::Engine, "ab", &style(FontWeight::Regular), &[], true);
  let unit = &units[0];
  let stops = unit.carets.as_ref().expect("carets asked for");
  let glyphs = &unit.glyphs.as_ref().expect("engine glyphs").glyphs;
  assert_eq!(stops.len(), 3, "start, after a, after b");
  assert_eq!(stops[0].x, 0.0);
  assert_eq!(stops[1].x, glyphs[1].x, "the stop after a is b's pen position");
  assert_eq!(stops[2].x, unit.metrics.advance, "the last stop is the advance");
  // The Impeller path, shaping each prefix, lands within a texel.
  let prefix = prepare_units(&platform, ShaperKind::Impeller, "ab", &style(FontWeight::Regular), &[], true);
  let theirs = prefix[0].carets.as_ref().expect("carets");
  for (ours, theirs) in stops.iter().zip(theirs.iter()) {
    assert_eq!(ours.offset, theirs.offset);
    assert!((ours.x - theirs.x).abs() <= ADVANCE_TOLERANCE_PX, "caret {} vs {}", ours.x, theirs.x);
  }
}

#[test]
fn mask_cell_holds_the_glyph_coverage() {
  let fonts = FontSet::from_payloads(&[noto()]);
  let face = fonts.face(0).expect("face");
  let shaped = ShapedGlyphs::shape(face, 400, "H", 32.0, 0.0).expect("shaped");
  let glyph = shaped.glyphs[0].id;
  let request = CellRequest {
    kind: CellKind::Mask { ppem: 32.0 },
    weight: face.weight_setting(400),
    synthetic_bold: false,
    synthetic_italic: false,
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
    glyph,
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
  let dirty = packer.take_dirty().expect("something changed").expect("rects, not a growth");
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
      InsertOutcome::Grew => grew = true,
      InsertOutcome::Full => panic!("full before the cap"),
      InsertOutcome::Placed => {}
    }
  }
  assert!(grew);
  assert_ne!(packer.size(), (w0, h0));
  assert_eq!(packer.len(), 60);
  assert_eq!(packer.take_dirty(), Some(None), "a growth is the whole mirror");
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
  let fonts = FontSet::from_payloads(&[noto()]);
  let face = fonts.face(0).expect("face");
  let shaped = ShapedGlyphs::shape(face, 400, "H", MSDF_PPEM, 0.0).expect("shaped");
  let request = CellRequest {
    kind: CellKind::Msdf { ppem: MSDF_PPEM, range: MSDF_RANGE },
    weight: face.weight_setting(400),
    synthetic_bold: false,
    synthetic_italic: false,
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
  let fonts = FontSet::from_payloads(&[noto()]);
  let face = fonts.face(0).expect("face");
  let space = ShapedGlyphs::shape(face, 400, " ", SIZE, 0.0).expect("shaped").glyphs[0].id;
  for kind in [CellKind::Mask { ppem: SIZE }, CellKind::Msdf { ppem: MSDF_PPEM, range: MSDF_RANGE }] {
    let request = CellRequest {
      kind,
      weight: face.weight_setting(400),
      synthetic_bold: false,
      synthetic_italic: false,
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
