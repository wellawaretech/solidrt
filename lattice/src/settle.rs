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
//   reads next (a texture, the resource pool's hand-outs) sees, and
// - the sort thread has no instance-order re-sort in hand or waiting to
//   land: a finished sort latches a frame request, and the frame lands its
//   ids and draws them.
//
// The loop waits for the first and the last two and runs a frame for the
// other two, until all five hold at once: settling work can start more,
// and a frame can start work. The two threads are waited out on the wall,
// never with frames: a frame passes app time (the cap) and costs the
// threads the cores they sort and draw on. It owns no frame source: a
// stepped host passes the step that asks for one, a client on a display
// passes nothing and the display's frames come by themselves.
//
// The same loop serves two other ends (`Until`), for an app that never
// rests - a frame loop, a looping clip, a spinner - behind which something
// loads: its work done (nothing in flight, and a frame after the last of
// it in which nothing more started, so what the landing woke has run), and
// a condition of the caller's holding (Testing Library's waitFor, Unity's
// WaitUntil, Godot's wait_until). Both wait for work with no app time
// passing and step frames between, as the rest wait does; only the exit
// differs. Rest stays the default: it needs no condition and fails loudly
// on an app that never stops, which is a finding.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use flux::rquickjs::{Ctx, JsLifetime};
use tokio::sync::oneshot;

/// The time a settle gives up after, unless its caller says otherwise: far
/// past any transition an app runs, short enough that a runaway frame
/// callback fails promptly.
pub(crate) const DEFAULT_MAX_MS: f64 = 5000.0;

/// How often the raster and sort queues are re-read while they are all
/// that is left to drain: neither thread signals a waiter on this side,
/// and a headless frame or a re-sort runs in a few milliseconds.
const DRAIN_POLL: Duration = Duration::from_millis(1);

/// The longest a settle on an app-time cap waits for the raster and sort
/// threads to drain, on the wall: app time does not pass while nothing
/// runs a frame, so this bounds a thread that never answers (a lost GPU).
const DRAIN_MAX: Duration = Duration::from_secs(5);

/// What ends a settle (the top of this file).
pub(crate) enum Until<'a> {
  /// The app at rest: all four conditions.
  Rest,
  /// Its work done, whatever animates: nothing in flight, the raster thread
  /// drained, and one frame run after the last landing in which nothing
  /// more started. An app at rest is done at once.
  Work,
  /// The caller's condition holds, read after every landing and every
  /// frame. Err: the read itself failed (a predicate that threw); the
  /// caller keeps what was thrown.
  Holds(&'a dyn Fn() -> Result<bool, ()>),
}

/// Why a settle ended without its `Until`.
pub(crate) enum Stopped {
  /// The cap passed; what was left.
  Cap(Unsettled),
  /// The `Holds` read failed.
  Read,
}

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
  /// Instance-order re-sorts submitted and not yet landed in a frame; 0
  /// where no GUI is installed.
  pub sort_queue: usize,
  /// Of those, the re-sorts the sort thread is still working on.
  pub sort_running: usize,
}

impl Unsettled {
  pub(crate) fn read(ctx: &Ctx<'_>) -> Self {
    Self {
      in_flight: flux::in_flight(ctx),
      demand: flux::gui::frame::demand(ctx),
      timer_due: flux::timer_due(ctx),
      idle_due: flux::idle_due(ctx),
      raster_queue: flux::gui::alloy_context(ctx).map_or(0, |atx| atx.raster_counters().queue),
      sort_queue: flux::gui::alloy_context(ctx).map_or(0, |atx| atx.order_counters().pending),
      sort_running: flux::gui::alloy_context(ctx).map_or(0, |atx| atx.order_counters().running),
    }
  }

  /// Nothing for a frame to do: a finished re-sort is not quiet, since it
  /// latched the frame that lands it.
  fn quiet(&self) -> bool {
    self.in_flight.is_empty() && self.demand.is_empty() && !self.timer_due && !self.idle_due
  }

  fn at_rest(&self) -> bool {
    self.quiet() && self.threads_drained()
  }

  /// The work is done, whatever animates: nothing in flight, the raster
  /// and sort threads drained.
  fn work_done(&self) -> bool {
    self.in_flight.is_empty() && self.threads_drained()
  }

  fn threads_drained(&self) -> bool {
    self.raster_queue == 0 && self.sort_queue == 0
  }

  /// A thread is still at work: the raster on a command, the sort on a
  /// re-sort (one finished and not landed is a frame's job, not a wait).
  fn threads_busy(&self) -> bool {
    self.raster_queue > 0 || self.sort_running > 0
  }

  /// Only the threads are still busy: nothing to run a frame for, just a
  /// drain to wait out.
  fn threads_only(&self) -> bool {
    self.quiet() && self.threads_busy()
  }

  /// The threads are still busy with the last frame while no work is in
  /// flight: what a work wait and a condition wait drain before they
  /// read, whatever frames the app still demands - the frame is not over
  /// until it is drawn, and a read of what it drew comes after.
  fn threads_busy_idle(&self) -> bool {
    self.in_flight.is_empty() && self.threads_busy()
  }

  fn describe_sort(&self) -> Option<String> {
    if self.sort_running > 0 {
      Some(format!("{} instance order re-sort(s) still run on the sort thread", self.sort_running))
    } else if self.sort_queue > 0 {
      Some(format!("{} finished instance order re-sort(s) wait for a frame to land them", self.sort_queue))
    } else {
      None
    }
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
    parts.extend(self.describe_sort());
    parts.join("; ")
  }

  /// What is left of the work alone (`Until::Work`): the frames an app
  /// demands are not what that wait is for, so they are not what failed
  /// it.
  pub(crate) fn describe_work(&self) -> String {
    let mut parts = Vec::new();
    if !self.in_flight.is_empty() {
      parts.push(format!("still in flight: {}", flux::describe_in_flight(&self.in_flight)));
    }
    if self.raster_queue > 0 {
      parts.push(format!("the raster thread still has {} command(s) to run", self.raster_queue));
    }
    parts.extend(self.describe_sort());
    if parts.is_empty() {
      parts.push("work kept starting, a frame after the other".to_string());
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

/// Wait until `until` holds for the app of `ctx` (see the top of this
/// file), or until `cap` has passed: then what is left is the error.
/// `step` asks for the next frame on a host that steps; a host whose
/// frames come by themselves passes None. Runs in a task of the engine
/// (`ctx.spawn`).
pub(crate) async fn settle(
  ctx: &Ctx<'_>,
  until: Until<'_>,
  cap: Cap<'_>,
  step: Option<&dyn Fn()>,
) -> Result<(), Stopped> {
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
  // Frames run since work last landed (`Until::Work`): the frame after a
  // landing runs what the landing woke, and one in which nothing started
  // ends the wait.
  let mut idle_frames = 0u32;
  loop {
    // Read before the landing: this task is polled with the job queue dry,
    // so what the last frame started is in flight by now.
    if !flux::in_flight(ctx).is_empty() {
      idle_frames = 0;
    }
    let landed = flux::settled(ctx);
    match wall_left(&cap) {
      Some(left) => {
        if tokio::time::timeout(left, landed).await.is_err() {
          return Err(Stopped::Cap(Unsettled::read(ctx)));
        }
      }
      None => landed.await,
    }
    let mut state = Unsettled::read(ctx);
    // Only the threads left (or, past rest, the frame still drawing, the
    // re-sort still sorting): wait them out rather than spend frames on
    // them.
    let draining = |state: &Unsettled| match until {
      Until::Rest => state.threads_only(),
      Until::Work | Until::Holds(_) => state.threads_busy_idle(),
    };
    let drain_started = Instant::now();
    while draining(&state) {
      let left = match wall_left(&cap) {
        Some(left) => left.min(DRAIN_MAX.saturating_sub(drain_started.elapsed())),
        None => DRAIN_MAX.saturating_sub(drain_started.elapsed()),
      };
      if left.is_zero() {
        return Err(Stopped::Cap(state));
      }
      tokio::time::sleep(DRAIN_POLL.min(left)).await;
      state = Unsettled::read(ctx);
    }
    let done = match &until {
      Until::Rest => state.at_rest(),
      Until::Work => state.at_rest() || (idle_frames >= 1 && state.work_done()),
      Until::Holds(holds) => holds().map_err(|()| Stopped::Read)?,
    };
    if done {
      return Ok(());
    }
    let passed = match &cap {
      Cap::AppTime { max_ms, now_ms } => now_ms() - app_start >= *max_ms,
      Cap::Wall(max) => started.elapsed() >= *max,
    };
    if passed {
      return Err(Stopped::Cap(state));
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
          return Err(Stopped::Cap(Unsettled::read(ctx)));
        }
      }
      None => {
        let _ = ran.await;
      }
    }
    idle_frames += 1;
  }
}
