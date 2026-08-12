use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::event::AlloyEvent;

// The resize settle window: after a resize, re-latch a frame request on
// every frame signal until the deadline. A single repaint can race the
// platform's surface reconfiguration (device-observed on Android rotation:
// the frame draws for or into geometry that changes again by present time,
// reaching the screen wrong or not at all) and the demand gate would never
// retry on its own. Retries are present-only while nothing else changes
// (cached display list), and each one re-checks the surface geometry, so the
// last one after the platform settles is correct. Rapid resize streams just
// keep pushing the deadline out.
const RESIZE_SETTLE: Duration = Duration::from_millis(500);

// Surface-liveness policy for the platform loop: keep the window's content
// present and correct across the platform's surface lifecycle, without
// embedder glue.
//
// Repaint on expose, resize, and return to visibility: the demand gate
// otherwise leaves the recreated/undefined surface unpresented forever
// (Android destroys the EGL surface on background; an idle app never
// repaints on its own). The latch is cheap and the cached display list makes
// the frame present-only, so over-triggering is harmless.
//
// All three rebind before the repaint: the platform may have replaced the
// native surface behind the event (Android recreates the EGL surface on
// rotation as well as on resume), and a present into the stale binding can
// succeed silently without reaching the screen, so waiting for a present
// failure is not enough. The raster command channel is ordered and the
// repaint's frame is only built after the event is dispatched, so the raster
// thread picks up the current surface before that frame arrives.
//
// Pure state machine: the caller performs the rebind it reports and feeds it
// the clock, so the settle logic is unit-testable.
pub(crate) struct SurfaceLiveness {
  // The demand gate's frame-request latch (rendertree
  // Platform::frame_request_handle), registered by the embedder via
  // AlloyCommand::SetFrameRequestLatch. Until then only rebinds happen and
  // the resume/expose repaint degrades to the raster thread's
  // present-failure fallback.
  latch: Option<Arc<AtomicBool>>,
  settle_until: Option<Instant>,
  // A rapid resize stream (live drag-resize, rotation transitions) can put
  // several Resize events in one pump iteration; one rebind covers them all.
  resize_rebound: bool,
}

impl SurfaceLiveness {
  pub fn new() -> Self {
    SurfaceLiveness { latch: None, settle_until: None, resize_rebound: false }
  }

  pub fn set_latch(&mut self, latch: Arc<AtomicBool>) {
    self.latch = Some(latch);
  }

  /// Start a pump iteration: re-arms the per-iteration resize rebind.
  pub fn begin_pump(&mut self) {
    self.resize_rebound = false;
  }

  /// Feed one outgoing event; returns whether the caller must rebind the
  /// window surface (ordered ahead of any repaint this event triggers).
  pub fn on_event(&mut self, event: &AlloyEvent, now: Instant) -> bool {
    match event {
      AlloyEvent::Exposed | AlloyEvent::Visibility { visible: true } => {
        self.request_frame();
        true
      }
      AlloyEvent::Resize { .. } => {
        self.request_frame();
        self.settle_until = Some(now + RESIZE_SETTLE);
        if self.resize_rebound {
          false
        } else {
          self.resize_rebound = true;
          true
        }
      }
      _ => false,
    }
  }

  /// Feed one frame-signal emission (FrameRendered or Tick): inside the
  /// settle window this re-latches the frame request; past it the window
  /// closes.
  pub fn on_frame_signal(&mut self, now: Instant) {
    if let Some(deadline) = self.settle_until {
      if now < deadline {
        self.request_frame();
      } else {
        self.settle_until = None;
      }
    }
  }

  fn request_frame(&self) {
    if let Some(latch) = &self.latch {
      latch.store(true, Ordering::Relaxed);
    }
  }
}
