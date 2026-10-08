// Shaping a word on the glyph engine: harfrust shapes the text in font
// units at the face's weight instance, and the result is scaled to the run
// size and reduced to what the owned layout and a glyph consumer need - the
// pen position of every glyph, the run metrics (`RunMetrics`) and the
// cluster map, from which caret stops are one pass rather than a re-shape
// per grapheme prefix.
//
// Letter spacing is applied here too: after every cluster, the last one
// included, as CSS does. Two policies live here, applied to the clusters
// the face has no glyph for (harfrust's glyph 0, the notdef):
//
// - A cluster of whitespace or control characters (a tab, which no font
//   maps) takes the face's space glyph at the space's advance: blank ink,
//   never a missing-glyph box.
// - Under `Fallback::Registered`, every other such cluster is re-shaped on
//   the next registered face that covers its first character, roles
//   (aliased faces) first, consecutive clusters that pick the same face as
//   one piece so they kern and ligate among themselves. A glyph carries
//   the face it came from. What no registered face covers stays the
//   primary face's notdef box. The line box is the primary face's.
use super::fonts::{axis_location, Face, FaceId, FontSet};
use crate::rendertree::text::layout::RunMetrics;
use crate::rendertree::text::CaretStop;
use harfrust::{Buffer, GlyphId, ShapeOptions, ShaperFont};
use skrifa::instance::Size;
use skrifa::{FontRef, MetadataProvider};
use unicode_segmentation::UnicodeSegmentation;

/// What a word is shaped with besides its face: the run style's shaping
/// half, in the engine's own terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeStyle {
  /// CSS weight number, 100 to 900: the weight axis setting, or the
  /// synthetic bold threshold on a static face.
  pub weight: u16,
  /// CSS font-stretch percentage, 100 normal: the width axis setting; a
  /// face without the axis ignores it.
  pub stretch: f32,
  /// Pixels per em.
  pub size: f32,
  /// The line box as a multiple of `size`, 0 for the font's natural line.
  pub line_height: f32,
  /// Extra advance after every cluster, in pixels, the last one included.
  pub letter_spacing: f32,
}

/// Whether a cluster the run's face lacks may come from another face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fallback {
  /// The next registered face that covers it, roles first: what a `<text>`
  /// draws.
  Registered,
  /// No other face: a glyph is the face's own or its notdef. What a font
  /// handle over one face's atlas wants (`flux:font`), whose cells are that
  /// face's only.
  None,
}

/// One glyph of a shaped word, in run pixels from the word's pen origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
  /// The face the glyph belongs to: the run's, or the fallback that covered
  /// its cluster.
  pub face: FaceId,
  /// The font's glyph id (what a cell is keyed on).
  pub id: u16,
  /// The glyph origin: pen x plus the shaper's offset, y down.
  pub x: f32,
  pub y: f32,
  /// The pen advance this glyph contributes.
  pub advance: f32,
  /// Byte offset into the word of the cluster this glyph draws.
  pub cluster: u32,
}

/// A word shaped on the engine: what the seam hands the layout, the caret
/// path and a glyph consumer.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedGlyphs {
  pub glyphs: Vec<PlacedGlyph>,
  pub metrics: RunMetrics,
}

// A glyph as the shaper places it, before the pens are summed: its offset
// from the pen rather than its position.
#[derive(Clone, Copy)]
struct RawGlyph {
  face: FaceId,
  id: u16,
  x_offset: f32,
  y: f32,
  advance: f32,
  cluster: u32,
}

/// The glyph id of a character the face has no glyph for.
const NOTDEF: u16 = 0;

impl ShapedGlyphs {
  /// Shape `text` on `fonts`' face `primary` in `style`, with `fallback`
  /// deciding what another face may fill in. The style's line height is
  /// applied as Impeller did: the line box becomes `size * line_height`
  /// and the font's ascent and descent scale into it in their own ratio.
  /// Returns None when the face is unknown or harfrust cannot shape the
  /// text (a malformed font).
  pub fn shape(fonts: &FontSet, primary: FaceId, text: &str, style: &ShapeStyle, fallback: Fallback) -> Option<Self> {
    let face = fonts.face(primary)?;
    let mut raw = shape_on(face, primary, style, text, 0)?;
    blank_controls(face, primary, style, text, &mut raw);
    if fallback == Fallback::Registered {
      fill_from_fallbacks(fonts, primary, style, text, &mut raw);
    }
    if style.letter_spacing != 0.0 {
      space_clusters(&mut raw, style.letter_spacing);
    }
    let mut glyphs = Vec::with_capacity(raw.len());
    let mut pen = 0.0f32;
    for g in raw {
      glyphs.push(PlacedGlyph {
        face: g.face,
        id: g.id,
        x: pen + g.x_offset,
        y: g.y,
        advance: g.advance,
        cluster: g.cluster,
      });
      pen += g.advance;
    }
    let ink_width = ink_width(text, &glyphs);
    let (ascent, descent) = vertical_metrics(face, style);
    Some(Self { glyphs, metrics: RunMetrics { advance: pen, ink_width, ascent, descent } })
  }

  /// Caret stops of the word: one per grapheme cluster boundary, UTF-16
  /// offsets from the word's start, x from its pen origin. Read off the
  /// cluster map, which harfrust keeps monotone per grapheme.
  pub fn caret_stops(&self, text: &str) -> Vec<CaretStop> {
    let mut stops = Vec::new();
    let mut offset = 0u32;
    let mut push = |byte: usize, offset: u32| {
      let x = self.glyphs.iter().find(|g| g.cluster as usize >= byte).map_or(self.metrics.advance, |g| g.x);
      stops.push(CaretStop { offset, x });
    };
    push(0, 0);
    for (start, grapheme) in text.grapheme_indices(true) {
      offset += grapheme.encode_utf16().count() as u32;
      push(start + grapheme.len(), offset);
    }
    stops
  }
}

// `text` shaped on `face` as glyphs in text order, their clusters offset
// by `base` (the byte offset of `text` within the word it is a piece of).
fn shape_on(face: &Face, face_id: FaceId, style: &ShapeStyle, text: &str, base: u32) -> Option<Vec<RawGlyph>> {
  let font = face.instance(style.weight, style.stretch);
  let shaper = ShaperFont::new(&font);
  let mut buffer = Buffer::new();
  buffer.push_str(text);
  buffer.guess_segment_properties();
  harfrust::shape(&shaper, &mut buffer, ShapeOptions::new()).ok()?;
  // harfrust positions in font units; one factor to run pixels.
  let scale = style.size / face.units_per_em();
  Some(
    buffer
      .glyph_infos()
      .iter()
      .zip(buffer.glyph_positions())
      .map(|(info, pos)| RawGlyph {
        face: face_id,
        id: GlyphId::new(info.glyph_id).to_u32() as u16,
        x_offset: pos.x_offset as f32 * scale,
        // Shaper y offsets are y-up; runs are y-down.
        y: -(pos.y_offset as f32) * scale,
        advance: pos.x_advance as f32 * scale,
        cluster: base + info.cluster,
      })
      .collect(),
  )
}

// The byte range of the cluster glyph `i` of `raw` draws: from its cluster
// to the next glyph's greater cluster, or to the end of the text (`len`).
fn cluster_range(raw: &[RawGlyph], i: usize, len: usize) -> (usize, usize) {
  let start = raw[i].cluster as usize;
  let end = raw[i + 1..].iter().map(|g| g.cluster as usize).find(|&c| c > start).unwrap_or(len);
  (start, end)
}

// A notdef whose cluster is whitespace or control characters only becomes
// the face's space glyph at the space's advance.
fn blank_controls(face: &Face, face_id: FaceId, style: &ShapeStyle, text: &str, raw: &mut [RawGlyph]) {
  let mut space: Option<RawGlyph> = None;
  for i in 0..raw.len() {
    if raw[i].id != NOTDEF {
      continue;
    }
    let (start, end) = cluster_range(raw, i, text.len());
    if !text[start..end].chars().all(|c| c.is_whitespace() || c.is_control()) {
      continue;
    }
    if space.is_none() {
      space = shape_on(face, face_id, style, " ", 0).and_then(|mut glyphs| glyphs.pop());
    }
    if let Some(space) = space {
      raw[i].id = space.id;
      raw[i].advance = space.advance;
      raw[i].x_offset = 0.0;
      raw[i].y = 0.0;
    }
  }
}

// Every run of notdef glyphs re-shaped on the faces that cover its
// clusters: consecutive clusters covered by the same face go as one piece.
// A cluster no face covers keeps its notdef.
fn fill_from_fallbacks(fonts: &FontSet, primary: FaceId, style: &ShapeStyle, text: &str, raw: &mut Vec<RawGlyph>) {
  if raw.iter().all(|g| g.id != NOTDEF) {
    return;
  }
  let len = text.len();
  let covering = |at: usize| text[at..].chars().next().and_then(|ch| fonts.fallback_face(primary, ch));
  let mut out = Vec::with_capacity(raw.len());
  let mut i = 0;
  while i < raw.len() {
    if raw[i].id != NOTDEF {
      out.push(raw[i]);
      i += 1;
      continue;
    }
    let (start, _) = cluster_range(raw, i, len);
    let face = covering(start);
    // Extend the piece over the glyphs of this cluster and the following
    // notdef clusters the same face covers.
    let mut j = i + 1;
    while j < raw.len() && raw[j].id == NOTDEF {
      let (next, _) = cluster_range(raw, j, len);
      if next != start && covering(next) != face {
        break;
      }
      j += 1;
    }
    let (_, end) = cluster_range(raw, j - 1, len);
    let piece = face
      .and_then(|id| fonts.face(id).map(|f| (id, f)))
      .and_then(|(id, f)| shape_on(f, id, style, &text[start..end], start as u32));
    match piece {
      Some(glyphs) => out.extend(glyphs),
      None => out.extend_from_slice(&raw[i..j]),
    }
    i = j;
  }
  *raw = out;
}

// Letter spacing: `spacing` more advance after the last glyph of every
// cluster (a base and its marks share one), the last cluster included, as
// CSS letter-spacing adds after every character.
fn space_clusters(raw: &mut [RawGlyph], spacing: f32) {
  let len = raw.len();
  for i in 0..len {
    if i + 1 == len || raw[i + 1].cluster != raw[i].cluster {
      raw[i].advance += spacing;
    }
  }
}

/// The run's ascent and descent at the style's size: the font's hhea
/// values scaled, the leading folded into the descent (the line box is
/// reported as ascent plus descent, leading included below the baseline),
/// the whole box re-scaled to `size * line_height` when a multiplier is
/// set.
fn vertical_metrics(face: &Face, style: &ShapeStyle) -> (f32, f32) {
  let bytes = face.bytes().as_ref().as_ref();
  let Ok(font) = FontRef::from_index(bytes, 0) else {
    return (style.size, 0.0);
  };
  let location = axis_location(&font, face.weight_setting(style.weight), face.width_setting(style.stretch));
  let metrics = font.metrics(Size::new(style.size), &location);
  // skrifa's descent is the table's descender, negative below the baseline.
  let ascent = metrics.ascent;
  let descent = -metrics.descent + metrics.leading;
  let natural = ascent + descent;
  if style.line_height > 0.0 && natural > 0.0 {
    let factor = style.line_height * style.size / natural;
    (ascent * factor, descent * factor)
  } else {
    (ascent, descent)
  }
}

/// The pen position after the last cluster that is not whitespace: what
/// must fit on a line (the unit's advance includes the trailing spaces,
/// its ink width does not).
fn ink_width(text: &str, glyphs: &[PlacedGlyph]) -> f32 {
  let ink_end = text.trim_end().len();
  let mut pen = 0.0f32;
  let mut ink = 0.0f32;
  for glyph in glyphs {
    pen += glyph.advance;
    if (glyph.cluster as usize) < ink_end {
      ink = pen;
    }
  }
  ink
}
