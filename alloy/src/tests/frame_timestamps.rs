// frame_timestamps.rs: the span charged to a frame and the sweep over the
// display stack's answers, as pure functions (the EGL calls themselves only
// exist on Android).

use std::collections::VecDeque;

use crate::frame_timestamps::{span_micros, sweep, Entry, Poll, Swept};

const MS: i64 = 1_000_000;

#[test]
fn span_is_begin_to_complete_without_a_previous_frame() {
  assert_eq!(span_micros(20 * MS, 5 * MS, None), Some(15_000));
}

#[test]
fn span_is_floored_at_the_previous_completion() {
  // Queued 2 ms in, behind a frame that completed at 12 ms: charged for its
  // own 8 ms of execution, not the 18 ms it waited and ran.
  assert_eq!(span_micros(20 * MS, 2 * MS, Some(12 * MS)), Some(8_000));
  // A previous completion before this frame began does not shorten it.
  assert_eq!(span_micros(20 * MS, 5 * MS, Some(3 * MS)), Some(15_000));
}

#[test]
fn span_rejects_a_completion_before_the_begin() {
  assert_eq!(span_micros(4 * MS, 5 * MS, None), None);
}

fn queue(ids: &[u64]) -> VecDeque<Entry> {
  ids.iter().map(|&frame_id| Entry { frame_id, begin_ns: frame_id as i64 * 10 * MS }).collect()
}

#[test]
fn sweep_stops_at_the_first_pending_frame() {
  let mut pending = queue(&[1, 2, 3]);
  let mut last = None;
  let got = sweep(&mut pending, &mut last, |id| if id == 1 { Poll::Complete(16 * MS) } else { Poll::Pending });
  // Frame 1 began at 10 ms and completed at 16: 6 ms. Frames 2 and 3 wait.
  assert_eq!(got, Swept { latest_micros: Some(6_000), gone: 0 });
  assert_eq!(pending.iter().map(|e| e.frame_id).collect::<Vec<_>>(), vec![2, 3]);
  assert_eq!(last, Some(16 * MS));
}

#[test]
fn sweep_drops_gone_frames_and_reports_the_latest_completed() {
  let mut pending = queue(&[1, 2, 3]);
  let mut last = None;
  let got = sweep(&mut pending, &mut last, |id| match id {
    1 => Poll::Gone,
    2 => Poll::Complete(27 * MS),
    _ => Poll::Complete(41 * MS),
  });
  // Frame 2: 20 -> 27 = 7 ms. Frame 3 began at 30, behind 2's completion at
  // 27: 30 -> 41 = 11 ms, the latest, and the previous completion moves on.
  assert_eq!(got, Swept { latest_micros: Some(11_000), gone: 1 });
  assert!(pending.is_empty());
  assert_eq!(last, Some(41 * MS));
}

#[test]
fn sweep_with_nothing_settled_reports_nothing() {
  let mut pending = queue(&[1]);
  let mut last = None;
  assert_eq!(sweep(&mut pending, &mut last, |_| Poll::Pending), Swept::default());
  assert_eq!(pending.len(), 1);
  assert_eq!(last, None);
}

#[test]
fn sweep_counts_every_gone_frame() {
  let mut pending = queue(&[1, 2, 3]);
  let mut last = None;
  assert_eq!(sweep(&mut pending, &mut last, |_| Poll::Gone), Swept { latest_micros: None, gone: 3 });
  assert!(pending.is_empty());
  assert_eq!(last, None);
}
