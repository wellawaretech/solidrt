// Text decoration (underline) metrics and painting. The owned engine shapes
// one run per wrap unit, so it draws decorations itself, one rect per line,
// from the fonts' own metrics (read per face at registration, see
// glyphs::Face::underline).
use crate::impellers::{DisplayListBuilder, Point, Rect, Size};
use crate::rendertree::text::layout::{Layout, PlacedRun};
use crate::rendertree::PaintState;

/// Underline geometry in em: `position` is the stroke's center below the
/// baseline (the OpenType `post` value, negated), `thickness` its height.
/// Skia draws the font's underline this way, so a rect from these matches
/// what every other renderer draws for the font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnderlineMetrics {
  pub position: f32,
  pub thickness: f32,
}

impl UnderlineMetrics {
  /// What a face without a `post` table gets (the table the underline
  /// metrics live in): the shipped Noto fonts' values.
  pub const DEFAULT: UnderlineMetrics = UnderlineMetrics { position: 0.10, thickness: 0.05 };
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
/// and trailing whitespace hangs, as a browser's decoration does per line.
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
      crate::rendertree::counters::note_draw();
      builder.draw_rect(&rect, &paint.to_paint());
      i = j;
    }
  }
}
