use crate::frame_history::{FrameHistory, FrameRecord, Window};

fn record(at_ms: f64, total_ms: f32, nodes_painted: u32, backdrops_prepainted: u32) -> FrameRecord {
  FrameRecord { at_ms, total_ms, nodes_painted, backdrops_prepainted, period_ms: 16.67, ..FrameRecord::default() }
}

fn history(records: &[FrameRecord]) -> FrameHistory {
  let mut h = FrameHistory::new();
  for r in records {
    h.push(r.clone());
  }
  h
}

#[test]
fn capture_frames_stay_out_of_the_timing_figures() {
  let stalled = FrameRecord { captures: 1, ..record(300.0, 120.0, 30, 0) };
  let h = history(&[record(100.0, 1.0, 10, 0), record(200.0, 2.0, 20, 0), stalled]);
  let w = h.summarize(Window::Frames(3), 1000.0).expect("three frames");
  assert_eq!(w.frames, 3);
  assert_eq!(w.capture_frames, 1);
  assert_eq!(w.max_ms, 2.0);
  assert_eq!(w.slow_frames, 0);
  assert_eq!(w.worst.at_ms, 200.0);
  // A window of nothing but capture frames still answers with them.
  let w = h.summarize(Window::Frames(1), 1000.0).expect("last frame");
  assert_eq!(w.capture_frames, 1);
  assert_eq!(w.max_ms, 120.0);
  assert_eq!(w.slow_frames, 1);
}

#[test]
fn empty_window_is_none() {
  let h = history(&[record(100.0, 1.0, 10, 0)]);
  assert!(h.summarize(Window::Ms(50.0), 1000.0).is_none());
  assert!(FrameHistory::new().summarize(Window::Frames(3), 1000.0).is_none());
}

#[test]
fn reach_reports_what_the_ring_holds() {
  assert_eq!(FrameHistory::new().reach(1000.0), None);
  let h = history(&[record(100.0, 1.0, 10, 0), record(400.0, 1.0, 10, 0)]);
  assert_eq!(h.reach(1000.0), Some((2, 900.0, 600.0)));
}

#[test]
fn gpu_share_is_exec_per_present_over_the_present_interval() {
  use alloy::RasterCounters;
  let at = |at_ms: f64, frame: u64, frame_exec_micros: u64, pass_exec_micros: u64| FrameRecord {
    frame,
    raster: RasterCounters {
      frame_exec_micros: Some(frame_exec_micros),
      pass_exec_micros: Some(pass_exec_micros),
      ..RasterCounters::default()
    },
    ..record(at_ms, 1.0, 10, 0)
  };
  // Sixty presents over one second, 2 ms of window draw and 1 ms of passes
  // each: 3 ms of a 16.67 ms present interval, 18%.
  let h = history(&[at(0.0, 0, 0, 0), at(500.0, 30, 60_000, 30_000), at(1000.0, 60, 120_000, 60_000)]);
  let rates = h.summarize(Window::Ms(1000.0), 1000.0).expect("frames").raster_rates.expect("two records");
  assert!((rates.present_ms - 16.667).abs() < 0.01, "present_ms {}", rates.present_ms);
  assert!((rates.gpu_share_pct().expect("timed") - 18.0).abs() < 0.05);
  // No frame timing source (a context without timer queries): no share.
  let untimed = FrameRecord { raster: RasterCounters::default(), ..record(0.0, 1.0, 10, 0) };
  let later = FrameRecord { frame: 60, ..record(1000.0, 1.0, 10, 0) };
  let h = history(&[untimed, later]);
  let rates = h.summarize(Window::Ms(1000.0), 1000.0).expect("frames").raster_rates;
  assert!(rates.is_none() || rates.expect("rates").gpu_share_pct().is_none());
}

#[test]
fn target_rates_attribute_a_window_s_passes_by_target() {
  use alloy::TargetCounters;
  use std::sync::Arc;
  let target = |id: u64, label: &str, passes: u64, issue: u64, exec: u64, vertices: u64| TargetCounters {
    id,
    label: Some(label.to_string()),
    passes,
    pass_issue_micros: issue,
    pass_exec_micros: Some(exec),
    vertices,
  };
  let at = |at_ms: f64, frame: u64, targets: Vec<TargetCounters>| FrameRecord {
    frame,
    targets: Arc::new(targets),
    ..record(at_ms, 1.0, 10, 0)
  };
  // Sixty presents: the scene renders every frame (1 ms GPU each), the
  // shadow atlas every other frame, a probe that existed before the window
  // never, and a view created mid-window (absent from the first record)
  // renders 30 times from zero.
  let h = history(&[
    at(0.0, 0, vec![target(1, "scene", 100, 10_000, 100_000, 6_000), target(2, "atlas", 50, 5_000, 20_000, 0), target(3, "probe", 6, 600, 3_000, 0)]),
    at(1000.0, 60, vec![
      target(1, "scene", 160, 40_000, 160_000, 366_000),
      target(2, "atlas", 80, 8_000, 35_000, 0),
      target(3, "probe", 6, 600, 3_000, 0),
      target(4, "view", 30, 3_000, 15_000, 180_000),
    ]),
  ]);
  let w = h.summarize(Window::Ms(1000.0), 1000.0).expect("frames");
  let ids: Vec<u64> = w.target_rates.iter().map(|t| t.id).collect();
  assert_eq!(ids, vec![1, 2, 4]);
  let scene = &w.target_rates[0];
  assert_eq!(scene.label.as_deref(), Some("scene"));
  assert!((scene.passes_per_frame - 1.0).abs() < 1e-6);
  assert!((scene.issue_ms_per_frame - 0.5).abs() < 1e-6);
  assert!((scene.exec_ms_per_frame.expect("timed") - 1.0).abs() < 1e-6);
  assert!((scene.vertices_per_frame - 6000.0).abs() < 1e-3);
  let atlas = &w.target_rates[1];
  assert!((atlas.passes_per_frame - 0.5).abs() < 1e-6);
  assert!((atlas.exec_ms_per_frame.expect("timed") - 0.25).abs() < 1e-6);
  let view = &w.target_rates[2];
  assert!((view.passes_per_frame - 0.5).abs() < 1e-6);
  assert!((view.vertices_per_frame - 3000.0).abs() < 1e-3);
}

#[test]
fn time_window_drops_older_frames() {
  let h = history(&[record(100.0, 1.0, 10, 0), record(900.0, 2.0, 20, 0), record(950.0, 3.0, 30, 0)]);
  let w = h.summarize(Window::Ms(200.0), 1000.0).expect("two frames inside");
  assert_eq!(w.frames, 2);
  assert_eq!(w.window_ms, 200.0);
  assert_eq!(w.window_frames, None);
  assert_eq!(w.worst.frame, 0);
  assert_eq!(w.max_ms, 3.0);
}

#[test]
fn frame_window_ignores_age() {
  let h = history(&[record(100.0, 1.0, 10, 0), record(200.0, 4.0, 20, 0), record(300.0, 3.0, 30, 0)]);
  // Long after the last record, a time window is empty; the frame count
  // still answers with the last two rebuilds, reaching back to the older.
  assert!(h.summarize(Window::Ms(10_000.0), 100_000.0).is_none());
  let w = h.summarize(Window::Frames(2), 100_000.0).expect("last two frames");
  assert_eq!(w.frames, 2);
  assert_eq!(w.window_frames, Some(2));
  assert_eq!(w.window_ms, 100_000.0 - 200.0);
  assert_eq!(w.worst.total_ms, 4.0);
  assert_eq!(w.nodes_painted_max, 30);
}

#[test]
fn frame_window_clamps_to_the_ring() {
  let h = history(&[record(100.0, 1.0, 10, 0)]);
  let w = h.summarize(Window::Frames(0), 200.0).expect("one frame");
  assert_eq!(w.frames, 1);
  assert_eq!(w.window_frames, Some(1));
}

#[test]
fn window_totals_survive_a_cheaper_worst() {
  // The glass fade's rebuild (one prepainted panel) is cheaper than a later
  // caret repaint, so `worst` is not it; the window total still says a fade
  // ran, and the widest walk is the fade's.
  let h = history(&[record(100.0, 1.0, 80, 1), record(600.0, 2.0, 40, 0)]);
  let w = h.summarize(Window::Ms(1000.0), 1000.0).expect("both frames");
  assert_eq!(w.worst.backdrops_prepainted, 0);
  assert_eq!(w.backdrops_prepainted, 1);
  assert_eq!(w.nodes_painted_max, 80);
}
