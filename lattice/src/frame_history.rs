use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::Instant;

use alloy::rendertree::counters::LayoutCounters;
use alloy::RasterCounters;

/// Frames kept: ~10 s at 60 Hz. A query summarizes a window of at most this
/// many recent rebuilds; older frames fall off the ring.
const CAPACITY: usize = 600;

/// Longest window a query may ask for, in ms; a longer ask is clamped so the
/// answer never silently covers less than it claims (the ring is bounded).
pub const WINDOW_MAX_MS: f64 = 10_000.0;

/// How far back a stats query looks: a span of wall time, or the last n
/// frames that changed the picture. Records are stamped with wall time, so a
/// time window keeps expiring while the app's clock is paused and frames are
/// stepped by hand; the frame count is the window for that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Window {
  Ms(f64),
  Frames(usize),
}

impl Window {
  /// The ask the ring can honor: a span up to WINDOW_MAX_MS, a count of one
  /// to CAPACITY frames.
  pub fn clamped(self) -> Window {
    match self {
      Window::Ms(ms) => Window::Ms(ms.clamp(0.0, WINDOW_MAX_MS)),
      Window::Frames(n) => Window::Frames(n.clamp(1, CAPACITY)),
    }
  }
}

/// Milliseconds on the client's monotonic clock (process origin). Stamped on
/// every record and reported in the stats payload as `timeMs`, so two
/// samples can be differenced without trusting the caller's wall clock.
pub fn now_ms() -> f64 {
  static ORIGIN: OnceLock<Instant> = OnceLock::new();
  ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// One fully rebuilt frame's JS-thread cost and activity, raw. The EMAs in
/// overlay::Stats give a stable number to watch; these give the frame that
/// hurt. `total_ms` is the JS-thread critical path (render handler + layout +
/// post-layout + paint + hover), the figure a slow frame is judged by.
#[derive(Clone, Copy, Default)]
pub struct FrameRecord {
  pub at_ms: f64,
  pub frame: u64,
  /// Refresh period the frame was judged against (ms); `total_ms` beyond it
  /// is a slow frame.
  pub period_ms: f32,
  pub js_ms: f32,
  pub layout_ms: f32,
  pub post_ms: f32,
  pub paint_ms: f32,
  pub hover_ms: f32,
  pub total_ms: f32,
  pub counters: LayoutCounters,
  pub nodes_painted: u32,
  /// Backdrop panels re-filtered ahead of a fading ancestor's opacity group
  /// (see rendertree composite PaintStats); with nodes_painted, the paint
  /// activity of the frame, which the latest-frame stats lose on a reuse.
  pub backdrops_prepainted: u32,
  /// The raster counters as they stood when the frame was recorded; two
  /// records give a rate over the frames between them.
  pub raster: RasterCounters,
}

/// Summary of the rebuilt frames inside a query window (see `summarize`).
pub struct WindowSummary {
  /// The span the summary reaches back from the query instant (ms): the
  /// asked span for a time window, the oldest record's age for a count.
  pub window_ms: f64,
  /// The count asked for, for a frame-count window.
  pub window_frames: Option<usize>,
  pub frames: usize,
  pub p50_ms: f32,
  pub p95_ms: f32,
  pub max_ms: f32,
  pub slow_frames: usize,
  pub worst: FrameRecord,
  /// Backdrop panels re-filtered under a fading group, summed over the
  /// window: nonzero means a glass fade ran, whichever frame was worst.
  pub backdrops_prepainted: u32,
  /// The widest paint walk in the window.
  pub nodes_painted_max: u32,
  /// Rates over the window's span, derived from the raster samples of its
  /// first and last record; per-frame figures divide by the frames presented
  /// between them (the frame index), not by rebuilds. None with fewer than
  /// two records.
  pub raster_rates: Option<RasterRates>,
}

pub struct RasterRates {
  /// Presents missed while a next frame was demanded, over the window: the
  /// direct jank count (a repeated frame that every average hides). A count,
  /// not a rate - one miss is one visible hitch.
  pub missed_presents: u64,
  pub fence_timeouts_per_sec: f32,
  pub passes_per_frame: f32,
  pub pass_issue_ms_per_frame: f32,
  /// None when the client's GL context has no timer queries.
  pub pass_exec_ms_per_frame: Option<f32>,
  pub frame_exec_ms_per_frame: Option<f32>,
  pub cmd_ms_per_sec: f32,
}

/// Bounded ring of the most recent frames that changed the picture (a tree
/// rebuild, or GPU content presented through a reused display list),
/// written once per such frame by the draw loop and read by the stats
/// query. Cheap on the hot path: one fixed-size push under a lock
/// nobody else contends for.
pub struct FrameHistory {
  ring: VecDeque<FrameRecord>,
}

impl FrameHistory {
  pub fn new() -> Self {
    FrameHistory { ring: VecDeque::with_capacity(CAPACITY) }
  }

  pub fn push(&mut self, record: FrameRecord) {
    if self.ring.len() == CAPACITY {
      self.ring.pop_front();
    }
    self.ring.push_back(record);
  }

  /// Summarize the frames inside `window` (clamped, see Window::clamped) as
  /// of `now_ms`. None when no frame falls inside it: an idle app changes
  /// nothing, and "no frames" must read differently from "frames, all
  /// fast".
  pub fn summarize(&self, window: Window, now_ms: f64) -> Option<WindowSummary> {
    let (start, window_ms, window_frames) = match window.clamped() {
      Window::Ms(ms) => (self.ring.partition_point(|r| r.at_ms < now_ms - ms), ms, None),
      Window::Frames(n) => {
        let start = self.ring.len().saturating_sub(n);
        let reach = self.ring.get(start).map_or(0.0, |r| now_ms - r.at_ms);
        (start, reach, Some(n))
      }
    };
    let frames: Vec<&FrameRecord> = self.ring.range(start..).collect();
    if frames.is_empty() {
      return None;
    }
    let mut totals: Vec<f32> = frames.iter().map(|r| r.total_ms).collect();
    totals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pct = |p: f32| totals[((totals.len() - 1) as f32 * p).round() as usize];
    let worst = **frames
      .iter()
      .max_by(|a, b| a.total_ms.partial_cmp(&b.total_ms).unwrap_or(std::cmp::Ordering::Equal))
      .expect("non-empty window");
    let slow_frames = frames.iter().filter(|r| r.total_ms > r.period_ms).count();
    let raster_rates = match (frames.first(), frames.last()) {
      (Some(first), Some(last)) if frames.len() >= 2 && last.at_ms > first.at_ms => {
        let span_s = ((last.at_ms - first.at_ms) / 1000.0) as f32;
        let n = last.frame.saturating_sub(first.frame).max(1) as f32;
        let d = |a: u64, b: u64| b.saturating_sub(a) as f32;
        let exec_per_frame = |a: Option<u64>, b: Option<u64>| match (a, b) {
          (Some(a), Some(b)) => Some(d(a, b) / 1000.0 / n),
          _ => None,
        };
        Some(RasterRates {
          missed_presents: last.raster.missed_presents.saturating_sub(first.raster.missed_presents),
          fence_timeouts_per_sec: d(first.raster.fence_timeouts, last.raster.fence_timeouts) / span_s,
          passes_per_frame: d(first.raster.passes, last.raster.passes) / n,
          pass_issue_ms_per_frame: d(first.raster.pass_issue_micros, last.raster.pass_issue_micros) / 1000.0 / n,
          pass_exec_ms_per_frame: exec_per_frame(first.raster.pass_exec_micros, last.raster.pass_exec_micros),
          frame_exec_ms_per_frame: exec_per_frame(first.raster.frame_exec_micros, last.raster.frame_exec_micros),
          cmd_ms_per_sec: d(first.raster.cmd_micros, last.raster.cmd_micros) / 1000.0 / span_s,
        })
      }
      _ => None,
    };
    Some(WindowSummary {
      window_ms,
      window_frames,
      frames: frames.len(),
      p50_ms: pct(0.5),
      p95_ms: pct(0.95),
      max_ms: *totals.last().expect("non-empty"),
      slow_frames,
      worst,
      backdrops_prepainted: frames.iter().map(|r| r.backdrops_prepainted).sum(),
      nodes_painted_max: frames.iter().map(|r| r.nodes_painted).max().unwrap_or(0),
      raster_rates,
    })
  }
}
