// Owned shaping: every wrap unit of the text as its own single-line Impeller
// paragraph, cached per input fingerprint (ParaKey), plus the line layouts
// derived from them per width. The breaking itself is text::layout.
use super::{OverflowWrap, Text, TextOverflow, TextRun, ATOM_CHAR, MAX_CACHED_WIDTHS};
use crate::impellers::{
  FontStyle, FontWeight, Paragraph, ParagraphBuilder, ParagraphStyle, Size, TextAlignment, TypographyContext,
};
use crate::rendertree::text::layout::{self, Align, Layout, LineCursor, LineExtent, Run, RunMetrics, Wrap};
use crate::rendertree::text::RunStyle;
use crate::rendertree::PaintState;

// Snapshot of every input that feeds paragraph shaping; the cache is valid
// only while the owning Text still matches it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ParaKey {
  runs: Vec<TextRun>,
  font_family: String,
  font_size: f32,
  font_style: FontStyle,
  font_weight: FontWeight,
  text_alignment: TextAlignment,
  max_lines: u32,
  text_overflow: TextOverflow,
  overflow_wrap: OverflowWrap,
  text_indent: f32,
  text_wrap: Wrap,
  line_height: f32,
  paint: PaintState,
}

impl ParaKey {
  pub(super) fn matches(&self, t: &Text) -> bool {
    self.runs == t.runs
      && self.font_family == t.font_family
      && self.font_size == t.font_size
      && self.font_style == t.font_style
      && self.font_weight == t.font_weight
      && self.text_alignment == t.text_alignment
      && self.max_lines == t.max_lines
      && self.text_overflow == t.text_overflow
      && self.overflow_wrap == t.overflow_wrap
      && self.text_indent == t.text_indent
      && self.text_wrap == t.text_wrap
      && self.line_height == t.line_height
      && self.paint == t.paint
  }

  pub(super) fn of(t: &Text) -> Self {
    Self {
      runs: t.runs.clone(),
      font_family: t.font_family.clone(),
      font_size: t.font_size,
      font_style: t.font_style,
      font_weight: t.font_weight,
      text_alignment: t.text_alignment,
      max_lines: t.max_lines,
      text_overflow: t.text_overflow.clone(),
      overflow_wrap: t.overflow_wrap,
      text_indent: t.text_indent,
      text_wrap: t.text_wrap,
      line_height: t.line_height,
      paint: t.paint.clone(),
    }
  }
}

// One piece of a wrap unit shaped as a single-line paragraph (or an atom's
// box), plus what the breaker needs to know about it and what re-splitting it
// finer needs.
#[derive(Clone)]
pub(super) struct ShapedRun {
  // None for an atom: nothing to draw here, the element paints itself.
  pub(super) paragraph: Option<Paragraph>,
  pub(super) run: Run,
  pub(super) text: String,
  // Index into the per-run styles the runs were shaped with; also the index
  // of the TextRun the piece came from.
  pub(super) style: usize,
}

// A layout for one width. `runs` is Some when overflowing units were
// re-split at grapheme boundaries for this width: the layout's run indices
// then refer to it instead of `OwnedCache::runs`.
#[derive(Clone)]
pub(super) struct OwnedLayout {
  pub(super) width: f32,
  pub(super) layout: Layout,
  pub(super) runs: Option<Vec<ShapedRun>>,
}

#[derive(Clone, Default)]
pub(super) struct OwnedCache {
  pub(super) key: Option<ParaKey>,
  pub(super) runs: Vec<ShapedRun>,
  // Ellipsis run in the paragraph's default style, when text_overflow asks
  // for one.
  pub(super) ellipsis: Option<ShapedRun>,
  pub(super) layouts: Vec<OwnedLayout>,
}

impl OwnedCache {
  pub(super) fn runs_for(&self, index: usize) -> &[ShapedRun] {
    self.layouts[index].runs.as_deref().unwrap_or(&self.runs)
  }
}

impl std::fmt::Debug for OwnedCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "OwnedCache({} runs, {} layouts)", self.runs.len(), self.layouts.len())
  }
}

impl Text {
  // Shape every wrap unit of the current text as its own single-line
  // paragraph, unless the cache already holds them for the current inputs.
  pub(super) fn prepare_owned(&self, typography: &TypographyContext, owned: &mut OwnedCache) {
    if owned.key.as_ref().is_some_and(|k| k.matches(self)) {
      return;
    }
    owned.runs.clear();
    owned.layouts.clear();
    owned.ellipsis = None;
    owned.key = Some(ParaKey::of(self));

    // A wrap unit may straddle styled runs (a code span and the comma glued
    // to it). Each (unit, run) intersection is shaped on its own; pieces
    // after the first are glued so the breaker keeps the unit whole.
    let styles = self.run_styles();
    let mut run_starts = Vec::with_capacity(self.runs.len());
    let mut offset = 0;
    for run in &self.runs {
      run_starts.push(offset);
      offset += run.text.len();
    }
    let mut run_index = 0;
    for segment in layout::segments(&self.computed_text) {
      let mut piece_start = segment.start;
      let mut first = true;
      while piece_start < segment.end {
        while run_index + 1 < self.runs.len() && run_starts[run_index + 1] <= piece_start {
          run_index += 1;
        }
        let run_end = run_starts[run_index] + self.runs[run_index].text.len();
        let piece_end = segment.end.min(run_end);
        let raw = &self.computed_text[piece_start..piece_end];
        let last_piece = piece_end == segment.end;
        let hard_break = segment.hard_break && last_piece;
        let shaped = match self.runs[run_index].atom {
          Some(size) => Self::atom_piece(run_index, size, &self.runs[run_index], hard_break, !first),
          None => {
            // The break characters themselves are not shaped.
            let text = raw.trim_end_matches(['\n', '\r', '\u{2028}', '\u{2029}']);
            let Some(shaped) = Self::shape_piece(typography, &styles, run_index, text, hard_break, !first) else {
              return;
            };
            shaped
          }
        };
        owned.runs.push(shaped);
        first = false;
        piece_start = piece_end;
      }
    }
    if let TextOverflow::Ellipsis(s) = &self.text_overflow {
      let styles = [self.paragraph_style(&self.run_style())];
      owned.ellipsis = Self::shape_piece(typography, &styles, 0, s, false, false);
    }
  }

  // One piece of a wrap unit as a single-line paragraph in `styles[style]`.
  // An empty piece (a blank line) still needs a line box, which a space
  // supplies at zero advance.
  fn shape_piece(
    typography: &TypographyContext,
    styles: &[ParagraphStyle],
    style: usize,
    text: &str,
    hard_break: bool,
    glue: bool,
  ) -> Option<ShapedRun> {
    let blank = text.is_empty();
    crate::rendertree::counters::note_para_shape();
    let mut para_builder = ParagraphBuilder::new(typography)?;
    para_builder.push_style(&styles[style]);
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
    Some(ShapedRun {
      paragraph: Some(paragraph),
      run: Run { metrics, hard_break, glue, float: None, clear: None },
      text: text.to_string(),
      style,
    })
  }

  // An atom's box as a run: as wide as the box on the line, its whole height
  // above the baseline (bottom on the baseline, HTML's default for inline
  // blocks), nothing to draw.
  fn atom_piece(style: usize, size: Size, run: &TextRun, hard_break: bool, glue: bool) -> ShapedRun {
    let metrics = RunMetrics { advance: size.width, ink_width: size.width, ascent: size.height, descent: 0.0 };
    let run = Run { metrics, hard_break, glue, float: run.float, clear: run.clear };
    ShapedRun { paragraph: None, run, text: ATOM_CHAR.to_string(), style }
  }

  // Re-split the wrap units starting at `units` (first-piece indices into
  // `runs`) at grapheme boundaries: every grapheme becomes its own wrap
  // unit, trailing whitespace staying with the grapheme before it. Returns
  // the full run list with those units replaced.
  fn split_graphemes(
    typography: &TypographyContext,
    styles: &[ParagraphStyle],
    runs: &[ShapedRun],
    units: &[usize],
  ) -> Vec<ShapedRun> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = Vec::with_capacity(runs.len() + units.len() * 8);
    let mut i = 0;
    while i < runs.len() {
      // An atom is a box: it overflows whole.
      if !units.contains(&i) || runs[i].paragraph.is_none() {
        out.push(runs[i].clone());
        i += 1;
        continue;
      }
      // The unit: this piece plus the glued ones after it.
      let mut end = i + 1;
      while end < runs.len() && runs[end].run.glue {
        end += 1;
      }
      for piece in &runs[i..end] {
        if piece.paragraph.is_none() {
          out.push(piece.clone());
          continue;
        }
        let mut pending = String::new();
        let flush = |out: &mut Vec<ShapedRun>, pending: &mut String| {
          if pending.is_empty() {
            return;
          }
          if let Some(shaped) = Self::shape_piece(typography, styles, piece.style, pending, false, false) {
            out.push(shaped);
          }
          pending.clear();
        };
        for grapheme in piece.text.graphemes(true) {
          if !grapheme.chars().all(char::is_whitespace) {
            flush(&mut out, &mut pending);
          }
          pending.push_str(grapheme);
        }
        flush(&mut out, &mut pending);
        if piece.run.hard_break {
          if let Some(last) = out.last_mut() {
            last.run.hard_break = true;
          }
        }
      }
      i = end;
    }
    out
  }

  fn run_styles(&self) -> Vec<ParagraphStyle> {
    self.runs.iter().map(|r| self.paragraph_style(&r.overrides.resolve(self))).collect()
  }

  // The per-run style resolved against this Text, as an Impeller paragraph
  // style: foreground and font, without the paragraph-level settings.
  pub(super) fn paragraph_style(&self, run: &RunStyle) -> ParagraphStyle {
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

  // Line layout for `width` from the prepared runs, cached per width like the
  // paragraph path. Pure arithmetic, except that a unit wider than the line
  // is re-split at grapheme boundaries (overflowWrap: anywhere) and laid out
  // again, which shapes the new pieces. Returns the index into `layouts`.
  pub(super) fn owned_layout(&self, typography: &TypographyContext, owned: &mut OwnedCache, width: f32) -> usize {
    if let Some(i) = owned.layouts.iter().position(|l| l.width == width) {
      return i;
    }
    let align = match self.text_alignment {
      TextAlignment::Center => Align::Center,
      TextAlignment::Right => Align::Right,
      TextAlignment::Justify => Align::Justify,
      // LTR only for now, so start is left and end is right.
      TextAlignment::End => Align::Right,
      TextAlignment::Left | TextAlignment::Start => Align::Left,
    };
    let ellipsis = owned.ellipsis.as_ref().map(|e| e.run.metrics);
    let metrics = |shaped: &[ShapedRun]| shaped.iter().map(|r| r.run).collect::<Vec<Run>>();
    // Per-line extent: the first line indented by a positive text_indent, the
    // lines after it by a negative one (hanging); a hard break does not start
    // a new "first line", as in CSS.
    let indent = self.text_indent;
    let extent = |c: LineCursor| {
      let x = match (c.index == 0, indent >= 0.0) {
        (true, true) | (false, false) => indent.abs(),
        _ => 0.0,
      };
      vec![LineExtent { x, width: (width - x).max(0.0) }]
    };
    let wrap = self.text_wrap;
    let mut layout = layout::layout_wrap(&metrics(&owned.runs), &extent, align, self.max_lines, ellipsis, wrap);
    let mut runs = None;
    if self.overflow_wrap == OverflowWrap::Anywhere && !layout.overflowing.is_empty() {
      let split = Self::split_graphemes(typography, &self.run_styles(), &owned.runs, &layout.overflowing);
      layout = layout::layout_wrap(&metrics(&split), &extent, align, self.max_lines, ellipsis, wrap);
      runs = Some(split);
    }
    if owned.layouts.len() >= MAX_CACHED_WIDTHS {
      owned.layouts.remove(0);
    }
    owned.layouts.push(OwnedLayout { width, layout, runs });
    owned.layouts.len() - 1
  }
}
