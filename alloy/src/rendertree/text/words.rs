// The shared word cache: shaped single-line paragraphs keyed on (unit text,
// resolved run style), one per platform context, so a word seen once (in any
// text, at any width) is never shaped again while it stays hot. Bounded LRU;
// the ordering and eviction are the `lru` crate's, this only chooses the key,
// the value and the counters. Cleared when the registered fonts change.
//
// This is the ONLY place shaped paragraphs are kept: a text's own cache holds
// metrics and piece strings, and paint fetches the paragraph per visible run
// from here (a miss shapes on the spot and counts as a paraShape). So the
// paragraph working set is what was recently drawn, bounded by CAPACITY,
// however long the mounted content is.
use super::RunStyle;
use crate::impellers::{Paragraph, ParagraphBuilder, ParagraphStyle, TypographyContext};
use crate::rendertree::text::layout::RunMetrics;
use lru::LruCache;
use std::num::NonZeroUsize;

// Distinct (word, style) pairs kept. A text-heavy screen is one to two
// thousand words; this holds several screens' worth before the oldest go.
const CAPACITY: usize = 8192;

#[derive(Clone, PartialEq, Eq, Hash)]
struct WordKey {
  text: String,
  style: RunStyle,
}

#[derive(Clone)]
pub struct ShapedWord {
  pub paragraph: Paragraph,
  pub metrics: RunMetrics,
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
  /// The shaped word for `text` in `style`, shaping it on a miss. None only
  /// when the paragraph builder itself fails.
  pub fn get_or_shape(&mut self, typography: &TypographyContext, text: &str, style: &RunStyle) -> Option<ShapedWord> {
    let key = WordKey { text: text.to_string(), style: style.clone() };
    if let Some(word) = self.words.get(&key) {
      crate::rendertree::counters::note_word_hit();
      return Some(word.clone());
    }
    let word = shape(typography, text, &paragraph_style(style))?;
    self.words.put(key, word.clone());
    Some(word)
  }

  pub fn clear(&mut self) {
    self.words.clear();
  }

  pub fn len(&self) -> usize {
    self.words.len()
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
  Some(ShapedWord { paragraph, metrics })
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
