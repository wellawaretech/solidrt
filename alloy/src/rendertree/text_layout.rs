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

/// Horizontal placement of lines within the layout width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
  Left,
  Center,
  Right,
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
}

/// Greedy line breaking: runs go on the current line while their ink fits in
/// `max_width`; a run that fits nowhere gets a line of its own and overflows.
/// `max_lines` of 0 means unlimited. Each line is as tall as its tallest run
/// and every run's baseline sits on the line's baseline.
pub fn layout(runs: &[Run], max_width: f32, align: Align, max_lines: u32) -> Layout {
  let mut out = Layout::default();
  let mut line = Line::default();
  let mut pen = 0.0f32;
  let mut y = 0.0f32;

  let lines_full = |out: &Layout| max_lines > 0 && out.lines.len() as u32 >= max_lines;

  let close_line = |out: &mut Layout, line: &mut Line, y: &mut f32| {
    let offset = match align {
      Align::Left => 0.0,
      Align::Center => ((max_width - line.width) / 2.0).max(0.0),
      Align::Right => (max_width - line.width).max(0.0),
    };
    for placed in &mut out.runs[line.first..line.end] {
      placed.x += offset;
      placed.y = *y + line.ascent - runs[placed.run].metrics.ascent;
    }
    line.y = *y;
    *y += line.height;
    out.width = out.width.max(line.width);
    out.lines.push(*line);
    *line = Line { first: out.runs.len(), end: out.runs.len(), ..Line::default() };
  };

  for (index, run) in runs.iter().enumerate() {
    let has_content = line.end > line.first;
    if has_content && !run.glue && pen + unit_ink(runs, index) > max_width {
      close_line(&mut out, &mut line, &mut y);
      if lines_full(&out) {
        break;
      }
      pen = 0.0;
    }
    out.runs.push(PlacedRun { run: index, x: pen, y: 0.0 });
    line.end = out.runs.len();
    line.width = pen + run.metrics.ink_width;
    line.ascent = line.ascent.max(run.metrics.ascent);
    line.height = line.height.max(run.metrics.height());
    pen += run.metrics.advance;
    if run.hard_break {
      close_line(&mut out, &mut line, &mut y);
      if lines_full(&out) {
        break;
      }
      pen = 0.0;
    }
  }
  if line.end > line.first {
    close_line(&mut out, &mut line, &mut y);
  }
  out.height = y;
  out
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
