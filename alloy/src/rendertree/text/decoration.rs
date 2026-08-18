// Text decoration (underline) metrics and painting. Impeller's own paragraph
// decoration is per paragraph and skips trailing whitespace; the owned engine
// shapes one paragraph per wrap unit, so it draws decorations itself, one
// rect per line, from the fonts' own metrics.
use crate::impellers::{DisplayListBuilder, Point, Rect, Size};
use crate::rendertree::text::layout::{Layout, PlacedRun};
use crate::rendertree::PaintState;
use std::collections::HashMap;

/// Underline geometry in em: `position` is the stroke's center below the
/// baseline (the OpenType `post` value, negated), `thickness` its height.
/// Skia draws the font's underline this way, so a rect from these matches
/// Impeller's own decoration pixel for pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnderlineMetrics {
  pub position: f32,
  pub thickness: f32,
}

impl UnderlineMetrics {
  /// What an unregistered family gets (Impeller's system-font fallback):
  /// the shipped Noto fonts' values.
  pub const DEFAULT: UnderlineMetrics = UnderlineMetrics { position: 0.10, thickness: 0.05 };
}

/// Underline metrics of the registered fonts, keyed by alias and by the
/// fonts' own family names, so a role ("sans") and a name ("Noto Sans")
/// both resolve. First registration per key wins; weight and style variants
/// share metrics.
#[derive(Clone, Debug, Default)]
pub struct FontMetricsTable {
  by_family: HashMap<String, UnderlineMetrics>,
}

impl FontMetricsTable {
  /// Record `bytes`' underline metrics under `alias` and its family names.
  /// A font ttf-parser cannot read, or one without underline metrics, adds
  /// nothing: its families fall back to `UnderlineMetrics::DEFAULT`.
  pub fn register(&mut self, bytes: &[u8], alias: Option<&str>) {
    let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
      return;
    };
    let Some(underline) = face.underline_metrics() else {
      return;
    };
    let upem = face.units_per_em() as f32;
    let metrics =
      UnderlineMetrics { position: -(underline.position as f32) / upem, thickness: underline.thickness as f32 / upem };
    let names = face.names();
    let families = names
      .into_iter()
      .filter(|n| n.name_id == ttf_parser::name_id::FAMILY || n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
      .filter_map(|n| n.to_string());
    for key in alias.map(str::to_string).into_iter().chain(families) {
      self.by_family.entry(key).or_insert(metrics);
    }
  }

  pub fn underline(&self, family: &str) -> UnderlineMetrics {
    self.by_family.get(family).copied().unwrap_or(UnderlineMetrics::DEFAULT)
  }
}

/// A run's resolved underline, in pixels: `offset` from the baseline to the
/// stroke's top, `thickness` its height. None when not underlined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
  pub offset: f32,
  pub thickness: f32,
}

impl Underline {
  /// The font's underline at `font_size`, with either value overridden.
  pub fn resolve(metrics: UnderlineMetrics, font_size: f32, offset: Option<f32>, thickness: Option<f32>) -> Self {
    let thickness = thickness.unwrap_or(metrics.thickness * font_size);
    let offset = offset.unwrap_or(metrics.position * font_size - thickness / 2.0);
    Self { offset, thickness }
  }
}

/// Draw the underlines of `layout` at `origin`. `underline_of` answers a
/// placed run with its underline and paint (None: not underlined, or an
/// atom), `ink_of` with its ink width. Maximal runs of adjacent underlined
/// runs with the same geometry and paint on a line become one rect from the
/// first run's start to the last run's ink end, so spaces inside are covered
/// and trailing whitespace hangs, as Impeller's own decoration does per line.
pub fn draw_underlines<'a>(
  builder: &mut DisplayListBuilder,
  origin: Point,
  layout: &Layout,
  underline_of: impl Fn(&PlacedRun) -> Option<(Underline, &'a PaintState)>,
  ink_of: impl Fn(&PlacedRun) -> f32,
) {
  for line in &layout.lines {
    let baseline = origin.y + line.y + line.ascent;
    let placed = &layout.runs[line.first..line.end];
    let mut i = 0;
    while i < placed.len() {
      let Some((underline, paint)) = underline_of(&placed[i]) else {
        i += 1;
        continue;
      };
      let start = placed[i].x;
      let mut end = start + ink_of(&placed[i]);
      let mut j = i + 1;
      while j < placed.len() {
        match underline_of(&placed[j]) {
          Some((next, next_paint)) if next == underline && next_paint == paint => {
            end = placed[j].x + ink_of(&placed[j]);
            j += 1;
          }
          _ => break,
        }
      }
      let rect = Rect::new(
        Point::new(origin.x + start, baseline + underline.offset),
        Size::new(end - start, underline.thickness),
      );
      builder.draw_rect(&rect, &paint.to_paint());
      i = j;
    }
  }
}
