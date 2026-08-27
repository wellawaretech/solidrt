mod decoration;
pub mod layout;
mod paragraph;
mod runs;
mod shape;
mod words;

pub use decoration::{FontMetricsTable, Underline, UnderlineMetrics};
pub use runs::{RunOverrides, RunStyle, Span, TextRun, ATOM_CHAR};
pub use shape::{prepare_units, PreparedRun, PreparedUnit};
pub use words::{CaretStop, WordCache};

use crate::impellers::{DisplayListBuilder, FontStyle, FontWeight, Point, Rect, Size, TextAlignment};
use crate::rendertree::text::layout::{PlacedRun, Run, Wrap};
use crate::rendertree::{
  Bounded, BuildContext, Buildable, Damage, Element, ElementKind, Measurable, MeasureContext, PaintState,
  PlatformContext,
};
use paragraph::ParaCache;
use shape::{OwnedCache, ShapedRun};
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

// The engine text defaults, shared by Default and the null-reset paths in
// set_font_size / set_font_weight. Weight is Medium, not Regular: Impeller
// antialiases text in grayscale only, so small type on a 1x desktop display
// renders as hairlines that bleed into dark backgrounds. Costs a little
// extra weight on 2-3x screens that never needed it; see
// okf/backlog/dpi-aware-default-font-weight.md.
pub const DEFAULT_FONT_SIZE: f32 = 20.0;
pub const DEFAULT_FONT_WEIGHT: FontWeight = FontWeight::Medium;

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
  // Owned-layout counterpart: the run metrics per wrap-unit piece (measured
  // once per key through the shared word cache; the shaped paragraphs
  // themselves live only in that cache) plus the line layouts derived from
  // them, keyed by width.
  owned: RefCell<OwnedCache>,
}

impl Default for Text {
  fn default() -> Self {
    Self {
      computed_text: String::new(),
      runs: Vec::new(),
      paragraph_engine: false,
      font_family: "sans".to_string(),
      font_size: DEFAULT_FONT_SIZE,
      font_style: FontStyle::Normal,
      font_weight: DEFAULT_FONT_WEIGHT,
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
    // Lines wrap at and start from the content box - the box taffy measured
    // the text against, and the inset place_atoms already applies to inline
    // atoms (okf/done/padding-box-divergence.md). x/y are detached-only
    // geometry, where the content box is the whole frame at origin zero.
    let origin = Point::new(ctx.content.origin.x + self.x.unwrap_or(0.0), ctx.content.origin.y + self.y.unwrap_or(0.0));
    let width = self.w.unwrap_or(ctx.content.size.width);
    if !self.paragraph_engine {
      let mut owned = self.owned.borrow_mut();
      self.prepare_owned(ctx.platform, &mut owned);
      let index = self.owned_layout(ctx.platform, &mut owned, width);
      let owned = &*owned;
      let runs = owned.runs_for(index);
      let layout = &owned.layouts[index].layout;
      let styles = self.run_styles();
      // Paragraphs come from the shared word cache per visible run (a miss
      // shapes on the spot); nothing shaped is retained on the text itself.
      {
        let typography = ctx.platform.typography();
        let mut words = ctx.platform.words();
        let mut draw = |shaped: &ShapedRun, x: f32, y: f32| {
          if let Some(word) = words.get_or_shape(&typography, &shaped.text, &styles[shaped.style]) {
            builder.draw_paragraph(&word.paragraph, Point::new(origin.x + x, origin.y + y));
          }
        };
        for placed in &layout.runs {
          let shaped = &runs[placed.run];
          if !shaped.atom {
            draw(shaped, placed.x, placed.y);
          }
        }
        if let (Some((x, y)), Some(ellipsis)) = (layout.ellipsis, owned.ellipsis.as_ref()) {
          draw(ellipsis, x, y);
        }
      }
      // CSS decorating boxes: the text's underline is one line in its own
      // style under everything (atoms excepted); a span's underline is its
      // own line in the span's style. Both may cover a run.
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
          |placed| (!runs[placed.run].atom).then_some((underline, &style.paint)),
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
            if shaped.atom {
              return None;
            }
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
  // A box-level answer on purpose: `fallback` is taken raw, with no content
  // inset. Both callers pass a frame where that is the right box - the
  // bounding-box path (tree::compute_corners) passes the layout box and wants
  // the element's box, and the detached capture path passes the inherited
  // frame, which IS the content box for a node with no layout. Line-level ink
  // (content origin, wrap width) is painted_extent's job
  // (okf/done/padding-box-divergence.md).
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

  /// The box the lines and decorations paint into when built against
  /// `content` (the content box build() reads its origin and width from), in
  /// the text's own frame. None under the paragraph engine, whose extent is
  /// not read back.
  pub(crate) fn painted_extent(&self, platform: &PlatformContext, content: Rect) -> Option<Rect> {
    if self.paragraph_engine {
      return None;
    }
    let origin = Point::new(content.origin.x + self.x.unwrap_or(0.0), content.origin.y + self.y.unwrap_or(0.0));
    let width = self.w.unwrap_or(content.size.width);
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(platform, &mut owned);
    let index = self.owned_layout(platform, &mut owned, width);
    let layout = &owned.layouts[index].layout;
    // Ink overhangs its line box (italics, descenders, an underline pushed
    // below the last line); a line height of slack on every side covers it.
    let slack = layout.lines.iter().map(|l| l.height).fold(0.0, f32::max);
    Some(Rect::new(origin, Size::new(width.max(layout.width), layout.height)).inflate(slack, slack))
  }

  /// Content for a Text outside a tree (measureText, tests): the plain string
  /// as computed_text AND as one unstyled run. The two must agree - shaping
  /// walks the runs to cover the text and indexes them by position, so text
  /// with no runs is a panic, not an empty paragraph. In-tree Texts get both
  /// from their span children (RenderTree::sync_text).
  pub fn set_plain_text(&mut self, text: String) {
    self.runs.clear();
    if !text.is_empty() {
      self.runs.push(TextRun { text: text.clone(), ..TextRun::default() });
    }
    self.computed_text = text;
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
      .filter(|p| runs[p.run].atom)
      .map(|p| (self.runs[runs[p.run].style].node, Point::new(p.x, p.y)))
      .collect()
  }

  /// The span whose text is under `point` (text-local, box `size`), on the
  /// owned path, from the layout the last paint used; None on a miss, on the
  /// paragraph path, or when nothing has been laid out yet. Atoms are hit as
  /// elements, not through here.
  pub fn hit_run(&self, point: Point, content: Rect) -> Option<u64> {
    if self.paragraph_engine {
      return None;
    }
    let owned = self.owned.borrow();
    if !owned.key.as_ref().is_some_and(|k| k.matches(self)) {
      return None;
    }
    // The content box, matching build(): the lines were laid out at the
    // content width and drawn from the content origin, so the lookup and the
    // point both resolve against that box, not the border box
    // (okf/done/padding-box-divergence.md).
    let width = self.w.unwrap_or(content.size.width);
    // Paint and hit derive the width from the same content_box() arithmetic,
    // so the nearest layout is normally an exact match; the tolerance keeps
    // span hits alive should a caller ever round differently, and a wrap half
    // a pixel off resolves the same runs.
    let index = owned
      .layouts
      .iter()
      .enumerate()
      .filter(|(_, l)| (l.width - width).abs() < 0.5)
      .min_by(|(_, a), (_, b)| {
        (a.width - width).abs().partial_cmp(&(b.width - width).abs()).expect("layout widths are finite")
      })
      .map(|(i, _)| i)?;
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    let local = point
      - Point::new(content.origin.x + self.x.unwrap_or(0.0), content.origin.y + self.y.unwrap_or(0.0)).to_vector();
    let line = layout.lines.iter().find(|l| local.y >= l.y && local.y < l.y + l.height)?;
    layout.runs[line.first..line.end]
      .iter()
      .find(|p| {
        let shaped = &runs[p.run];
        !shaped.atom && local.x >= p.x && local.x < p.x + shaped.run.metrics.advance
      })
      .map(|p| self.runs[runs[p.run].style].node)
  }

  pub fn set_underline(&mut self, on: Option<bool>) -> Damage {
    self.underline = on.unwrap_or(false);
    Damage::Paint
  }
  pub fn set_underline_offset(&mut self, v: Option<f32>) -> Damage {
    self.underline_offset = v;
    Damage::Paint
  }
  pub fn set_underline_thickness(&mut self, v: Option<f32>) -> Damage {
    self.underline_thickness = v;
    Damage::Paint
  }

  pub fn set_paragraph_engine(&mut self, on: bool) -> Damage {
    self.paragraph_engine = on;
    Damage::Layout
  }
  pub fn set_text_overflow(&mut self, v: Option<TextOverflow>) -> Damage {
    self.text_overflow = v.unwrap_or_default();
    Damage::Layout
  }
  pub fn set_overflow_wrap(&mut self, v: Option<OverflowWrap>) -> Damage {
    self.overflow_wrap = v.unwrap_or_default();
    Damage::Layout
  }
  pub fn set_text_indent(&mut self, v: Option<f32>) -> Damage {
    self.text_indent = v.unwrap_or(0.0);
    Damage::Layout
  }
  pub fn set_text_wrap(&mut self, v: Option<Wrap>) -> Damage {
    self.text_wrap = v.unwrap_or_default();
    Damage::Layout
  }

  // Box overrides paint within (or independent of) the layout box, so none of
  // them affect layout.
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_w(&mut self, v: Option<f32>) -> Damage {
    self.w = v;
    Damage::Paint
  }
  pub fn set_h(&mut self, v: Option<f32>) -> Damage {
    self.h = v;
    Damage::Paint
  }

  // All other text properties feed measurement, so every change affects layout.
  // The resolved font family name and FontWeight come in already decoded.
  // None on the numeric props resets to the Default value.
  pub fn set_font_family(&mut self, family: Option<String>) -> Damage {
    self.font_family = family.unwrap_or_else(|| Self::default().font_family);
    Damage::Layout
  }
  pub fn set_font_size(&mut self, v: Option<f32>) -> Damage {
    self.font_size = v.unwrap_or(DEFAULT_FONT_SIZE);
    Damage::Layout
  }
  pub fn set_line_height(&mut self, v: Option<f32>) -> Damage {
    self.line_height = v.unwrap_or(0.0);
    Damage::Layout
  }
  pub fn set_max_lines(&mut self, v: Option<u32>) -> Damage {
    self.max_lines = v.unwrap_or(0);
    Damage::Layout
  }
  // fontWeight is numeric on the JS surface, so it resets like the numbers.
  pub fn set_font_weight(&mut self, weight: Option<FontWeight>) -> Damage {
    self.font_weight = weight.unwrap_or(DEFAULT_FONT_WEIGHT);
    Damage::Layout
  }
  pub fn set_font_style(&mut self, style: Option<FontStyle>) -> Damage {
    self.font_style = style.unwrap_or(FontStyle::Normal);
    Damage::Layout
  }
  pub fn set_text_alignment(&mut self, alignment: Option<TextAlignment>) -> Damage {
    self.text_alignment = alignment.unwrap_or(TextAlignment::Left);
    Damage::Layout
  }

  pub fn initial_style() -> Style {
    Style { display: Display::Block, ..Default::default() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Text(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Text(self))
  }
}
