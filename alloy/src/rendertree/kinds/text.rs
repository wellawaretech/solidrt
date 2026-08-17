use super::PaintState;
use crate::impellers::{
  DisplayListBuilder, FontStyle, FontWeight, Paragraph, ParagraphBuilder, ParagraphStyle, Point, Rect, Size,
  TextAlignment, TypographyContext,
};
use crate::rendertree::text_layout::{self, Align, Clear, Layout, LineCursor, LineExtent, Run, RunMetrics, Side, Wrap};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};
use std::cell::RefCell;
use taffy::{AvailableSpace, Display, Style};

// Shaping bound: at most this many widths cached per text node. A layout pass
// probes the intrinsic width (f32::MAX) plus the resolved width, and paint
// asks for the content width, so a handful covers a frame; oldest is evicted.
const MAX_CACHED_WIDTHS: usize = 4;

/// Which engine lays the text out. `Paragraph` hands the whole text to one
/// Impeller paragraph per width; `Owned` shapes each wrap unit as its own
/// single-line paragraph and breaks/places lines in text_layout (the
/// experimental path of okf/backlog/text-layout-owned.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextLayoutMode {
  #[default]
  Paragraph,
  Owned,
}

/// What happens to text cut off by `max_lines`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TextOverflow {
  #[default]
  Clip,
  /// The string drawn at the end of the last line in the paragraph's default
  /// style, the last line trimmed until it fits.
  Ellipsis(String),
}

/// What happens to a wrap unit wider than its line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverflowWrap {
  /// Keep it whole and let it overflow (CSS's default).
  Normal,
  /// Split it at grapheme boundaries, only when it does not fit alone.
  #[default]
  Anywhere,
}

#[derive(Clone, Debug)]
pub struct Text {
  // Concatenation of every span's text; what search and snapshots read.
  pub computed_text: String,
  // The same text as styled runs, in order: each span leaf's text with the
  // overrides layered along its span ancestry. Resolved against this Text's
  // own fields at shape time, so a `<text>` prop change needs no resync.
  pub runs: Vec<TextRun>,
  pub layout_mode: TextLayoutMode,
  pub font_family: String,
  pub font_size: f32,
  pub font_style: FontStyle,
  pub font_weight: FontWeight,
  pub text_alignment: TextAlignment,
  // 0 = unlimited.
  pub max_lines: u32,
  pub text_overflow: TextOverflow,
  pub overflow_wrap: OverflowWrap,
  // First-line indent in pixels; negative hangs: the first line starts at 0
  // and every following line is indented by the magnitude. Owned path only.
  pub text_indent: f32,
  // Owned path only.
  pub text_wrap: Wrap,
  pub line_height: f32,
  // Paint-time box overrides, mirroring Rectangle's x/y/w/h. x/y offset the
  // drawn paragraph. w overrides the shaping (wrap) width, which otherwise
  // falls back to the inherited layout size - detached text has no box of its
  // own, so give it a w for an unwrapped natural line. h cannot affect shaping
  // (paragraph height falls out of the text); it only feeds the reported
  // bounds. None of these affect layout.
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  pub paint: PaintState,
  // Shaped paragraphs for the current inputs, keyed by layout width. Shaping
  // dominates measure/build cost and properties are written directly from
  // several places, so validity is checked by fingerprint (ParaKey) instead
  // of setter hooks. Interior-mutable: measure and build take &self.
  cache: RefCell<ParaCache>,
  // Owned-layout counterpart: shaped runs (one paragraph per wrap unit, shaped
  // once per key) plus the line layouts derived from them, keyed by width.
  owned: RefCell<OwnedCache>,
}

impl Default for Text {
  fn default() -> Self {
    Self {
      computed_text: String::new(),
      runs: Vec::new(),
      layout_mode: TextLayoutMode::default(),
      font_family: "sans".to_string(),
      font_size: 20.0,
      font_style: FontStyle::Normal,
      // Medium, not Regular: Impeller antialiases text in grayscale only, so
      // small type on a 1x desktop display renders as hairlines that bleed
      // into dark backgrounds. Costs a little extra weight on 2-3x screens
      // that never needed it; see okf/backlog/dpi-aware-default-font-weight.md.
      font_weight: FontWeight::Medium,
      text_alignment: TextAlignment::Left,
      max_lines: 0,
      text_overflow: TextOverflow::default(),
      overflow_wrap: OverflowWrap::default(),
      text_indent: 0.0,
      text_wrap: Wrap::default(),
      line_height: 0.0,
      x: None,
      y: None,
      w: None,
      h: None,
      paint: PaintState::default(),
      cache: RefCell::new(ParaCache::default()),
      owned: RefCell::new(OwnedCache::default()),
    }
  }
}

// Snapshot of every input that feeds paragraph shaping; the cache is valid
// only while the owning Text still matches it.
#[derive(Clone, Debug, PartialEq)]
struct ParaKey {
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
  fn matches(&self, t: &Text) -> bool {
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

  fn of(t: &Text) -> Self {
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

#[derive(Clone, Default)]
struct ParaCache {
  key: Option<ParaKey>,
  entries: Vec<(f32, Paragraph)>,
}

// One piece of a wrap unit shaped as a single-line paragraph (or an atom's
// box), plus what the breaker needs to know about it and what re-splitting it
// finer needs.
#[derive(Clone)]
struct ShapedRun {
  // None for an atom: nothing to draw here, the element paints itself.
  paragraph: Option<Paragraph>,
  run: Run,
  text: String,
  // Index into the per-run styles the runs were shaped with; also the index
  // of the TextRun the piece came from.
  style: usize,
}

// A layout for one width. `runs` is Some when overflowing units were
// re-split at grapheme boundaries for this width: the layout's run indices
// then refer to it instead of `OwnedCache::runs`.
#[derive(Clone)]
struct OwnedLayout {
  width: f32,
  layout: Layout,
  runs: Option<Vec<ShapedRun>>,
}

#[derive(Clone, Default)]
struct OwnedCache {
  key: Option<ParaKey>,
  runs: Vec<ShapedRun>,
  // Ellipsis run in the paragraph's default style, when text_overflow asks
  // for one.
  ellipsis: Option<ShapedRun>,
  layouts: Vec<OwnedLayout>,
}

impl OwnedCache {
  fn runs_for(&self, index: usize) -> &[ShapedRun] {
    self.layouts[index].runs.as_deref().unwrap_or(&self.runs)
  }
}

impl std::fmt::Debug for OwnedCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "OwnedCache({} runs, {} layouts)", self.runs.len(), self.layouts.len())
  }
}

// Manual impl: impellers::Paragraph has no Debug.
impl std::fmt::Debug for ParaCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ParaCache({} entries)", self.entries.len())
  }
}

impl Buildable for Text {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let origin = Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0));
    let width = self.w.unwrap_or(ctx.size.width);
    if self.layout_mode == TextLayoutMode::Owned {
      let typography = ctx.platform.typography();
      let mut owned = self.owned.borrow_mut();
      self.prepare_owned(&typography, &mut owned);
      let index = self.owned_layout(&typography, &mut owned, width);
      let owned = &*owned;
      let runs = owned.runs_for(index);
      let layout = &owned.layouts[index].layout;
      for placed in &layout.runs {
        if let Some(paragraph) = &runs[placed.run].paragraph {
          builder.draw_paragraph(paragraph, Point::new(origin.x + placed.x, origin.y + placed.y));
        }
      }
      if let (Some((x, y)), Some(Some(paragraph))) = (layout.ellipsis, owned.ellipsis.as_ref().map(|e| &e.paragraph)) {
        builder.draw_paragraph(paragraph, Point::new(origin.x + x, origin.y + y));
      }
      return;
    }
    let Some(paragraph) = self.shaped(&ctx.platform.typography(), width) else {
      return;
    };
    builder.draw_paragraph(&paragraph, origin);
  }
}

impl Measurable for Text {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    crate::rendertree::counters::note_measure_call();
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size::new(w, h);
    }
    if self.layout_mode == TextLayoutMode::Owned {
      return self.measure_owned(ctx);
    }

    let Some(intrinsic) = self.shaped(&ctx.platform.typography(), f32::MAX) else {
      return Size::zero();
    };

    let max_intrinsic_width = intrinsic.get_max_intrinsic_width();
    let min_intrinsic_width = intrinsic.get_min_intrinsic_width();

    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => max_intrinsic_width.min(w),
      AvailableSpace::MaxContent => max_intrinsic_width,
      AvailableSpace::MinContent => min_intrinsic_width,
    });

    let Some(paragraph) = self.shaped(&ctx.platform.typography(), width) else {
      return Size::zero();
    };

    let height = ctx.known.height.unwrap_or_else(|| paragraph.get_height());

    Size::new(width, height)
  }
}

impl Bounded for Text {
  fn local_bounds(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }
}

impl Text {
  // Shape (or fetch the cached) paragraph for `width`. One paragraph serves
  // measure and paint: foreground and alignment are baked in even where
  // measurement does not need them, since they don't change the metrics.
  fn shaped(&self, typography: &TypographyContext, width: f32) -> Option<Paragraph> {
    let mut cache = self.cache.borrow_mut();
    if !cache.key.as_ref().is_some_and(|k| k.matches(self)) {
      cache.entries.clear();
      cache.key = Some(ParaKey::of(self));
    }
    if let Some((_, paragraph)) = cache.entries.iter().find(|(w, _)| *w == width) {
      return Some(paragraph.clone());
    }
    crate::rendertree::counters::note_para_shape();

    // Paragraph-level settings are read from the first pushed style, so every
    // run's style carries them; inner runs' copies are ignored by Impeller.
    let mut para_builder = ParagraphBuilder::new(typography)?;
    let mut pushed = 0;
    // Atoms are an owned-path feature; the paragraph engine has no
    // placeholders, so they are left out here.
    for run in self.runs.iter().filter(|r| r.atom.is_none()) {
      let mut style = self.paragraph_style(&run.overrides.resolve(self));
      style.set_text_alignment(self.text_alignment);
      // 0 means no cap: keep txt's unlimited default. Passing 0 through reads
      // as "the first line is the last" in Skia's line breaker, so every
      // paragraph would shape single-line.
      if self.max_lines > 0 {
        style.set_max_lines(self.max_lines);
      }
      if let TextOverflow::Ellipsis(s) = &self.text_overflow {
        style.set_ellipsis(Some(s));
      }
      para_builder.push_style(&style);
      para_builder.add_text(&run.text);
      pushed += 1;
    }
    if pushed == 0 {
      // Empty text still needs a style for its (zero-line) metrics.
      para_builder.push_style(&self.paragraph_style(&self.run_style()));
    }
    let paragraph = para_builder.build(width)?;

    if cache.entries.len() >= MAX_CACHED_WIDTHS {
      cache.entries.remove(0);
    }
    cache.entries.push((width, paragraph.clone()));
    Some(paragraph)
  }

  fn measure_owned(&self, ctx: &MeasureContext) -> Size {
    let typography = ctx.platform.typography();
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(&typography, &mut owned);
    let runs: Vec<Run> = owned.runs.iter().map(|r| r.run).collect();
    // The intrinsic widths are of the runs alone; an indented line needs its
    // indent on top, else a shrink-to-fit text wraps where it need not.
    let indent = self.text_indent.abs();
    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => (text_layout::max_intrinsic_width(&runs) + indent).min(w),
      AvailableSpace::MaxContent => text_layout::max_intrinsic_width(&runs) + indent,
      AvailableSpace::MinContent => text_layout::min_intrinsic_width(&runs) + indent,
    });
    let height = ctx.known.height.unwrap_or_else(|| {
      let index = self.owned_layout(&typography, &mut owned, width);
      owned.layouts[index].layout.height
    });
    Size::new(width, height)
  }

  // Shape every wrap unit of the current text as its own single-line
  // paragraph, unless the cache already holds them for the current inputs.
  fn prepare_owned(&self, typography: &TypographyContext, owned: &mut OwnedCache) {
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
    for segment in text_layout::segments(&self.computed_text) {
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
  fn paragraph_style(&self, run: &RunStyle) -> ParagraphStyle {
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

  // This Text's own fields as the run style every span layers on.
  pub fn run_style(&self) -> RunStyle {
    RunStyle {
      font_family: self.font_family.clone(),
      font_size: self.font_size,
      font_style: self.font_style,
      font_weight: self.font_weight,
      line_height: self.line_height,
      paint: self.paint.clone(),
    }
  }

  // Line layout for `width` from the prepared runs, cached per width like the
  // paragraph path. Pure arithmetic, except that a unit wider than the line
  // is re-split at grapheme boundaries (overflowWrap: anywhere) and laid out
  // again, which shapes the new pieces. Returns the index into `layouts`.
  fn owned_layout(&self, typography: &TypographyContext, owned: &mut OwnedCache, width: f32) -> usize {
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
    let mut layout = text_layout::layout_wrap(&metrics(&owned.runs), &extent, align, self.max_lines, ellipsis, wrap);
    let mut runs = None;
    if self.overflow_wrap == OverflowWrap::Anywhere && !layout.overflowing.is_empty() {
      let split = Self::split_graphemes(typography, &self.run_styles(), &owned.runs, &layout.overflowing);
      layout = text_layout::layout_wrap(&metrics(&split), &extent, align, self.max_lines, ellipsis, wrap);
      runs = Some(split);
    }
    if owned.layouts.len() >= MAX_CACHED_WIDTHS {
      owned.layouts.remove(0);
    }
    owned.layouts.push(OwnedLayout { width, layout, runs });
    owned.layouts.len() - 1
  }

  /// Record an atom's measured box, from the layout pass. Returns whether it
  /// changed (a change re-shapes the paragraph via the cache key).
  pub fn set_atom_size(&mut self, node: u64, size: Size) -> bool {
    let Some(run) = self.runs.iter_mut().find(|r| r.node == node && r.atom.is_some()) else {
      return false;
    };
    if run.atom == Some(size) {
      return false;
    }
    run.atom = Some(size);
    true
  }

  /// Where the atoms sit for a layout at `width` (content width), as (node,
  /// top-left) relative to the text's box: the layout pass writes these into
  /// the atoms' computed layouts after the text's own. Owned path only.
  pub fn atom_positions(&self, typography: &TypographyContext, width: f32) -> Vec<(u64, Point)> {
    if self.layout_mode != TextLayoutMode::Owned || self.runs.iter().all(|r| r.atom.is_none()) {
      return Vec::new();
    }
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(typography, &mut owned);
    let index = self.owned_layout(typography, &mut owned, width);
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    layout
      .runs
      .iter()
      .chain(&layout.floats)
      .filter(|p| runs[p.run].paragraph.is_none())
      .map(|p| (self.runs[runs[p.run].style].node, Point::new(p.x, p.y)))
      .collect()
  }

  /// The span whose text is under `point` (text-local, box `size`), on the
  /// owned path, from the layout the last paint used; None on a miss, on the
  /// paragraph path, or when nothing has been laid out yet. Atoms are hit as
  /// elements, not through here.
  pub fn hit_run(&self, point: Point, size: Size) -> Option<u64> {
    if self.layout_mode != TextLayoutMode::Owned {
      return None;
    }
    let owned = self.owned.borrow();
    if !owned.key.as_ref().is_some_and(|k| k.matches(self)) {
      return None;
    }
    let width = self.w.unwrap_or(size.width);
    let index = owned.layouts.iter().position(|l| l.width == width)?;
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    let local = point - Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)).to_vector();
    let line = layout.lines.iter().find(|l| local.y >= l.y && local.y < l.y + l.height)?;
    layout.runs[line.first..line.end]
      .iter()
      .find(|p| {
        let shaped = &runs[p.run];
        shaped.paragraph.is_some() && local.x >= p.x && local.x < p.x + shaped.run.metrics.advance
      })
      .map(|p| self.runs[runs[p.run].style].node)
  }

  pub fn set_layout_mode(&mut self, mode: TextLayoutMode) -> Damage {
    self.layout_mode = mode;
    Damage::Layout
  }
  pub fn set_text_overflow(&mut self, v: TextOverflow) -> Damage {
    self.text_overflow = v;
    Damage::Layout
  }
  pub fn set_overflow_wrap(&mut self, v: OverflowWrap) -> Damage {
    self.overflow_wrap = v;
    Damage::Layout
  }
  pub fn set_text_indent(&mut self, v: f32) -> Damage {
    self.text_indent = v;
    Damage::Layout
  }
  pub fn set_text_wrap(&mut self, v: Wrap) -> Damage {
    self.text_wrap = v;
    Damage::Layout
  }

  // Box overrides paint within (or independent of) the layout box, so none of
  // them affect layout.
  pub fn set_x(&mut self, v: f32) -> Damage {
    self.x = Some(v);
    Damage::Paint
  }
  pub fn set_y(&mut self, v: f32) -> Damage {
    self.y = Some(v);
    Damage::Paint
  }
  pub fn set_w(&mut self, v: f32) -> Damage {
    self.w = Some(v);
    Damage::Paint
  }
  pub fn set_h(&mut self, v: f32) -> Damage {
    self.h = Some(v);
    Damage::Paint
  }

  // All other text properties feed measurement, so every change affects layout.
  // The resolved font family name and FontWeight come in already decoded.
  pub fn set_font_family(&mut self, family: String) -> Damage {
    self.font_family = family;
    Damage::Layout
  }
  pub fn set_font_size(&mut self, v: f32) -> Damage {
    self.font_size = v;
    Damage::Layout
  }
  pub fn set_line_height(&mut self, v: f32) -> Damage {
    self.line_height = v;
    Damage::Layout
  }
  pub fn set_max_lines(&mut self, v: u32) -> Damage {
    self.max_lines = v;
    Damage::Layout
  }
  pub fn set_font_weight(&mut self, weight: FontWeight) -> Damage {
    self.font_weight = weight;
    Damage::Layout
  }
  pub fn set_font_style(&mut self, style: FontStyle) -> Damage {
    self.font_style = style;
    Damage::Layout
  }
  pub fn set_text_alignment(&mut self, alignment: TextAlignment) -> Damage {
    self.text_alignment = alignment;
    Damage::Layout
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Text(self), Style { display: Display::Block, ..Default::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Text(self))
  }
}

/// The character an inline atom occupies in the paragraph text: U+FFFC OBJECT
/// REPLACEMENT CHARACTER, whose UAX #14 class allows a break before and after
/// it, so an atom is its own wrap unit.
pub const ATOM_CHAR: &str = "\u{FFFC}";

/// A run of a paragraph: a span leaf's text plus the overrides in effect for
/// it (its own layered over its span ancestors'), or an inline atom - a
/// laid-out element child of the `<text>` that flows with the words as one
/// unbreakable unit, `ATOM_CHAR` wide in the text and `atom` (its measured
/// box) wide on the line, bottom on the baseline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextRun {
  pub text: String,
  pub overrides: RunOverrides,
  /// The node this run comes from: the leaf span, or the atom element. What
  /// a hit on the run resolves to.
  pub node: u64,
  /// The atom's margin box, written by the layout pass; None for text.
  pub atom: Option<Size>,
  /// A floated atom: out of the flow, an exclusion for the lines it overlaps.
  pub float: Option<Side>,
  /// The atom starts a line below the earlier floats on that side.
  pub clear: Option<Clear>,
}

/// A run's fully resolved style.
#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
  pub font_family: String,
  pub font_size: f32,
  pub font_style: FontStyle,
  pub font_weight: FontWeight,
  pub line_height: f32,
  pub paint: PaintState,
}

/// Per-span style overrides. `None` inherits from the enclosing span or, at
/// the top, from the `<text>` itself. Cascade is intra-paragraph only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunOverrides {
  pub font_family: Option<String>,
  pub font_size: Option<f32>,
  pub font_style: Option<FontStyle>,
  pub font_weight: Option<FontWeight>,
  pub line_height: Option<f32>,
  pub paint: Option<PaintState>,
}

impl RunOverrides {
  /// `child` layered over `self`: a child's Some wins.
  pub fn layer(&self, child: &RunOverrides) -> RunOverrides {
    RunOverrides {
      font_family: child.font_family.clone().or_else(|| self.font_family.clone()),
      font_size: child.font_size.or(self.font_size),
      font_style: child.font_style.or(self.font_style),
      font_weight: child.font_weight.or(self.font_weight),
      line_height: child.line_height.or(self.line_height),
      paint: child.paint.clone().or_else(|| self.paint.clone()),
    }
  }

  pub fn resolve(&self, text: &Text) -> RunStyle {
    RunStyle {
      font_family: self.font_family.clone().unwrap_or_else(|| text.font_family.clone()),
      font_size: self.font_size.unwrap_or(text.font_size),
      font_style: self.font_style.unwrap_or(text.font_style),
      font_weight: self.font_weight.unwrap_or(text.font_weight),
      line_height: self.line_height.unwrap_or(text.line_height),
      paint: self.paint.clone().unwrap_or_else(|| text.paint.clone()),
    }
  }
}

/// A run of a paragraph: the leaf `d-span` carries text, a `<span>` carries
/// style overrides for everything under it. One kind serves both, since a
/// span with text and children is just a run followed by more runs.
#[derive(Clone, Debug, Default)]
pub struct Span {
  pub text: String,
  pub overrides: RunOverrides,
}

impl Span {
  // Span text feeds the parent paragraph's measurement, so it affects layout.
  pub fn set_text(&mut self, text: String) -> Damage {
    self.text = text;
    Damage::Layout
  }

  // Metrics-affecting overrides are Layout; the paint alone is Paint. Either
  // way the owning Text re-collects its runs (RenderTree::sync_span_parent).
  pub fn set_font_family(&mut self, family: String) -> Damage {
    self.overrides.font_family = Some(family);
    Damage::Layout
  }
  pub fn set_font_size(&mut self, v: f32) -> Damage {
    self.overrides.font_size = Some(v);
    Damage::Layout
  }
  pub fn set_line_height(&mut self, v: f32) -> Damage {
    self.overrides.line_height = Some(v);
    Damage::Layout
  }
  pub fn set_font_weight(&mut self, weight: FontWeight) -> Damage {
    self.overrides.font_weight = Some(weight);
    Damage::Layout
  }
  pub fn set_font_style(&mut self, style: FontStyle) -> Damage {
    self.overrides.font_style = Some(style);
    Damage::Layout
  }
  /// The paint override, created from the paragraph default on first write
  /// so paint setters (color, gradient) have something to write into.
  pub fn paint_override_mut(&mut self) -> &mut PaintState {
    self.overrides.paint.get_or_insert_with(PaintState::default)
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Span(self))
  }
}
