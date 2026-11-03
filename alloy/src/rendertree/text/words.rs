// The shared word cache: shaped single-line words keyed on (text, the run
// style's shaping half), one per platform context, so a piece of text seen
// once (in any text, at any width, in any color) is never shaped again while
// it stays hot. The text is a wrap unit (what layout measures) or a line's
// run of joined units (what paint draws, see Text::build); both are plain
// entries. Bounded by bytes and scan-resistant: a segmented LRU, the
// ordering within each segment the `lru` crate's; this chooses the key, the
// value, the segments, the budgets and the counters. Cleared when the
// registered fonts change.
//
// The segments: a word enters PROBATION when it is shaped and moves to
// PROTECTED on its first hit there; a word PROTECTED evicts drops back to
// the front of PROBATION rather than out, and only PROBATION evicts for
// good. So a pass over thousands of words seen once (prepareText over a
// long document) churns PROBATION only, and the words the screen keeps
// drawing, each hit every paint, stay where it cannot reach them. A plain
// LRU let one such pass flush them all, and every text on screen re-shaped
// on the frames after it.
//
// This is the ONLY place shaped words are kept: a text's own cache holds
// metrics and piece strings, and paint fetches the glyphs per visible run
// from here (a miss shapes on the spot and counts as a wordShape). So the
// working set is what was recently drawn or prepared, bounded by the two
// budgets, however long the mounted content is.
use super::glyphs::{weight_value, Fallback, FontSet, PlacedGlyph, ShapeStyle, ShapedGlyphs};
use super::{FontStyle, FontWeight, RunStyle};
use crate::rendertree::text::layout::RunMetrics;
use lru::LruCache;
use std::rc::Rc;

// Bytes the protected words may take: the words hit at least once after
// being shaped. At the 650 to 760 bytes a word measured on a license corpus
// (resident growth and the structures summed) that is some 6k words,
// several text-heavy screens' worth of the words drawn every paint. No pass
// over new words reaches them, whatever its length.
const PROTECTED_BYTES: usize = 4 << 20;
// Bytes for the words shaped and not hit since, plus the protected words
// demoted to wait for a second chance: some 17k words. How long a word
// seen once survives, so it is the larger segment: a long document's whole
// vocabulary (14.7k words for 600k characters of license text) is still
// here when the text is prepared again, and that prepare shapes nothing.
// A smaller one let every word the text uses once fall out before its
// second prepare came round to it.
const PROBATION_BYTES: usize = 12 << 20;
// What the allocator adds to every allocation (glibc's chunk header and its
// 16-byte rounding, about): an entry is five allocations, six with carets.
const ALLOCATION_OVERHEAD: usize = 16;
// One slot of lru's hash map: a 16-byte bucket and its control byte at the
// table's 7/8 load.
const HASH_SLOT_BYTES: usize = 20;

/// The half of a run style that shaping reads, by bits: what a cached word
/// is valid for. Paint is not in it, so the same word in another color, or
/// in a color that animates, is the same entry.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ShapingKey {
  font_family: String,
  font_size: u32,
  font_style: FontStyle,
  font_weight: FontWeight,
  font_stretch: u32,
  line_height: u32,
  letter_spacing: u32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct WordKey {
  text: String,
  style: ShapingKey,
  fallback: Fallback,
}

impl WordKey {
  fn new(text: &str, style: &RunStyle, fallback: Fallback) -> Self {
    let style = ShapingKey {
      font_family: style.font_family.clone(),
      font_size: style.font_size.to_bits(),
      font_style: style.font_style,
      font_weight: style.font_weight,
      font_stretch: style.font_stretch.to_bits(),
      line_height: style.line_height.to_bits(),
      letter_spacing: style.letter_spacing.to_bits(),
    };
    Self { text: text.to_string(), style, fallback }
  }
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

// What an entry costs: the key and the value inline in lru's boxed node
// with its two links, the hash slot, the shared glyph record, the strings
// and the glyph and caret buffers, each allocation with its overhead.
fn entry_bytes(key: &WordKey, word: &ShapedWord) -> usize {
  let node = std::mem::size_of::<WordKey>() + std::mem::size_of::<ShapedWord>() + 2 * std::mem::size_of::<usize>();
  // The Rc's two counts and the glyph record.
  let record = 2 * std::mem::size_of::<usize>() + std::mem::size_of::<ShapedGlyphs>();
  let glyphs = word.glyphs.glyphs.capacity() * std::mem::size_of::<PlacedGlyph>();
  let strings = key.text.len() + key.style.font_family.len();
  let carets = word.carets.as_ref().map_or(0, |stops| {
    2 * std::mem::size_of::<usize>() + stops.len() * std::mem::size_of::<CaretStop>() + ALLOCATION_OVERHEAD
  });
  node + HASH_SLOT_BYTES + record + glyphs + strings + carets + 5 * ALLOCATION_OVERHEAD
}

// One segment: an LRU bounded by the bytes its entries cost.
struct Segment {
  words: LruCache<WordKey, ShapedWord>,
  bytes: usize,
  budget: usize,
}

impl Segment {
  fn new(budget: usize) -> Self {
    Self { words: LruCache::unbounded(), bytes: 0, budget }
  }

  // `key`'s word, moved to the front.
  fn get(&mut self, key: &WordKey) -> Option<&ShapedWord> {
    self.words.get(key)
  }

  fn peek(&self, key: &WordKey) -> Option<&ShapedWord> {
    self.words.peek(key)
  }

  fn pop(&mut self, key: &WordKey) -> Option<ShapedWord> {
    let word = self.words.pop(key)?;
    self.bytes -= entry_bytes(key, &word);
    Some(word)
  }

  // Insert at the front, then evict from the cold end until the segment is
  // within its budget again (never the entry just inserted). Returns the
  // evicted entries, coldest first.
  fn push(&mut self, key: WordKey, word: ShapedWord) -> Vec<(WordKey, ShapedWord)> {
    self.bytes += entry_bytes(&key, &word);
    if let Some((replaced, old)) = self.words.push(key, word) {
      self.bytes -= entry_bytes(&replaced, &old);
    }
    let mut evicted = Vec::new();
    while self.bytes > self.budget && self.words.len() > 1 {
      let Some((key, word)) = self.words.pop_lru() else { break };
      self.bytes -= entry_bytes(&key, &word);
      evicted.push((key, word));
    }
    evicted
  }

  // Keep `stops` on `key`'s entry, its standing unchanged. False when the
  // segment does not hold it.
  fn set_carets(&mut self, key: &WordKey, stops: Rc<[CaretStop]>) -> bool {
    let Some(word) = self.words.peek_mut(key) else { return false };
    let before = entry_bytes(key, word);
    word.carets = Some(stops);
    self.bytes = self.bytes + entry_bytes(key, word) - before;
    true
  }

  fn clear(&mut self) {
    self.words.clear();
    self.bytes = 0;
  }
}

pub struct WordCache {
  probation: Segment,
  protected: Segment,
}

impl Default for WordCache {
  fn default() -> Self {
    Self { probation: Segment::new(PROBATION_BYTES), protected: Segment::new(PROTECTED_BYTES) }
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
    self.with_word(fonts, text, style, fallback, ShapedWord::clone)
  }

  /// `get_or_shape` for a reader that needs only the metrics (layout): read
  /// in place, so a hit clones nothing.
  pub fn metrics(&mut self, fonts: &FontSet, text: &str, style: &RunStyle, fallback: Fallback) -> Option<RunMetrics> {
    self.with_word(fonts, text, style, fallback, |word| word.metrics)
  }

  /// The caret stops of `text` in `style` (shaping it on a miss): one per
  /// grapheme cluster boundary from the word's start (offset 0, x 0) to its
  /// end, in order, read off the cluster map in one pass. Computed once per
  /// cached word and shared; asking for them is not a use of the word.
  pub fn carets(
    &mut self,
    fonts: &FontSet,
    text: &str,
    style: &RunStyle,
    fallback: Fallback,
  ) -> Option<Rc<[CaretStop]>> {
    let key = WordKey::new(text, style, fallback);
    let held = self.protected.peek(&key).or_else(|| self.probation.peek(&key));
    if let Some(stops) = held.and_then(|w| w.carets.clone()) {
      return Some(stops);
    }
    // A held word is read where it sits (no use counted); a missing one is
    // shaped, which is its first.
    let glyphs = match held {
      Some(word) => word.glyphs.clone(),
      None => self.get_or_shape(fonts, text, style, fallback)?.glyphs,
    };
    let stops: Rc<[CaretStop]> = glyphs.caret_stops(text).into();
    if !self.protected.set_carets(&key, stops.clone()) {
      self.probation.set_carets(&key, stops.clone());
    }
    Some(stops)
  }

  pub fn clear(&mut self) {
    self.probation.clear();
    self.protected.clear();
  }

  pub fn len(&self) -> usize {
    self.probation.words.len() + self.protected.words.len()
  }

  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  /// Whether `text` in `style` is in the protected segment: what the tests
  /// read to pin which words a pass may not evict.
  #[cfg(test)]
  pub(crate) fn is_protected(&self, text: &str, style: &RunStyle, fallback: Fallback) -> bool {
    self.protected.words.contains(&WordKey::new(text, style, fallback))
  }

  /// The bytes each segment's entries cost, (probation, protected), and the
  /// budgets they keep to: what the tests read.
  #[cfg(test)]
  pub(crate) fn bytes(&self) -> ((usize, usize), (usize, usize)) {
    ((self.probation.bytes, self.probation.budget), (self.protected.bytes, self.protected.budget))
  }

  // The word for `text` in `style` handed to `read` where it sits, shaping it
  // on a miss.
  fn with_word<R>(
    &mut self,
    fonts: &FontSet,
    text: &str,
    style: &RunStyle,
    fallback: Fallback,
    read: impl FnOnce(&ShapedWord) -> R,
  ) -> Option<R> {
    let key = WordKey::new(text, style, fallback);
    // A protected word moves to the front of its segment: one lookup, the
    // hit every paint of a word on screen takes.
    if let Some(word) = self.protected.get(&key) {
      crate::rendertree::counters::note_word_hit();
      return Some(read(word));
    }
    // A probation word's second use promotes it; the words PROTECTED evicts
    // for it drop back to probation's front.
    if let Some(word) = self.probation.pop(&key) {
      crate::rendertree::counters::note_word_hit();
      let out = read(&word);
      for (demoted, demoted_word) in self.protected.push(key, word) {
        self.probation.push(demoted, demoted_word);
      }
      return Some(out);
    }
    let word = shape(fonts, text, style, fallback)?;
    let out = read(&word);
    self.probation.push(key, word);
    Some(out)
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
