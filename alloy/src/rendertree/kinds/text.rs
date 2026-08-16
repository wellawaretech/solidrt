use super::PaintState;
use crate::impellers::{
  DisplayListBuilder, FontStyle, FontWeight, Paragraph, ParagraphBuilder, ParagraphStyle, Point, Rect, Size,
  TextAlignment, TypographyContext,
};
use crate::rendertree::text_layout::{self, Align, Layout, Run, RunMetrics};
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

// One wrap unit shaped as a single-line paragraph, plus what the breaker
// needs to know about it.
#[derive(Clone)]
struct ShapedRun {
  paragraph: Paragraph,
  run: Run,
}

#[derive(Clone, Default)]
struct OwnedCache {
  key: Option<ParaKey>,
  runs: Vec<ShapedRun>,
  layouts: Vec<(f32, Layout)>,
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
      let layout = Self::owned_layout(self, &mut owned, width);
      let owned = &*owned;
      for placed in &owned.layouts[layout].1.runs {
        let paragraph = &owned.runs[placed.run].paragraph;
        builder.draw_paragraph(paragraph, Point::new(origin.x + placed.x, origin.y + placed.y));
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
    for run in &self.runs {
      let mut style = self.paragraph_style(&run.overrides.resolve(self));
      style.set_text_alignment(self.text_alignment);
      // 0 means no cap: keep txt's unlimited default. Passing 0 through reads
      // as "the first line is the last" in Skia's line breaker, so every
      // paragraph would shape single-line.
      if self.max_lines > 0 {
        style.set_max_lines(self.max_lines);
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
    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => text_layout::max_intrinsic_width(&runs).min(w),
      AvailableSpace::MaxContent => text_layout::max_intrinsic_width(&runs),
      AvailableSpace::MinContent => text_layout::min_intrinsic_width(&runs),
    });
    let height = ctx.known.height.unwrap_or_else(|| {
      let layout = Self::owned_layout(self, &mut owned, width);
      owned.layouts[layout].1.height
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
    owned.key = Some(ParaKey::of(self));

    // A wrap unit may straddle styled runs (a code span and the comma glued
    // to it). Each (unit, run) intersection is shaped on its own; pieces
    // after the first are glued so the breaker keeps the unit whole.
    let styles: Vec<ParagraphStyle> =
      self.runs.iter().map(|r| self.paragraph_style(&r.overrides.resolve(self))).collect();
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
        // The break characters themselves are not shaped: an empty remainder
        // (a blank line) still needs a line box, which a space supplies.
        let text = raw.trim_end_matches(['\n', '\r', '\u{2028}', '\u{2029}']);
        let blank = text.is_empty();
        crate::rendertree::counters::note_para_shape();
        let Some(mut para_builder) = ParagraphBuilder::new(typography) else {
          return;
        };
        para_builder.push_style(&styles[run_index]);
        para_builder.add_text(if blank { " " } else { text });
        let Some(paragraph) = para_builder.build(f32::MAX) else {
          return;
        };
        let height = paragraph.get_height();
        let ascent = paragraph.get_line_metrics().map(|m| m.get_ascent(0) as f32).unwrap_or(height);
        let metrics = RunMetrics {
          advance: if blank { 0.0 } else { paragraph.get_max_intrinsic_width() },
          ink_width: if blank { 0.0 } else { paragraph.get_longest_line_width().max(0.0) },
          ascent,
          descent: (height - ascent).max(0.0),
        };
        owned.runs.push(ShapedRun {
          paragraph,
          run: Run { metrics, hard_break: segment.hard_break && last_piece, glue: !first },
        });
        first = false;
        piece_start = piece_end;
      }
    }
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

  // Line layout for `width` from the prepared runs; pure arithmetic, cached
  // per width like the paragraph path. Returns the index into `layouts`.
  fn owned_layout(&self, owned: &mut OwnedCache, width: f32) -> usize {
    if let Some(i) = owned.layouts.iter().position(|(w, _)| *w == width) {
      return i;
    }
    let runs: Vec<Run> = owned.runs.iter().map(|r| r.run).collect();
    let align = match self.text_alignment {
      TextAlignment::Center => Align::Center,
      TextAlignment::Right => Align::Right,
      // LTR only for now, so start is left and end is right.
      TextAlignment::End => Align::Right,
      TextAlignment::Left | TextAlignment::Start | TextAlignment::Justify => Align::Left,
    };
    let layout = text_layout::layout(&runs, width, align, self.max_lines);
    if owned.layouts.len() >= MAX_CACHED_WIDTHS {
      owned.layouts.remove(0);
    }
    owned.layouts.push((width, layout));
    owned.layouts.len() - 1
  }

  pub fn set_layout_mode(&mut self, mode: TextLayoutMode) -> Damage {
    self.layout_mode = mode;
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

/// A styled run of a paragraph: a span leaf's text plus the overrides in
/// effect for it (its own layered over its span ancestors').
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextRun {
  pub text: String,
  pub overrides: RunOverrides,
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
