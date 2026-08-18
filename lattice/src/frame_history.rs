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
  /// The raster counters as they stood when the frame was recorded; two
  /// records give a rate over the frames between them.
  pub raster: RasterCounters,
}

/// Summary of the rebuilt frames inside a query window (see `summarize`).
pub struct WindowSummary {
  pub window_ms: f64,
  pub frames: usize,
  pub p50_ms: f32,
  pub p95_ms: f32,
  pub max_ms: f32,
  pub slow_frames: usize,
  pub worst: FrameRecord,
  /// Rates over the window's span, derived from the raster samples of its
  /// first and last record; per-frame figures divide by the frames presented
  /// between them (the frame index), not by rebuilds. None with fewer than
  /// two records.
  pub raster_rates: Option<RasterRates>,
}

pub struct RasterRates {
  pub fence_timeouts_per_sec: f32,
  pub passes_per_frame: f32,
  pub pass_ms_per_frame: f32,
  pub cmd_ms_per_sec: f32,
}

/// Bounded ring of the most recent rebuilt frames, written once per rebuild
/// by the draw loop and read by the stats query. Cheap on the hot path: one
/// fixed-size push under a lock nobody else contends for.
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

  /// Summarize the frames recorded in the last `window_ms` (clamped to
  /// WINDOW_MAX_MS) before `now_ms`. None when no frame falls inside it: an
  /// idle app rebuilds nothing, and "no frames" must read differently from
  /// "frames, all fast".
  pub fn summarize(&self, window_ms: f64, now_ms: f64) -> Option<WindowSummary> {
    let window_ms = window_ms.clamp(0.0, WINDOW_MAX_MS);
    let since = now_ms - window_ms;
    let start = self.ring.partition_point(|r| r.at_ms < since);
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
        Some(RasterRates {
          fence_timeouts_per_sec: d(first.raster.fence_timeouts, last.raster.fence_timeouts) / span_s,
          passes_per_frame: d(first.raster.passes, last.raster.passes) / n,
          pass_ms_per_frame: d(first.raster.pass_micros, last.raster.pass_micros) / 1000.0 / n,
          cmd_ms_per_sec: d(first.raster.cmd_micros, last.raster.cmd_micros) / 1000.0 / span_s,
        })
      }
      _ => None,
    };
    Some(WindowSummary {
      window_ms,
      frames: frames.len(),
      p50_ms: pct(0.5),
      p95_ms: pct(0.95),
      max_ms: *totals.last().expect("non-empty"),
      slow_frames,
      worst,
      raster_rates,
    })
  }
}
