use crate::frame_history::{FrameHistory, FrameRecord, Window};

fn record(at_ms: f64, total_ms: f32, nodes_painted: u32, backdrops_prepainted: u32) -> FrameRecord {
  FrameRecord { at_ms, total_ms, nodes_painted, backdrops_prepainted, period_ms: 16.67, ..FrameRecord::default() }
}

fn history(records: &[FrameRecord]) -> FrameHistory {
  let mut h = FrameHistory::new();
  for r in records {
    h.push(*r);
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
