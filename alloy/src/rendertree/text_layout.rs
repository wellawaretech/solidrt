// Owned text layout (okf/backlog/text-layout-owned.md): pure arithmetic over
// pre-measured runs, in the shape of pretext's prepare/layout split. The
// shaper (today an Impeller single-line paragraph per run) supplies RunMetrics
// once; this module segments text into wrap units, breaks lines greedily and
// places runs on baselines. It knows nothing about fonts, paragraphs or the
// scripting engine, so measurement and drawing can be swapped underneath it.
//
// LTR only for now: runs are placed in logical order and "start" is left. Bidi
// levels become an input to `layout` later, not a redesign (see the backlog
// item's Bidi section).

use unicode_linebreak::{linebreaks, BreakOpportunity};

/// One wrap unit: a byte range of the source text ending at a UAX #14 break
/// opportunity. Trailing whitespace belongs to the segment before it, so a
/// segment's advance includes the gap that follows it and its ink width does
/// not; a line may end on the ink and let the whitespace hang.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
  pub start: usize,
  pub end: usize,
  /// The break after this segment is mandatory (newline, paragraph separator).
  pub hard_break: bool,
}

/// Split `text` at every UAX #14 break opportunity. Never below word level:
/// CJK yields per-ideograph segments because the standard allows breaks
/// there, Latin yields whole words. Empty text yields no segments.
pub fn segments(text: &str) -> Vec<Segment> {
  let mut out = Vec::new();
  let mut start = 0;
  for (end, opportunity) in linebreaks(text) {
    if end == start {
      continue;
    }
    // UAX #14 also reports end-of-text as mandatory; here hard_break means a
    // break character follows, so the final segment is exempt.
    let hard_break = opportunity == BreakOpportunity::Mandatory && end < text.len();
    out.push(Segment { start, end, hard_break });
    start = end;
  }
  out
}

/// What the shaper reports for one run, in layout units (pixels).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunMetrics {
  /// Full advance including trailing whitespace: where the next run starts.
  pub advance: f32,
  /// Advance up to the last inked glyph: what must fit on the line.
  pub ink_width: f32,
  /// Distance from the run's top to its baseline.
  pub ascent: f32,
  /// Distance from the baseline to the run's bottom.
  pub descent: f32,
}

impl RunMetrics {
  fn height(&self) -> f32 {
    self.ascent + self.descent
  }
}

/// A run to lay out: its metrics plus whether a hard break follows it.
/// `glue` marks a continuation piece of the previous run's wrap unit (a unit
/// that straddles styled runs): no break is allowed before it, and the unit's
/// pieces are fitted on a line together.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Run {
  pub metrics: RunMetrics,
  pub hard_break: bool,
  pub glue: bool,
}

/// Ink width of the wrap unit starting at `first`: the advances of every
/// piece but the last, plus the last piece's ink.
fn unit_ink(runs: &[Run], first: usize) -> f32 {
  let mut width = 0.0;
  let mut i = first;
  loop {
    let next_glued = runs.get(i + 1).is_some_and(|r| r.glue);
    if next_glued {
      width += runs[i].metrics.advance;
      i += 1;
    } else {
      return width + runs[i].metrics.ink_width;
    }
  }
}

/// Horizontal placement of lines within the layout width. `Justify` spreads
/// the slack of every wrapped line over the gaps between its wrap units; the
/// last line and lines ending in a hard break stay left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
  Left,
  Center,
  Right,
  Justify,
}

/// A run's position in the layout: `x`/`y` is the run's top-left, already
/// baseline-aligned within its line, relative to the layout origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedRun {
  pub run: usize,
  pub x: f32,
  pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Line {
  pub y: f32,
  pub height: f32,
  /// Baseline offset from the line's top.
  pub ascent: f32,
  /// Ink width: from the first run's start to the last run's last inked glyph.
  pub width: f32,
  /// Range into `Layout::runs`.
  pub first: usize,
  pub end: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
  pub lines: Vec<Line>,
  pub runs: Vec<PlacedRun>,
  /// Widest line's ink width.
  pub width: f32,
  pub height: f32,
  /// Runs were dropped because `max_lines` was reached.
  pub truncated: bool,
  /// Where the ellipsis run goes (top-left, baseline-aligned on the last
  /// line) when truncated and an ellipsis was supplied.
  pub ellipsis: Option<(f32, f32)>,
  /// First-piece indices of wrap units whose ink alone exceeds the width;
  /// they overflow their line. The caller may re-split them finer and lay
  /// out again (overflowWrap: anywhere).
  pub overflowing: Vec<usize>,
}

/// Greedy line breaking: wrap units (a run plus the glued runs after it) go
/// on the current line while their ink fits in `max_width`; a unit that fits
/// nowhere gets a line of its own and overflows. `max_lines` of 0 means
/// unlimited; when it cuts the text short and `ellipsis` gives the metrics of
/// an ellipsis run, the last line is trimmed until the ellipsis fits and its
/// position is reported. Each line is as tall as its tallest run and every
/// run's baseline sits on the line's baseline.
pub fn layout(runs: &[Run], max_width: f32, align: Align, max_lines: u32, ellipsis: Option<RunMetrics>) -> Layout {
  let mut b = Breaker { runs, max_width, align, out: Layout::default(), line: Line::default(), pen: 0.0, y: 0.0 };
  // Whether closing the open line would exceed the cap: the open line counts.
  let last_line = |b: &Breaker| max_lines > 0 && b.out.lines.len() as u32 + 1 >= max_lines;

  let mut index = 0;
  while index < runs.len() {
    let run = runs[index];
    let has_content = b.line.end > b.line.first;
    if !run.glue {
      let ink = unit_ink(runs, index);
      if has_content && b.pen + ink > max_width {
        if last_line(&b) {
          b.out.truncated = true;
          break;
        }
        b.close_line(true);
      }
      if ink > max_width {
        b.out.overflowing.push(index);
      }
    }
    b.place(index);
    if run.hard_break {
      let more = index + 1 < runs.len();
      if more && last_line(&b) {
        b.out.truncated = true;
        break;
      }
      b.close_line(false);
    }
    index += 1;
  }

  if b.out.truncated {
    if let Some(ell) = ellipsis {
      b.trim_for_ellipsis(ell);
    }
  }
  if b.line.end > b.line.first || b.out.ellipsis.is_some() {
    b.close_line(false);
  }
  b.out.height = b.y;
  b.out
}

struct Breaker<'a> {
  runs: &'a [Run],
  max_width: f32,
  align: Align,
  out: Layout,
  // The open line: `first..end` of `out.runs` are on it, `pen` is where the
  // next run starts, `y` its top.
  line: Line,
  pen: f32,
  y: f32,
}

impl Breaker<'_> {
  fn place(&mut self, index: usize) {
    let run = &self.runs[index];
    self.out.runs.push(PlacedRun { run: index, x: self.pen, y: 0.0 });
    self.line.end = self.out.runs.len();
    self.line.width = self.pen + run.metrics.ink_width;
    self.line.ascent = self.line.ascent.max(run.metrics.ascent);
    self.line.height = self.line.height.max(run.metrics.height());
    self.pen += run.metrics.advance;
  }

  // Close the open line: align its runs (justify only when the line was
  // wrapped), settle their y on the baseline, and start the next line.
  fn close_line(&mut self, wrapped: bool) {
    let slack = (self.max_width - self.line.width).max(0.0);
    let offset = match self.align {
      Align::Left | Align::Justify => 0.0,
      Align::Center => slack / 2.0,
      Align::Right => slack,
    };
    let (first, end) = (self.line.first, self.line.end);
    let mut justify_step = 0.0;
    if self.align == Align::Justify && wrapped {
      let units = self.out.runs[first..end].iter().filter(|p| !self.runs[p.run].glue).count();
      if units > 1 {
        justify_step = slack / (units - 1) as f32;
      }
    }
    let mut unit = 0usize;
    for (i, placed) in self.out.runs[first..end].iter_mut().enumerate() {
      let run = &self.runs[placed.run];
      if i > 0 && !run.glue {
        unit += 1;
      }
      placed.x += offset + justify_step * unit as f32;
      placed.y = self.y + self.line.ascent - run.metrics.ascent;
    }
    if let Some((x, y)) = &mut self.out.ellipsis {
      // The ellipsis sits on this (last) line: `x` holds its pen and `y` its
      // ascent until now.
      let ascent = *y;
      *x += offset;
      *y = self.y + self.line.ascent - ascent;
    }
    if justify_step > 0.0 {
      self.line.width = self.max_width;
    }
    self.line.y = self.y;
    self.y += self.line.height;
    self.out.width = self.out.width.max(self.line.width);
    self.out.lines.push(self.line);
    self.line = Line { first: self.out.runs.len(), end: self.out.runs.len(), ..Line::default() };
    self.pen = 0.0;
  }

  // Drop runs from the end of the open line until the ellipsis fits after
  // the last one's ink, then reserve its slot. A line that cannot fit even
  // the ellipsis alone keeps just the ellipsis.
  fn trim_for_ellipsis(&mut self, ell: RunMetrics) {
    let mut ell_x = 0.0;
    while self.line.end > self.line.first {
      let placed = self.out.runs[self.line.end - 1];
      ell_x = placed.x + self.runs[placed.run].metrics.ink_width;
      if ell_x + ell.ink_width <= self.max_width {
        break;
      }
      self.out.runs.pop();
      self.line.end -= 1;
      ell_x = 0.0;
    }
    // Recompute the line box from what remains plus the ellipsis.
    let mut ascent = ell.ascent;
    let mut height = ell.height();
    for placed in &self.out.runs[self.line.first..self.line.end] {
      let m = self.runs[placed.run].metrics;
      ascent = ascent.max(m.ascent);
      height = height.max(m.height());
    }
    self.line.ascent = ascent;
    self.line.height = height;
    self.line.width = ell_x + ell.ink_width;
    self.out.ellipsis = Some((ell_x, ell.ascent));
  }
}

/// Width the runs take with no wrapping at all: the widest hard-broken line.
pub fn max_intrinsic_width(runs: &[Run]) -> f32 {
  let mut widest = 0.0f32;
  let mut pen = 0.0f32;
  let mut ink = 0.0f32;
  for run in runs {
    ink = pen + run.metrics.ink_width;
    pen += run.metrics.advance;
    if run.hard_break {
      widest = widest.max(ink);
      pen = 0.0;
      ink = 0.0;
    }
  }
  widest.max(ink)
}

/// Narrowest width that breaks nowhere inside a wrap unit: the widest unit's
/// ink.
pub fn min_intrinsic_width(runs: &[Run]) -> f32 {
  (0..runs.len()).filter(|&i| !runs[i].glue).fold(0.0f32, |w, i| w.max(unit_ink(runs, i)))
}
