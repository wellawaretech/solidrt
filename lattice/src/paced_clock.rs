use std::sync::atomic::{AtomicU64, Ordering};
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
}

impl PacedClock {
  pub fn new() -> Self {
    Self {
      present: PresentClock::new(),
      now_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      offset: Arc::new(AtomicU64::new(0.0f64.to_bits())),
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
  }

  // The app-time reading in ms. Stepped: it only advances on `tick` (once per
  // present), which is what an animation timestamp wants.
  pub fn now_ms(&self) -> f64 {
    f64::from_bits(self.now_ms.load(Ordering::Relaxed))
  }

  // The presentation period backing the timeline, for consumers scheduling
  // against it (video frame selection's half-period lookahead).
  pub fn period_ms(&self) -> f64 {
    self.present.period_ms()
  }
}
