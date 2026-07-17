use std::time::{Duration, Instant};

use alloy::impellers::{
  Color, DisplayListBuilder, Paint, ParagraphBuilder, ParagraphStyle, Point, Rect, Size, TextAlignment,
  TypographyContext,
};
use cpu_time::ProcessTime;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

const REFRESH_INTERVAL: f32 = 1.0;
const MIB: f32 = 1024.0 * 1024.0;
const PARA_WIDTH: f32 = 200.0;

/// Wall-clock duration of each phase of one fully rebuilt frame, handed from
/// the draw loop to the overlay.
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
/// outside the draw loop (the dev server's stats query). Times are smoothed
/// milliseconds, same values the overlay renders; reused/skipped are the last
/// full second's demand-gate counts.
#[derive(Clone, Copy, Default)]
pub struct StatsSnapshot {
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
  /// Layout-activity counters from the last full rebuild, raw (not smoothed):
  /// these are counts to reason about, not rates to watch. See
  /// alloy::rendertree::counters.
  pub node_count: usize,
  pub measure_calls: u32,
  pub para_shapes: u32,
  pub dirtied: u32,
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
  // JS render-handler cost (onFrame + flush, ms) and setProperty calls per
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
  layout_counters: alloy::rendertree::counters::LayoutCounters,
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
      layout_counters: alloy::rendertree::counters::LayoutCounters::default(),
    };
    stats.sample();
    stats
  }

  /// Whether the once-per-second overlay sample is due. The overlay's
  /// per-second figures (fps, mem, cpu, drawn/reused/skipped) change on this
  /// cadence, so a due overlay is itself a reason to draw a frame even when the
  /// app requested none. Pure read: the timer only resets when draw() samples.
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
  }

  /// JS render-handler time (onFrame + flush, ms) and setProperty count for the
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

  /// Fold one rebuilt frame's phase timings into the moving averages, weighted
  /// by the gap since the last rebuild (time-aware smoothing, see smooth).
  /// Called on every full rebuild, whether or not the overlay draws.
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
  pub fn record_layout_activity(&mut self, nodes: usize, counters: alloy::rendertree::counters::LayoutCounters) {
    self.node_count = nodes;
    self.layout_counters = counters;
  }

  /// Plain-data copy of the current figures for readers outside the draw loop.
  /// `fps` and `textures` are owned by the platform/registry, so the caller
  /// supplies them.
  pub fn snapshot(&self, fps: u32, textures: usize) -> StatsSnapshot {
    StatsSnapshot {
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
      node_count: self.node_count,
      measure_calls: self.layout_counters.measure_calls,
      para_shapes: self.layout_counters.para_shapes,
      dirtied: self.layout_counters.dirtied,
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

  pub fn draw(
    &mut self,
    b: &mut DisplayListBuilder,
    typography: &TypographyContext,
    safe_area: Rect,
    fps: u32,
    paint_stats: alloy::rendertree::composite::PaintStats,
    textures: usize,
  ) {
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 1.0, 1.0, 1.0));

    let mut style = ParagraphStyle::default();
    style.set_foreground(&paint);
    style.set_font_family("Noto Sans Mono");
    style.set_font_size(14.0);
    style.set_font_weight(alloy::impellers::FontWeight::Bold);
    style.set_text_alignment(TextAlignment::Right);

    let Some(mut pb) = ParagraphBuilder::new(typography) else {
      return;
    };
    pb.push_style(&style);

    let mut text = format!("{:.0}% CPU {:.0} MEM {} FPS", self.proc_cpu, self.proc_rss as f32 / MIB, fps);
    // Each timing is shown as a share of the measured frame period (js_ms and
    // frame_ms are smoothed the same way on the JS thread, so a share stays
    // within 100%). Shares sum to ~100% when CPU-bound; less means idle or
    // GPU-bound headroom. A share is relative to the current frame, so one phase
    // shrinks when another grows. JS = onFrame + flush; LAY/PNT/PST/HOV = native
    // draw phases. SET is a raw count (setProperty writes/frame), not a share.
    let frame_ms = self.frame_ms;
    let pct = |ms: f32| if frame_ms > 0.0 { ms / frame_ms * 100.0 } else { 0.0 };
    text.push_str(&format!("\nJS {:.0}% SET {:.0}", pct(self.js_ms), self.set_count));
    // Native draw phases as frame shares: LAY layout, PNT paint, PST postLayout,
    // HOV hover.
    text.push_str(&format!(
      "\nLAY {:.0}% PNT {:.0}%\nPST {:.0}% HOV {:.0}%",
      pct(self.phases.layout),
      pct(self.phases.paint),
      pct(self.phases.post),
      pct(self.phases.hover),
    ));
    // Demand-gate savings/sec: frames served from the cached display list
    // (reuse) and frames skipped entirely (skip). Hidden when the gate saved
    // nothing this second - every frame a full rebuild, which FPS already shows.
    if self.reused + self.skipped > 0 {
      text.push_str(&format!("\n{} reuse {} skip", self.reused, self.skipped));
    }
    // Repaint boundaries this frame: reused+recorded. Hidden when the app
    // declares none.
    if paint_stats.boundaries_reused + paint_stats.boundaries_recorded > 0 {
      text.push_str(&format!("\n{}+{} BND", paint_stats.boundaries_reused, paint_stats.boundaries_recorded));
    }
    // Snapshot boundaries this frame: reused+rasterized.
    if paint_stats.snapshots_reused + paint_stats.snapshots_rasterized > 0 {
      text.push_str(&format!("\n{}+{} SNP", paint_stats.snapshots_reused, paint_stats.snapshots_rasterized));
    }
    // Textures currently held in the registry (GL/Impeller texture pairs in use).
    if textures > 0 {
      text.push_str(&format!("\n{} TEX", textures));
    }

    pb.add_text(&text);

    let Some(paragraph) = pb.build(PARA_WIDTH) else {
      return;
    };
    let x = safe_area.origin.x + safe_area.size.width - PARA_WIDTH - 10.0;
    let y = safe_area.origin.y + 10.0;

    // Darkening backdrop so the white text stays legible over light content. The
    // paragraph is right-aligned in PARA_WIDTH, so its right edge sits at
    // x + PARA_WIDTH and the box only needs to span the longest line.
    let pad = 10.0;
    let text_w = paragraph.get_longest_line_width();
    let text_h = paragraph.get_height();
    let bg =
      Rect::new(Point::new(x + PARA_WIDTH - text_w - pad, y - pad), Size::new(text_w + pad * 2.0, text_h + pad * 2.0));
    let mut bg_paint = Paint::default();
    bg_paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, 0.7));
    b.draw_rect(&bg, &bg_paint);

    b.draw_paragraph(&paragraph, Point::new(x, y));
  }
}
