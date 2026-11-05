// The cadence hold: which whole number of display refreshes each frame
// interval is held to when the app cannot make the refresh rate (tier 3 of
// okf/design/frame-timing.md; the work is okf/done/cadence-hold-app-control.md).
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
// no margin, so the hold rises on measured intervals - but only on a
// sustained share of them, so a startup burst or one heavy mount (a handful
// of long frames) does not hold the app for the rest of its run. A hold that
// is too long shows nothing (every interval equals it), so the step down is
// a prediction from the frame's slot use, which stays pessimistic: under a
// hold the device lowers the capacity it measures (a governor clocks the
// GPU down, the scheduler moves the frame's threads to slower cores), so a
// held reading overstates what the lower hold would cost. The prediction
// steps down as far as it can prove, never on a guess; the rest is the
// app's (a reset or a policy change) or an idle gap's
// (okf/notes/cadence-hold-capacity.md has the measurements).

/// A cadence-hold policy (the embedder's choice, AlloyCommand::SetCadenceHold).
/// `Off` is every frame as soon as it is ready; `Auto` runs the controller with a
/// maximum slot length in ms: the hold never grows past it, and a workload
/// that needs more is held at the maximum and misses (counted as such),
/// which is the Frame Pacing library's rule too. Dropping to unheld beyond
/// the maximum was tried and flapped: a workload sitting on the maximum
/// shows intervals on both sides of it. `Fixed` locks the interval, for an
/// app that knows its target and for probes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CadenceHold {
  Off,
  Auto { max_ms: u32 },
  Fixed(u32),
}

// Presents the rise looks back over, and how many of them must have shown
// an interval longer than the hold before it rises: a third. A startup
// burst or one heavy mount (a 453 ms first frame and four slow ones on a
// tablet) stays under it; a workload that needs the longer interval
// reaches it within about ten presents.
const UP_WINDOW: usize = 30;
const UP_LONG: usize = 10;

// Presents after a hold change whose intervals are not taken as evidence
// for a rise: the pipeline re-forms across a change (a held chain drains
// the swap queue, an unheld one refills it), and the first intervals show
// the transition, not the workload. Measured on the 50 Hz TV box: the two
// presents right after a step down to unheld showed an interval of 2 with
// 30 ms of work for a 7 ms frame, and reverted every step.
const SETTLE_PRESENTS: u32 = 3;

// Presents after a measured interval longer than the hold during which the
// pipelined estimate may count as evidence for a rise. The estimate exists
// for the case where the pipeline hides the work behind intervals of 1,1,2;
// without a long interval there is nothing to fix, and under a pipelined
// chain the measured CPU and GPU times are stretched by the pipeline's own
// waits (the 50 Hz TV box read 19 ms of CPU and 20 of GPU for a 7 ms frame
// at full rate, with every interval 1), so the estimate alone is not
// evidence.
const MISS_MEMORY_PRESENTS: u32 = 30;

// An idle gap this long between presents ends the frame stream the hold was
// learned on: the next interaction starts unheld and the measured rule
// decides afresh. Without it a hold that a mount or a heavier scene raised
// outlives it indefinitely, because idle feeds the controller nothing to
// step down on, and every later animation inherits it (a tablet measured
// stuck at three refreshes through minutes of idle, 2026-09-22).
const IDLE_RESET_MS: f64 = 500.0;

// Slack when turning the policy's maximum slot length into whole slots, in
// slots: 50 ms at a 16.667 ms period is three slots, not 2.9999.
const MAX_HOLD_SLACK: f64 = 0.01;

// Headroom the predicted slot use must leave in the shorter slot before a
// step down is taken, ms: the pacing budget's own headroom (its MARGIN_MS),
// which is the runtime's one measured statement about how close to a
// vsync a frame may finish.
const DOWN_MARGIN_MS: f32 = 2.0;

// Span of presents that must all predict a fit in the shorter slot before
// the hold steps down, one step at a time; a workload sitting on a
// boundary settles on the higher side instead of flapping across it.
const DOWN_WINDOW_MS: f64 = 1000.0;

// Consecutive presents inside the down window that may fail the fit before
// it closes: one is a spike and keeps the window, two in a row close it.
const DOWN_UNFIT_CLOSE: u32 = 2;

// A rise within this many presents of a step down reverts it: the step's
// settle plus one rise window, the soonest the share can answer. A
// reverted step proved the prediction wrong for this stream, and it is
// not tried again (see CadenceController::floor).
const REVERT_PRESENTS: u32 = SETTLE_PRESENTS + UP_WINDOW as u32;

// Consecutive long intervals that revert a step down inside its revert
// span, without waiting for the share: the step was the controller's own
// prediction, so the evidence it was wrong needs no more than this. A
// stream that misses 30% of its frames at the lower hold sits under the
// share for seconds; two in a row end the stutter within a few frames.
const REVERT_RUN: u32 = 2;

/// What one present taught the controller, for its diagnostics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldChange {
  pub from: u32,
  pub to: u32,
  /// The typical long interval that raised the hold, or the predicted slot
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
  // The intervals the last presents showed as evidence (the measured one,
  // or the pipelined estimate where that says more), at most UP_WINDOW,
  // since the last hold change and past its settle.
  evidence: std::collections::VecDeque<u32>,
  // When the current down window opened (ms on the caller's clock), and
  // the consecutive presents in it whose slot use did not predict a fit
  // one step down (see DOWN_UNFIT_CLOSE).
  window_start_ms: Option<f64>,
  window_unfit: u32,
  // The lowest hold a step down may reach, and whether the last change was
  // a step down: a step the measured rule reverts (REVERT_PRESENTS) raises
  // the floor above it, so a misjudged stream stutters once, not every
  // window. An idle gap, a reset and a policy change lower it to 1.
  floor: u32,
  stepped_down: bool,
  // Consecutive presents past the settle whose interval exceeded the hold
  // (see REVERT_RUN).
  run_long: u32,
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
      evidence: std::collections::VecDeque::with_capacity(UP_WINDOW),
      window_start_ms: None,
      window_unfit: 0,
      floor: 1,
      stepped_down: false,
      run_long: 0,
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
  /// presents fed so far, the last present's interval and work, the down
  /// window's age, and the long intervals in the rise's window.
  pub fn diagnostics(&self, now_ms: f64) -> String {
    let window = self.window_start_ms.map_or(0.0, |s| now_ms - s);
    let long = self.evidence.iter().filter(|&&i| i > self.hold).count();
    let recent: Vec<String> = self.recent.iter().map(|(i, c, g, dt)| format!("({i} {c:.1}+{g:.1} @{dt:.0})")).collect();
    format!(
      "hold {}, floor {}, fed {}, last interval {}, last work {:.1}ms, down window {:.0}/{:.0}ms, unfit {}, long {}/{}, since change {}, recent {}",
      self.hold,
      self.floor,
      self.fed,
      self.last_interval,
      self.last_work_ms,
      window,
      DOWN_WINDOW_MS,
      self.window_unfit,
      long,
      self.evidence.len(),
      self.since_change,
      recent.join(" ")
    )
  }

  /// Apply a policy; the hold it implies takes effect at once (Off and
  /// Fixed are immediate, Auto starts unheld and learns). Returns the new
  /// hold when it differs from the current one.
  pub fn set_policy(&mut self, policy: CadenceHold) -> Option<u32> {
    self.policy = policy;
    let hold = match policy {
      CadenceHold::Off | CadenceHold::Auto { .. } => 1,
      CadenceHold::Fixed(k) => k.max(1),
    };
    self.relearn(hold)
  }

  /// End the hold the controller learned, keeping the policy: Auto starts
  /// unheld again (Off and Fixed keep theirs). The app's forced reset at a
  /// phase start, and a reload's. Returns the new hold when it differs.
  pub fn reset(&mut self) -> Option<u32> {
    match self.policy {
      CadenceHold::Auto { .. } => self.relearn(1),
      CadenceHold::Off | CadenceHold::Fixed(_) => None,
    }
  }

  pub fn hold(&self) -> u32 {
    self.hold
  }

  pub fn policy(&self) -> CadenceHold {
    self.policy
  }

  // Forget everything the stream taught, the floor included, and start at
  // `hold`: a new stream (policy, reset, idle gap).
  fn relearn(&mut self, hold: u32) -> Option<u32> {
    self.floor = 1;
    self.restart(hold)
  }

  // Start learning afresh at `hold`: the evidence and the down window were
  // gathered under the previous one. Returns the hold when it changed.
  fn restart(&mut self, hold: u32) -> Option<u32> {
    self.stepped_down = false;
    self.run_long = 0;
    self.evidence.clear();
    self.window_start_ms = None;
    self.window_unfit = 0;
    if hold == self.hold {
      return None;
    }
    self.hold = hold;
    self.since_change = 0;
    Some(hold)
  }

  /// Feed one demanded present: `interval` is the refreshes the display
  /// showed the previous frame for (the honest count, at least 1); `cpu_ms`
  /// the frame's CPU side (emission to swap call) and `gpu_ms` its GPU
  /// time; `pipelined` whether the release
  /// chain lets the next frame's CPU work overlap this frame's GPU work
  /// when unheld (swap pacing does); `now_ms` is on the caller's clock and
  /// `period_ms` the refresh period in force. Returns the change when the
  /// hold moves.
  ///
  /// Both directions read the frame as the pipeline runs it, `max(cpu,
  /// gpu)` (the Frame Pacing library's estimate): the swap returns at the
  /// queue and the GPU finishes while the next frame's CPU side runs, and
  /// a vsync-locked chain overlaps that way too (a tablet completed 96% of
  /// its frames after the next had begun, every refresh made with 21 ms of
  /// summed work at 60 Hz). Under a pipelined chain the interval hides the
  /// work behind 1,1,2, so there the estimate also counts as evidence for
  /// a rise; the step down predicts from it. The pacing delay a
  /// vsync-locked frame starts after is latency, not slot use: every frame
  /// starts at the same phase, so it shifts when the frame is shown, not
  /// how often (a phone made 1896 of 1897 refreshes at 90 Hz with 3.9 ms
  /// of CPU from 6.7 ms into the period).
  pub fn on_present(
    &mut self,
    interval: u32,
    cpu_ms: f32,
    gpu_ms: f32,
    pipelined: bool,
    now_ms: f64,
    period_ms: f64,
  ) -> Option<HoldChange> {
    let CadenceHold::Auto { max_ms } = self.policy else {
      return None;
    };
    let from = self.hold;
    let work_ms = cpu_ms + gpu_ms;
    let idle = self.fed > 0 && now_ms - self.last_now_ms >= IDLE_RESET_MS;
    self.fed += 1;
    self.last_interval = interval;
    self.last_work_ms = work_ms;
    if self.recent.len() >= 8 {
      self.recent.pop_front();
    }
    self.recent.push_back((interval, cpu_ms, gpu_ms, now_ms - self.last_now_ms));
    self.last_now_ms = now_ms;
    // The first present after an idle gap carries the gap as its interval,
    // evidence of nothing: it only resets a hold above 1 (see IDLE_RESET_MS).
    if idle {
      return self.relearn(1).map(|to| HoldChange { from, to, need: 1, work_ms });
    }
    let settling = self.since_change < SETTLE_PRESENTS;
    self.since_change = self.since_change.saturating_add(1);
    let max_hold = ((max_ms as f64 / period_ms + MAX_HOLD_SLACK).floor() as u32).max(1);
    // The interval the chain needs as the display would show it: the
    // measured one, or under a pipelined chain that has shown a longer
    // interval recently, the pipelined estimate when that says more.
    let recent_miss = interval > self.hold || self.since_miss < MISS_MEMORY_PRESENTS;
    self.since_miss = if interval > self.hold { 0 } else { self.since_miss.saturating_add(1) };
    let pipelined_ms = cpu_ms.max(gpu_ms);
    let mut shown = interval;
    if pipelined && recent_miss {
      let estimate = ((pipelined_ms + DOWN_MARGIN_MS) as f64 / period_ms).ceil().max(1.0) as u32;
      shown = shown.max(estimate);
    }
    // Up, on sustained need: a share of the window's presents showed the
    // frame longer than the hold, once the chain has settled after a
    // change. The hold rises to the typical (median) long interval, not
    // the worst, so one very long frame does not set it.
    if !settling {
      if self.evidence.len() >= UP_WINDOW {
        self.evidence.pop_front();
      }
      self.evidence.push_back(shown);
      self.run_long = if shown > self.hold { self.run_long + 1 } else { 0 };
    }
    let reverting = self.stepped_down && self.since_change <= REVERT_PRESENTS;
    if reverting && self.run_long >= REVERT_RUN {
      // Back to the hold the step left, and never below it again.
      self.floor = self.floor.max(from + 1);
      return self.restart(from + 1).map(|to| HoldChange { from, to, need: shown, work_ms });
    }
    if self.evidence.iter().filter(|&&i| i > self.hold).count() >= UP_LONG {
      let mut long: Vec<u32> = self.evidence.iter().copied().filter(|&i| i > self.hold).collect();
      long.sort_unstable();
      let typical = long[(long.len() - 1) / 2];
      // Never past the policy's maximum: a workload that needs more is
      // held at the maximum and its longer intervals are misses.
      let to = typical.min(max_hold).max(self.hold);
      if reverting {
        self.floor = self.floor.max(from + 1);
      }
      return self.restart(to).map(|to| HoldChange { from, to, need: typical, work_ms });
    }
    if shown > self.hold {
      // A long interval is not a fit one step down either.
      self.window_start_ms = None;
      self.window_unfit = 0;
      return None;
    }
    // Down, on proof: the frame's slot use fits one slot fewer, for a
    // whole window.
    let fits_below =
      self.hold > self.floor && (cpu_ms.max(gpu_ms) + DOWN_MARGIN_MS) as f64 <= (self.hold - 1) as f64 * period_ms;
    if !fits_below {
      self.window_unfit += 1;
      if self.window_unfit >= DOWN_UNFIT_CLOSE || self.hold <= self.floor {
        self.window_start_ms = None;
        self.window_unfit = 0;
      }
      return None;
    }
    self.window_unfit = 0;
    match self.window_start_ms {
      None => self.window_start_ms = Some(now_ms),
      Some(start) if now_ms - start >= DOWN_WINDOW_MS => {
        let to = self.hold - 1;
        let change = self.restart(to).map(|to| HoldChange { from, to, need: to, work_ms });
        self.stepped_down = true;
        return change;
      }
      Some(_) => {}
    }
    None
  }
}
