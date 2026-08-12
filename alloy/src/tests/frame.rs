use crate::rendertree::{FrameDriver, PlatformContext};

// The gate's inputs are the one-shot request latch, the caller's own demand,
// and playback mode; commit/finish need a live render thread and are covered
// by the integration paths.

#[test]
fn gate_skips_without_demand() {
  let platform = PlatformContext::new(Vec::new());
  let mut driver = FrameDriver::new();

  assert!(driver.begin(&platform, false).is_none());

  platform.request_frame();
  assert!(driver.begin(&platform, false).is_some());
  // The request is one-shot: consumed by the begin above.
  assert!(driver.begin(&platform, false).is_none());
}

#[test]
fn extra_demand_passes_the_gate_and_still_consumes_the_latch() {
  let platform = PlatformContext::new(Vec::new());
  let mut driver = FrameDriver::new();

  assert!(driver.begin(&platform, true).is_some());

  // A pending request is drained even when extra demand carried the frame:
  // that frame is about to draw, so the demand is spent either way.
  platform.request_frame();
  assert!(driver.begin(&platform, true).is_some());
  assert!(driver.begin(&platform, false).is_none());
}

#[test]
fn always_render_never_gates() {
  let platform = PlatformContext::new(Vec::new());
  platform.set_always_render(true);
  let mut driver = FrameDriver::new();

  assert!(driver.begin(&platform, false).is_some());
  assert!(driver.begin(&platform, false).is_some());
}
