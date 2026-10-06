// The shared word cache: shaped single-line words keyed on (text, resolved
// run style, shaper), one per platform context, so a piece of text seen
// once (in any text, at any width) is never shaped again while it stays
// hot. The text is a wrap unit (what layout measures) or a line's run of
// joined units (what paint draws, see Text::build); both are plain entries.
// Bounded LRU; the ordering and eviction are the `lru` crate's, this only
// chooses the key, the value and the counters. Cleared when the registered
// fonts change.
//
// This is the ONLY place shaped words are kept: a text's own cache holds
// metrics and piece strings, and paint fetches the paragraph per visible
// run from here (a miss shapes on the spot and counts as a paraShape). So
// the working set is what was recently drawn, bounded by CAPACITY, however
// long the mounted content is.
//
// Two shapers sit behind the one seam (`Shaper`): Impeller's paragraph
// builder, which also draws (`Shaped::Paragraph`, what every `<text>` paints
// with), and the glyph engine (`Shaped::Glyphs`, glyph ids and kerned
// positions for a consumer that draws glyphs itself). A word shaped either
// way carries the same `RunMetrics`, so layout does not care which; the
// key tells them apart.
use super::glyphs::{weight_value, FontSet, ShapedGlyphs};
use super::RunStyle;
use crate::impellers::{Paragraph, ParagraphBuilder, ParagraphStyle, TypographyContext};
use crate::rendertree::text::layout::RunMetrics;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::rc::Rc;

// Distinct (word, style) pairs kept. A text-heavy screen is one to two
// thousand words; this holds several screens' worth before the oldest go.
const CAPACITY: usize = 8192;

#[derive(Clone, PartialEq, Eq, Hash)]
struct WordKey {
  text: String,
  style: RunStyle,
  engine: bool,
}

/// Which shaper a word goes through, with what it shapes against.
#[derive(Clone, Copy)]
pub enum Shaper<'a> {
  /// Impeller's paragraph builder: the words every `<text>` draws.
  Impeller(&'a TypographyContext),
  /// The glyph engine: words a consumer draws glyph by glyph.
  Engine(&'a FontSet),
}

impl Shaper<'_> {
  fn is_engine(&self) -> bool {
    matches!(self, Shaper::Engine(_))
  }
}

/// What a shaped word holds, per shaper.
#[derive(Clone)]
pub enum Shaped {
  /// Drawn by `draw_paragraph`.
  Paragraph(Paragraph),
  /// Drawn by whoever asked: glyph ids, positions and the cluster map.
  Glyphs(Rc<ShapedGlyphs>),
}

#[derive(Clone)]
pub struct ShapedWord {
  pub shaped: Shaped,
  pub metrics: RunMetrics,
  // Caret stops, computed on first request (editing asks, layout never does).
  carets: Option<Rc<[CaretStop]>>,
}

/// A caret position inside a shaped word: the UTF-16 offset of a grapheme
/// cluster boundary (relative to the word's start) and the pen x there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretStop {
  pub offset: u32,
  pub x: f32,
}

pub struct WordCache {
  words: LruCache<WordKey, ShapedWord>,
}

impl Default for WordCache {
  fn default() -> Self {
    Self { words: LruCache::new(NonZeroUsize::new(CAPACITY).expect("word cache capacity is non-zero")) }
  }
}

impl WordCache {
  /// The shaped word for `text` in `style` on `shaper`, shaping it on a
  /// miss. None only when the shaper itself fails (the paragraph builder,
  /// a face the engine cannot shape with).
  pub fn get_or_shape(&mut self, shaper: Shaper<'_>, text: &str, style: &RunStyle) -> Option<ShapedWord> {
    let key = WordKey { text: text.to_string(), style: style.clone(), engine: shaper.is_engine() };
    if let Some(word) = self.words.get(&key) {
      crate::rendertree::counters::note_word_hit();
      return Some(word.clone());
    }
    let word = match shaper {
      Shaper::Impeller(typography) => shape(typography, text, &paragraph_style(style))?,
      Shaper::Engine(fonts) => shape_engine(fonts, text, style)?,
    };
    self.words.put(key, word.clone());
    Some(word)
  }

  /// The caret stops of `text` in `style` (shaping it on a miss): one per
  /// grapheme cluster boundary from the word's start (offset 0, x 0) to its
  /// end, in order. Computed once per cached word and shared.
  pub fn carets(&mut self, shaper: Shaper<'_>, text: &str, style: &RunStyle) -> Option<Rc<[CaretStop]>> {
    let key = WordKey { text: text.to_string(), style: style.clone(), engine: shaper.is_engine() };
    if let Some(stops) = self.words.get(&key).and_then(|w| w.carets.clone()) {
      return Some(stops);
    }
    let stops: Rc<[CaretStop]> = match shaper {
      Shaper::Impeller(typography) => self.caret_stops(typography, text, style)?.into(),
      // The engine's cluster map gives every stop in one pass.
      Shaper::Engine(_) => match &self.get_or_shape(shaper, text, style)?.shaped {
        Shaped::Glyphs(glyphs) => glyphs.caret_stops(text).into(),
        Shaped::Paragraph(_) => return None,
      },
    };
    self.get_or_shape(shaper, text, style)?;
    if let Some(word) = self.words.get_mut(&key) {
      word.carets = Some(stops.clone());
    }
    Some(stops)
  }

  // Impeller exposes no glyph positions (its glyph-info bounds come back
  // without them), so each grapheme prefix of the word is shaped on its own,
  // through this cache, and its advance is the caret x after that grapheme.
  // Kerning across the cut is lost, a sub-pixel matter for the caret.
  fn caret_stops(&mut self, typography: &TypographyContext, text: &str, style: &RunStyle) -> Option<Vec<CaretStop>> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut stops = vec![CaretStop { offset: 0, x: 0.0 }];
    let mut offset = 0u32;
    for (start, grapheme) in text.grapheme_indices(true) {
      offset += grapheme.encode_utf16().count() as u32;
      let prefix = &text[..start + grapheme.len()];
      let word = self.get_or_shape(Shaper::Impeller(typography), prefix, style)?;
      stops.push(CaretStop { offset, x: word.metrics.advance });
    }
    Some(stops)
  }

  pub fn clear(&mut self) {
    self.words.clear();
  }

  pub fn len(&self) -> usize {
    self.words.len()
  }

  pub fn is_empty(&self) -> bool {
    self.words.is_empty()
  }
}

// One piece of text as a single-line paragraph. An empty piece (a blank
// line) still needs a line box, which a space supplies at zero advance.
fn shape(typography: &TypographyContext, text: &str, style: &ParagraphStyle) -> Option<ShapedWord> {
  let blank = text.is_empty();
  crate::rendertree::counters::note_para_shape();
  let mut para_builder = ParagraphBuilder::new(typography)?;
  para_builder.push_style(style);
  para_builder.add_text(if blank { " " } else { text });
  let paragraph = para_builder.build(f32::MAX)?;
  let height = paragraph.get_height();
  let ascent = paragraph.get_line_metrics().map(|m| m.get_ascent(0) as f32).unwrap_or(height);
  let metrics = RunMetrics {
    advance: if blank { 0.0 } else { paragraph.get_max_intrinsic_width() },
    ink_width: if blank { 0.0 } else { paragraph.get_longest_line_width().max(0.0) },
    ascent,
    descent: (height - ascent).max(0.0),
  };
  Some(ShapedWord { shaped: Shaped::Paragraph(paragraph), metrics, carets: None })
}

// One piece of text on the glyph engine: the face the style's family
// resolves to, at the style's weight. An empty piece shapes to no glyphs
// and the face's line box, as the paragraph path's space does.
fn shape_engine(fonts: &FontSet, text: &str, style: &RunStyle) -> Option<ShapedWord> {
  crate::rendertree::counters::note_para_shape();
  let face = fonts.face(fonts.resolve(&style.font_family)?)?;
  let shaped = ShapedGlyphs::shape(face, weight_value(style.font_weight), text, style.font_size, style.line_height)?;
  let metrics = shaped.metrics;
  Some(ShapedWord { shaped: Shaped::Glyphs(Rc::new(shaped)), metrics, carets: None })
}

// A resolved run style as an Impeller paragraph style: foreground and font,
// without the paragraph-level settings.
pub(super) fn paragraph_style(run: &RunStyle) -> ParagraphStyle {
  let mut style = ParagraphStyle::default();
  let paint = run.paint.to_paint();
  style.set_foreground(&paint);
  style.set_font_family(&run.font_family);
  style.set_font_size(run.font_size);
  style.set_font_style(run.font_style);
  style.set_font_weight(run.font_weight);
  style.set_height(run.line_height);
  style
}
