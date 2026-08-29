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

/// Which edge a floated run leaves the flow for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
  Left,
  Right,
}

/// Which earlier floats a run must start below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clear {
  Left,
  Right,
  Both,
}

/// A run to lay out: its metrics plus whether a hard break follows it.
/// `glue` marks a continuation piece of the previous run's wrap unit (a unit
/// that straddles styled runs): no break is allowed before it, and the unit's
/// pieces are fitted on a line together. A `float` run is out of the flow:
/// placed at the top of the line where it occurs (this line if still empty,
/// else the next), against that side of the line's extent, its box (advance
/// by height) excludes itself from every line whose top band overlaps it.
/// A `clear` run starts a new line below every earlier float on that side
/// (a floated run with `clear` goes below them instead of beside).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Run {
  pub metrics: RunMetrics,
  pub hard_break: bool,
  pub glue: bool,
  pub float: Option<Side>,
  pub clear: Option<Clear>,
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

/// Where a line is about to open: `index` counts lines from 0, `y` is the
/// line's top and `height` the height of the run opening it (the line may
/// still grow taller). Asked once per line, as CSS shapes do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineCursor {
  pub index: usize,
  pub y: f32,
  pub height: f32,
}

/// One horizontal span a line may use: its start x within the layout and its
/// width. The extent hook answers a cursor with the line's spans in left to
/// right order (one for a plain column, two around an exclusion in the
/// middle); an empty answer means no room on that line, and the breaker
/// moves down by the cursor's height and asks again, so a hook must leave
/// room below every exclusion. Alignment works inside each span; the
/// intrinsic widths ignore the hook.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineExtent {
  pub x: f32,
  pub width: f32,
}

impl LineExtent {
  /// The whole layout width.
  pub fn full(width: f32) -> Self {
    Self { x: 0.0, width }
  }
}

/// A filled span of a line: the extent it was given plus what went in it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineSegment {
  pub x: f32,
  pub width: f32,
  /// Ink used from `x`: to the last run's last inked glyph (the full width
  /// when justified).
  pub ink: f32,
  /// Range into `Layout::runs`.
  pub first: usize,
  pub end: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
  pub y: f32,
  pub height: f32,
  /// Baseline offset from the line's top.
  pub ascent: f32,
  /// Range into `Layout::runs`: every segment's runs, in order.
  pub first: usize,
  pub end: usize,
  /// The line's spans in left to right order, as the extent hook gave them;
  /// a segment can be empty when nothing fit in it.
  pub segments: Vec<LineSegment>,
}

impl Line {
  /// Rightmost ink edge over the line's segments, from the layout origin.
  pub fn right_edge(&self) -> f32 {
    self.segments.iter().map(|s| s.x + s.ink).fold(0.0, f32::max)
  }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
  pub lines: Vec<Line>,
  pub runs: Vec<PlacedRun>,
  /// Floated runs, out of the flow: top-left of each, in text order.
  pub floats: Vec<PlacedRun>,
  /// Right edge of the widest line's ink, from the layout origin; floats
  /// count.
  pub width: f32,
  /// Bottom of the last line or the lowest float, whichever is lower.
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
/// on the current line while their ink fits in the line's current segment,
/// then in its next segment, then on the next line; `extent` supplies the
/// segments per line (`vec![LineExtent::full(w)]` for a plain column). A unit
/// that fits in no segment of an empty line is placed anyway and overflows.
/// `max_lines` of 0 means unlimited; when it cuts the text short and
/// `ellipsis` gives the metrics of an ellipsis run, the last line's last
/// segment is trimmed until the ellipsis fits and its position is reported.
/// Each line is as tall as its tallest run and every run's baseline sits on
/// the line's baseline, across segments.
pub fn layout(
  runs: &[Run],
  extent: &dyn Fn(LineCursor) -> Vec<LineExtent>,
  align: Align,
  max_lines: u32,
  ellipsis: Option<RunMetrics>,
) -> Layout {
  layout_capped(runs, extent, align, max_lines, ellipsis, None)
}

/// How lines are chosen beyond greedy fitting (CSS text-wrap). Post-passes
/// over the greedy result, arithmetic only; both keep the greedy line count
/// and neither applies once `max_lines` truncates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wrap {
  /// Greedy: each line takes as much as fits.
  #[default]
  Wrap,
  /// Even line lengths: the widest line gives up its last unit while the
  /// line count holds, so a two-line heading does not leave a lone word.
  Balance,
  /// Greedy, except a lone word on the last line pulls a unit down from the
  /// line above when that keeps the count.
  Pretty,
}

/// `layout` with a wrapping policy.
pub fn layout_wrap(
  runs: &[Run],
  extent: &dyn Fn(LineCursor) -> Vec<LineExtent>,
  align: Align,
  max_lines: u32,
  ellipsis: Option<RunMetrics>,
  wrap: Wrap,
) -> Layout {
  let mut best = layout(runs, extent, align, max_lines, ellipsis);
  if best.truncated || best.lines.len() < 2 {
    return best;
  }
  let lines = best.lines.len();
  let overflowing = best.overflowing.len();
  // A candidate is acceptable when it changed nothing but where lines break.
  let same_shape = |l: &Layout| l.lines.len() == lines && l.overflowing.len() == overflowing && !l.truncated;
  match wrap {
    Wrap::Wrap => {}
    Wrap::Balance => loop {
      // Cap every line just under the widest one's ink edge, so that line
      // drops its last unit; stop when that costs a line.
      let cap = best.lines.iter().map(|l| l.right_edge()).fold(0.0, f32::max);
      let next = layout_capped(runs, extent, align, max_lines, ellipsis, Some((0, cap)));
      if !same_shape(&next) || next.lines.iter().map(|l| l.right_edge()).fold(0.0, f32::max) >= cap {
        break;
      }
      best = next;
    },
    Wrap::Pretty => {
      let last = &best.lines[lines - 1];
      let units = |l: &Line, out: &Layout| out.runs[l.first..l.end].iter().filter(|p| !runs[p.run].glue).count();
      if units(last, &best) == 1 {
        // Cap the line above (and only it) under its ink edge so its last
        // unit joins the lone word.
        let cap = best.lines[lines - 2].right_edge();
        let next = layout_capped(runs, extent, align, max_lines, ellipsis, Some((lines - 2, cap)));
        if same_shape(&next) && units(&next.lines[lines - 1], &next) >= 2 {
          best = next;
        }
      }
    }
  }
  best
}

// `layout` with an optional break cap `(from_line, x)`: on lines with index
// >= from_line a unit only fits while its ink ends left of `x` (from the
// layout origin). Fitting only; alignment and justify still use the real
// extents.
fn layout_capped(
  runs: &[Run],
  extent: &dyn Fn(LineCursor) -> Vec<LineExtent>,
  align: Align,
  max_lines: u32,
  ellipsis: Option<RunMetrics>,
  cap: Option<(usize, f32)>,
) -> Layout {
  let mut b = Breaker {
    runs,
    extent,
    align,
    cap,
    out: Layout::default(),
    line: Line::default(),
    extents: Vec::new(),
    seg: 0,
    seg_first: 0,
    seg_ink: 0.0,
    pen: 0.0,
    y: 0.0,
    pending: Vec::new(),
    exclusions: Vec::new(),
  };
  // Whether closing the open line would exceed the cap: the open line counts.
  let last_line = |b: &Breaker| max_lines > 0 && b.out.lines.len() as u32 + 1 >= max_lines;

  let mut index = 0;
  'runs: while index < runs.len() {
    let run = runs[index];
    if let Some(clear) = run.clear {
      if b.line.end > b.line.first {
        if last_line(&b) {
          b.out.truncated = true;
          break;
        }
        b.close_line(false);
      }
      b.flush_pending();
      b.clear(clear);
    }
    if run.float.is_some() {
      // Out of the flow: on this line's top if it is still empty (re-ask its
      // extents so it lands before the segments are cut), else on the next.
      if b.line.end == b.line.first {
        b.extents.clear();
      }
      b.pending.push(index);
      if run.hard_break && b.line.end > b.line.first {
        b.close_line(false);
      }
      index += 1;
      continue;
    }
    if !run.glue {
      let ink = unit_ink(runs, index);
      let height = run.metrics.height();
      loop {
        let width = b.open(height);
        if b.pen + ink <= width && b.under_cap(b.pen + ink) {
          break;
        }
        // Does not fit here: try the line's next segment, then the next
        // line if this one has content; an empty line takes it regardless.
        if b.next_segment(true) {
          continue;
        }
        if b.line.end == b.line.first {
          b.out.overflowing.push(index);
          break;
        }
        if last_line(&b) {
          b.out.truncated = true;
          break 'runs;
        }
        b.close_line(true);
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
  // Floats after the last text land below it; past a truncation they are cut
  // with the text.
  if !b.out.truncated {
    b.flush_pending();
  }
  b.out.height = b.y.max(b.exclusions.iter().map(|e| e.bottom).fold(0.0, f32::max));
  b.out
}

// A float's box: the lines it overlaps lose `left..right`.
#[derive(Clone, Copy)]
struct Exclusion {
  left: f32,
  right: f32,
  top: f32,
  bottom: f32,
  side: Side,
}

struct Breaker<'a> {
  runs: &'a [Run],
  extent: &'a dyn Fn(LineCursor) -> Vec<LineExtent>,
  align: Align,
  cap: Option<(usize, f32)>,
  out: Layout,
  // The open line: `first..end` of `out.runs` are on it, `y` its top. Its
  // segments are `extents` (empty until the line's first unit arrives) and
  // `seg` is the one being filled: `seg_first..` of `out.runs` are in it,
  // `pen` is where the next run starts relative to the segment's x and
  // `seg_ink` how far the segment's ink reaches.
  line: Line,
  extents: Vec<LineExtent>,
  seg: usize,
  seg_first: usize,
  seg_ink: f32,
  pen: f32,
  y: f32,
  // Float runs waiting for the next line top, and the boxes of the placed ones.
  pending: Vec<usize>,
  exclusions: Vec<Exclusion>,
}

impl Breaker<'_> {
  // The current segment's width, opening the line first if needed: asks
  // `extent` with the opening run's `height`, places the floats waiting for
  // this line top against the answer's outer edges, cuts the answer around
  // every float overlapping the line's top band, and skips down past lines
  // with no room: an empty answer, or one a float covers whole. An answer
  // of zero-width extents with no float in the band is taken as it is,
  // since nothing further down can open room and the descent would never
  // end (a text with content laid out against a 0-wide box).
  fn open(&mut self, height: f32) -> f32 {
    if self.extents.is_empty() {
      let mut segments = loop {
        let extents = (self.extent)(LineCursor { index: self.out.lines.len(), y: self.y, height });
        self.place_pending(&extents);
        let segments = self.cut(&extents, height);
        if !segments.is_empty() || height <= 0.0 || (!extents.is_empty() && !self.blocked(height)) {
          break segments;
        }
        self.y += height;
      };
      if segments.is_empty() {
        segments.push(LineExtent { x: 0.0, width: 0.0 });
      }
      self.extents = segments;
      self.seg = 0;
      self.seg_first = self.out.runs.len();
      self.seg_ink = 0.0;
      self.pen = 0.0;
    }
    self.extents[self.seg].width
  }

  // Whether a float overlaps the open line's band at the current y.
  fn blocked(&self, height: f32) -> bool {
    let (top, bottom) = (self.y, self.y + height);
    self.exclusions.iter().any(|e| e.top < bottom && e.bottom > top)
  }

  // Whether an ink edge at `pen` in the current segment is left of the break
  // cap, if one applies to the open line.
  fn under_cap(&self, pen: f32) -> bool {
    match self.cap {
      Some((from, x)) if self.out.lines.len() >= from => self.extents[self.seg].x + pen < x,
      _ => true,
    }
  }

  // Place the waiting floats at the current y without opening a line: asks
  // the hook for the edges with the tallest float's height.
  fn flush_pending(&mut self) {
    if self.pending.is_empty() {
      return;
    }
    let height = self.pending.iter().map(|&i| self.runs[i].metrics.height()).fold(0.0, f32::max);
    let extents = (self.extent)(LineCursor { index: self.out.lines.len(), y: self.y, height });
    self.place_pending(&extents);
    self.extents.clear();
  }

  // Move the (empty) open line down below every float on the cleared side.
  fn clear(&mut self, clear: Clear) {
    let bottom = self
      .exclusions
      .iter()
      .filter(|e| match clear {
        Clear::Left => e.side == Side::Left,
        Clear::Right => e.side == Side::Right,
        Clear::Both => true,
      })
      .map(|e| e.bottom)
      .fold(self.y, f32::max);
    if bottom > self.y {
      self.y = bottom;
      self.extents.clear();
    }
  }

  // Place the waiting floats at the current y: a left float against the
  // first extent's left edge, a right float against the last extent's right
  // edge, each beside the same-side floats its top band overlaps.
  fn place_pending(&mut self, extents: &[LineExtent]) {
    let (Some(first), Some(last)) = (extents.first(), extents.last()) else {
      return;
    };
    let (left_edge, right_edge) = (first.x, last.x + last.width);
    for index in std::mem::take(&mut self.pending) {
      let run = self.runs[index];
      let (width, height) = (run.metrics.advance, run.metrics.height());
      let (top, bottom) = (self.y, self.y + height);
      let side = run.float.unwrap_or(Side::Left);
      let overlapping = self.exclusions.iter().filter(|e| e.side == side && e.top < bottom && e.bottom > top);
      let x = match side {
        Side::Left => overlapping.map(|e| e.right).fold(left_edge, f32::max),
        Side::Right => overlapping.map(|e| e.left).fold(right_edge, f32::min) - width,
      };
      self.exclusions.push(Exclusion { left: x, right: x + width, top, bottom, side });
      self.out.floats.push(PlacedRun { run: index, x, y: top });
      self.out.width = self.out.width.max(x + width);
    }
  }

  // The extents minus every float overlapping the band `y..y+height`.
  fn cut(&self, extents: &[LineExtent], height: f32) -> Vec<LineExtent> {
    let (top, bottom) = (self.y, self.y + height);
    let mut cuts: Vec<(f32, f32)> =
      self.exclusions.iter().filter(|e| e.top < bottom && e.bottom > top).map(|e| (e.left, e.right)).collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    for e in extents {
      let (mut x, end) = (e.x, e.x + e.width);
      for &(left, right) in &cuts {
        if right <= x || left >= end {
          continue;
        }
        if left > x {
          out.push(LineExtent { x, width: left - x });
        }
        x = x.max(right);
      }
      if end > x {
        out.push(LineExtent { x, width: end - x });
      }
    }
    out
  }

  fn place(&mut self, index: usize) {
    let run = &self.runs[index];
    self.open(run.metrics.height());
    self.out.runs.push(PlacedRun { run: index, x: self.pen, y: 0.0 });
    self.line.end = self.out.runs.len();
    self.seg_ink = self.pen + run.metrics.ink_width;
    self.line.ascent = self.line.ascent.max(run.metrics.ascent);
    self.line.height = self.line.height.max(run.metrics.height());
    self.pen += run.metrics.advance;
  }

  // Move to the line's next segment, closing the current one; false when
  // the current segment is the line's last.
  fn next_segment(&mut self, wrapped: bool) -> bool {
    if self.seg + 1 >= self.extents.len() {
      return false;
    }
    self.close_segment(wrapped);
    self.seg += 1;
    self.seg_first = self.out.runs.len();
    self.seg_ink = 0.0;
    self.pen = 0.0;
    true
  }

  // Close the current segment: align its runs within its extent (justify
  // only when text overflowed it), and record it on the line. Run y is the
  // line's business, settled at close_line.
  fn close_segment(&mut self, wrapped: bool) {
    let e = self.extents[self.seg];
    let slack = (e.width - self.seg_ink).max(0.0);
    let offset = match self.align {
      Align::Left | Align::Justify => 0.0,
      Align::Center => slack / 2.0,
      Align::Right => slack,
    };
    let (first, end) = (self.seg_first, self.out.runs.len());
    let mut justify_step = 0.0;
    if self.align == Align::Justify && wrapped {
      let units = self.out.runs[first..end].iter().filter(|p| !self.runs[p.run].glue).count();
      if units > 1 {
        justify_step = slack / (units - 1) as f32;
      }
    }
    let mut unit = 0usize;
    for (i, placed) in self.out.runs[first..end].iter_mut().enumerate() {
      if i > 0 && !self.runs[placed.run].glue {
        unit += 1;
      }
      placed.x += e.x + offset + justify_step * unit as f32;
    }
    if let Some((x, _)) = &mut self.out.ellipsis {
      // The ellipsis sits in this (last) segment: `x` holds its pen until now.
      *x += e.x + offset;
    }
    let ink = if justify_step > 0.0 { e.width } else { self.seg_ink };
    self.out.width = self.out.width.max(e.x + ink);
    self.line.segments.push(LineSegment { x: e.x, width: e.width, ink, first, end });
  }

  // Close the open line: close its last segment, settle every run's y on the
  // baseline, and start the next line.
  fn close_line(&mut self, wrapped: bool) {
    self.close_segment(wrapped);
    let (first, end) = (self.line.first, self.line.end);
    for placed in &mut self.out.runs[first..end] {
      placed.y = self.y + self.line.ascent - self.runs[placed.run].metrics.ascent;
    }
    if let Some((_, y)) = &mut self.out.ellipsis {
      // `y` held the ellipsis run's ascent until now.
      *y = self.y + self.line.ascent - *y;
    }
    self.line.y = self.y;
    self.y += self.line.height;
    let next = Line { first: self.out.runs.len(), end: self.out.runs.len(), ..Line::default() };
    self.out.lines.push(std::mem::replace(&mut self.line, next));
    self.extents.clear();
    self.pen = 0.0;
  }

  // Drop runs from the end of the open line's current segment until the
  // ellipsis fits after the last one's ink, then reserve its slot. A segment
  // that cannot fit even the ellipsis alone keeps just the ellipsis.
  fn trim_for_ellipsis(&mut self, ell: RunMetrics) {
    let width = self.open(ell.height());
    let mut ell_x = 0.0;
    while self.line.end > self.seg_first {
      let placed = self.out.runs[self.line.end - 1];
      ell_x = placed.x + self.runs[placed.run].metrics.ink_width;
      if ell_x + ell.ink_width <= width {
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
    self.seg_ink = ell_x + ell.ink_width;
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
