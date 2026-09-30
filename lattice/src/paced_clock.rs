use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

// A frame signal whose refresh count is worth more than this is a suspension
// (app backgrounded, system stall), not a slow frame: the animation timeline
// advances one period across it instead of replaying time the app never
// lived through. Policy, so it lives here and not in alloy's counter, which
// reports the honest count.
const SUSPEND_MS: f64 = 500.0;

// Refresh rate assumed before the first set_hz call.
const DEFAULT_HZ: f64 = 60.0;

/// How one frame signal advances the app timelines: the dev clock control's
/// verdict for the frame (see runtime.rs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Advance {
  /// Paused (scale 0, no step queued): frame delivery is gated and neither
  /// timeline moves.
  Paused,
  /// A queued step while paused: exactly one period on both timelines.
  Step,
  /// Running at a scale. At 1.0 the animation reading follows the counted
  /// refreshes and the timer reading the wall clock; any other scale
  /// advances both by period * scale.
  Run(f64),
}

// The app's timelines over alloy's frame signals (see
// okf/design/frame-timing.md, the clocks table and D1/D3/D5). Cloneable and
// thread-safe so it can back the flux::Clock closure (state is shared, not
// copied).
//
// - `now_ms`, the animation reading: advances by the display refreshes each
//   frame signal covered, as alloy counted them (`refreshes` on
//   FrameRendered / Tick) - one period per frame at full rate, whole
//   multiples when a frame took longer. No smoothing and no lag: the count
//   is wall-true by construction, so the reading keeps wall rate over any
//   span while every step is a whole number of periods. A count worth a
//   suspension (SUSPEND_MS) advances one period instead.
// - `timer_now_ms`, the timer reading: the raw wall clock minus time not
//   lived through (paused or scaled stretches), no quantization. Timer
//   deadlines judged against it stay wall-accurate whatever the frames do;
//   firing stays quantized to the tick sites. Unlike the animation reading
//   it does not skip suspensions: a timer that came due while the app was
//   backgrounded fires on the resume tick, browser-style (one-shots once
//   each; intervals collapse to one fire per advance).
// - the input reading, what an input event's `timeStamp` is
//   (okf/plans/event-timestamp.md): neither of the two above can carry it.
//   Like the animation reading it advances by counted refreshes. Like the
//   timer reading it lives through a suspension (two taps on either side
//   of a background stretch are far apart) and it has a reading for any
//   instant, not only for a tick: an event is stamped with when it
//   happened (`input_at_ms`), which for a move is the time of the position
//   it carries (alloy's resample.rs) and for the rest the platform's event
//   time. The reading is placed against the signal's grid instant (where
//   alloy's refresh count puts the signal), not against the instant the
//   tick ran: the grid advances by exactly the counted refreshes, so the
//   reading of an instant does not depend on which tick it is read after,
//   and no tick's execution jitter reaches a stamp.
#[derive(Clone)]
pub struct PacedClock {
  // f64 bits: latest animation reading in ms.
  now_ms: Arc<AtomicU64>,
  // f64 bits: latest timer-timeline reading in ms (see timer_now_ms).
  timer_ms: Arc<AtomicU64>,
  // f64 bits: wall time the timer timeline has not lived through (startup
  // and paused or scaled stretches - suspensions are lived through),
  // subtracted from the raw reading while running at scale 1.
  timer_offset: Arc<AtomicU64>,
  // Whether tick has run at least once. The first tick anchors the timer
  // timeline at the current reading instead of living through the raw
  // stretch before it (engine build and eval at cold start): without the
  // anchor, that stretch would count against every timer registered at
  // module init and fire them all on the first frame.
  started: Arc<AtomicBool>,
  // f64 bits: latest known refresh rate in Hz.
  hz: Arc<AtomicU64>,
  // f64 bits: the input reading latched by the last tick.
  input_ms: Arc<AtomicU64>,
  // f64 bits: the raw wall reading of the last tick's grid instant, which
  // the reading of any other instant is measured from.
  input_grid: Arc<AtomicU64>,
  // f64 bits: how fast the input reading runs against the wall: the clock
  // control's scale, 0 while paused or stepped.
  input_rate: Arc<AtomicU64>,
}

impl PacedClock {
  pub fn new() -> Self {
    Self {
      now_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      timer_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      timer_offset: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      started: Arc::new(AtomicBool::new(false)),
      hz: Arc::new(AtomicU64::new(DEFAULT_HZ.to_bits())),
      input_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      input_grid: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      input_rate: Arc::new(AtomicU64::new(0.0f64.to_bits())),
    }
  }

  // Update the refresh rate a refresh is worth. Ignored if not positive so a
  // bogus report cannot stall or reverse the clock.
  pub fn set_hz(&self, hz: f32) {
    if hz > 0.0 {
      self.hz.store((hz as f64).to_bits(), Ordering::Relaxed);
    }
  }

  // Consume one frame signal: `raw_ms` is the wall reading on the shared
  // origin, `grid_ms` the signal's grid instant on the same origin (where
  // alloy's count places the signal), `refreshes` the display refreshes the
  // signal covered (alloy's count), `advance` the clock control's verdict
  // for the frame.
  pub fn tick(&self, raw_ms: f64, grid_ms: f64, refreshes: u32, advance: Advance) {
    let period = self.period_ms();
    let step = match advance {
      Advance::Paused => 0.0,
      Advance::Step => period,
      Advance::Run(scale) if scale == 1.0 => {
        let counted = refreshes as f64 * period;
        if counted > SUSPEND_MS {
          period
        } else {
          counted
        }
      }
      Advance::Run(scale) => period * scale,
    };
    let now = f64::from_bits(self.now_ms.load(Ordering::Relaxed)) + step;
    self.now_ms.store(now.to_bits(), Ordering::Relaxed);
    // The input reading: the same step, except that a suspension is lived
    // through (the counted refreshes as they are), and the rate the reading
    // runs at around this tick's grid instant.
    let (input_step, input_rate) = match advance {
      Advance::Paused | Advance::Step => (step, 0.0),
      Advance::Run(scale) if scale == 1.0 => (refreshes as f64 * period, 1.0),
      Advance::Run(scale) => (step, scale),
    };
    let input = f64::from_bits(self.input_ms.load(Ordering::Relaxed)) + input_step;
    self.input_ms.store(input.to_bits(), Ordering::Relaxed);
    self.input_grid.store(grid_ms.to_bits(), Ordering::Relaxed);
    self.input_rate.store(input_rate.to_bits(), Ordering::Relaxed);
    // The timer timeline: raw wall time at scale 1 (no quantization, no
    // suspension skip); paused, stepped or scaled frames advance it by the
    // same step as the animation reading and re-anchor its offset so the
    // return to scale 1 is jump-free. The first tick only anchors (see
    // `started`). The max() only guards monotonicity against a raw reading
    // landing inside the just-re-anchored offset.
    let mut timer = f64::from_bits(self.timer_ms.load(Ordering::Relaxed));
    let first = !self.started.swap(true, Ordering::Relaxed);
    if first || advance != Advance::Run(1.0) {
      if !first {
        timer += step;
      }
      self.timer_offset.store((raw_ms - timer).to_bits(), Ordering::Relaxed);
    } else {
      let toff = f64::from_bits(self.timer_offset.load(Ordering::Relaxed));
      timer = (raw_ms - toff).max(timer);
    }
    self.timer_ms.store(timer.to_bits(), Ordering::Relaxed);
  }

  // The animation reading in ms. Stepped: it only advances on `tick` (once
  // per frame signal), which is what an animation timestamp wants.
  pub fn now_ms(&self) -> f64 {
    f64::from_bits(self.now_ms.load(Ordering::Relaxed))
  }

  // The timer-timeline reading in ms: wall-accurate at scale 1, sharing the
  // pause/step/scale policy with `now_ms` (see the struct docs for the
  // contrast). Virtual timer deadlines advance against this reading. Stepped:
  // it only advances on `tick`.
  pub fn timer_now_ms(&self) -> f64 {
    f64::from_bits(self.timer_ms.load(Ordering::Relaxed))
  }

  // A fresh timer-timeline reading between ticks, from the caller's current
  // raw wall reading (same origin tick() is fed): what schedule-time timer
  // deadlines anchor to, so a timer registered mid-frame does not measure
  // its delay from the previous tick's stale reading and fire up to one
  // frame early. Before the first tick there is no anchor yet, so it
  // reports the stepped reading; while paused or scaled it can read up to
  // one tick gap past it (the offset re-anchors per tick), which a deadline
  // absorbs as at-most-one-quantum extra delay.
  pub fn timer_live_ms(&self, raw_ms: f64) -> f64 {
    let latched = f64::from_bits(self.timer_ms.load(Ordering::Relaxed));
    if !self.started.load(Ordering::Relaxed) {
      return latched;
    }
    let toff = f64::from_bits(self.timer_offset.load(Ordering::Relaxed));
    (raw_ms - toff).max(latched)
  }

  // The input reading the last tick latched: the stamp of what a frame
  // produces itself (the hover refresh, which has no event behind it).
  pub fn input_frame_ms(&self) -> f64 {
    f64::from_bits(self.input_ms.load(Ordering::Relaxed))
  }

  // The stamp of an input event that happened at the raw wall reading
  // `raw_ms` (same origin tick() is fed): the latched reading plus the
  // event's distance from the last tick's grid instant, at the clock
  // control's rate, so it stands still while paused. The distance is
  // negative for what happened before the grid instant (a resampled move,
  // an event that waited behind a frame). Before the first tick there is
  // no anchor and it reports the latched reading.
  pub fn input_at_ms(&self, raw_ms: f64) -> f64 {
    let latched = f64::from_bits(self.input_ms.load(Ordering::Relaxed));
    if !self.started.load(Ordering::Relaxed) {
      return latched;
    }
    let since = raw_ms - f64::from_bits(self.input_grid.load(Ordering::Relaxed));
    latched + since * f64::from_bits(self.input_rate.load(Ordering::Relaxed))
  }

  // The refresh period a refresh is worth, for consumers scheduling or
  // judging against it (video frame selection's half-period lookahead, the
  // frame history's slow-frame threshold).
  pub fn period_ms(&self) -> f64 {
    1000.0 / f64::from_bits(self.hz.load(Ordering::Relaxed))
  }
}
