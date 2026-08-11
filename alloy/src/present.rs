use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// Per-present timestamps for frame pacing: the time the present that just
// completed effectively happened (see PresentClock). This is the fact source
// an app-time policy layer builds its animation timeline on; it knows nothing
// about that timeline (pausing, scaling, suspension skipping are the
// consumer's business).
//
// KNOWN ISSUE (frame pacing): this is a MODEL of the display's frame cadence,
// not a real vsync/vblank signal. We advance one refresh period per present
// and then slowly correct toward the raw wall-clock sample, which stays smooth
// under the jittery swap-return times produced by Wayland/Mesa
// mailbox/triple-buffering (where the swap call does not block per-vblank)
// while still tracking real time over the long run. The compositor presents
// exactly one frame per vblank, so the present COUNT is the steady signal even
// when the present TIMESTAMP is not.
//
// The correct fix is to read the platform's actual presentation timing
// (Wayland presentation-time, DRM vblank, macOS CVDisplayLink, Windows DWM,
// Android Choreographer). There is currently no cross-platform Rust crate that
// unifies these; winit's frame-pacing API (rust-windowing/winit#2412) is still
// open. When such a source exists, implement it behind this same seam:
// on_present() reports the measured timestamps instead of modeled ones, and
// consumers are unchanged.

// Refresh period assumed before the first set_hz call.
const DEFAULT_HZ: f64 = 60.0;
// Correction gain (0..1): how fast the modeled clock pulls toward the raw
// clock per present. Low gain keeps the cadence smooth; it still converges
// over many frames.
const GAIN: f64 = 0.05;
// A raw-vs-modeled gap beyond this is a stall of the present stream (app
// backgrounded, system halt), not swap jitter: an order of magnitude beyond
// any legitimate swap jitter or GC pause. A real timestamp source would
// report the jump after a stall, so the model snaps to the raw reading
// instead of creeping toward it at GAIN per present (which would leak the
// gap into consumers as a slow fast-forward).
const STALL_MS: f64 = 500.0;

// A modeled per-present timestamp source. `on_present` is called once per
// present with the raw wall-clock reading and returns the present's modeled
// timestamp; one caller is expected to drive it (the host's frame verb).
// Cloneable and thread-safe so the driving and reading sides can live on
// different threads (state is shared, not copied).
#[derive(Clone)]
pub struct PresentClock {
  // f64 bits: latest modeled presentation time in ms.
  now_ms: Arc<AtomicU64>,
  // f64 bits: latest known refresh rate in Hz.
  hz: Arc<AtomicU64>,
}

impl PresentClock {
  pub fn new() -> Self {
    Self {
      now_ms: Arc::new(AtomicU64::new(0.0f64.to_bits())),
      hz: Arc::new(AtomicU64::new(DEFAULT_HZ.to_bits())),
    }
  }

  // Update the refresh rate used to derive the per-present period. Ignored if
  // not positive so a bogus report cannot stall or reverse the clock.
  pub fn set_hz(&self, hz: f32) {
    if hz > 0.0 {
      self.hz.store((hz as f64).to_bits(), Ordering::Relaxed);
    }
  }

  /// The refresh period the model advances by per present.
  pub fn period_ms(&self) -> f64 {
    1000.0 / f64::from_bits(self.hz.load(Ordering::Relaxed))
  }

  /// The modeled timestamp of the present that just completed: one period
  /// after the previous one, nudged toward `raw_ms` (see GAIN), or snapped to
  /// it after a stall (see STALL_MS).
  pub fn on_present(&self, raw_ms: f64) -> f64 {
    let mut clock = f64::from_bits(self.now_ms.load(Ordering::Relaxed));
    clock += self.period_ms();
    let gap = raw_ms - clock;
    if gap.abs() > STALL_MS {
      clock = raw_ms;
    } else {
      clock += gap * GAIN;
    }
    self.now_ms.store(clock.to_bits(), Ordering::Relaxed);
    clock
  }
}
