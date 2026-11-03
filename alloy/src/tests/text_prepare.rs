// Prepared text (rendertree/text/shape.rs `prepare_units`, `PreparedColumns`)
// and the shared word cache it shapes through (words.rs): the cache's scan
// resistance, its byte budgets, its key, and the column form a script reads.
use crate::renderer::Color;
use crate::rendertree::text::{
  prepare_units, Fallback, FontStyle, FontWeight, PreparedColumns, PreparedRun, RunStyle, WordCache,
};
use crate::rendertree::PaintState;
use std::rc::Rc;

// The size every check shapes at.
const SIZE: f32 = 16.0;
// Bound on the one-off words any pass here takes: far more than either
// segment's budget holds, so a loop that reaches it is a broken budget.
const BULK_WORDS_MAX: usize = 100_000;
// How close to its budget probation counts as full: less than one entry.
const FULL_SLACK_BYTES: usize = 1024;
// Distinct words in the long text: the 600k-character license corpus's
// vocabulary (14.7k) and some.
const LONG_TEXT_WORDS: usize = 15_000;

fn style(size: f32) -> RunStyle {
  RunStyle {
    font_family: "sans".to_string(),
    font_size: size,
    font_style: FontStyle::Normal,
    font_weight: FontWeight::Regular,
    font_stretch: 100.0,
    line_height: 0.0,
    letter_spacing: 0.0,
    paint: PaintState::default(),
  }
}

fn shapes_taken() -> u32 {
  crate::rendertree::counters::take().word_shapes
}

// A word the screen draws again and again is protected; a pass over
// one-off words churns probation only, however long it is.
#[test]
fn a_bulk_pass_leaves_hot_words_in_the_cache() {
  let platform = super::text_platform();
  let fonts = platform.glyphs();
  let mut words = platform.words();
  let s = style(SIZE);
  for _ in 0..2 {
    words.get_or_shape(&fonts, "hot", &s, Fallback::Registered).expect("shaped");
  }
  assert!(words.is_protected("hot", &s, Fallback::Registered), "a second use promotes");
  words.get_or_shape(&fonts, "once", &s, Fallback::Registered).expect("shaped");
  assert!(!words.is_protected("once", &s, Fallback::Registered), "a first use does not");
  // One-off words until probation is full, then as many again: the pass
  // has turned probation over at least once.
  let mut full_at = None;
  let mut i = 0;
  loop {
    words.get_or_shape(&fonts, &format!("w{i}"), &s, Fallback::Registered).expect("shaped");
    i += 1;
    let ((probation, budget), _) = words.bytes();
    if full_at.is_none() && probation + FULL_SLACK_BYTES >= budget {
      full_at = Some(i);
    }
    if full_at.is_some_and(|n| i >= 2 * n) {
      break;
    }
    assert!(i < BULK_WORDS_MAX, "probation never filled");
  }
  assert!(words.is_protected("hot", &s, Fallback::Registered), "the bulk pass did not reach it");
  shapes_taken();
  words.get_or_shape(&fonts, "hot", &s, Fallback::Registered).expect("shaped");
  assert_eq!(shapes_taken(), 0, "a hit, not a re-shape");
  words.get_or_shape(&fonts, "once", &s, Fallback::Registered).expect("shaped");
  assert_eq!(shapes_taken(), 1, "the one-off word was evicted");
}

// Each segment keeps to its byte budget, and between them they hold far
// more than the 8192 words the count-bounded cache did.
#[test]
fn segments_keep_to_their_byte_budgets() {
  let platform = super::text_platform();
  let fonts = platform.glyphs();
  let mut words = WordCache::default();
  let s = style(SIZE);
  let mut i = 0;
  loop {
    let word = format!("word{i}");
    // Twice: the second use promotes, so protected fills too.
    words.get_or_shape(&fonts, &word, &s, Fallback::Registered).expect("shaped");
    words.get_or_shape(&fonts, &word, &s, Fallback::Registered).expect("shaped");
    i += 1;
    let ((probation, probation_budget), (protected, protected_budget)) = words.bytes();
    assert!(probation <= probation_budget, "probation {probation} over {probation_budget}");
    assert!(protected <= protected_budget, "protected {protected} over {protected_budget}");
    if probation + FULL_SLACK_BYTES >= probation_budget {
      break;
    }
    assert!(i < BULK_WORDS_MAX, "the segments never filled");
  }
  assert!(words.len() > 8192, "{} words", words.len());
}

// A long text of words each used once (a license list's vocabulary) is
// still held when it is prepared again: probation outlives one pass.
#[test]
fn a_long_texts_second_prepare_shapes_nothing() {
  let platform = super::text_platform();
  let text: String = (0..LONG_TEXT_WORDS).map(|i| format!("w{i} ")).collect();
  prepare_units(&platform, &text, &style(SIZE), &[], false, Fallback::Registered);
  shapes_taken();
  let units = prepare_units(&platform, &text, &style(SIZE), &[], false, Fallback::Registered);
  assert_eq!(units.len(), LONG_TEXT_WORDS);
  assert_eq!(shapes_taken(), 0);
}

// Paint is not part of the key: the same word in another color is a hit.
#[test]
fn a_word_in_another_color_is_the_same_entry() {
  let platform = super::text_platform();
  let fonts = platform.glyphs();
  let mut words = platform.words();
  let plain = style(SIZE);
  let mut red = style(SIZE);
  red.paint.color = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
  words.get_or_shape(&fonts, "colored", &plain, Fallback::Registered).expect("shaped");
  shapes_taken();
  words.get_or_shape(&fonts, "colored", &red, Fallback::Registered).expect("shaped");
  assert_eq!(shapes_taken(), 0);
  assert_eq!(words.len(), 1);
  // A shaping input still is part of it.
  words.get_or_shape(&fonts, "colored", &style(SIZE * 2.0), Fallback::Registered).expect("shaped");
  assert_eq!(shapes_taken(), 1);
}

// Asking for carets is not a second use: it does not promote, and the
// stops count toward the word's bytes.
#[test]
fn carets_do_not_count_as_a_use() {
  let platform = super::text_platform();
  let fonts = platform.glyphs();
  let mut words = platform.words();
  let s = style(SIZE);
  words.get_or_shape(&fonts, "caret", &s, Fallback::Registered).expect("shaped");
  let without = words.bytes().0 .0;
  words.carets(&fonts, "caret", &s, Fallback::Registered).expect("stops");
  assert!(!words.is_protected("caret", &s, Fallback::Registered));
  assert!(words.bytes().0 .0 > without);
}

// Columns: UTF-16 offsets into the text (a JS string's indexing) past
// characters of two bytes, four bytes and a surrogate pair; flags; the run
// column; carets absolute; the glyph table indexed per unit.
#[test]
fn columns_index_the_text_as_utf16() {
  let platform = super::text_platform();
  let text = "na\u{ef}ve \u{1F600} ok\nnext";
  let base = style(SIZE);
  // "ok" in a larger size: its own run.
  let ok = text.find("ok").expect("ok");
  let runs = [PreparedRun { start: ok, end: ok + 2, style: style(SIZE * 2.0) }];
  let units = prepare_units(&platform, text, &base, &runs, true, Fallback::Registered);
  let columns = PreparedColumns::from_units(text, &units, true, true, true);
  let utf16: Vec<u16> = text.encode_utf16().collect();
  assert_eq!(columns.len(), 4);
  assert_eq!(columns.start, vec![0, 6, 9, 12]);
  assert_eq!(columns.end, vec![6, 9, 12, utf16.len() as u32]);
  assert_eq!(columns.hard_break, vec![0, 0, 1, 0]);
  assert_eq!(columns.glue, vec![0, 0, 0, 0]);
  assert_eq!(columns.run, Some(vec![-1, -1, 0, -1]));
  assert!(columns.ascent[2] > columns.ascent[0], "the run's larger size");
  let carets = columns.carets.as_ref().expect("carets asked for");
  assert_eq!(carets.first.len(), 5);
  // "naive ": a stop per grapheme from the unit's start to its shaped end
  // (the trailing space is shaped with the word; only break characters
  // are not).
  let naive = &carets.offset[carets.first[0] as usize..carets.first[1] as usize];
  assert_eq!(naive, &[0, 1, 2, 3, 4, 5, 6]);
  // The emoji is one grapheme of two UTF-16 units.
  let emoji = &carets.offset[carets.first[1] as usize..carets.first[2] as usize];
  assert_eq!(emoji, &[6, 8, 9]);
  let glyphs = columns.glyphs.as_ref().expect("glyphs asked for");
  assert_eq!(glyphs.first.len(), 5);
  assert_eq!(*glyphs.first.last().expect("end") as usize, glyphs.id.len());
  assert_eq!(glyphs.first[1] - glyphs.first[0], 6, "one glyph per character of \"naive \"");
}

// Without the optional parts the columns carry none of them.
#[test]
fn columns_leave_out_what_was_not_asked_for() {
  let platform = super::text_platform();
  let text = "two words";
  let units = prepare_units(&platform, text, &style(SIZE), &[], false, Fallback::Registered);
  let columns = PreparedColumns::from_units(text, &units, false, false, false);
  assert_eq!(columns.len(), 2);
  assert!(columns.run.is_none() && columns.carets.is_none() && columns.glyphs.is_none());
  let empty = PreparedColumns::from_units("", &[], false, true, false);
  assert!(empty.is_empty());
  assert_eq!(empty.carets.expect("carets").first, vec![0]);
}

// The units are shared, not copied: the glyphs of a word live once. (The
// break character is not shaped, so "echo\n" is the word "echo".)
#[test]
fn units_share_the_shaped_word() {
  let platform = super::text_platform();
  let twice = prepare_units(&platform, "echo\necho", &style(SIZE), &[], false, Fallback::Registered);
  assert_eq!(twice.len(), 2);
  assert!(Rc::ptr_eq(&twice[0].glyphs, &twice[1].glyphs));
}
