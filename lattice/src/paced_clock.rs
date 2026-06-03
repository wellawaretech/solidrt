use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// KNOWN ISSUE (frame pacing): this is a MODEL of the display's frame cadence, not
// a real vsync/vblank signal. We advance one refresh period per present and then
// slowly correct toward the raw wall-clock sample, which stays smooth under the
// jittery swap-return times produced by Wayland/Mesa mailbox/triple-buffering
// (where the swap call does not block per-vblank) while still tracking real time
// over the long run. The compositor presents exactly one frame per vblank, so the
// present COUNT is the steady signal even when the present TIMESTAMP is not.
//
// The correct fix is to read the platform's actual presentation timing (Wayland
// presentation-time, DRM vblank, macOS CVDisplayLink, Windows DWM, Android
// Choreographer). There is currently no cross-platform Rust crate that unifies
// these; winit's frame-pacing API (rust-windowing/winit#2412) is still open. When
// such a source exists, replace this whole struct with it: the run-mode clock
// closure in lib.rs is the only caller, so it is a single-file swap.

// Refresh period assumed before the first DisplayRefreshRate event arrives.
const DEFAULT_HZ: f64 = 60.0;
// Correction gain (0..1): how fast the paced clock pulls toward the raw clock per
// present. Low gain keeps the cadence smooth; it still converges over many frames.
const GAIN: f64 = 0.05;

// A present-count paced clock. `tick` is called once per present with the raw
// wall-clock reading; `now_ms` returns the smoothed time used for the rAF
// timestamp and the render event. Cloneable and thread-safe so it can back the
// flux::Clock closure (state is shared, not copied).
#[derive(Clone)]
pub struct PacedClock {
  // f64 bits: latest paced time in ms.
  now_ms: Arc<AtomicU64>,
  // f64 bits: latest known refresh rate in Hz.
  hz: Arc<AtomicU64>,
}

impl PacedClock {
  pub fn new() -> Self {
    Self {
      now_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      hz: Arc::new(AtomicU64::new(DEFAULT_HZ.to_bits())),
    }
  }

  // Update the refresh rate used to derive the per-present period. Ignored if not
  // positive so a bogus report cannot stall or reverse the clock.
  pub fn set_hz(&self, hz: f32) {
    if hz > 0.0 {
      self.hz.store((hz as f64).to_bits(), Ordering::Relaxed);
    }
  }

  // Advance one refresh period, then nudge toward the raw wall-clock reading.
  // Called once per present.
  pub fn tick(&self, raw_ms: f64) {
    let hz = f64::from_bits(self.hz.load(Ordering::Relaxed));
    let period = 1000.0 / hz;
    let mut clock = f64::from_bits(self.now_ms.load(Ordering::Relaxed));
    clock += period;
    clock += (raw_ms - clock) * GAIN;
    self.now_ms.store(clock.to_bits(), Ordering::Relaxed);
  }

  // The smoothed time in ms. Stepped: it only advances on `tick` (once per
  // present), which is what an animation timestamp wants.
  pub fn now_ms(&self) -> f64 {
    f64::from_bits(self.now_ms.load(Ordering::Relaxed))
  }
}