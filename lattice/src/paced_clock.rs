use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use alloy::PresentClock;

// An expected-vs-observed jump beyond this is a suspension (app backgrounded,
// system stall), not drift: presents stopped while time ran on. Time the app
// never lived through is skipped rather than replayed - the app advances one
// period across it. Same magnitude as alloy's stall threshold, different
// meaning: there it is timestamp fidelity, here it is timeline policy.
const SUSPEND_MS: f64 = 500.0;

// The app's animation timeline over alloy's per-present timestamps (see
// alloy::PresentClock, which models the display cadence until real
// presentation timing exists). `tick` is called once per present with the raw
// wall-clock reading; `now_ms` returns the app-time reading used for the rAF
// timestamp and the render event. This layer owns the timeline policy: the
// dev clock's pause/step/scale semantics and suspension skipping. Cloneable
// and thread-safe so it can back the flux::Clock closure (state is shared,
// not copied).
//
// Two readings share the pause/step/scale policy but differ in anchoring:
//
// - `now_ms`, the animation reading: smooth by construction (one period per
//   present, slow correction), which means it runs BEHIND the wall clock by
//   up to (signal period - refresh period) / GAIN whenever frame signals
//   arrive slower than the refresh cadence. Right for animation timestamps,
//   wrong for deadlines.
// - `timer_now_ms`, the timer reading: the raw wall clock minus time not
//   lived through (paused or scaled stretches), no smoothing. Timer
//   deadlines judged against it stay wall-accurate no matter how slowly
//   frames arrive; firing stays quantized to the tick sites. Unlike the
//   animation reading it does not skip suspensions: a timer that came due
//   while the app was backgrounded fires on the resume tick, browser-style
//   (one-shots once each; intervals collapse to one fire per advance).
#[derive(Clone)]
pub struct PacedClock {
  present: PresentClock,
  // f64 bits: latest app-time reading in ms.
  now_ms: Arc<AtomicU64>,
  // f64 bits: presentation time this clock has not lived through (paused or
  // scaled stretches, suspensions), subtracted from the present timestamp so
  // the eventual return to scale 1 resumes exactly where the clock stopped
  // instead of jumping through the gap. Zero until the clock first skips
  // time.
  offset: Arc<AtomicU64>,
  // f64 bits: latest timer-timeline reading in ms (see timer_now_ms).
  timer_ms: Arc<AtomicU64>,
  // f64 bits: wall time the timer timeline has not lived through (startup
  // and paused or scaled stretches - suspensions are lived through),
  // subtracted from the raw reading at scale 1.
  timer_offset: Arc<AtomicU64>,
  // Whether tick has run at least once. The first tick anchors the timer
  // timeline at the current reading instead of living through the raw
  // stretch before it (engine build and eval at cold start): without the
  // anchor, that stretch would count against every timer registered at
  // module init and fire them all on the first frame.
  started: Arc<AtomicBool>,
}

impl PacedClock {
  pub fn new() -> Self {
    Self {
      present: PresentClock::new(),
      now_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      offset: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      timer_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      timer_offset: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      started: Arc::new(AtomicBool::new(false)),
    }
  }

  // Update the refresh rate the presentation model derives its period from.
  pub fn set_hz(&self, hz: f32) {
    self.present.set_hz(hz);
  }

  // Consume one present: fetch its timestamp from the presentation model,
  // then apply the timeline policy for `scale`. At scale 1 the clock follows
  // the presentation timeline (minus the accrued offset); a jump beyond
  // SUSPEND_MS is a suspension, folded into `offset` while the app advances
  // exactly one period. At any other scale - including 0, a paused frame -
  // the clock advances period * scale on its own and re-anchors `offset` to
  // the presentation timeline, so resuming normal speed is jump-free.
  pub fn tick(&self, raw_ms: f64, scale: f64) {
    let t = self.present.on_present(raw_ms);
    let period = self.present.period_ms();
    let mut now = f64::from_bits(self.now_ms.load(Ordering::Relaxed));
    if scale == 1.0 {
      let offset = f64::from_bits(self.offset.load(Ordering::Relaxed));
      let expected = now + period;
      let observed = t - offset;
      if (observed - expected).abs() > SUSPEND_MS {
        self.offset.store((t - expected).to_bits(), Ordering::Relaxed);
        now = expected;
      } else {
        now = observed;
      }
    } else {
      now += period * scale;
      self.offset.store((t - now).to_bits(), Ordering::Relaxed);
    }
    self.now_ms.store(now.to_bits(), Ordering::Relaxed);
    // The timer timeline: raw wall time at scale 1 (no smoothing, no
    // suspension skip); any other scale advances period * scale in lockstep
    // with the animation reading and re-anchors its offset so the return to
    // scale 1 is jump-free. The first tick only anchors (see `started`).
    // The max() only guards monotonicity against a raw reading landing
    // inside the just-re-anchored offset.
    let mut timer = f64::from_bits(self.timer_ms.load(Ordering::Relaxed));
    let first = !self.started.swap(true, Ordering::Relaxed);
    if first || scale != 1.0 {
      if !first {
        timer += period * scale;
      }
      self.timer_offset.store((raw_ms - timer).to_bits(), Ordering::Relaxed);
    } else {
      let toff = f64::from_bits(self.timer_offset.load(Ordering::Relaxed));
      timer = (raw_ms - toff).max(timer);
    }
    self.timer_ms.store(timer.to_bits(), Ordering::Relaxed);
  }

  // The app-time reading in ms. Stepped: it only advances on `tick` (once per
  // present), which is what an animation timestamp wants.
  pub fn now_ms(&self) -> f64 {
    f64::from_bits(self.now_ms.load(Ordering::Relaxed))
  }

  // The timer-timeline reading in ms: wall-accurate at scale 1, sharing the
  // pause/step/scale policy with `now_ms` (see the struct docs for the
  // contrast). Virtual timer deadlines advance against this reading so they
  // never inherit the animation reading's lag under slow frames. Stepped:
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

  // The presentation period backing the timeline, for consumers scheduling
  // or judging against it (video frame selection's half-period lookahead,
  // the frame history's slow-frame threshold).
  pub fn period_ms(&self) -> f64 {
    self.present.period_ms()
  }
}
