use super::Text;
use crate::impellers::{FontStyle, FontWeight, Size};
use crate::rendertree::kinds::hash_f32;
use crate::rendertree::text::layout::{Clear, Side};
use crate::rendertree::{Damage, Element, ElementKind, PaintState};
use std::hash::{Hash, Hasher};

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

/// A run's fully resolved style. Also the style half of the word cache key,
/// hence Hash and Eq (floats by bits, see `hash_f32`).
#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
  pub font_family: String,
  pub font_size: f32,
  pub font_style: FontStyle,
  pub font_weight: FontWeight,
  pub line_height: f32,
  pub paint: PaintState,
}

impl Eq for RunStyle {}

impl Hash for RunStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.font_family.hash(state);
    hash_f32(self.font_size, state);
    self.font_style.hash(state);
    self.font_weight.hash(state);
    hash_f32(self.line_height, state);
    self.paint.hash(state);
  }
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
  pub underline: Option<bool>,
  pub underline_offset: Option<f32>,
  pub underline_thickness: Option<f32>,
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
      underline: child.underline.or(self.underline),
      underline_offset: child.underline_offset.or(self.underline_offset),
      underline_thickness: child.underline_thickness.or(self.underline_thickness),
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

/// A run of a paragraph: the `#text` leaf carries text, a `<span>` carries
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
  // Underline is paint-only: it neither shapes nor breaks.
  pub fn set_underline(&mut self, on: bool) -> Damage {
    self.overrides.underline = Some(on);
    Damage::Paint
  }
  pub fn set_underline_offset(&mut self, v: f32) -> Damage {
    self.overrides.underline_offset = Some(v);
    Damage::Paint
  }
  pub fn set_underline_thickness(&mut self, v: f32) -> Damage {
    self.overrides.underline_thickness = Some(v);
    Damage::Paint
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
