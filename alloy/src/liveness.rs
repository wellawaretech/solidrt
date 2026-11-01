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
// Held redraws: a platform that will not show the window until the surface
// has a frame (Android's SurfaceView, see redraw.rs) asks with a
// generation, and the answer is the first swap of a frame built after the
// ask. A frame already in flight was built before it (the old size, or the
// surface before a recreate), so it does not count: the request waits for
// the next frame signal, and frames from that signal's number on answer.
// Every signal re-latches until then, since one repaint can be consumed by
// a frame that does not count, and the platform waits on the answer.
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
  redraw: Redraw,
}

// A held redraw's progress (see the struct comment).
#[derive(Clone, Copy)]
enum Redraw {
  None,
  // Asked; no frame signal emitted since.
  Requested { generation: i32 },
  // A frame numbered `from` or later answers it.
  Awaiting { generation: i32, from: u64 },
}

impl SurfaceLiveness {
  pub fn new() -> Self {
    SurfaceLiveness { latch: None, settle_until: None, resize_rebound: false, redraw: Redraw::None }
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

  /// Feed one frame-signal emission (FrameRendered or Tick); `next_frame` is
  /// the number the frame built from it gets. Inside the settle window this
  /// re-latches the frame request; past it the window closes. A held redraw
  /// re-latches until answered.
  pub fn on_frame_signal(&mut self, now: Instant, next_frame: u64) {
    if let Some(deadline) = self.settle_until {
      if now < deadline {
        self.request_frame();
      } else {
        self.settle_until = None;
      }
    }
    match self.redraw {
      Redraw::None => {}
      Redraw::Requested { generation } => {
        self.redraw = Redraw::Awaiting { generation, from: next_frame };
        self.request_frame();
      }
      Redraw::Awaiting { .. } => self.request_frame(),
    }
  }

  /// The platform holds the window until the surface has a frame; a newer
  /// request supersedes an unanswered one.
  pub fn on_redraw_requested(&mut self, generation: i32) {
    self.redraw = Redraw::Requested { generation };
    self.request_frame();
  }

  /// Feed one present notification: the generation of the held redraw this
  /// swap answers, if any.
  pub fn on_present(&mut self, frame: u64, swapped: bool) -> Option<i32> {
    match self.redraw {
      Redraw::Awaiting { generation, from } if swapped && frame >= from => {
        self.redraw = Redraw::None;
        Some(generation)
      }
      _ => None,
    }
  }

  fn request_frame(&self) {
    if let Some(latch) = &self.latch {
      latch.store(true, Ordering::Relaxed);
    }
  }
}
