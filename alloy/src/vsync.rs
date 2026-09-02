// Vsync source for frame pacing: a platform backend that arms one display
// vsync callback per request() and answers each with one signal, so the main
// loop can defer frame signals to the display's clock. One-shot re-arm keeps
// the demand-driven contract: no request, no armed callback, no wakeups while
// idle. start() returns None on platforms without a backend; the main loop
// then keeps present-return pacing.
//
// Backends: Android (AChoreographer, below). iOS would slot in here via
// SDL_SetiOSAnimationCallback (CADisplayLink) when iOS support lands. This
// module is the only ndk / ndk-sys consumer in the tree; if SDL ships its own
// choreographer API (libsdl-org/SDL#15013, milestone 3.8.0), reimplement the
// Android backend on that and drop both deps - callers only speak
// request()/try_take().

use std::cell::Cell;
use std::sync::mpsc;
use std::time::Duration;

/// Frame-release policy for the main loop (AlloyCommand::SetFramePacing).
/// VsyncLocked defers each present's frame signal to the display vsync:
/// production phase-locks to the clock the platform batches input on, and a
/// built frame waits in the buffer queue as briefly as possible - best
/// input-to-glass latency, but the release chain's jitter periodically beats
/// the queue's slack and drops a latch. SwapPaced emits the frame signal at
/// present-return: the queue fills and the blocking swap paces production -
/// metronomic presentation at ~1-2 frames more input latency. Policy is
/// chosen above alloy (from the input-modality facts); on platforms without
/// a vsync backend both behave the same.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FramePacing {
  VsyncLocked,
  SwapPaced,
}

pub struct VsyncSource {
  req_tx: mpsc::Sender<(u64, Duration)>,
  signal_rx: mpsc::Receiver<u64>,
  // Generation of the latest request. Each signal carries the generation of
  // the request it answers; try_take discards signals from superseded
  // requests, so a late signal (its present already released by the caller's
  // fallback) can never release a future present early.
  generation: Cell<u64>,
}

impl VsyncSource {
  /// Start the platform vsync backend, or None if this platform has none.
  /// `wake` runs on the backend thread after each signal is queued, so a main
  /// loop blocked on the SDL event queue notices (same pattern as the raster
  /// thread's post-present wake).
  #[allow(unused_variables)]
  pub fn start(wake: impl Fn() + Send + 'static) -> Option<VsyncSource> {
    #[cfg(target_os = "android")]
    {
      let (req_tx, req_rx) = mpsc::channel::<(u64, Duration)>();
      let (signal_tx, signal_rx) = mpsc::channel::<u64>();
      std::thread::Builder::new()
        .name("srt-vsync".into())
        .spawn(move || android::run(req_rx, signal_tx, wake))
        .expect("Failed to spawn vsync thread");
      Some(VsyncSource { req_tx, signal_rx, generation: Cell::new(0) })
    }
    #[cfg(not(target_os = "android"))]
    None
  }

  /// Arm one frame callback; the matching signal arrives via `try_take` after
  /// `wake`, `delay` after the vsync fires. The delay exists because the
  /// platform delivers the vsync's input batch on its own thread in parallel
  /// with the callback: a signal emitted at the vsync itself races that input
  /// and loses often (measured: 37-44 frames drawn per 60 moves), wasting the
  /// frame on not-yet-dirty state. Requests sent while one is being served
  /// coalesce into it. Each request supersedes the previous one: an
  /// unanswered earlier request's signal will be discarded by `try_take`.
  pub fn request(&self, delay: Duration) {
    self.generation.set(self.generation.get() + 1);
    self.req_tx.send((self.generation.get(), delay)).ok();
  }

  /// Drain queued vsync signals; true if the latest request's own signal was
  /// among them. Signals from superseded requests are discarded silently -
  /// their newer request is still outstanding, so a false return leaves the
  /// armed state untouched at the caller.
  pub fn try_take(&self) -> bool {
    let mut took = false;
    while let Ok(g) = self.signal_rx.try_recv() {
      if g == self.generation.get() {
        took = true;
      }
    }
    took
  }
}

/// Rolling estimate of the frame pipeline cost - frame-signal emission to
/// present-return, the full JS build + raster + present chain - driving how
/// late after vsync the frame signal can fire. Later is better (the frame
/// consumes fresher input and the buffer waits less for the compositor
/// latch), bounded by the pipeline still having to finish before the next
/// latch.
pub struct PacingBudget {
  samples: [f32; Self::WINDOW],
  idx: usize,
  filled: usize,
  // Last armed delay; delay() slews toward its target from here.
  delay_ms: Option<f32>,
}

impl PacingBudget {
  const WINDOW: usize = 32;
  // Headroom added on top of the observed worst pipeline cost.
  const MARGIN_MS: f32 = 2.0;
  // Max movement of the armed delay per request (one per frame). The target
  // steps whenever the worst-of-WINDOW picks up or retires an outlier; each
  // step shifts the release phase of the whole production chain, and the
  // buffer queue downstream has to absorb the swing. Slewing turns the step
  // into a drift the queue absorbs frame by frame.
  const SLEW_MS: f32 = 0.5;

  pub fn new() -> PacingBudget {
    PacingBudget { samples: [0.0; Self::WINDOW], idx: 0, filled: 0, delay_ms: None }
  }

  /// Record one emission-to-present duration. Samples beyond 1.5 periods are
  /// slipped frames (the chain waited out an extra vsync), not steady-state
  /// cost; folding them in would drag the start earlier for a whole window.
  pub fn record(&mut self, ms: f32, period: std::time::Duration) {
    if ms > period.as_secs_f32() * 1500.0 {
      return;
    }
    self.samples[self.idx] = ms;
    self.idx = (self.idx + 1) % Self::WINDOW;
    self.filled = (self.filled + 1).min(Self::WINDOW);
  }

  /// Signal delay after vsync: as late as the estimated budget allows, but
  /// never before the input-arrival floor and never past a margin before the
  /// next vsync. The floor exists because the platform needs several ms to
  /// route the vsync's input batch into the SDL queue, and a signal ahead of
  /// its input wastes the frame. 8ms, scaled down to 60% of the period for
  /// high-refresh displays. Measured on-device: 10ms bought nothing over 8 -
  /// the ~5 skipped frames/s that remain are the platform pairing deliveries
  /// across vsync boundaries (input-resampling territory, not a delay
  /// problem) - while eating build margin. An empty window (chain start)
  /// uses the floor. Call once per armed request: the returned delay moves at
  /// most SLEW_MS from the previous call's (see SLEW_MS).
  pub fn delay(&mut self, period: std::time::Duration) -> std::time::Duration {
    let period_ms = period.as_secs_f32() * 1000.0;
    let floor = (period_ms * 0.6).min(8.0);
    let budget = match self.filled {
      0 => period_ms / 2.0,
      n => {
        let worst = self.samples[..n].iter().fold(0.0_f32, |a, &b| a.max(b));
        worst + Self::MARGIN_MS
      }
    };
    let target = (period_ms - budget).clamp(floor, period_ms - Self::MARGIN_MS);
    let delay_ms = match self.delay_ms {
      None => target,
      Some(cur) => cur + (target - cur).clamp(-Self::SLEW_MS, Self::SLEW_MS),
    };
    self.delay_ms = Some(delay_ms);
    std::time::Duration::from_secs_f32(delay_ms / 1000.0)
  }

  /// Last armed delay in ms, for diagnostics; does not advance the slew.
  pub fn current_ms(&self) -> f32 {
    self.delay_ms.unwrap_or(0.0)
  }
}

// Headroom on the vsync-signal deadline beyond its latest legitimate
// arrival (request + period + delay): sleep overshoot on the vsync thread
// plus channel/wake latency into the main loop.
const VSYNC_SLACK: std::time::Duration = std::time::Duration::from_millis(4);

/// What `FrameRelease::on_present` asks of the caller.
pub(crate) enum Release {
  /// Emit the frame signal now (no vsync backend, or SwapPaced): the
  /// blocking swap paces production.
  Emit,
  /// The present's frame signal waits for the vsync signal; when `arm` is
  /// Some the caller must arm one VsyncSource request with that delay (the
  /// chain start out of idle - normally the release below pre-armed it).
  Deferred { arm: Option<std::time::Duration> },
}

/// What `FrameRelease::on_wake` asks of the caller.
pub(crate) enum Wake {
  /// Nothing to release.
  Idle,
  /// Release the deferred presents: emit `emit` frame signals, then arm a
  /// VsyncSource request with `arm`'s delay when Some (the pre-arm for the
  /// next vsync). `timed_out` = the fallback fired instead of a signal
  /// (diagnostics; the superseded signal will be discarded by try_take).
  Release { emit: u32, timed_out: bool, arm: Option<std::time::Duration> },
}

/// What `FrameRelease::set_pacing` asks of the caller.
pub(crate) enum PacingChange {
  Unchanged,
  /// The policy changed (log it). `released` deferred presents must emit
  /// their frame signals now (presents already deferred to a vsync signal
  /// must not strand when leaving VsyncLocked; the outstanding vsync
  /// request's signal drains harmlessly with nothing pending); 0 when
  /// nothing was deferred, in which case no signal fires and the
  /// frame-signal clock stays untouched.
  Changed { released: u32 },
}

/// The vsync frame-release state machine (see FramePacing): which presents'
/// frame signals defer to the display vsync, when the one outstanding
/// request is armed and with what delay, and when the release fallback fires
/// instead of a lost signal. Pure policy in the liveness.rs mold: the caller
/// performs the effects each decision names (emitting frame signals, arming
/// VsyncSource requests) and feeds the clock, so the invariants unit-test
/// without a display (src/tests/release.rs). Without a vsync backend, and
/// under SwapPaced, every present releases immediately and nothing arms.
pub(crate) struct FrameRelease {
  /// Whether a platform vsync backend exists; without one every decision is
  /// Release::Emit and the rest of the state never engages.
  backend: bool,
  /// Frame-release policy; VsyncLocked until the embedder's policy arrives.
  pacing: FramePacing,
  /// Presents whose frame signal awaits the vsync signal (at most one in
  /// practice: the UI thread builds the next frame only after the emission).
  pending: u32,
  /// Whether a vsync request is outstanding (at most one ever is). Armed at
  /// signal emission for the NEXT vsync - not at present-return, which lands
  /// near the vsync boundary after the full build+draw pipeline and loses
  /// the re-arm race often enough to halve the frame rate (measured
  /// 41-51/60). Disarmed by taking the signal; a signal taken with nothing
  /// pending ends the chain (demand stopped), costing one spare callback.
  armed: bool,
  /// The fallback deadline: the latest instant the armed request's signal
  /// could legitimately arrive (request time + one period to the next
  /// choreographer vsync + the armed delay + slack). Anchoring on the
  /// request keeps the fallback tight - a lost signal costs a ~1.6-period
  /// production gap instead of the 2-3 a present-return anchor allowed -
  /// while never firing before a healthy signal could still arrive. Racing
  /// a merely-late one is harmless: the fallback supersedes it (new request
  /// generation) and the chain re-locks at the next vsync.
  deadline: std::time::Instant,
  /// Pipeline cost estimator for the signal delay; samples open at each
  /// vsync-released emission and close at the matching present.
  budget: PacingBudget,
  /// The open sample's emission instant. Tick-triggered presents (first
  /// frame out of idle) have no open mark and are not sampled.
  signal_emitted: Option<std::time::Instant>,
}

impl FrameRelease {
  pub fn new(backend: bool, now: std::time::Instant) -> Self {
    FrameRelease {
      backend,
      pacing: FramePacing::VsyncLocked,
      pending: 0,
      armed: false,
      deadline: now,
      budget: PacingBudget::new(),
      signal_emitted: None,
    }
  }

  /// Feed one present-return; `period` is the current refresh period.
  pub fn on_present(&mut self, now: std::time::Instant, period: std::time::Duration) -> Release {
    if !self.backend || self.pacing != FramePacing::VsyncLocked {
      return Release::Emit;
    }
    if let Some(emitted) = self.signal_emitted.take() {
      self.budget.record(now.duration_since(emitted).as_secs_f32() * 1000.0, period);
    }
    self.pending += 1;
    // Normally the signal releasing this present is already armed
    // (pre-armed when the previous one was emitted); this request only
    // starts the chain on the first present out of idle.
    Release::Deferred { arm: self.arm(now, period) }
  }

  /// Feed one loop wake with the VsyncSource drain's result. One signal
  /// releases all pending; a signal past the fallback deadline is replaced
  /// by the fallback, which also disarms so the release pre-arms a fresh
  /// request superseding the late one.
  pub fn on_wake(&mut self, now: std::time::Instant, period: std::time::Duration, signal_taken: bool) -> Wake {
    if signal_taken {
      self.armed = false;
    }
    if self.pending == 0 {
      return Wake::Idle;
    }
    let timed_out = !signal_taken && now >= self.deadline;
    if timed_out {
      self.armed = false;
    }
    if !signal_taken && !timed_out {
      return Wake::Idle;
    }
    let emit = self.pending;
    self.pending = 0;
    self.signal_emitted = Some(now);
    // Pre-arm the signal for the next vsync while this frame is being
    // built: the signal timing must not depend on when the build's present
    // returns (see `armed`). The frame this emission triggers has until
    // that signal - a full period plus the delay - to present, or it slips
    // a frame.
    Wake::Release { emit, timed_out, arm: self.arm(now, period) }
  }

  /// Feed a frame-pacing policy write. Leaving VsyncLocked releases the
  /// deferred presents and resets the vsync-side state: the open budget
  /// sample dies (its present will return under different pacing, so the
  /// duration would be a bogus pipeline cost), and the chain disarms - the
  /// outstanding request's signal still drains harmlessly, but a return to
  /// VsyncLocked then arms a fresh request with a fresh deadline instead of
  /// trusting the stale one.
  pub fn set_pacing(&mut self, p: FramePacing) -> PacingChange {
    if self.pacing == p {
      return PacingChange::Unchanged;
    }
    self.pacing = p;
    if p == FramePacing::SwapPaced {
      let released = self.pending;
      self.pending = 0;
      self.armed = false;
      self.signal_emitted = None;
      PacingChange::Changed { released }
    } else {
      PacingChange::Changed { released: 0 }
    }
  }

  /// The fallback deadline to wake at; None when no present is deferred
  /// (the caller sleeps toward its idle-tick deadline instead).
  pub fn wait_deadline(&self) -> Option<std::time::Instant> {
    (self.pending > 0).then_some(self.deadline)
  }

  /// Whether no present is deferred: the idle-tick gate (while one is, the
  /// real frame signal is at most a refresh period away, fallback included).
  pub fn idle(&self) -> bool {
    self.pending == 0
  }

  /// Last armed signal delay in ms, for the 1/s diagnostics line.
  pub fn current_delay_ms(&self) -> f32 {
    self.budget.current_ms()
  }

  // Arm a request if none is outstanding: pick the delay, set the fallback
  // deadline, and hand the delay to the caller for VsyncSource::request.
  fn arm(&mut self, now: std::time::Instant, period: std::time::Duration) -> Option<std::time::Duration> {
    if self.armed {
      return None;
    }
    let delay = self.budget.delay(period);
    self.deadline = now + period + delay + VSYNC_SLACK;
    self.armed = true;
    Some(delay)
  }
}

#[cfg(target_os = "android")]
mod android {
  use std::cell::Cell;
  use std::sync::mpsc;

  pub fn run(req_rx: mpsc::Receiver<(u64, std::time::Duration)>, signal_tx: mpsc::Sender<u64>, wake: impl Fn()) {
    // The choreographer instance is per-thread and requires that thread to
    // own a looper; callbacks are dispatched from this thread's poll calls.
    let looper = ndk::looper::ThreadLooper::prepare();
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
      log::warn!("[alloy] no choreographer on this device; vsync pacing disabled");
    }
    let fired = Cell::new(false);
    while let Ok((mut generation, mut delay)) = req_rx.recv() {
      // A burst of requests collapses into one callback answering the latest
      // generation: the consumer flushes everything pending on each signal.
      while let Ok((g, d)) = req_rx.try_recv() {
        generation = g;
        delay = d;
      }
      if !choreographer.is_null() {
        fired.set(false);
        unsafe {
          ndk_sys::AChoreographer_postFrameCallback(
            choreographer,
            Some(frame_callback),
            &fired as *const Cell<bool> as *mut core::ffi::c_void,
          );
        }
        while !fired.get() {
          if looper.poll_once().is_err() {
            // A broken looper cannot dispatch the callback; answer now
            // instead of stalling frame production (= present-return pacing).
            log::warn!("[alloy] vsync looper poll failed; answering without vsync");
            break;
          }
        }
        if fired.get() && !delay.is_zero() {
          std::thread::sleep(delay);
        }
      }
      if signal_tx.send(generation).is_err() {
        return;
      }
      wake();
    }
  }

  // Runs on the vsync thread, inside looper.poll_once(). The frame timestamp
  // is deliberately unused for now: the frame signal is emitted (and timed)
  // sub-millisecond later on the main loop, and the paced clock models time
  // as frame counts, not wall-clock samples.
  unsafe extern "C" fn frame_callback(_frame_time_nanos: core::ffi::c_long, data: *mut core::ffi::c_void) {
    (*(data as *const Cell<bool>)).set(true);
  }
}
