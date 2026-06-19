use super::PaintState;
use crate::rendertree::{BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};
use crate::impellers::{
  DisplayListBuilder, FontStyle, FontWeight, Paragraph, ParagraphBuilder, ParagraphStyle, Point, TextAlignment,
  TypographyContext,
};
use std::cell::RefCell;
use taffy::prelude::*;

// Shaping bound: at most this many widths cached per text node. A layout pass
// probes the intrinsic width (f32::MAX) plus the resolved width, and paint
// asks for the content width, so a handful covers a frame; oldest is evicted.
const MAX_CACHED_WIDTHS: usize = 4;

#[derive(Clone, Debug)]
pub struct Text {
  pub computed_text: String,
  pub font_family: String,
  pub font_size: f32,
  pub font_style: FontStyle,
  pub font_weight: FontWeight,
  pub text_alignment: TextAlignment,
  pub max_lines: u32,
  pub line_height: f32,
  pub paint: PaintState,
  // Shaped paragraphs for the current inputs, keyed by layout width. Shaping
  // dominates measure/build cost and properties are written directly from
  // several places, so validity is checked by fingerprint (ParaKey) instead
  // of setter hooks. Interior-mutable: measure and build take &self.
  cache: RefCell<ParaCache>,
}

impl Default for Text {
  fn default() -> Self {
    Self {
      computed_text: String::new(),
      font_family: "Noto Sans".to_string(),
      font_size: 20.0,
      font_style: FontStyle::Normal,
      font_weight: FontWeight::Medium,
      text_alignment: TextAlignment::Left,
      max_lines: 0,
      line_height: 0.0,
      paint: PaintState::default(),
      cache: RefCell::new(ParaCache::default()),
    }
  }
}

// Snapshot of every input that feeds paragraph shaping; the cache is valid
// only while the owning Text still matches it.
#[derive(Clone, Debug, PartialEq)]
struct ParaKey {
  text: String,
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
    self.text == t.computed_text
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
      text: t.computed_text.clone(),
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

// Manual impl: impellers::Paragraph has no Debug.
impl std::fmt::Debug for ParaCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ParaCache({} entries)", self.entries.len())
  }
}

impl Buildable for Text {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let Some(paragraph) = self.shaped(&ctx.platform.typography, ctx.size.w) else {
      return;
    };
    builder.draw_paragraph(&paragraph, Point::new(0.0, 0.0));
  }
}

impl Measurable for Text {
  fn measure(&self, ctx: &MeasureContext) -> Size<f32> {
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size { width: w, height: h };
    }

    let Some(intrinsic) = self.shaped(&ctx.platform.typography, f32::MAX) else {
      return Size::ZERO;
    };

    let max_intrinsic_width = intrinsic.get_max_intrinsic_width();
    let min_intrinsic_width = intrinsic.get_min_intrinsic_width();

    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => max_intrinsic_width.min(w),
      AvailableSpace::MaxContent => max_intrinsic_width,
      AvailableSpace::MinContent => min_intrinsic_width,
    });

    let Some(paragraph) = self.shaped(&ctx.platform.typography, width) else {
      return Size::ZERO;
    };

    let height = ctx.known.height.unwrap_or_else(|| paragraph.get_height());

    Size { width, height }
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

    let mut style = ParagraphStyle::default();
    let paint = self.paint.to_paint();
    style.set_foreground(&paint);
    style.set_font_family(&self.font_family);
    style.set_font_size(self.font_size);
    style.set_font_style(self.font_style);
    style.set_font_weight(self.font_weight);
    style.set_text_alignment(self.text_alignment);
    style.set_max_lines(self.max_lines);
    style.set_height(self.line_height);

    let mut para_builder = ParagraphBuilder::new(typography)?;
    para_builder.push_style(&style);
    para_builder.add_text(&self.computed_text);
    let paragraph = para_builder.build(width)?;

    if cache.entries.len() >= MAX_CACHED_WIDTHS {
      cache.entries.remove(0);
    }
    cache.entries.push((width, paragraph.clone()));
    Some(paragraph)
  }

  // All text properties feed measurement, so every change affects layout. The
  // resolved font family name and FontWeight come in already decoded.
  pub fn set_font_family(&mut self, family: String) -> bool {
    self.font_family = family;
    true
  }
  pub fn set_font_size(&mut self, v: f32) -> bool {
    self.font_size = v;
    true
  }
  pub fn set_line_height(&mut self, v: f32) -> bool {
    self.line_height = v;
    true
  }
  pub fn set_max_lines(&mut self, v: u32) -> bool {
    self.max_lines = v;
    true
  }
  pub fn set_font_weight(&mut self, weight: FontWeight) -> bool {
    self.font_weight = weight;
    true
  }
  pub fn set_text_alignment(&mut self, alignment: TextAlignment) -> bool {
    self.text_alignment = alignment;
    true
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Text(self), Style { display: Display::Block, ..Default::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Text(self))
  }
}

#[derive(Clone, Debug, Default)]
pub struct Span {
  pub text: String,
}

impl Span {
  // Span text feeds the parent paragraph's measurement, so it affects layout.
  pub fn set_text(&mut self, text: String) -> bool {
    self.text = text;
    true
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Span(self))
  }
}
