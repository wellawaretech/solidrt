mod decoration;
pub mod layout;
mod paragraph;
mod runs;
mod shape;
mod words;

pub use decoration::{FontMetricsTable, Underline, UnderlineMetrics};
pub use runs::{RunOverrides, RunStyle, Span, TextRun, ATOM_CHAR};
pub use shape::{prepare_units, PreparedUnit};
pub use words::WordCache;

use crate::impellers::{DisplayListBuilder, FontStyle, FontWeight, Point, Rect, Size, TextAlignment};
use crate::rendertree::text::layout::{PlacedRun, Run, Wrap};
use crate::rendertree::{
  Bounded, BuildContext, Buildable, Damage, Element, ElementKind, Measurable, MeasureContext, PaintState,
  PlatformContext,
};
use paragraph::ParaCache;
use shape::OwnedCache;
use std::cell::RefCell;
use taffy::{AvailableSpace, Display, Style};

// Shaping bound: at most this many widths cached per text node. A layout pass
// probes the intrinsic width (f32::MAX) plus the resolved width, and paint
// asks for the content width, so a handful covers a frame; oldest is evicted.
const MAX_CACHED_WIDTHS: usize = 4;

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
  // Lay the text out with one Impeller paragraph per width instead of the
  // owned breaker (text::layout). Kept as a reference/fallback engine; not
  // exposed as a prop. Spans, atoms, floats, indent and wrap are owned-only.
  pub paragraph_engine: bool,
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
  // Underline (CSS text-decoration: underline) in the run's paint. Offset
  // (baseline to the stroke's top) and thickness in pixels; None takes the
  // font's own metrics (see text::decoration). Paint-only, owned path only.
  pub underline: bool,
  pub underline_offset: Option<f32>,
  pub underline_thickness: Option<f32>,
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
      paragraph_engine: false,
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
      underline: false,
      underline_offset: None,
      underline_thickness: None,
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

impl Buildable for Text {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let origin = Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0));
    let width = self.w.unwrap_or(ctx.size.width);
    if !self.paragraph_engine {
      let mut owned = self.owned.borrow_mut();
      self.prepare_owned(ctx.platform, &mut owned);
      let index = self.owned_layout(ctx.platform, &mut owned, width);
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
      // CSS decorating boxes: the text's underline is one line in its own
      // style under everything (atoms excepted); a span's underline is its
      // own line in the span's style. Both may cover a run.
      let styles = self.run_styles();
      let font_metrics = ctx.platform.font_metrics();
      let ink_of = |placed: &PlacedRun| runs[placed.run].run.metrics.ink_width;
      if self.underline {
        let style = self.run_style();
        let underline = Underline::resolve(
          font_metrics.underline(&style.font_family),
          style.font_size,
          self.underline_offset,
          self.underline_thickness,
        );
        decoration::draw_underlines(
          builder,
          origin,
          layout,
          |placed| runs[placed.run].paragraph.as_ref().map(|_| (underline, &style.paint)),
          ink_of,
        );
      }
      if self.runs.iter().any(|r| r.overrides.underline == Some(true)) {
        decoration::draw_underlines(
          builder,
          origin,
          layout,
          |placed| {
            let shaped = &runs[placed.run];
            shaped.paragraph.as_ref()?;
            let overrides = &self.runs[shaped.style].overrides;
            if overrides.underline != Some(true) {
              return None;
            }
            let style = &styles[shaped.style];
            let underline = Underline::resolve(
              font_metrics.underline(&style.font_family),
              style.font_size,
              overrides.underline_offset.or(self.underline_offset),
              overrides.underline_thickness.or(self.underline_thickness),
            );
            Some((underline, &style.paint))
          },
          ink_of,
        );
      }
      return;
    }
    self.build_paragraph(ctx.platform, builder, origin, width);
  }
}

impl Measurable for Text {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    crate::rendertree::counters::note_measure_call();
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size::new(w, h);
    }
    if self.paragraph_engine {
      return self.measure_paragraph(ctx);
    }
    self.measure_owned(ctx)
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
  fn measure_owned(&self, ctx: &MeasureContext) -> Size {
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(ctx.platform, &mut owned);
    let runs: Vec<Run> = owned.runs.iter().map(|r| r.run).collect();
    // The intrinsic widths are of the runs alone; an indented line needs its
    // indent on top, else a shrink-to-fit text wraps where it need not.
    let indent = self.text_indent.abs();
    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => (layout::max_intrinsic_width(&runs) + indent).min(w),
      AvailableSpace::MaxContent => layout::max_intrinsic_width(&runs) + indent,
      AvailableSpace::MinContent => layout::min_intrinsic_width(&runs) + indent,
    });
    let height = ctx.known.height.unwrap_or_else(|| {
      let index = self.owned_layout(ctx.platform, &mut owned, width);
      owned.layouts[index].layout.height
    });
    Size::new(width, height)
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
  pub fn atom_positions(&self, platform: &PlatformContext, width: f32) -> Vec<(u64, Point)> {
    if self.paragraph_engine || self.runs.iter().all(|r| r.atom.is_none()) {
      return Vec::new();
    }
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(platform, &mut owned);
    let index = self.owned_layout(platform, &mut owned, width);
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
    if self.paragraph_engine {
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

  pub fn set_underline(&mut self, on: bool) -> Damage {
    self.underline = on;
    Damage::Paint
  }
  pub fn set_underline_offset(&mut self, v: f32) -> Damage {
    self.underline_offset = Some(v);
    Damage::Paint
  }
  pub fn set_underline_thickness(&mut self, v: f32) -> Damage {
    self.underline_thickness = Some(v);
    Damage::Paint
  }

  pub fn set_paragraph_engine(&mut self, on: bool) -> Damage {
    self.paragraph_engine = on;
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
