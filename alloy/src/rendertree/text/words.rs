// The shared word cache: shaped single-line words keyed on (text, resolved
// run style), one per platform context, so a piece of text seen once (in
// any text, at any width) is never shaped again while it stays hot. The
// text is a wrap unit (what layout measures) or a line's run of joined
// units (what paint draws, see Text::build); both are plain entries.
// Bounded LRU; the ordering and eviction are the `lru` crate's, this only
// chooses the key, the value and the counters. Cleared when the registered
// fonts change.
//
// This is the ONLY place shaped words are kept: a text's own cache holds
// metrics and piece strings, and paint fetches the glyphs per visible run
// from here (a miss shapes on the spot and counts as a wordShape). So the
// working set is what was recently drawn, bounded by CAPACITY, however long
// the mounted content is.
use super::glyphs::{weight_value, Fallback, FontSet, ShapeStyle, ShapedGlyphs};
use super::RunStyle;
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
  fallback: Fallback,
}

/// A word shaped on the glyph engine: its glyph ids, kerned positions and
/// cluster map (what paint draws and carets read), and its run metrics
/// (what layout measures).
#[derive(Clone)]
pub struct ShapedWord {
  pub glyphs: Rc<ShapedGlyphs>,
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
  /// The shaped word for `text` in `style` over `fonts` under `fallback`,
  /// shaping it on a miss. None only when the shaper itself fails (no
  /// registered face, a face harfrust cannot shape with).
  pub fn get_or_shape(
    &mut self,
    fonts: &FontSet,
    text: &str,
    style: &RunStyle,
    fallback: Fallback,
  ) -> Option<ShapedWord> {
    let key = WordKey { text: text.to_string(), style: style.clone(), fallback };
    if let Some(word) = self.words.get(&key) {
      crate::rendertree::counters::note_word_hit();
      return Some(word.clone());
    }
    let word = shape(fonts, text, style, fallback)?;
    self.words.put(key, word.clone());
    Some(word)
  }

  /// The caret stops of `text` in `style` (shaping it on a miss): one per
  /// grapheme cluster boundary from the word's start (offset 0, x 0) to its
  /// end, in order, read off the cluster map in one pass. Computed once per
  /// cached word and shared.
  pub fn carets(
    &mut self,
    fonts: &FontSet,
    text: &str,
    style: &RunStyle,
    fallback: Fallback,
  ) -> Option<Rc<[CaretStop]>> {
    let key = WordKey { text: text.to_string(), style: style.clone(), fallback };
    if let Some(stops) = self.words.get(&key).and_then(|w| w.carets.clone()) {
      return Some(stops);
    }
    let word = self.get_or_shape(fonts, text, style, fallback)?;
    let stops: Rc<[CaretStop]> = word.glyphs.caret_stops(text).into();
    if let Some(word) = self.words.get_mut(&key) {
      word.carets = Some(stops.clone());
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

// One piece of text on the glyph engine: the face the style's family
// resolves to, at the style's weight. An empty piece (a blank line) shapes
// to no glyphs and the face's line box.
fn shape(fonts: &FontSet, text: &str, style: &RunStyle, fallback: Fallback) -> Option<ShapedWord> {
  crate::rendertree::counters::note_word_shape();
  let primary = fonts.resolve(&style.font_family)?;
  let shaping = ShapeStyle {
    weight: weight_value(style.font_weight),
    stretch: style.font_stretch,
    size: style.font_size,
    line_height: style.line_height,
    letter_spacing: style.letter_spacing,
  };
  let shaped = ShapedGlyphs::shape(fonts, primary, text, &shaping, fallback)?;
  let metrics = shaped.metrics;
  Some(ShapedWord { glyphs: Rc::new(shaped), metrics, carets: None })
}
