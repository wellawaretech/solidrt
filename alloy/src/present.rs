// The refresh count behind every frame signal: how many display refreshes
// passed between this frame signal and the previous one. The app timeline
// (lattice's PacedClock) advances by exactly this, so what an app's onFrame
// sees is one period per frame at full rate and whole multiples when a
// frame took longer - the requestAnimationFrame / Choreographer contract.
// The design, the alternatives tried before it and the tiers above and
// below it are in okf/design/frame-timing.md (D1, D2, D8).
//
// The count is ESTIMATED from reference instants (a swap's return on the
// raster thread, a vsync release, an idle tick's deadline), because no
// platform backend reports presentation timing yet. A reference instant sits
// near the refresh its frame follows but not on it: under a mailbox
// compositor a swap can return up to ~0.6 periods early or late, so no
// single interval can tell one late refresh from two early ones - the
// per-sample round() that was tried flipped between 0, 1 and 2. This
// counter instead measures the CUMULATIVE refreshes since an anchor and only
// moves the count off one when the measurement disagrees with it by more
// than the noise can explain. Noise never accumulates against a cumulative
// anchor, and the band is wider than the noise, so a full-rate app counts
// exactly one every signal with no correction term, a 2:1 cadence counts 2
// every frame, and a 4.3:1 cadence settles into 4-4-4-5 within two frames.
// Over any span the count stays within 1.75 periods of the measurement,
// which is the long-run wall truth by construction.
//
// KNOWN ISSUE (presentation timing): the correct source is the platform's
// presentation feedback (wp_presentation, EGL_ANDROID_get_frame_timestamps,
// the ANGLE sync-control extensions, CVDisplayLink), which reports the
// refresh a frame was shown on; with it the tolerance is never exercised
// and this estimator is only the fallback. Per platform, behind this same
// seam: okf/backlog/presentation-feedback.md.

// Refresh periods a signal's reference instant may sit from the refresh it
// follows. Swap returns under mailbox compositors land up to ~0.6 periods
// off (the bound the raster thread's former run-based miss accounting also
// assumed). Below one, so a duplicate signal (two emissions at one instant,
// a full period ahead) is always counted as zero.
const TOLERANCE: f64 = 0.75;

// Signals over which the anchor's phase is refined before it freezes. The
// first reference instant carries the same noise as every later one, and an
// anchor taken from it alone would eat that much of the tolerance on every
// comparison; averaging the phase over this many on-cadence signals leaves
// a residual of noise / sqrt(WARMUP_SIGNALS) instead.
const WARMUP_SIGNALS: u32 = 8;

// Refresh rate assumed before the first set_hz call.
const DEFAULT_HZ: f64 = 60.0;

/// Counts the display refreshes each frame signal covers (see the module
/// comment). Owned by the main loop, which is the one caller; reference
/// instants are milliseconds on the caller's own monotonic origin.
pub struct RefreshCounter {
  hz: f64,
  // Position, in ms, that `counted` refreshes precede the last reference:
  // the measured refresh count of a reference is (reference - anchor) /
  // period. None before the first signal.
  anchor_ms: Option<f64>,
  // Reference instant of the last signal, for re-anchoring on a rate change.
  last_ms: f64,
  // Refreshes reported since the anchor.
  counted: u64,
  // On-cadence signals that have refined the anchor so far (see WARMUP_SIGNALS).
  warmup: u32,
}

impl RefreshCounter {
  pub fn new() -> Self {
    RefreshCounter { hz: DEFAULT_HZ, anchor_ms: None, last_ms: 0.0, counted: 0, warmup: 0 }
  }

  /// Update the refresh rate the count is measured against; true when it
  /// changed. Ignored (false) if not positive, so a bogus report cannot
  /// stall the count, or if unchanged, so the caller can feed it every
  /// iteration. A change re-anchors so the measurement equals the count at
  /// the last signal: under two periods of drift are dropped once per mode
  /// change instead of being re-measured in the wrong unit.
  pub fn set_hz(&mut self, hz: f32) -> bool {
    let hz = hz as f64;
    if hz <= 0.0 || hz == self.hz {
      return false;
    }
    self.hz = hz;
    if self.anchor_ms.is_some() {
      self.anchor_ms = Some(self.last_ms - self.counted as f64 * self.period_ms());
      self.warmup = 0;
    }
    true
  }

  /// The refresh period the count is measured against.
  pub fn period_ms(&self) -> f64 {
    1000.0 / self.hz
  }

  /// Count the refreshes one frame signal covers, given its reference
  /// instant (ms on the caller's monotonic origin). The first signal counts
  /// as one and anchors the measurement one period behind itself.
  pub fn on_signal(&mut self, reference_ms: f64) -> u32 {
    let period = self.period_ms();
    let Some(anchor) = self.anchor_ms else {
      self.anchor_ms = Some(reference_ms - period);
      self.last_ms = reference_ms;
      self.counted = 1;
      return 1;
    };
    self.last_ms = reference_ms;
    let measured = (reference_ms - anchor) / period;
    // The drift the count would have if this signal counts as one refresh.
    let after = measured - self.counted as f64 - 1.0;
    let n = if after > TOLERANCE {
      1 + (after - TOLERANCE).ceil() as u64
    } else if after < -TOLERANCE {
      0
    } else {
      1
    };
    self.counted += n;
    if n == 1 && self.warmup < WARMUP_SIGNALS {
      // A running mean of the phase error over the warm-up signals: the
      // anchor moves toward the grid the references actually sit on.
      self.warmup += 1;
      self.anchor_ms = Some(anchor + after * period / self.warmup as f64);
    }
    n as u32
  }
}

/// One frame signal as the main loop counted it. `presented` is false for an
/// idle Tick; `demanded` is the frame-request latch as the raster thread
/// sampled it at present time (false for a Tick), which is what separates a
/// miss from an idle gap; `missed` is the misses this signal closed (see
/// `RefreshCounting::count`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignalRecord {
  pub frame: u64,
  pub reference_ms: f64,
  pub refreshes: u32,
  pub presented: bool,
  pub demanded: bool,
  pub missed: u32,
}

/// What one frame signal counted: the refreshes it covered, and the presents
/// the display missed in the interval it closed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Counted {
  pub refreshes: u32,
  pub missed: u32,
}

// Frame signals the ledger keeps; enough for a few seconds at any refresh
// rate a stats window would ask about.
const LEDGER_CAPACITY: usize = 512;

/// The last LEDGER_CAPACITY frame signals with their refresh counts: the
/// cadence an app actually achieved, as a readable fact rather than an
/// inference from rates. Written by the main loop beside the counter; the
/// stats readers and the missed-present accounting move onto it in the
/// second stage of okf/plans/frame-signal-refresh-count.md.
pub struct SignalLedger {
  ring: Vec<SignalRecord>,
  next: usize,
}

impl SignalLedger {
  pub fn new() -> Self {
    SignalLedger { ring: Vec::with_capacity(LEDGER_CAPACITY), next: 0 }
  }

  pub fn push(&mut self, record: SignalRecord) {
    if self.ring.len() < LEDGER_CAPACITY {
      self.ring.push(record);
    } else {
      self.ring[self.next] = record;
    }
    self.next = (self.next + 1) % LEDGER_CAPACITY;
  }

  /// The kept records, oldest first.
  pub fn records(&self) -> impl Iterator<Item = &SignalRecord> {
    let split = if self.ring.len() < LEDGER_CAPACITY { 0 } else { self.next };
    self.ring[split..].iter().chain(self.ring[..split].iter())
  }
}

/// Refresh-count tallies over a stretch of frame signals, for the main
/// loop's 1/s diagnostics line: at full rate every signal counts one (zero
/// and multi both 0); below it, multi grows and max shows the cadence; a
/// zero is a duplicate or stray signal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CountTally {
  pub signals: u32,
  pub refreshes: u32,
  pub zero: u32,
  pub multi: u32,
  pub max: u32,
}

/// The main loop's refresh counting in one place: the counter, the ledger it
/// records into, the running tally, and the missed-present accounting the
/// count makes possible (see `count`).
pub struct RefreshCounting {
  counter: RefreshCounter,
  ledger: SignalLedger,
  tally: CountTally,
  // Refreshes counted by idle Ticks since the last present: a present's
  // interval is measured from the previous present, and Ticks in between
  // (a JS-bound app leaves the loop idle in its own terms) split it.
  since_present: u32,
  // Whether the previous present left the swap with a next frame already
  // demanded: only then is the interval it opens a demanded one, and only a
  // demanded interval can miss. False across an idle stretch, a non-drawn
  // present and a refresh-rate change, so the interval that follows any of
  // those is not judged.
  prev_present_demanded: bool,
}

impl RefreshCounting {
  pub fn new() -> Self {
    RefreshCounting {
      counter: RefreshCounter::new(),
      ledger: SignalLedger::new(),
      tally: CountTally::default(),
      since_present: 0,
      prev_present_demanded: false,
    }
  }

  /// A refresh-rate change re-anchors the counter and drops the interval in
  /// flight: it would mix two periods. Safe to feed every iteration; an
  /// unchanged rate is a no-op.
  pub fn set_hz(&mut self, hz: f32) {
    if self.counter.set_hz(hz) {
      self.prev_present_demanded = false;
    }
  }

  /// Count one frame signal's refreshes from its reference instant and
  /// record it; `presented` / `demanded` distinguish a present from a Tick.
  /// A present also closes an interval: the refreshes since the previous
  /// present, Ticks included. When that previous present left the swap with
  /// a next frame demanded, every refresh of the interval beyond the first
  /// is a present the display expected and did not get - jank as a count,
  /// not a rate - and is reported as `missed`. An interval opened by an
  /// idle present is idle, not jank, whatever its length.
  pub fn count(&mut self, reference_ms: f64, frame: u64, presented: bool, demanded: bool) -> Counted {
    let refreshes = self.counter.on_signal(reference_ms);
    let mut missed = 0;
    if presented {
      let interval = self.since_present + refreshes;
      if self.prev_present_demanded {
        missed = interval.saturating_sub(1);
      }
      self.since_present = 0;
      self.prev_present_demanded = demanded;
    } else {
      self.since_present += refreshes;
    }
    self.ledger.push(SignalRecord { frame, reference_ms, refreshes, presented, demanded, missed });
    self.tally.signals += 1;
    self.tally.refreshes += refreshes;
    self.tally.zero += (refreshes == 0) as u32;
    self.tally.multi += (refreshes > 1) as u32;
    self.tally.max = self.tally.max.max(refreshes);
    Counted { refreshes, missed }
  }

  /// The tally since the last take, reset to zero.
  pub fn take_tally(&mut self) -> CountTally {
    std::mem::take(&mut self.tally)
  }

  pub fn ledger(&self) -> &SignalLedger {
    &self.ledger
  }
}
