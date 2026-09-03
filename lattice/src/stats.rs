use std::time::{Duration, Instant};

use alloy::rendertree::composite::PaintStats;
use alloy::rendertree::counters::LayoutCounters;
use cpu_time::ProcessTime;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

const REFRESH_INTERVAL: f32 = 1.0;

/// Wall-clock duration of each phase of one frame, handed from the draw loop
/// to the overlay; all zero for a frame that was reused or skipped.
#[derive(Clone, Copy, Default)]
pub struct FramePhases {
  pub layout: Duration,
  pub post: Duration,
  pub paint: Duration,
  pub hover: Duration,
}

/// Smoothed (milliseconds) value of each phase. Per-frame timings jitter too
/// much to read raw; the moving average gives a stable number.
#[derive(Default)]
struct PhaseEma {
  layout: f32,
  post: f32,
  paint: f32,
  hover: f32,
}

/// Plain-data copy of the current stats, published every frame for readers
/// outside the draw loop (the dev server's stats query) and the figures the
/// HUD renders (overlay.rs). Times are smoothed milliseconds; reused/skipped
/// are the last full second's demand-gate counts. Some fields have only the
/// query as reader, so builds without one see them as unread.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
#[derive(Clone, Copy, Default)]
pub struct StatsSnapshot {
  /// Present index of the latest frame the draw loop saw (frame::RenderFrame).
  pub frame: u64,
  pub fps: u32,
  pub cpu_pct: f32,
  pub mem_bytes: u64,
  pub js_ms: f32,
  pub frame_ms: f32,
  pub set_count: f32,
  pub layout_ms: f32,
  pub post_ms: f32,
  pub paint_ms: f32,
  pub hover_ms: f32,
  pub reused: u32,
  pub skipped: u32,
  pub textures: usize,
  /// GPU-side execution time per frame (ms) over the last sample window:
  /// window draws plus shader passes, from the raster thread's timer
  /// queries. None when the client's context has none.
  pub gpu_ms: Option<f32>,
  /// Layout-activity counters from the last full rebuild, raw (not smoothed):
  /// these are counts to reason about, not rates to watch. See
  /// alloy::rendertree::counters.
  pub node_count: usize,
  pub measure_calls: u32,
  pub para_shapes: u32,
  pub word_hits: u32,
  pub dirtied: u32,
  pub cache_gets: u32,
  pub cache_hits: u32,
  /// The latest frame's paint walk counts: nodes entered (the mounted count
  /// minus this is what viewport culling skipped, alloy::rendertree::cull)
  /// and the repaint/snapshot boundary figures. All zero when that frame was
  /// reused or skipped; the last rebuild's figures live in frame_history.
  pub paint: PaintStats,
}

// Smoothing time constant (seconds): a value settles to ~63% of a step in this
// long, ~99% in ~4.6x it. The weight is derived from the wall-clock gap between
// updates (see smooth), so this holds whether frames arrive at 60 Hz or, when
// the app idles, once per second. A fixed per-sample weight could not: when idle
// the phases update ~1x/second, so a plain EMA took the better part of a minute
// to forget a spike.
const SMOOTH_TAU: f32 = 0.15;

// Exponential smoothing toward `sample`, weighted by the elapsed time `dt` so
// the response is frame-rate independent. A zeroed average jumps straight to the
// first real sample instead of crawling up from 0.
fn smooth(prev: f32, sample: f32, dt: f32) -> f32 {
  if prev == 0.0 {
    return sample;
  }
  let alpha = 1.0 - (-dt / SMOOTH_TAU).exp();
  prev + alpha * (sample - prev)
}

/// Process stats (RSS memory + CPU%) sampled at most once per second and
/// rendered in the top-right debug overlay alongside the frame rate.
pub struct Stats {
  system: System,
  pid: Option<Pid>,
  proc_rss: u64,
  proc_cpu: f32,
  last_refresh: Instant,
  last_cpu_time: ProcessTime,
  last_cpu_wall: Instant,
  phases: PhaseEma,
  // Instants of the last js/draw smoothing updates; the gap feeds the
  // time-aware weight (see smooth) so the averages settle in ~SMOOTH_TAU even
  // when frames are sparse.
  last_js: Instant,
  last_draw: Instant,
  // The frame's JS cost (timers, rAF, onFrame + flush, ms) and setProperty calls per
  // frame, both moving-averaged. Sourced from native thread-local timers so
  // collecting them adds no work to the JS side.
  js_ms: f32,
  set_count: f32,
  // Smoothed wall-clock frame period (ms), measured on the JS thread between
  // render-handler invocations. Denominator for the timing percentages:
  // dividing by a same-thread, same-smoothing measure keeps a share <= 100%.
  // (alloy's fps is a 1-second average; mixing it with the fast js_ms overshot
  // past 200% whenever the frame time jumped.)
  frame_ms: f32,
  // Demand-gate accounting. The *_acc fields accumulate across the current
  // second; the plain fields are snapshotted from them once per second (in
  // sample) so the overlay shows per-second reuse/skip counts.
  reused_acc: u32,
  skipped_acc: u32,
  reused: u32,
  skipped: u32,
  // Layout activity of the last full rebuild (see StatsSnapshot); latched
  // raw so an idle app keeps reporting its last rebuild's figures.
  node_count: usize,
  layout_counters: LayoutCounters,
  // Boundary/snapshot counts of the latest frame's paint walk; all zero when
  // that frame was reused or skipped (no walk). Not latched like the layout
  // activity: the overlay presents these as what the current frame did, so
  // a stale rebuild's counts would read as live.
  paint_stats: PaintStats,
  // GPU execution accounting: the latest (frame, cumulative exec micros)
  // the draw loop recorded, the mark the last sample took, and the
  // per-frame figure computed between them. None while the raster thread
  // reports no timer queries.
  gpu_now: Option<(u64, u64)>,
  gpu_mark: Option<(u64, u64)>,
  gpu_ms: Option<f32>,
}

impl Stats {
  pub fn new() -> Self {
    let system = System::new_with_specifics(RefreshKind::nothing());
    let pid = sysinfo::get_current_pid().ok();
    let mut stats = Self {
      system,
      pid,
      proc_rss: 0,
      proc_cpu: 0.0,
      last_refresh: Instant::now(),
      last_cpu_time: ProcessTime::now(),
      last_cpu_wall: Instant::now(),
      phases: PhaseEma::default(),
      last_js: Instant::now(),
      last_draw: Instant::now(),
      js_ms: 0.0,
      set_count: 0.0,
      frame_ms: 0.0,
      reused_acc: 0,
      skipped_acc: 0,
      reused: 0,
      skipped: 0,
      node_count: 0,
      layout_counters: LayoutCounters::default(),
      paint_stats: PaintStats::default(),
      gpu_now: None,
      gpu_mark: None,
      gpu_ms: None,
    };
    stats.sample();
    stats
  }

  /// Whether the once-per-second overlay sample is due. The overlay's
  /// per-second figures (fps, mem, cpu, drawn/reused/skipped) change on this
  /// cadence, so a due overlay is itself a reason to draw a frame even when the
  /// app requested none. Pure read - but record_js's refresh() resets the same
  /// timer, so the draw loop must latch this before recording the frame's JS
  /// figures or a due overlay is never observed.
  pub fn overlay_due(&self) -> bool {
    self.last_refresh.elapsed().as_secs_f32() >= REFRESH_INTERVAL
  }

  /// Re-sample only after REFRESH_INTERVAL has elapsed; called every frame.
  fn refresh(&mut self) {
    if self.last_refresh.elapsed().as_secs_f32() >= REFRESH_INTERVAL {
      self.sample();
    }
  }

  fn sample(&mut self) {
    if let Some(pid) = self.pid {
      self.system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
      );
      if let Some(proc) = self.system.process(pid) {
        self.proc_rss = proc.memory();
      }
    }

    // Process CPU% via the cross-platform process clock (clock_gettime /
    // GetProcessTimes): CPU time consumed over wall time since last sample.
    let now_cpu = ProcessTime::now();
    let now_wall = Instant::now();
    let cpu_delta = now_cpu.duration_since(self.last_cpu_time).as_secs_f32();
    let wall_delta = now_wall.duration_since(self.last_cpu_wall).as_secs_f32();
    if wall_delta > 0.0 {
      self.proc_cpu = (cpu_delta / wall_delta) * 100.0;
    }
    self.last_cpu_time = now_cpu;
    self.last_cpu_wall = now_wall;
    self.last_refresh = now_wall;

    // Snapshot the per-second demand-gate counters and start a new window.
    self.reused = self.reused_acc;
    self.skipped = self.skipped_acc;
    self.reused_acc = 0;
    self.skipped_acc = 0;

    // GPU time per frame over the window just closed.
    if let (Some((f0, us0)), Some((f1, us1))) = (self.gpu_mark, self.gpu_now) {
      if f1 > f0 {
        self.gpu_ms = Some(us1.saturating_sub(us0) as f32 / 1000.0 / (f1 - f0) as f32);
      }
    }
    self.gpu_mark = self.gpu_now;
  }

  /// The raster thread's cumulative GPU execution counters as of `frame`
  /// (window draws plus shader passes). Recorded every frame, before
  /// `record_js` closes a sample window, so the per-frame figure spans
  /// exactly the window's frames.
  pub fn record_gpu(&mut self, frame: u64, raster: &alloy::RasterCounters) {
    self.gpu_now = match (raster.frame_exec_micros, raster.pass_exec_micros) {
      (Some(f), Some(p)) => Some((frame, f + p)),
      _ => None,
    };
  }

  /// The frame's JS time (timers, rAF, onFrame + flush, ms) and setProperty count for the
  /// frame. Recorded every frame, before the demand gate, since flush runs even
  /// when the native draw is skipped. Also drives the once-per-second sample
  /// (cpu/mem, per-second counters), so those stay fresh with the overlay off.
  pub fn record_js(&mut self, js_ms: f32, set_count: u32) {
    self.refresh();
    let now = Instant::now();
    let dt = (now - self.last_js).as_secs_f32();
    self.last_js = now;
    self.js_ms = smooth(self.js_ms, js_ms, dt);
    self.set_count = smooth(self.set_count, set_count as f32, dt);
    // The gap between render handlers is the frame period; smoothed the same way
    // as js_ms so percentages built from the two stay consistent.
    self.frame_ms = smooth(self.frame_ms, dt * 1000.0, dt);
  }

  /// Fold one frame's phase timings into the moving averages, weighted by the
  /// gap since the previous call (time-aware smoothing, see smooth). Called on
  /// every frame the draw loop sees, whether or not the overlay draws: the
  /// rebuild's measured phases, or all zero when the frame was reused or
  /// skipped. Recording the zeros is what keeps these on the same cadence as
  /// frame_ms, so a share of it means something; without them a static tree
  /// (display-list reuse forever) would hold its last rebuild's phases as if
  /// live. The cost of a rare rebuild is kept by frame_history, not here.
  pub fn record_frame(&mut self, phases: FramePhases) {
    let now = Instant::now();
    let dt = (now - self.last_draw).as_secs_f32();
    self.last_draw = now;
    self.phases.layout = smooth(self.phases.layout, phases.layout.as_secs_f32() * 1000.0, dt);
    self.phases.post = smooth(self.phases.post, phases.post.as_secs_f32() * 1000.0, dt);
    self.phases.paint = smooth(self.phases.paint, phases.paint.as_secs_f32() * 1000.0, dt);
    self.phases.hover = smooth(self.phases.hover, phases.hover.as_secs_f32() * 1000.0, dt);
  }

  /// Latch one rebuild's layout activity: live node count plus the counters
  /// taken from the rendertree (measure calls, paragraph shapes, dirtied
  /// caches since the previous rebuild).
  pub fn record_layout_activity(&mut self, nodes: usize, counters: LayoutCounters) {
    self.node_count = nodes;
    self.layout_counters = counters;
  }

  /// Record the frame's paint walk counts (see the field): the rebuild's, or
  /// zero for a reused or skipped frame.
  pub fn record_paint(&mut self, paint_stats: PaintStats) {
    self.paint_stats = paint_stats;
  }

  /// Plain-data copy of the current figures for readers outside the draw loop.
  /// `fps` and `textures` are owned by the platform/registry, so the caller
  /// supplies them.
  pub fn snapshot(&self, frame: u64, fps: u32, textures: usize) -> StatsSnapshot {
    StatsSnapshot {
      frame,
      fps,
      cpu_pct: self.proc_cpu,
      mem_bytes: self.proc_rss,
      js_ms: self.js_ms,
      frame_ms: self.frame_ms,
      set_count: self.set_count,
      layout_ms: self.phases.layout,
      post_ms: self.phases.post,
      paint_ms: self.phases.paint,
      hover_ms: self.phases.hover,
      reused: self.reused,
      skipped: self.skipped,
      textures,
      gpu_ms: self.gpu_ms,
      node_count: self.node_count,
      measure_calls: self.layout_counters.measure_calls,
      para_shapes: self.layout_counters.para_shapes,
      word_hits: self.layout_counters.word_hits,
      dirtied: self.layout_counters.dirtied,
      cache_gets: self.layout_counters.cache_gets,
      cache_hits: self.layout_counters.cache_hits,
      paint: self.paint_stats,
    }
  }

  /// A frame served from the cached display list (present-only reuse).
  pub fn note_reused(&mut self) {
    self.reused_acc += 1;
  }

  /// A frame skipped entirely by the demand-driven gate (nothing requested it).
  pub fn note_skipped(&mut self) {
    self.skipped_acc += 1;
  }
}
