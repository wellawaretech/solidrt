// The cadence hold: which whole number of display refreshes each frame
// interval is held to when the app cannot make the refresh rate (tier 3 of
// okf/design/frame-timing.md; the work is okf/plans/cadence-hold.md).
//
// Below the refresh rate a frame is shown for a varying number of refreshes
// (3 and 4 alternating for a 25 fps app on a 90 Hz panel), which the eye
// reads as judder even though the app timeline advances honestly. Android's
// Frame Pacing library holds a swap interval that is a whole number of
// refreshes and only moves it when the measured frame time has clearly
// crossed a boundary; this is that controller. It reads two facts per
// present - the refresh interval the display actually showed the frame for
// (the honest count) and the frame's work time - and answers with the hold
// (the interval in refreshes); FrameRelease (vsync.rs) is the mechanism that
// enforces it, and the embedder's policy (CadenceHold) decides whether the
// trade - frames at the boundary for a metronomic cadence - is on at all.
//
// Up and down use different evidence on purpose. A hold that is too short
// shows itself: the interval exceeds it. That is measured, exact, and needs
// no margin, so the hold rises on measured intervals. A hold that is too
// long shows nothing (every interval equals it), so the step down is a
// prediction from the frame's slot use - its start offset plus its work plus
// a small margin - which can be wrong; a wrong step is reverted by the
// measured rule within two frames, and each reverted step doubles the wait
// before the next try, so a workload the prediction misjudges costs a hitch
// a few times a minute at worst rather than a flap.

/// The embedder's cadence-hold policy (AlloyCommand::SetCadenceHold).
/// `Off` is today's behavior (every frame as soon as it is ready); `Auto`
/// runs the controller with a maximum slot length in ms: the hold never
/// grows past it, and a workload that needs more is held at the maximum
/// and misses (counted as such), which is the Frame Pacing library's rule
/// too. Dropping to unheld beyond the maximum was tried and flapped: a
/// workload sitting on the maximum shows intervals on both sides of it.
/// `Fixed` locks the interval, for an app that knows its target and for
/// probes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CadenceHold {
  Off,
  Auto { max_ms: u32 },
  Fixed(u32),
}

// Consecutive presents whose interval must exceed the hold before it
// rises: one dropped frame must not cost a step, two in a row is a
// workload that crossed the boundary.
const UP_FRAMES: u32 = 2;

// Presents after a hold change whose intervals are not taken as evidence
// for a rise: the pipeline re-forms across a change (a held chain drains
// the swap queue, an unheld one refills it), and the first intervals show
// the transition, not the workload. Measured on the 50 Hz TV box: the two
// presents right after a step down to unheld showed an interval of 2 with
// 30 ms of work for a 7 ms frame, and reverted every step.
const SETTLE_PRESENTS: u32 = 3;

// Presents after a measured interval longer than the hold during which the
// pipelined estimate may raise the hold. The estimate exists for the case
// where the pipeline hides the work behind intervals of 1,1,2; without a
// long interval there is nothing to fix, and under a pipelined chain the
// measured CPU and GPU times are stretched by the pipeline's own waits (the
// 50 Hz TV box read 19 ms of CPU and 20 of GPU for a 7 ms frame at full
// rate, with every interval 1), so the estimate alone is not evidence.
const MISS_MEMORY_PRESENTS: u32 = 30;

// Slack when turning the policy's maximum slot length into whole slots, in
// slots: 50 ms at a 16.667 ms period is three slots, not 2.9999.
const MAX_HOLD_SLACK: f64 = 0.01;

// Headroom the predicted slot use must leave in the shorter slot before a
// step down is tried, ms: the pacing budget's own headroom (its MARGIN_MS),
// which is the runtime's one measured statement about how close to a
// vsync a frame may finish.
const DOWN_MARGIN_MS: f32 = 2.0;

// Span of presents that must all predict a fit in the shorter slot before
// the hold steps down, one step at a time; a workload sitting on a
// boundary settles on the higher side instead of flapping across it.
const DOWN_WINDOW_MS: f64 = 1000.0;

// The longest the window grows to when steps down keep being reverted
// (each reverted step doubles it), so a misjudged workload is retried a
// couple of times a minute at most.
const DOWN_WINDOW_MAX_MS: f64 = 32000.0;

/// What one present taught the controller, for its diagnostics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldChange {
  pub from: u32,
  pub to: u32,
  /// The measured interval that raised the hold, or the predicted slot
  /// count that lowered it.
  pub need: u32,
  pub work_ms: f32,
}

/// The hold controller: a pure state machine over per-present facts
/// (tested in src/tests/cadence.rs). `hold()` is the interval in refreshes
/// the release chain enforces; 1 means no hold.
pub struct CadenceController {
  policy: CadenceHold,
  hold: u32,
  // Consecutive presents whose interval exceeded the hold, and the largest
  // interval among them: the hold rises to the worst of the run.
  over: u32,
  over_worst: u32,
  // When the current down window opened (ms on the caller's clock), and
  // the consecutive presents in it whose slot use did not predict a fit
  // one step down: one such present is a spike and keeps the window (as
  // one long interval does not raise the hold), UP_FRAMES in a row close
  // it.
  window_start_ms: Option<f64>,
  window_unfit: u32,
  // The down window in force and the instant of the last step down, for
  // the backoff: a step up inside the window after a step down doubles it,
  // a window that passes without one resets it.
  down_window_ms: f64,
  last_down_ms: Option<f64>,
  // Presents fed since the last hold change (see SETTLE_PRESENTS), and
  // since the last measured interval longer than the hold (see
  // MISS_MEMORY_PRESENTS).
  since_change: u32,
  since_miss: u32,
  // For the diagnostics line: presents fed, the last present's facts, and
  // the last few presents' (interval, cpu, gpu, ms since the previous).
  fed: u64,
  last_interval: u32,
  last_work_ms: f32,
  recent: std::collections::VecDeque<(u32, f32, f32, f64)>,
  last_now_ms: f64,
}

impl CadenceController {
  pub fn new() -> Self {
    CadenceController {
      policy: CadenceHold::Off,
      hold: 1,
      over: 0,
      over_worst: 0,
      window_start_ms: None,
      window_unfit: 0,
      down_window_ms: DOWN_WINDOW_MS,
      last_down_ms: None,
      since_change: SETTLE_PRESENTS,
      since_miss: u32::MAX,
      fed: 0,
      last_interval: 0,
      last_work_ms: 0.0,
      recent: std::collections::VecDeque::new(),
      last_now_ms: 0.0,
    }
  }

  /// One line of controller state for the 1/s diagnostics: the hold, the
  /// presents fed so far, the last present's interval and work, and the
  /// down window's age against its length.
  pub fn diagnostics(&self, now_ms: f64) -> String {
    let window = self.window_start_ms.map_or(0.0, |s| now_ms - s);
    let recent: Vec<String> = self.recent.iter().map(|(i, c, g, dt)| format!("({i} {c:.1}+{g:.1} @{dt:.0})")).collect();
    format!(
      "hold {}, fed {}, last interval {}, last work {:.1}ms, down window {:.0}/{:.0}ms, unfit {}, over {}, since change {}, recent {}",
      self.hold, self.fed, self.last_interval, self.last_work_ms, window, self.down_window_ms, self.window_unfit, self.over, self.since_change, recent.join(" ")
    )
  }

  /// Apply a policy; the hold it implies takes effect at once (Off and
  /// Fixed are immediate, Auto starts unheld and learns). Returns the new
  /// hold when it differs from the current one.
  pub fn set_policy(&mut self, policy: CadenceHold) -> Option<u32> {
    self.policy = policy;
    self.over = 0;
    self.window_start_ms = None;
    self.down_window_ms = DOWN_WINDOW_MS;
    self.last_down_ms = None;
    let hold = match policy {
      CadenceHold::Off | CadenceHold::Auto { .. } => 1,
      CadenceHold::Fixed(k) => k.max(1),
    };
    if hold == self.hold {
      return None;
    }
    self.hold = hold;
    self.since_change = 0;
    Some(hold)
  }

  pub fn hold(&self) -> u32 {
    self.hold
  }

  /// Feed one demanded present: `interval` is the refreshes the display
  /// showed the previous frame for (the honest count, at least 1); `cpu_ms`
  /// the frame's CPU side (emission to swap call) and `gpu_ms` its GPU
  /// time; `start_offset_ms` how far into its slot the frame started (the
  /// pacing delay after the vsync under vsync-locked release, zero where
  /// the slot starts at the emission); `pipelined` whether the release
  /// chain lets the next frame's CPU work overlap this frame's GPU work
  /// when unheld (swap pacing does, a vsync-locked chain runs one frame at
  /// a time); `now_ms` is on the caller's clock and `period_ms` the refresh
  /// period in force. Returns the change when the hold moves.
  ///
  /// Under a pipelined chain the interval hides the work: a frame whose
  /// CPU and GPU halves each fit a period but not together shows intervals
  /// of 1,1,2, never two long ones in a row, so the measured rule cannot
  /// rise. There the pipelined frame time, `max(cpu, gpu)` (the Frame
  /// Pacing library's estimate for that regime), also counts as evidence
  /// for a rise; a held chain is sequential either way, so the step down
  /// always predicts from `offset + cpu + gpu`.
  pub fn on_present(
    &mut self,
    interval: u32,
    cpu_ms: f32,
    gpu_ms: f32,
    start_offset_ms: f32,
    pipelined: bool,
    now_ms: f64,
    period_ms: f64,
  ) -> Option<HoldChange> {
    let CadenceHold::Auto { max_ms } = self.policy else {
      return None;
    };
    let from = self.hold;
    let work_ms = cpu_ms + gpu_ms;
    self.fed += 1;
    self.last_interval = interval;
    self.last_work_ms = work_ms;
    if self.recent.len() >= 8 {
      self.recent.pop_front();
    }
    self.recent.push_back((interval, cpu_ms, gpu_ms, now_ms - self.last_now_ms));
    self.last_now_ms = now_ms;
    let settling = self.since_change < SETTLE_PRESENTS;
    self.since_change = self.since_change.saturating_add(1);
    let max_hold = ((max_ms as f64 / period_ms + MAX_HOLD_SLACK).floor() as u32).max(1);
    // The interval the chain needs as the display would show it: the
    // measured one, or under a pipelined chain that has shown a longer
    // interval recently, the pipelined estimate when that says more.
    let recent_miss = interval > self.hold || self.since_miss < MISS_MEMORY_PRESENTS;
    self.since_miss = if interval > self.hold { 0 } else { self.since_miss.saturating_add(1) };
    let mut shown = interval;
    if pipelined && recent_miss {
      let estimate = ((cpu_ms.max(gpu_ms) + DOWN_MARGIN_MS) as f64 / period_ms).ceil().max(1.0) as u32;
      shown = shown.max(estimate);
    }
    // A step down that has held for its whole window earns the base window back.
    if self.last_down_ms.is_some_and(|t| now_ms - t >= self.down_window_ms) {
      self.down_window_ms = DOWN_WINDOW_MS;
      self.last_down_ms = None;
    }
    // Up, on measurement: the display showed the frame longer than the
    // hold, twice in a row, once the chain has settled after a change.
    if shown > self.hold && !settling {
      self.over_worst = if self.over == 0 { shown } else { self.over_worst.max(shown) };
      self.over += 1;
      self.window_start_ms = None;
      if self.over >= UP_FRAMES {
        self.over = 0;
        // Never past the policy's maximum: a workload that needs more is
        // held at the maximum and its longer intervals are misses.
        self.hold = self.over_worst.min(max_hold).max(self.hold);
        self.since_change = 0;
        if self.last_down_ms.is_some() {
          // The step down this reverts was a misjudgment: wait longer next time.
          self.down_window_ms = (self.down_window_ms * 2.0).min(DOWN_WINDOW_MAX_MS);
          self.last_down_ms = None;
        }
      }
      return (self.hold != from).then_some(HoldChange { from, to: self.hold, need: shown, work_ms });
    }
    self.over = 0;
    // Down, on prediction: the frame's slot use fits one slot fewer, for a
    // whole window.
    let fits_below = self.hold > 1 && (start_offset_ms + work_ms + DOWN_MARGIN_MS) as f64 <= (self.hold - 1) as f64 * period_ms;
    if !fits_below {
      self.window_unfit += 1;
      if self.window_unfit >= UP_FRAMES || self.hold == 1 {
        self.window_start_ms = None;
        self.window_unfit = 0;
      }
      return None;
    }
    self.window_unfit = 0;
    match self.window_start_ms {
      None => self.window_start_ms = Some(now_ms),
      Some(start) if now_ms - start >= self.down_window_ms => {
        self.hold -= 1;
        self.since_change = 0;
        self.window_start_ms = None;
        self.last_down_ms = Some(now_ms);
      }
      Some(_) => {}
    }
    (self.hold != from).then_some(HoldChange { from, to: self.hold, need: self.hold, work_ms })
  }
}
