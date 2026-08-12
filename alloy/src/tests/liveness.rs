use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use impellers::{ISize, Point, Rect, Size};

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
  liveness.on_frame_signal(start + Duration::from_millis(100));
  assert!(latch.swap(false, Ordering::Relaxed));
  liveness.on_frame_signal(start + Duration::from_millis(499));
  assert!(latch.swap(false, Ordering::Relaxed));

  // The first signal past the deadline closes the window; nothing after
  // latches.
  liveness.on_frame_signal(start + Duration::from_millis(500));
  assert!(!latch.load(Ordering::Relaxed));
  liveness.on_frame_signal(start + Duration::from_millis(501));
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
  liveness.on_frame_signal(start + Duration::from_millis(700));
  assert!(latch.swap(false, Ordering::Relaxed));
  liveness.on_frame_signal(start + Duration::from_millis(901));
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
  liveness.on_frame_signal(now + Duration::from_millis(100));
}
