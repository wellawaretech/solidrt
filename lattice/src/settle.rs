// Waiting for the app to come to rest (okf/done/test-harness.md, D11): the
// one condition behind `app.settle()` in a test and the control API's
// `settle` query. An app is at rest when
//
// - nothing it started is in flight (flux's holds: a fetch, a file read, a
//   query, the glyph cells a text drew without; what stands, like a server
//   or a timer not yet due, is not waited for) and the job queue is dry,
// - no timer is due (on the frame timeline a due timer fires with the next
//   frame, so it is work waiting that no frame request stands for), no idle
//   callback waits for the idle period a frame brings, and
// - no frame is demanded (the request latch, which a tree write, a running
//   transition and an `onFrame` callback all set), and
// - the raster thread has run every command sent to it: a submitted frame
//   is work in flight until it is drawn and its GPU work has completed,
//   which is when the frame retires (`retiredFrames`) and what a test
//   reads next (a texture, the resource pool's hand-outs) sees.
//
// The loop waits for the first and the last and runs a frame for the other
// two, until all four hold at once: settling work can start more, and a
// frame can start work. It owns no frame source: a stepped host passes the step that asks
// for one, a client on a display passes nothing and the display's frames
// come by themselves.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use flux::rquickjs::{Ctx, JsLifetime};
use tokio::sync::oneshot;

/// The time a settle gives up after, unless its caller says otherwise: far
/// past any transition an app runs, short enough that a runaway frame
/// callback fails promptly.
pub(crate) const DEFAULT_MAX_MS: f64 = 5000.0;

/// How often the raster queue is re-read while it is the one thing left
/// to drain: the raster thread signals no waiter on this side, and a
/// headless frame runs in a few milliseconds.
const RASTER_POLL: Duration = Duration::from_millis(1);

/// The longest a settle on an app-time cap waits for the raster thread to
/// drain, on the wall: app time does not pass while nothing runs a frame,
/// so this bounds a raster thread that never answers (a lost GPU).
const RASTER_DRAIN_MAX: Duration = Duration::from_secs(5);

/// What bounds a settle.
pub(crate) enum Cap<'a> {
  /// App time, read through `now_ms`: the frames a stepped host runs. Work
  /// in flight passes no app time, so it is bounded by the host's own cap
  /// on the test.
  AppTime { max_ms: f64, now_ms: &'a dyn Fn() -> f64 },
  /// Wall time, on a client whose frames the display paces.
  Wall(Duration),
}

/// What kept an app from coming to rest within the cap.
pub(crate) struct Unsettled {
  pub in_flight: Vec<(&'static str, u32)>,
  pub demand: Vec<String>,
  pub timer_due: bool,
  pub idle_due: bool,
  /// Raster commands sent and not yet run (a frame being drawn, an
  /// upload); 0 where no GUI is installed.
  pub raster_queue: usize,
}

impl Unsettled {
  pub(crate) fn read(ctx: &Ctx<'_>) -> Self {
    Self {
      in_flight: flux::in_flight(ctx),
      demand: flux::gui::frame::demand(ctx),
      timer_due: flux::timer_due(ctx),
      idle_due: flux::idle_due(ctx),
      raster_queue: flux::gui::alloy_context(ctx).map_or(0, |atx| atx.raster_counters().queue),
    }
  }

  fn at_rest(&self) -> bool {
    self.in_flight.is_empty() && self.demand.is_empty() && !self.timer_due && !self.idle_due && self.raster_queue == 0
  }

  /// Only the raster thread is still busy: nothing to run a frame for,
  /// just a drain to wait out.
  fn raster_only(&self) -> bool {
    self.in_flight.is_empty() && self.demand.is_empty() && !self.timer_due && !self.idle_due && self.raster_queue > 0
  }

  /// What is left, as a sentence part: "frames are still demanded by
  /// onFrame; still in flight: 1 fetch".
  pub(crate) fn describe(&self) -> String {
    let mut parts = Vec::new();
    if !self.demand.is_empty() {
      parts.push(format!("frames are still demanded by {}", self.demand.join(", ")));
    }
    if self.timer_due {
      parts.push("a timer is due at every frame (one that re-arms with no delay)".to_string());
    }
    if self.idle_due {
      parts.push("an idle callback waits at every frame (one that re-registers itself)".to_string());
    }
    if !self.in_flight.is_empty() {
      parts.push(format!("still in flight: {}", flux::describe_in_flight(&self.in_flight)));
    }
    if self.raster_queue > 0 {
      parts.push(format!("the raster thread still has {} command(s) to run", self.raster_queue));
    }
    parts.join("; ")
  }
}

/// Who waits for the next frame to have run, in this engine.
#[derive(Clone, Default, JsLifetime)]
struct FrameWaiters(#[qjs(skip_trace)] Rc<RefCell<Vec<oneshot::Sender<()>>>>);

/// A frame was delivered to the engine (the frame verb, after the frame's
/// JS): whoever waited on it goes on. No-op when nobody did.
pub(crate) fn frame_ran(ctx: &Ctx<'_>) {
  let Some(waiters) = ctx.userdata::<FrameWaiters>() else {
    return;
  };
  let waiting = std::mem::take(&mut *waiters.0.borrow_mut());
  for waiter in waiting {
    let _ = waiter.send(());
  }
}

/// Resolves once the next frame has run in this engine (`frame_ran`).
pub(crate) fn next_frame(ctx: &Ctx<'_>) -> oneshot::Receiver<()> {
  if ctx.userdata::<FrameWaiters>().is_none() {
    // Already stored means another settle got there first; either is fine.
    let _ = ctx.store_userdata(FrameWaiters::default());
  }
  let (tx, rx) = oneshot::channel();
  ctx.userdata::<FrameWaiters>().expect("frame waiters installed").0.borrow_mut().push(tx);
  rx
}

/// Wait until the app of `ctx` is at rest (see the top of this file), or
/// until `cap` has passed: then what is left is the error. `step` asks for
/// the next frame on a host that steps; a host whose frames come by
/// themselves passes None. Runs in a task of the engine (`ctx.spawn`).
pub(crate) async fn settle(ctx: &Ctx<'_>, cap: Cap<'_>, step: Option<&dyn Fn()>) -> Result<(), Unsettled> {
  let started = Instant::now();
  let app_start = match &cap {
    Cap::AppTime { now_ms, .. } => now_ms(),
    Cap::Wall(_) => 0.0,
  };
  // What is left of a wall cap; None on an app-time cap, which no wait on
  // the wall counts against.
  let wall_left = |cap: &Cap<'_>| match cap {
    Cap::Wall(max) => Some(max.saturating_sub(started.elapsed())),
    Cap::AppTime { .. } => None,
  };
  loop {
    let landed = flux::settled(ctx);
    match wall_left(&cap) {
      Some(left) => {
        if tokio::time::timeout(left, landed).await.is_err() {
          return Err(Unsettled::read(ctx));
        }
      }
      None => landed.await,
    }
    let mut state = Unsettled::read(ctx);
    // Only the GPU side left: wait it out rather than spend frames on it.
    let drain_started = Instant::now();
    while state.raster_only() {
      let left = match wall_left(&cap) {
        Some(left) => left.min(RASTER_DRAIN_MAX.saturating_sub(drain_started.elapsed())),
        None => RASTER_DRAIN_MAX.saturating_sub(drain_started.elapsed()),
      };
      if left.is_zero() {
        return Err(state);
      }
      tokio::time::sleep(RASTER_POLL.min(left)).await;
      state = Unsettled::read(ctx);
    }
    if state.at_rest() {
      return Ok(());
    }
    let passed = match &cap {
      Cap::AppTime { max_ms, now_ms } => now_ms() - app_start >= *max_ms,
      Cap::Wall(max) => started.elapsed() >= *max,
    };
    if passed {
      return Err(state);
    }
    // A frame: it fires the due timers, runs the frame callbacks and the
    // flush, and draws what was demanded. Registered ahead of the step, so
    // a frame that runs at once is not missed.
    let ran = next_frame(ctx);
    if let Some(step) = step {
      step();
    }
    match wall_left(&cap) {
      Some(left) => {
        if tokio::time::timeout(left, ran).await.is_err() {
          return Err(Unsettled::read(ctx));
        }
      }
      None => {
        let _ = ran.await;
      }
    }
  }
}
