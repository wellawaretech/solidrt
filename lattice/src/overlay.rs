use std::time::Instant;

use alloy::impellers::{
  Color, DisplayListBuilder, Paint, ParagraphBuilder, ParagraphStyle, Point, Rect, TextAlignment,
  TypographyContext,
};
use cpu_time::ProcessTime;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

const REFRESH_INTERVAL: f32 = 1.0;
const MIB: f32 = 1024.0 * 1024.0;
const PARA_WIDTH: f32 = 200.0;

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
    };
    stats.sample();
    stats
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
  }

  pub fn draw(
    &mut self,
    b: &mut DisplayListBuilder,
    typography: &TypographyContext,
    safe_area: Rect,
    fps: u32,
    requested_fps: u32,
  ) {
    self.refresh();

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
    // REQ counts frames requested per second (the demand-driven latch); FPS
    // counts frames actually drawn. Once frames are gated the two converge.
    let text = format!(
      "{} FPS\n{} REQ\n{:.0} MiB\n{:.0}% CPU",
      fps,
      requested_fps,
      self.proc_rss as f32 / MIB,
      self.proc_cpu
    );
    pb.add_text(&text);

    let Some(paragraph) = pb.build(PARA_WIDTH) else {
      return;
    };
    let x = safe_area.origin.x + safe_area.size.width - PARA_WIDTH - 10.0;
    let y = safe_area.origin.y + 10.0;
    b.draw_paragraph(&paragraph, Point::new(x, y));
  }
}