// Shaping a word on the glyph engine: harfrust shapes the text in font
// units at the face's weight instance, and the result is scaled to the run
// size and reduced to what the owned layout and a glyph consumer need - the
// pen position of every glyph, the run metrics (`RunMetrics`, the same
// numbers Impeller's single-line paragraph reports, so a word shaped either
// way lays out the same) and the cluster map, from which caret stops are
// one pass rather than a re-shape per grapheme prefix.
use super::fonts::Face;
use crate::rendertree::text::layout::RunMetrics;
use crate::rendertree::text::CaretStop;
use harfrust::{Buffer, GlyphId, ShapeOptions, ShaperFont};
use unicode_segmentation::UnicodeSegmentation;

/// One glyph of a shaped word, in run pixels from the word's pen origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
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

impl ShapedGlyphs {
  /// Shape `text` with `face` at `weight` and `size` pixels per em.
  /// `line_height` is the run's multiplier (0 for the font's natural line),
  /// applied as Impeller does: the line box becomes `size * line_height`
  /// and the font's ascent and descent scale into it in their own ratio.
  /// Returns None when harfrust cannot shape the text (a malformed font).
  pub fn shape(face: &Face, weight: u16, text: &str, size: f32, line_height: f32) -> Option<Self> {
    let units_per_em = face.units_per_em();
    let vertical = vertical_metrics(face, weight, size, line_height);
    let font = face.instance(weight);
    let shaper = ShaperFont::new(&font);
    let mut buffer = Buffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    harfrust::shape(&shaper, &mut buffer, ShapeOptions::new()).ok()?;
    // harfrust positions in font units; one factor to run pixels.
    let scale = size / units_per_em;
    let mut glyphs = Vec::with_capacity(buffer.len());
    let mut pen = 0.0f32;
    for (info, pos) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
      let advance = pos.x_advance as f32 * scale;
      glyphs.push(PlacedGlyph {
        id: GlyphId::new(info.glyph_id).to_u32() as u16,
        x: pen + pos.x_offset as f32 * scale,
        // Shaper y offsets are y-up; runs are y-down.
        y: -(pos.y_offset as f32) * scale,
        advance,
        cluster: info.cluster,
      });
      pen += advance;
    }
    let ink_width = ink_width(text, &glyphs);
    let (ascent, descent) = vertical;
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

/// The run's ascent and descent at `size`: the font's hhea values scaled,
/// the leading folded into the descent (Impeller reports the line box as
/// ascent plus descent, leading included below the baseline), the whole
/// box re-scaled to `size * line_height` when a multiplier is set.
fn vertical_metrics(face: &Face, weight: u16, size: f32, line_height: f32) -> (f32, f32) {
  let bytes = face.bytes().as_ref().as_ref();
  let Some(font) = swash::FontRef::from_index(bytes, 0) else {
    return (size, 0.0);
  };
  let coords = weight_coords(&font, face.weight_setting(weight));
  let metrics = font.metrics(&coords).scale(size);
  let ascent = metrics.ascent;
  let descent = metrics.descent + metrics.leading;
  let natural = ascent + descent;
  if line_height > 0.0 && natural > 0.0 {
    let factor = line_height * size / natural;
    (ascent * factor, descent * factor)
  } else {
    (ascent, descent)
  }
}

/// The normalized coordinates swash wants for a weight axis setting, in the
/// font's axis order (every other axis at its default).
pub(super) fn weight_coords(font: &swash::FontRef<'_>, weight: Option<f32>) -> Vec<swash::NormalizedCoord> {
  let variations = font.variations();
  let mut coords = vec![0; variations.len()];
  if let Some(value) = weight {
    for var in variations {
      if var.tag() == swash::tag_from_bytes(b"wght") {
        coords[var.index()] = var.normalize(value);
      }
    }
  }
  coords
}

/// The pen position after the last cluster that is not whitespace: what
/// must fit on a line (Impeller's longest line width, which excludes the
/// trailing spaces the unit's advance includes).
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
