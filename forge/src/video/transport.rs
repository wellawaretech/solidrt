//! Transport, shared by every player: the clock that maps content time to
//! system time, the command channel a player is driven through, the state it
//! publishes back, and the policy for releasing a frame against that clock.
//! Engine-free and sink-agnostic: the plane player feeds the due time to
//! `releaseOutputBufferAtTime`, and the texture player adopts the same
//! anchor when its frame selection moves off the frame loop
//! (okf/plans/android-video-punch-through.md). Building the anchor inside one
//! player would have the other duplicate it, which is why it lives here.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

// How long before its release time a frame is handed to the surface when
// the display's grid is unknown: a fallback (ExoPlayer's 50 ms), see
// RELEASE_LEAD_PERIODS for the rule.
pub const RELEASE_LEAD_NS: i64 = 50_000_000;
// The release lead when the vsync grid is known, in refresh periods. One is
// enough and nothing is gained by more: the compositor's arrival deadline
// for a target vsync is one period minus its own phase offset before that
// vsync (0.94 of a period on the devices measured), so a hand-over one
// period ahead clears it. Handing over earlier is harmless rather than
// better - a transaction whose desired time has not come is queued and
// re-examined at every following compositor wake - which is why 50 ms and
// one period measure the same (okf/plans/android-video-punch-through.md).
pub const RELEASE_LEAD_PERIODS: i64 = 1;
// A frame later than this is dropped rather than shown: decode fell behind
// and skipping is how it catches up. One and a half 50 Hz periods.
pub const DROP_LATE_NS: i64 = 30_000_000;
// A frame later than THIS re-anchors the clock instead: the source or the
// decoder stalled (a rebuffer, a slow seek), and playing the backlog as a
// burst of drops would skip content. Continuing from here is what the eye
// wants after a stall.
pub const STALL_REANCHOR_NS: i64 = 250_000_000;
// How far before its vsync a snapped release time lands, in percent of the
// refresh period. The compositor latches a buffer whose desired present
// time is at or before the vsync it composes for, so a time just past a
// vsync slips a whole period; pulling it most of a period back keeps it
// on the intended vsync whatever the phase offsets are (ExoPlayer's 80%).
pub const VSYNC_OFFSET_PERCENT: i64 = 80;

/// The system clock frames are scheduled on: CLOCK_MONOTONIC in nanoseconds
/// on unix (Android's `System.nanoTime`, which
/// `releaseOutputBufferAtTime` takes), a process-relative monotonic reading
/// elsewhere (only the differences matter off Android).
pub fn monotonic_ns() -> i64 {
  #[cfg(unix)]
  {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64
  }
  #[cfg(not(unix))]
  {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as i64
  }
}

/// What a caller asks of a player. The worker owning the decoder drains
/// these between frames; `Close` (also sent by dropping the controls) ends
/// the worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
  Play,
  Pause,
  /// Seek to a content time in microseconds.
  Seek(i64),
  Close,
}

/// State a player publishes for its caller to read on any thread: written
/// by the worker as things happen, read without a round trip.
#[derive(Default)]
pub struct Shared {
  position_us: AtomicI64,
  playing: AtomicBool,
  finished: AtomicBool,
}

impl Shared {
  /// Presentation time of the last frame released, in microseconds.
  pub fn position_us(&self) -> i64 {
    self.position_us.load(Ordering::Relaxed)
  }
  pub fn set_position_us(&self, pts_us: i64) {
    self.position_us.store(pts_us, Ordering::Relaxed);
  }
  pub fn playing(&self) -> bool {
    self.playing.load(Ordering::Relaxed)
  }
  pub fn set_playing(&self, playing: bool) {
    self.playing.store(playing, Ordering::Relaxed);
  }
  /// Whether the stream's last frame has been released (a seek clears it).
  pub fn finished(&self) -> bool {
    self.finished.load(Ordering::Relaxed)
  }
  pub fn set_finished(&self, finished: bool) {
    self.finished.store(finished, Ordering::Relaxed);
  }
}

/// The caller's end of a player: commands in, published state out.
pub struct Controls {
  tx: Sender<Command>,
  shared: Arc<Shared>,
}

impl Controls {
  /// A fresh command channel and state pair; the worker takes the receiver
  /// and a clone of the state.
  pub fn new() -> (Controls, Receiver<Command>, Arc<Shared>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let shared = Arc::new(Shared::default());
    (Controls { tx, shared: shared.clone() }, rx, shared)
  }

  pub fn play(&self) {
    self.send(Command::Play);
  }

  pub fn pause(&self) {
    self.send(Command::Pause);
  }

  pub fn seek(&self, target_us: i64) {
    self.send(Command::Seek(target_us));
  }

  pub fn close(&self) {
    self.send(Command::Close);
  }

  pub fn playing(&self) -> bool {
    self.shared.playing()
  }

  pub fn position_us(&self) -> i64 {
    self.shared.position_us()
  }

  pub fn finished(&self) -> bool {
    self.shared.finished()
  }

  // A worker that already exited has dropped the receiver; a late command
  // is then a no-op, not an error.
  fn send(&self, cmd: Command) {
    self.tx.send(cmd).ok();
  }
}

/// The content-to-system clock: one anchor pair `(system ns, content us)`
/// set by the first frame after a reset, so a stream starts wherever it
/// starts (never assumed at zero). Reset on play, resume, seek and stall.
#[derive(Debug, Default)]
pub struct Anchor {
  origin: Option<(i64, i64)>,
}

impl Anchor {
  pub fn new() -> Anchor {
    Anchor { origin: None }
  }

  /// Forget the anchor: the next frame is due immediately and anchors the
  /// clock.
  pub fn reset(&mut self) {
    self.origin = None;
  }

  pub fn anchored(&self) -> bool {
    self.origin.is_some()
  }

  /// The system time a frame at `pts_us` is due, anchoring on it (at
  /// `now_ns`) when nothing is anchored yet.
  pub fn due_ns(&mut self, pts_us: i64, now_ns: i64) -> i64 {
    let (origin_ns, origin_pts_us) = *self.origin.get_or_insert((now_ns, pts_us));
    origin_ns + (pts_us - origin_pts_us) * 1000
  }
}

/// The display's vsync grid, for snapping release times onto it: the
/// refresh period and a sampler returning the latest vsync time observed on
/// the release clock (the plane's owner samples the platform's frame
/// callback while a plane is attached). Fresh samples keep the
/// extrapolation to a couple of periods, so a rounded period does not
/// drift; None from the sampler means no vsync seen yet, and release times
/// go through unsnapped.
pub struct VsyncGrid {
  pub period_ns: i64,
  pub sample_ns: Box<dyn Fn() -> Option<i64> + Send>,
}

impl VsyncGrid {
  /// How long before its release time a frame is handed to the surface on
  /// this grid.
  pub fn lead_ns(&self) -> i64 {
    self.period_ns * RELEASE_LEAD_PERIODS
  }

  /// The release time for a frame due at `due_ns`: its closest vsync,
  /// minus the offset.
  pub fn snap(&self, due_ns: i64) -> i64 {
    match (self.sample_ns)() {
      Some(vsync_ns) if self.period_ns > 0 => snap_to_vsync(due_ns, vsync_ns, self.period_ns),
      _ => due_ns,
    }
  }
}

/// `due_ns` moved to the vsync closest to it on the grid through
/// `vsync_ns` with spacing `period_ns`, minus `VSYNC_OFFSET_PERCENT` of a
/// period.
pub fn snap_to_vsync(due_ns: i64, vsync_ns: i64, period_ns: i64) -> i64 {
  let periods = (due_ns - vsync_ns).div_euclid(period_ns);
  let before = vsync_ns + periods * period_ns;
  let after = before + period_ns;
  let closest = if after - due_ns < due_ns - before { after } else { before };
  closest - period_ns * VSYNC_OFFSET_PERCENT / 100
}

/// What to do with a frame, given how early (positive) or late (negative)
/// its due time is in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
  /// Sleep this long, then release at the due time.
  Wait(i64),
  /// Release at the due time now (it is within the lead, or a little late).
  AtTime,
  /// Late enough to drop: decode is behind, skip to catch up.
  Drop,
  /// So late the clock must have stalled: re-anchor on this frame and show
  /// it now.
  Reanchor,
}

/// The release policy (ExoPlayer's shape, one threshold each), with the
/// lead the caller settled on (`lead_ns()` of a grid, else `RELEASE_LEAD_NS`).
pub fn classify(early_ns: i64, lead_ns: i64) -> Release {
  if early_ns > lead_ns {
    Release::Wait(early_ns - lead_ns)
  } else if early_ns >= -DROP_LATE_NS {
    Release::AtTime
  } else if early_ns >= -STALL_REANCHOR_NS {
    Release::Drop
  } else {
    Release::Reanchor
  }
}
