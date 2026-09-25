// Vsync source for frame pacing: a platform backend that arms one display
// vsync callback per request() and answers each with one signal, so the main
// loop can defer frame signals to the display's clock. One-shot re-arm keeps
// the demand-driven contract: no request, no armed callback, no wakeups while
// idle. start() returns None on platforms without a backend; the main loop
// then keeps present-return pacing.
//
// Backends: Android (AChoreographer) and macOS (CVDisplayLink), below. iOS
// would slot in here via SDL_SetiOSAnimationCallback (CADisplayLink) when
// iOS support lands. This module is the only ndk / ndk-sys consumer in the
// tree; if SDL ships its own choreographer API (libsdl-org/SDL#15013,
// milestone 3.8.0), reimplement the Android backend on that and drop both
// deps - callers only speak request()/try_take().

use std::cell::Cell;
use std::sync::mpsc;
use std::time::Duration;

/// Whether this platform's window swap paces frame production: with the
/// swap interval at 1 the swap blocks until the display has taken the
/// buffer, once per refresh, so present-return pacing (SwapPaced) runs at
/// the refresh rate. False on macOS: the ANGLE-Metal swap returns at once
/// (measured 2026-09-25 on an M1 at 60 Hz: 1575 presents/s, present 0.0 ms,
/// window in front or display asleep alike), and SDL's own display-link
/// pacing exists only on its native CGL path, which the GLES driver alloy
/// forces (gl::configure_opengl) never takes. There the vsync backend must
/// pace (VsyncLocked); without one, FrameRelease's floor holds a frame per
/// refresh period. Read above alloy where the pacing policy is chosen.
pub const fn swap_paces() -> bool {
  !cfg!(target_os = "macos")
}

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
  // Each signal carries the generation of the request it answers and the
  // instant of the vsync it was woken for (the Choreographer frame time on
  // the main loop's clock; the answer instant where no choreographer is
  // available), which is the reference the refresh count is taken from.
  signal_rx: mpsc::Receiver<(u64, std::time::Instant)>,
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
      let (signal_tx, signal_rx) = mpsc::channel::<(u64, std::time::Instant)>();
      std::thread::Builder::new()
        .name("srt-vsync".into())
        .spawn(move || android::run(req_rx, signal_tx, wake))
        .expect("Failed to spawn vsync thread");
      Some(VsyncSource { req_tx, signal_rx, generation: Cell::new(0) })
    }
    #[cfg(target_os = "macos")]
    {
      let (req_tx, req_rx) = mpsc::channel::<(u64, Duration)>();
      let (signal_tx, signal_rx) = mpsc::channel::<(u64, std::time::Instant)>();
      // The display link is created on the vsync thread; `ready` reports
      // whether it exists, so a machine without one keeps present-return
      // pacing (and its floor) instead of a backend that never signals.
      let (ready_tx, ready_rx) = mpsc::channel::<bool>();
      std::thread::Builder::new()
        .name("srt-vsync".into())
        .spawn(move || macos::run(req_rx, signal_tx, ready_tx, wake))
        .expect("Failed to spawn vsync thread");
      ready_rx.recv().unwrap_or(false).then_some(VsyncSource { req_tx, signal_rx, generation: Cell::new(0) })
    }
    #[cfg(not(any(target_os = "android", target_os = "macos")))]
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

  /// Drain queued vsync signals; Some(vsync instant) if the latest request's
  /// own signal was among them. Signals from superseded requests are
  /// discarded silently - their newer request is still outstanding, so a
  /// None leaves the armed state untouched at the caller.
  pub fn try_take(&self) -> Option<std::time::Instant> {
    let mut took = None;
    while let Ok((g, vsync)) = self.signal_rx.try_recv() {
      if g == self.generation.get() {
        took = Some(vsync);
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

  /// Record one emission-to-present duration. `slot` is the frame's slot:
  /// the refresh period, times the cadence hold when one is in force.
  /// Samples beyond 1.5 slots are slipped frames (the chain waited out an
  /// extra vsync), not steady-state cost; folding them in would drag the
  /// start earlier for a whole window.
  pub fn record(&mut self, ms: f32, slot: std::time::Duration) {
    if ms > slot.as_secs_f32() * 1500.0 {
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

// Under present-return pacing, how early before its slot end a present may
// return and still emit at once, as a divisor of the refresh period (4 = a
// quarter period). A blocking swap's return jitters around the period
// boundary by well under this on the stacks measured (Mesa, ANGLE/D3D11);
// a swap that returned in 0 ms (ANGLE-Metal) lands a full period early and
// is held to the grid.
const SWAP_RETURN_SLACK_DIVISOR: u32 = 4;

/// What `FrameRelease::on_present` asks of the caller.
pub(crate) enum Release {
  /// Emit the frame signal now: no vsync backend or SwapPaced (the blocking
  /// swap paces production), or the present's vsync signal already arrived
  /// and was banked (see `FrameRelease::banked`). `arm` is Some when a
  /// VsyncSource request must be armed with that delay (never without a
  /// backend; normally None here, the banked signal's release pre-armed).
  /// `vsync` is the banked signal's vsync instant when there was one: the
  /// reference this release's refreshes are counted from, on the vsync grid
  /// like every other released signal's, rather than the swap's return,
  /// which sits a variable throttle wait past the vsync.
  Emit { arm: Option<std::time::Duration>, vsync: Option<std::time::Instant> },
  /// The present's frame signal waits for the vsync signal; when `arm` is
  /// Some the caller must arm one VsyncSource request with that delay (the
  /// chain start out of idle - normally the release below pre-armed it).
  Deferred { arm: Option<std::time::Duration> },
}

/// What `FrameRelease::on_wake` asks of the caller.
pub(crate) enum Wake {
  /// Nothing to release.
  Idle,
  /// The vsync signal arrived ahead of the in-flight frame's present: it is
  /// kept for that present, which releases on arrival (see
  /// `FrameRelease::banked`). Arm a VsyncSource request with `arm`'s delay
  /// when Some, so the next vsync is still served on time.
  Banked { arm: Option<std::time::Duration> },
  /// The vsync fell inside the cadence-hold slot (see `FrameRelease::hold`):
  /// the deferred presents keep waiting for the slot's own vsync. Arm a
  /// VsyncSource request with `arm`'s delay when Some.
  Held { arm: Option<std::time::Duration> },
  /// Release the deferred presents: emit `emit` frame signals, then arm a
  /// VsyncSource request with `arm`'s delay when Some (the pre-arm for the
  /// next vsync). `timed_out` = the fallback fired instead of a signal
  /// (diagnostics; the superseded signal will be discarded by try_take).
  /// `reference` is the instant the released signals' refreshes are counted
  /// from: the signal's vsync, the slot end of a held SwapPaced release, or
  /// for a fallback the wake minus the delay the signal would have slept,
  /// which puts it near the vsync it stood in for.
  Release { emit: u32, timed_out: bool, arm: Option<std::time::Duration>, reference: std::time::Instant },
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
  Changed {
    released: u32,
  },
}

/// The vsync frame-release state machine (see FramePacing): which presents'
/// frame signals defer to the display vsync, when the one outstanding
/// request is armed and with what delay, and when the release fallback fires
/// instead of a lost signal. Pure policy in the liveness.rs mold: the caller
/// performs the effects each decision names (emitting frame signals, arming
/// VsyncSource requests) and feeds the clock, so the invariants unit-test
/// without a display (src/tests/release.rs). Without a vsync backend, and
/// under SwapPaced, a present releases at return once its slot has run
/// (the floor of one refresh period per frame, see `on_present`) and
/// nothing arms.
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
  /// generation) and the chain re-locks at the next vsync. Under SwapPaced
  /// it is instead the slot end a held present waits for (see `hold`).
  deadline: std::time::Instant,
  /// The cadence hold in force (cadence.rs): the whole number of refresh
  /// periods each frame interval is held to, 1 for none. A held present's
  /// frame signal waits for the slot end - the previous frame's start plus
  /// `hold` periods - under both policies: VsyncLocked skips the vsync
  /// signals inside the slot (`Wake::Held`), SwapPaced defers to the slot
  /// end as a timer deadline. Frames are held at the signal, not at the
  /// swap, so a held frame starts at its slot and samples input as late as
  /// the slot allows; a frame that outruns its slot releases at once and
  /// starts a new grid there.
  hold: u32,
  /// Start of the current slot: the instant of the last released frame
  /// signal on the grid the hold defines (a vsync instant under
  /// VsyncLocked, the slot end for a deferred SwapPaced release or one
  /// within the slack of it, the present's return otherwise). None until
  /// the first release; never cleared, since a stale slot lies in the past
  /// and holds nothing.
  slot_start: Option<std::time::Instant>,
  /// Pipeline cost estimator for the signal delay; samples open at each
  /// vsync-released emission and close at the matching present.
  budget: PacingBudget,
  /// The open sample's emission instant. Tick-triggered presents (first
  /// frame out of idle) have no open mark and are not sampled. Doubles as
  /// the in-flight mark: Some between a frame signal and the present it
  /// produces, which is the window a banked signal and the idle-tick gate
  /// (`idle`) care about.
  signal_emitted: Option<std::time::Instant>,
  /// A vsync signal taken while the frame it should release was still in
  /// flight - emitted, not yet presented - with the instant of its vsync.
  /// On Android the swap itself blocks until the previous frame's GPU work
  /// retires (libgui throttles EGL production one frame deep, and that
  /// retirement is bound to the compositor releasing the buffer), so the
  /// present-return lands after the next vsync signal about one frame in
  /// six on a 60 Hz panel. Ending the chain there cost a whole period each
  /// time (measured 1,1,1,1,2 present intervals, 51 fps). The banked signal
  /// releases the present the moment it returns instead; a second signal
  /// with nothing pending still ends the chain (demand stopped), so an
  /// animation that stops costs one spare callback more than before.
  banked: Option<std::time::Instant>,
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
      banked: None,
      hold: 1,
      slot_start: None,
    }
  }

  /// Feed one present-return at `now` (the swap's return instant); `period`
  /// is the current refresh period.
  pub fn on_present(&mut self, now: std::time::Instant, period: std::time::Duration) -> Release {
    if !self.vsync_locked() {
      // Present-return pacing on the slot grid: a present that returns
      // inside its slot defers to the slot end as a timer deadline. With a
      // hold the slot is the cadence hold's; at hold 1 it is one period,
      // and the deferral is the floor that keeps a swap which does not
      // block (see `swap_paces`) at the refresh rate instead of running
      // unbounded. A blocking swap returns about a period after the last
      // one, inside the slack, and is not deferred.
      if let Some(end) = self.slot_end(period) {
        if now + period / SWAP_RETURN_SLACK_DIVISOR < end {
          self.pending += 1;
          self.deadline = end;
          return Release::Deferred { arm: None };
        }
        // Within the slack of the slot end: emit now, but the next slot
        // starts at this one's end, so early returns never compound into
        // an extra frame per grid.
        if now < end {
          self.slot_start = Some(end);
          return Release::Emit { arm: None, vsync: None };
        }
      }
      self.slot_start = Some(now);
      return Release::Emit { arm: None, vsync: None };
    }
    if let Some(emitted) = self.signal_emitted.take() {
      self.budget.record(now.duration_since(emitted).as_secs_f32() * 1000.0, period * self.hold);
    }
    if let Some(vsync) = self.banked.take() {
      // Its vsync signal already came and went (see `banked`): release now,
      // a couple of milliseconds into the period rather than a period late
      // - unless that vsync fell inside the slot, in which case the present
      // waits for the slot's own vsync like any other.
      if !self.too_early(vsync, period) {
        self.signal_emitted = Some(now);
        self.slot_start = Some(vsync);
        return Release::Emit { arm: self.arm(now, period), vsync: Some(vsync) };
      }
    }
    self.pending += 1;
    // Normally the signal releasing this present is already armed
    // (pre-armed when the previous one was emitted); this request only
    // starts the chain on the first present out of idle.
    Release::Deferred { arm: self.arm(now, period) }
  }

  /// Feed one loop wake with the VsyncSource drain's result: `signal` is the
  /// vsync instant of the latest request's signal when it was among the
  /// drained ones. One signal releases all pending; a signal past the
  /// fallback deadline is replaced by the fallback, which also disarms so
  /// the release pre-arms a fresh request superseding the late one. A
  /// signal with nothing pending is banked while a frame is in flight and
  /// ends the chain otherwise; an in-flight window that sees neither
  /// present nor signal by the deadline is given up (the present, if it
  /// ever comes, starts a fresh chain).
  pub fn on_wake(&mut self, now: std::time::Instant, period: std::time::Duration, signal: Option<std::time::Instant>) -> Wake {
    if !self.backend || self.pacing != FramePacing::VsyncLocked {
      // Present-return pacing: the one thing to wake for is a deferred
      // present (held, or floored) whose slot has ended. Its reference is
      // the slot end itself, so the grid stays exact however late the wake
      // lands.
      if self.pending > 0 && now >= self.deadline {
        let emit = self.pending;
        self.pending = 0;
        let reference = self.deadline;
        self.slot_start = Some(reference);
        return Wake::Release { emit, timed_out: false, arm: None, reference };
      }
      return Wake::Idle;
    }
    let signal_taken = signal.is_some();
    if signal_taken {
      self.armed = false;
    }
    if self.pending == 0 {
      let in_flight = self.signal_emitted.is_some();
      if let (Some(vsync), true, None) = (signal, in_flight, self.banked) {
        self.banked = Some(vsync);
        return Wake::Banked { arm: self.arm(now, period) };
      }
      if signal_taken || (in_flight && now >= self.deadline) {
        self.banked = None;
        self.signal_emitted = None;
        self.armed = false;
      }
      return Wake::Idle;
    }
    let timed_out = !signal_taken && now >= self.deadline;
    if timed_out {
      self.armed = false;
    }
    if !signal_taken && !timed_out {
      return Wake::Idle;
    }
    // The vsync this wake stands for (see Wake::Release's reference).
    let reference = signal.unwrap_or_else(|| now.checked_sub(self.delay()).unwrap_or(now));
    if self.too_early(reference, period) {
      // Inside the slot: skip this vsync, keep the chain armed for the next.
      return Wake::Held { arm: self.arm(now, period) };
    }
    let emit = self.pending;
    self.pending = 0;
    self.signal_emitted = Some(now);
    self.slot_start = Some(reference);
    // Pre-arm the signal for the next vsync while this frame is being
    // built: the signal timing must not depend on when the build's present
    // returns (see `armed`). The frame this emission triggers has until
    // that signal - a full period plus the delay - to present, or it slips
    // a frame.
    Wake::Release { emit, timed_out, arm: self.arm(now, period), reference }
  }

  /// Feed the cadence hold (see `hold`); a change re-times a SwapPaced
  /// present already deferred to the old slot end.
  pub fn set_hold(&mut self, hold: u32, period: std::time::Duration) {
    self.hold = hold.max(1);
    if self.pending > 0 && (!self.backend || self.pacing != FramePacing::VsyncLocked) {
      if let Some(end) = self.slot_end(period) {
        self.deadline = end;
      }
    }
  }

  // The end of the current slot: its start plus the hold's periods.
  fn slot_end(&self, period: std::time::Duration) -> Option<std::time::Instant> {
    self.slot_start.map(|start| start + period * self.hold)
  }

  // Whether a vsync at `reference` falls inside the slot: before its end by
  // more than half a period, so the slot's own vsync - the first at or after
  // the end - is recognized through the reference's jitter against the grid.
  fn too_early(&self, reference: std::time::Instant, period: std::time::Duration) -> bool {
    self.hold > 1 && self.slot_end(period).is_some_and(|end| reference + period / 2 < end)
  }

  // The armed signal delay as a duration (the budget's last pick).
  fn delay(&self) -> std::time::Duration {
    std::time::Duration::from_secs_f32(self.budget.current_ms() / 1000.0)
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
      self.banked = None;
      PacingChange::Changed { released }
    } else {
      PacingChange::Changed { released: 0 }
    }
  }

  /// The fallback deadline to wake at; None when no present is deferred and
  /// no frame is in flight (the caller sleeps toward its idle-tick deadline
  /// instead). In flight, the deadline bounds the wait for the present or
  /// its signal; `on_wake` gives the window up when it passes.
  pub fn wait_deadline(&self) -> Option<std::time::Instant> {
    (self.pending > 0 || self.signal_emitted.is_some()).then_some(self.deadline)
  }

  /// Whether no present is deferred and no frame is in flight: the
  /// idle-tick gate. While a present is deferred, the real frame signal is
  /// at most a refresh period away (fallback included); while a frame is in
  /// flight, its present is - a Tick there drives an extra frame into a
  /// pipeline that is already producing one (the swap can outlast a period,
  /// see `banked`).
  pub fn idle(&self) -> bool {
    self.pending == 0 && self.signal_emitted.is_none()
  }

  /// Last armed signal delay in ms, for the 1/s diagnostics line.
  pub fn current_delay_ms(&self) -> f32 {
    self.budget.current_ms()
  }

  /// How far into its slot a released frame starts, ms: the signal delay
  /// after the vsync under VsyncLocked (the cadence controller counts it as
  /// slot use), zero under SwapPaced, where the slot starts at the emission.
  pub fn slot_offset_ms(&self) -> f32 {
    if self.vsync_locked() {
      self.budget.current_ms()
    } else {
      0.0
    }
  }

  /// Whether an unheld chain pipelines: under present-return pacing the
  /// next frame's signal follows the swap's return, so its CPU work overlaps
  /// this frame's GPU work and the throughput is the larger of the two,
  /// not their sum; a vsync-locked chain runs one frame at a time. The
  /// cadence controller reads it to judge an unheld frame's true cost.
  pub fn pipelined(&self) -> bool {
    !self.vsync_locked()
  }

  fn vsync_locked(&self) -> bool {
    self.backend && self.pacing == FramePacing::VsyncLocked
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

  pub fn run(
    req_rx: mpsc::Receiver<(u64, std::time::Duration)>,
    signal_tx: mpsc::Sender<(u64, std::time::Instant)>,
    wake: impl Fn(),
  ) {
    // The choreographer instance is per-thread and requires that thread to
    // own a looper; callbacks are dispatched from this thread's poll calls.
    let looper = ndk::looper::ThreadLooper::prepare();
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
      log::warn!("[alloy] no choreographer on this device; vsync pacing disabled");
    }
    // The vsync instant of the callback that fired, None until it does.
    let fired: Cell<Option<std::time::Instant>> = Cell::new(None);
    while let Ok((mut generation, mut delay)) = req_rx.recv() {
      // A burst of requests collapses into one callback answering the latest
      // generation: the consumer flushes everything pending on each signal.
      while let Ok((g, d)) = req_rx.try_recv() {
        generation = g;
        delay = d;
      }
      let mut vsync = None;
      if !choreographer.is_null() {
        fired.set(None);
        unsafe {
          ndk_sys::AChoreographer_postFrameCallback(
            choreographer,
            Some(frame_callback),
            &fired as *const Cell<Option<std::time::Instant>> as *mut core::ffi::c_void,
          );
        }
        while fired.get().is_none() {
          if looper.poll_once().is_err() {
            // A broken looper cannot dispatch the callback; answer now
            // instead of stalling frame production (= present-return pacing).
            log::warn!("[alloy] vsync looper poll failed; answering without vsync");
            break;
          }
        }
        vsync = fired.get();
        if vsync.is_some() && !delay.is_zero() {
          std::thread::sleep(delay);
        }
      }
      // No choreographer, or a broken looper: the answer instant is the
      // best reference there is.
      let vsync = vsync.unwrap_or_else(std::time::Instant::now);
      if signal_tx.send((generation, vsync)).is_err() {
        return;
      }
      wake();
    }
  }

  // Runs on the vsync thread, inside looper.poll_once(). The frame time is
  // CLOCK_MONOTONIC nanoseconds; it becomes an Instant by subtracting its
  // age (read on the same clock, here, before the pacing delay adds to it).
  // It is the reference the refresh count is taken from: on the vsync grid
  // rather than delay-plus-wake later. Note the value is the app's wake-up
  // time, a constant Display.getAppVsyncOffsetNanos() off a true vsync; a
  // constant phase offset does not change a refresh COUNT, so it is not
  // corrected here. A consumer needing the absolute vsync phase takes it
  // from the video plane's sampler (VideoPlaneView.java), which applies the
  // correction at its source.
  unsafe extern "C" fn frame_callback(frame_time_nanos: core::ffi::c_long, data: *mut core::ffi::c_void) {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    let age_ns = if libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) == 0 {
      (now.tv_sec as i64 * 1_000_000_000 + now.tv_nsec as i64 - frame_time_nanos as i64).max(0)
    } else {
      0
    };
    let vsync = std::time::Instant::now() - std::time::Duration::from_nanos(age_ns as u64);
    (*(data as *const Cell<Option<std::time::Instant>>)).set(Some(vsync));
  }
}

// CVDisplayLink is deprecated by Apple (macOS 15) in favour of CADisplayLink
// on NSView/NSWindow/NSScreen, and the binding carries that deprecation. It
// still runs (macOS 26 measured) and SDL's own CGL path paces on it; when it
// goes, the replacement is CADisplayLink through the same objc2 family, the
// same thread contract, an internal change to this module.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
mod macos {
  use std::ptr::NonNull;
  use std::sync::{mpsc, Arc, Condvar, Mutex};
  use std::time::{Duration, Instant};

  use objc2_core_foundation::CFRetained;
  use objc2_core_video::{
    kCVReturnSuccess, CVDisplayLink, CVGetCurrentHostTime, CVGetHostClockFrequency, CVOptionFlags, CVReturn,
    CVTimeStamp, CVTimeStampFlags,
  };

  // Without a request for this long the link is stopped, so CoreVideo's
  // thread does not wake per refresh while nothing animates; the next
  // request starts it again (one start per idle-to-active transition,
  // never per frame).
  const LINK_IDLE_STOP: Duration = Duration::from_millis(250);
  // Upper bound on the wait for the link's callback after a request: with
  // the display asleep the callbacks stop, and the request is answered at
  // the wake (the present-return reference) instead of stranding this
  // thread. Longer than any refresh period, so a healthy link never hits
  // it; the main loop's own fallback deadline covers the frame meanwhile.
  const SIGNAL_WAIT_TIMEOUT: Duration = Duration::from_millis(100);

  // The callback's hand-off to the vsync thread: the vsync instant of the
  // latest callback since the last take.
  struct Signal {
    vsync: Mutex<Option<Instant>>,
    fired: Condvar,
  }

  impl Signal {
    fn clear(&self) {
      *self.vsync.lock().expect("vsync signal mutex poisoned") = None;
    }

    fn wait(&self, timeout: Duration) -> Option<Instant> {
      let guard = self.vsync.lock().expect("vsync signal mutex poisoned");
      let (mut guard, _) =
        self.fired.wait_timeout_while(guard, timeout, |vsync| vsync.is_none()).expect("vsync signal mutex poisoned");
      guard.take()
    }
  }

  pub fn run(
    req_rx: mpsc::Receiver<(u64, Duration)>,
    signal_tx: mpsc::Sender<(u64, Instant)>,
    ready_tx: mpsc::Sender<bool>,
    wake: impl Fn(),
  ) {
    let signal = Arc::new(Signal { vsync: Mutex::new(None), fired: Condvar::new() });
    let link = create(&signal);
    if ready_tx.send(link.is_some()).is_err() {
      return;
    }
    let Some(link) = link else { return };
    let mut running = false;
    let mut start_warned = false;
    loop {
      let request = if running {
        match req_rx.recv_timeout(LINK_IDLE_STOP) {
          Ok(request) => request,
          Err(mpsc::RecvTimeoutError::Timeout) => {
            link.stop();
            running = false;
            continue;
          }
          Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
      } else {
        match req_rx.recv() {
          Ok(request) => request,
          Err(_) => break,
        }
      };
      let (mut generation, mut delay) = request;
      // A burst of requests collapses into one signal answering the latest
      // generation: the consumer flushes everything pending on each signal.
      while let Ok((g, d)) = req_rx.try_recv() {
        generation = g;
        delay = d;
      }
      if !running {
        if link.start() == kCVReturnSuccess {
          running = true;
        } else if !start_warned {
          start_warned = true;
          log::warn!("[alloy] display link start failed; answering without vsync");
        }
      }
      // The signal is the first callback after the request; one stored
      // before it belongs to a vsync already passed (the Choreographer's
      // one-shot post has the same meaning).
      signal.clear();
      let vsync = if running { signal.wait(SIGNAL_WAIT_TIMEOUT) } else { None };
      if vsync.is_some() && !delay.is_zero() {
        std::thread::sleep(delay);
      }
      // No link, or no callback in time: the answer instant is the best
      // reference there is.
      let vsync = vsync.unwrap_or_else(Instant::now);
      if signal_tx.send((generation, vsync)).is_err() {
        break;
      }
      wake();
    }
    if running {
      link.stop();
    }
  }

  // The link over the active displays, its callback wired to `signal`. The
  // callback's reference to the signal is a leaked Arc: the link lives for
  // the process (this thread only exits at shutdown), so nothing ever
  // reconstructs it.
  fn create(signal: &Arc<Signal>) -> Option<CFRetained<CVDisplayLink>> {
    let mut raw: *mut CVDisplayLink = std::ptr::null_mut();
    let status = unsafe { CVDisplayLink::create_with_active_cg_displays(NonNull::from(&mut raw)) };
    let Some(ptr) = NonNull::new(raw).filter(|_| status == kCVReturnSuccess) else {
      log::warn!("[alloy] no display link on this machine ({status}); present-return pacing");
      return None;
    };
    let link = unsafe { CFRetained::from_raw(ptr) };
    let user = Arc::into_raw(signal.clone()) as *mut core::ffi::c_void;
    let status = unsafe { link.set_output_callback(Some(output), user) };
    if status != kCVReturnSuccess {
      log::warn!("[alloy] display link callback rejected ({status}); present-return pacing");
      return None;
    }
    Some(link)
  }

  // Runs on CoreVideo's display-link thread, once per refresh while the link
  // runs. `now` is the vsync this callback stands for; its host time becomes
  // an Instant by subtracting its age, read on the same clock here (as the
  // Android backend does with the Choreographer frame time), so the
  // reference sits on the vsync grid rather than callback-plus-wake later.
  // A stamp without a valid host time is referenced at the callback itself.
  unsafe extern "C-unwind" fn output(
    _link: NonNull<CVDisplayLink>,
    now: NonNull<CVTimeStamp>,
    _output_time: NonNull<CVTimeStamp>,
    _flags_in: CVOptionFlags,
    _flags_out: NonNull<CVOptionFlags>,
    user: *mut core::ffi::c_void,
  ) -> CVReturn {
    let stamp = unsafe { now.as_ref() };
    let age = if stamp.flags & CVTimeStampFlags::HostTimeValid.0 != 0 {
      let ticks = CVGetCurrentHostTime().saturating_sub(stamp.hostTime);
      Duration::from_secs_f64(ticks as f64 / CVGetHostClockFrequency())
    } else {
      Duration::ZERO
    };
    let vsync = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
    let signal = unsafe { &*(user as *const Signal) };
    *signal.vsync.lock().expect("vsync signal mutex poisoned") = Some(vsync);
    signal.fired.notify_one();
    kCVReturnSuccess
  }
}
