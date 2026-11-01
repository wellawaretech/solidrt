use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::renderer::{ISize, Point, Rect, Size};

use crate::event::AlloyEvent;
use crate::liveness::SurfaceLiveness;

fn resize() -> AlloyEvent {
  AlloyEvent::Resize {
    size: ISize::new(800, 600),
    safe_area: Rect::new(Point::new(0.0, 0.0), Size::new(800.0, 600.0)),
    display_scale: 1.0,
  }
}

fn liveness_with_latch() -> (SurfaceLiveness, Arc<AtomicBool>) {
  let mut liveness = SurfaceLiveness::new();
  let latch = Arc::new(AtomicBool::new(false));
  liveness.set_latch(latch.clone());
  (liveness, latch)
}

#[test]
fn expose_and_return_to_visible_latch_and_rebind() {
  let (mut liveness, latch) = liveness_with_latch();
  let now = Instant::now();

  assert!(liveness.on_event(&AlloyEvent::Exposed, now));
  assert!(latch.swap(false, Ordering::Relaxed));

  assert!(liveness.on_event(&AlloyEvent::Visibility { visible: true }, now));
  assert!(latch.swap(false, Ordering::Relaxed));

  // Going hidden needs neither a rebind nor a repaint.
  assert!(!liveness.on_event(&AlloyEvent::Visibility { visible: false }, now));
  assert!(!latch.load(Ordering::Relaxed));

  // Unrelated events pass through untouched.
  assert!(!liveness.on_event(&AlloyEvent::Back, now));
  assert!(!latch.load(Ordering::Relaxed));
}

#[test]
fn resize_relatches_on_frame_signals_until_settled() {
  let (mut liveness, latch) = liveness_with_latch();
  let start = Instant::now();

  assert!(liveness.on_event(&resize(), start));
  assert!(latch.swap(false, Ordering::Relaxed));

  // Frame signals inside the settle window re-latch a repaint.
  liveness.on_frame_signal(start + Duration::from_millis(100), 0);
  assert!(latch.swap(false, Ordering::Relaxed));
  liveness.on_frame_signal(start + Duration::from_millis(499), 0);
  assert!(latch.swap(false, Ordering::Relaxed));

  // The first signal past the deadline closes the window; nothing after
  // latches.
  liveness.on_frame_signal(start + Duration::from_millis(500), 0);
  assert!(!latch.load(Ordering::Relaxed));
  liveness.on_frame_signal(start + Duration::from_millis(501), 0);
  assert!(!latch.load(Ordering::Relaxed));
}

#[test]
fn resize_stream_pushes_the_deadline_out() {
  let (mut liveness, latch) = liveness_with_latch();
  let start = Instant::now();

  liveness.on_event(&resize(), start);
  liveness.on_event(&resize(), start + Duration::from_millis(400));
  latch.store(false, Ordering::Relaxed);

  // 700ms after the first resize but inside the second one's window.
  liveness.on_frame_signal(start + Duration::from_millis(700), 0);
  assert!(latch.swap(false, Ordering::Relaxed));
  liveness.on_frame_signal(start + Duration::from_millis(901), 0);
  assert!(!latch.load(Ordering::Relaxed));
}

#[test]
fn resize_rebinds_once_per_pump_iteration() {
  let (mut liveness, _latch) = liveness_with_latch();
  let now = Instant::now();

  liveness.begin_pump();
  assert!(liveness.on_event(&resize(), now));
  assert!(!liveness.on_event(&resize(), now));

  // The next pump iteration rebinds again.
  liveness.begin_pump();
  assert!(liveness.on_event(&resize(), now));

  // Expose is not deduplicated (matches the pre-move per-event behavior).
  assert!(liveness.on_event(&AlloyEvent::Exposed, now));
  assert!(liveness.on_event(&AlloyEvent::Exposed, now));
}

#[test]
fn no_latch_registered_still_reports_rebinds() {
  let mut liveness = SurfaceLiveness::new();
  let now = Instant::now();

  assert!(liveness.on_event(&AlloyEvent::Exposed, now));
  assert!(liveness.on_event(&resize(), now));
  liveness.on_frame_signal(now + Duration::from_millis(100), 0);
}

#[test]
fn held_redraw_waits_for_a_frame_built_after_the_ask() {
  let (mut liveness, latch) = liveness_with_latch();
  let now = Instant::now();

  liveness.on_redraw_requested(7);
  assert!(latch.swap(false, Ordering::Relaxed), "the ask latches a repaint");

  // Frame 4 was in flight at the ask: built before it, it does not answer.
  assert_eq!(liveness.on_present(4, true), None);

  // The first signal after the ask names frame 5 as the first that counts,
  // and re-latches so frame 5 is built even if frame 4 took the first latch.
  liveness.on_frame_signal(now, 5);
  assert!(latch.swap(false, Ordering::Relaxed));

  // An unswapped present (backgrounded) does not answer; signals keep
  // re-latching until one does.
  assert_eq!(liveness.on_present(5, false), None);
  liveness.on_frame_signal(now, 6);
  assert!(latch.swap(false, Ordering::Relaxed));
  assert_eq!(liveness.on_present(6, true), Some(7));

  // Answered: nothing re-latches and later presents answer nothing.
  liveness.on_frame_signal(now, 7);
  assert!(!latch.load(Ordering::Relaxed));
  assert_eq!(liveness.on_present(7, true), None);
}

#[test]
fn a_newer_redraw_ask_supersedes_an_unanswered_one() {
  let (mut liveness, _latch) = liveness_with_latch();
  let now = Instant::now();

  liveness.on_redraw_requested(1);
  liveness.on_frame_signal(now, 3);
  // The newer ask restarts the wait: frame 3 was built before it.
  liveness.on_redraw_requested(2);
  assert_eq!(liveness.on_present(3, true), None);
  liveness.on_frame_signal(now, 4);
  assert_eq!(liveness.on_present(4, true), Some(2));
}
