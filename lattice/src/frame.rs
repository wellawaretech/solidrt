use std::cell::Cell;
use std::time::Instant;

/// What the runtime stamps about the frame it is about to hand to JS, read
/// back by draw() on the same (JS) thread. A thread-local rather than an
/// argument because the two sides are joined only by the JS render handler:
/// native stamps before emitting the "render" event, JS calls renderFrame(),
/// draw() reads. Zero timing calls cross into JS.
#[derive(Clone, Copy)]
pub struct RenderFrame {
  /// Instant captured just before the frame is delivered to JS. Read and
  /// cleared at draw() entry, the delta is the frame's JS (timers, rAF
  /// callbacks, the render handler's onFrame + flush); None on a native draw
  /// with no delivery (the paused path).
  pub start: Option<Instant>,
  /// Present index of the frame being computed.
  pub frame: u64,
  /// Refresh period the frame's cost is judged against, ms (the frame
  /// history's slow-frame threshold).
  pub period_ms: f32,
  /// When the frame is expected to reach the screen (alloy's frame signal
  /// says): the deadline its video content is latched against.
  pub present_at: Instant,
}

impl Default for RenderFrame {
  // Before any frame signal (the mount frame): the frame presents as soon
  // as it can, at the virtual time in playback.
  fn default() -> Self {
    RenderFrame { start: None, frame: 0, period_ms: 0.0, present_at: alloy::clock::now() }
  }
}

thread_local! {
  pub static RENDER_FRAME: Cell<RenderFrame> = Cell::new(RenderFrame::default());
}
