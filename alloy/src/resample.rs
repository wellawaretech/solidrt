use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{AlloyEvent, Modifiers, PointerType};

/// Pointer-move resampling against the frame clock: ALL pointer types buffer
/// their moves here, each sample with the time the platform gave it, and
/// dispatch one position per pointer per frame, so a frame's moves are one
/// batch (the runtime emits the "pointerFrame" terminator after it). Every
/// dispatched move carries the time of the position it carries, which is
/// what its `timeStamp` comes from: a lift is judged against when the
/// pointer was last really somewhere else, whatever the frame rate.
///
/// Touch is resampled by time, as Android and Flutter do. A frame's
/// position is the one at the frame's reference instant minus
/// RESAMPLE_LATENCY:
/// - samples on both sides of that instant: interpolated, at that instant.
/// - fresh samples, all older: the newest, at its own time.
/// - nothing fresh and the newest sample no older than PREDICT_MAX_PERIODS:
///   the delivery is late, not the finger stopped (Android batches touch to
///   the display's vsync, and a batch can miss the frame signal it belongs
///   to: nothing at one frame, two batches at the next, about five times a
///   second on a device). Under newest-wins dispatch that renders as a
///   one-frame stall and a double step, so the gap is bridged with a
///   predicted position: the step ahead mirrors the pointer's own travel
///   over the same span behind its newest sample.
/// - the frame after a predicted one, still nothing fresh: the finger
///   stopped. The newest real position goes out once, at its own time (so
///   earlier than the predicted move's), and the pointer is silent until
///   new input.
///
/// Mouse and pen are not resampled: desktop delivery is not batched to the
/// frame, and a predicted step would fake an overshoot every time the
/// device stops. They dispatch the newest buffered sample per frame, which
/// also bounds a high-polling-rate mouse to one hit test and JS dispatch
/// per frame. So does every pointer when the frame has no time (headless).
///
/// Moves feed the history on arrival and dispatch only through sample();
/// down/up stay on arrival (ordering), with down re-seeding the history so a
/// buffered pre-down move collapses into the down instead of dispatching
/// stale after it, and up flushing it: samples that no frame dispatched
/// yet, or the real position behind a predicted one, go out as one move
/// ahead of the up, so the lift is read against the pointer's last real
/// position.
pub struct Resampler {
  pointers: HashMap<(PointerType, u64), History>,
}

// How far behind a frame's reference instant touch is resampled. Android's
// own resampler places a batch's last sample this far behind the frame time
// (its RESAMPLE_LATENCY), so under vsync-locked pacing the target sits on
// the newest sample and nothing is predicted in steady state.
const RESAMPLE_LATENCY: Duration = Duration::from_millis(5);

// A newest sample less than this past the target is the target's own
// sample and is dispatched as it is (Android's RESAMPLE_MIN_DELTA). The
// target and the sample it sits on come from two readings of one vsync,
// and sample times are whole milliseconds before Android 14: without the
// margin a hair of the newest sample would stay fresh and the next frame
// would dispatch it in place of bridging a late delivery.
const RESAMPLE_MIN_AHEAD: Duration = Duration::from_millis(2);

// Oldest newest-sample, in refresh periods behind the target, that still
// reads as a late delivery and is bridged with a predicted step. A late
// batch is one period behind; the rest is room for a platform that places
// its samples further behind the frame than RESAMPLE_LATENCY, and for the
// frame signal's jitter. Past it the pointer stopped.
const PREDICT_MAX_PERIODS: f64 = 1.5;

// Span of samples kept per pointer: what interpolation and the mirrored
// prediction step reach back over, with room for a slow display.
const HISTORY_SPAN: Duration = Duration::from_millis(100);

struct History {
  // Real samples, oldest first, their times not decreasing. Never empty.
  samples: VecDeque<(Instant, f32, f32)>,
  modifiers: Modifiers,
  // Time of the last real position dispatched (a down counts); samples
  // after it are fresh. None until something is.
  emitted: Option<Instant>,
  // The last dispatched position was predicted, so a stop must settle back
  // to the newest real one.
  predicted: bool,
  // Movement since the last dispatch. Hardware deltas (mouse xrel/yrel) sum
  // here on push - positions collapse to one per frame, deltas must
  // accumulate or fast motion silently loses distance - and survive the
  // down() re-seed (the motion happened; a click mid-flick must not eat it).
  rel: (f32, f32),
  // This pointer reports hardware deltas (sticky from the first push that
  // carries one). Pointers without them (touch, synthetic) report movement
  // as the dispatched-position diff instead, so movement mirrors position
  // exactly - including the prediction bounce.
  hw: bool,
  // The last dispatched position, the baseline for derived movement. Seeded
  // by down() at the contact so the first move's movement is contact-based.
  dispatched: Option<(f32, f32)>,
}

impl History {
  // Fresh tracking state at a position: one undispatched sample, no
  // movement baseline. Callers adjust the fields their entry point implies.
  fn new(x: f32, y: f32, modifiers: Modifiers, at: Instant) -> History {
    History {
      samples: VecDeque::from([(at, x, y)]),
      modifiers,
      emitted: None,
      predicted: false,
      rel: (0.0, 0.0),
      hw: false,
      dispatched: None,
    }
  }

  // A down re-seeds the history at the contact. Position tracking restarts
  // (nothing buffered, movement baselined at the contact); accumulated
  // relative motion and the hw fact persist - the motion physically
  // happened, and a click mid-flick must not shorten a mouse-look turn. A
  // new field must decide here which side it is on.
  fn re_seed(&mut self, x: f32, y: f32, modifiers: Modifiers, at: Instant) {
    self.samples.clear();
    self.samples.push_back((at, x, y));
    self.modifiers = modifiers;
    self.emitted = Some(at);
    self.predicted = false;
    self.dispatched = Some((x, y));
  }

  fn newest(&self) -> (Instant, f32, f32) {
    *self.samples.back().expect("a history holds at least one sample")
  }

  // Samples arrived that no dispatch has covered yet.
  fn fresh(&self) -> bool {
    self.emitted.is_none_or(|emitted| self.newest().0 > emitted)
  }

  // The pointer's position at `at` on the line through its samples, held at
  // the ends.
  fn position_at(&self, at: Instant) -> (f32, f32) {
    let after = self.samples.partition_point(|s| s.0 <= at);
    if after == 0 {
      let (_, x, y) = self.samples[0];
      return (x, y);
    }
    let (t0, x0, y0) = self.samples[after - 1];
    let Some(&(t1, x1, y1)) = self.samples.get(after) else {
      return (x0, y0);
    };
    let f = (at - t0).as_secs_f32() / (t1 - t0).as_secs_f32();
    (x0 + (x1 - x0) * f, y0 + (y1 - y0) * f)
  }

  // The real position a lift or a stop leaves the pointer at, when the last
  // dispatch did not already say so: undispatched samples, or a predicted
  // step to take back.
  fn settle(&mut self) -> Option<(f32, f32, Instant)> {
    if !self.fresh() && !self.predicted {
      return None;
    }
    let (at, x, y) = self.newest();
    Some((x, y, at))
  }

  // This frame's position and its time (see Resampler). `target` is the
  // instant touch is resampled to and the refresh period, None where the
  // pointer or the frame takes the newest sample as it is.
  fn resample(&mut self, target: Option<(Instant, Duration)>) -> Option<(f32, f32, Instant)> {
    let (newest_at, newest_x, newest_y) = self.newest();
    if self.fresh() {
      self.predicted = false;
      // Interpolate when the target sits inside the samples and ahead of
      // what was dispatched; a fresh sample is never held back otherwise.
      let inside = target.filter(|(at, _)| {
        *at + RESAMPLE_MIN_AHEAD < newest_at && *at >= self.samples[0].0 && self.emitted.is_none_or(|emitted| *at > emitted)
      });
      return Some(match inside {
        Some((at, _)) => {
          let (x, y) = self.position_at(at);
          (x, y, at)
        }
        None => (newest_x, newest_y, newest_at),
      });
    }
    if self.predicted {
      let settled = self.settle();
      self.predicted = false;
      return settled;
    }
    let (at, period) = target?;
    let gap = at.checked_duration_since(newest_at)?;
    if gap.as_secs_f64() > period.as_secs_f64() * PREDICT_MAX_PERIODS {
      return None;
    }
    let (bx, by) = self.position_at(newest_at.checked_sub(gap)?);
    let (x, y) = (newest_x + (newest_x - bx), newest_y + (newest_y - by));
    // A pointer at rest predicts its own position: nothing to say.
    if (x, y) == (newest_x, newest_y) {
      return None;
    }
    self.predicted = true;
    Some((x, y, at))
  }

  // Record a dispatch of (x, y, at) and build its move. Movement: drain the
  // summed hardware deltas when the pointer has them (positions freeze in
  // relative mouse mode, so the position diff would read 0); otherwise diff
  // against the last dispatched position, so derived movement bounces
  // exactly when positions do.
  fn dispatch(&mut self, key: (PointerType, u64), (x, y, at): (f32, f32, Instant)) -> Sample {
    let (dx, dy) = if self.hw {
      std::mem::take(&mut self.rel)
    } else {
      let (bx, by) = self.dispatched.unwrap_or((x, y));
      (x - bx, y - by)
    };
    self.dispatched = Some((x, y));
    if !self.predicted {
      self.emitted = Some(self.emitted.map_or(at, |emitted| emitted.max(at)));
    }
    Sample { pointer_type: key.0, pointer_id: key.1, x, y, dx, dy, modifiers: self.modifiers, at, predicted: self.predicted }
  }
}

/// One resampled move to dispatch. `dx`/`dy` is the movement since the
/// previous dispatch: summed hardware deltas for pointers that report them
/// (the only motion signal in relative mouse mode, where positions freeze),
/// the dispatched-position diff otherwise. `at` is the time of the position:
/// the resample instant, or the sample's own. `predicted` marks a position
/// the pointer was never reported at: the step that bridges a late delivery.
pub struct Sample {
  pub pointer_type: PointerType,
  pub pointer_id: u64,
  pub x: f32,
  pub y: f32,
  pub dx: f32,
  pub dy: f32,
  pub modifiers: Modifiers,
  pub at: Instant,
  pub predicted: bool,
}

impl Resampler {
  pub fn new() -> Resampler {
    Resampler { pointers: HashMap::new() }
  }

  /// A pointer went down: seed its history at the contact position. The down
  /// event itself dispatches on arrival; sample() never re-emits it.
  pub fn down(&mut self, key: (PointerType, u64), x: f32, y: f32, modifiers: Modifiers, at: Instant) {
    match self.pointers.get_mut(&key) {
      Some(h) => h.re_seed(x, y, modifiers, at),
      None => {
        let mut h = History::new(x, y, modifiers, at);
        h.emitted = Some(at);
        h.dispatched = Some((x, y));
        self.pointers.insert(key, h);
      }
    }
  }

  /// A move arrived: record it for the next sample() call. `at` is when the
  /// pointer was there. `rel` is the hardware motion delta when the device
  /// reports one; it sums into the history (positions collapse to one per
  /// frame, deltas must accumulate).
  pub fn push(&mut self, key: (PointerType, u64), x: f32, y: f32, rel: Option<(f32, f32)>, modifiers: Modifiers, at: Instant) {
    let Some(h) = self.pointers.get_mut(&key) else {
      // Move without a down (a mouse hovering, a missed down across an
      // engine swap): track from here.
      let mut h = History::new(x, y, modifiers, at);
      if let Some(rel) = rel {
        h.rel = rel;
        h.hw = true;
      }
      self.pointers.insert(key, h);
      return;
    };
    h.modifiers = modifiers;
    if let Some((dx, dy)) = rel {
      h.rel.0 += dx;
      h.rel.1 += dy;
      h.hw = true;
    }
    let (newest_at, newest_x, newest_y) = h.newest();
    // The pointer is where it was (a resting finger beside a moving one
    // reports with every batch): its position keeps the time it got there.
    // Hardware deltas are motion of their own (relative mouse mode freezes
    // the position), so a sample that carries one always counts.
    if (x, y) == (newest_x, newest_y) && rel.is_none() {
      return;
    }
    // Times never go back within a pointer; two samples of one instant
    // keep the later position.
    let at = at.max(newest_at);
    if at == newest_at && h.samples.len() > 1 {
      h.samples.pop_back();
    }
    h.samples.push_back((at, x, y));
    while h.samples.len() > 2 && at.duration_since(h.samples[1].0) > HISTORY_SPAN {
      h.samples.pop_front();
    }
  }

  /// The pointer ended: its history goes, and what no frame dispatched of
  /// it comes back as one last move, to dispatch ahead of the up (which
  /// carries the final position itself, on arrival).
  pub fn up(&mut self, key: (PointerType, u64)) -> Option<Sample> {
    let mut h = self.pointers.remove(&key)?;
    let settled = h.settle()?;
    h.predicted = false;
    Some(h.dispatch(key, settled))
  }

  pub fn clear(&mut self) {
    self.pointers.clear();
  }

  /// The moves to dispatch for one frame. `frame` is the frame signal's
  /// reference instant and the refresh period, which touch is resampled
  /// against; None for a frame with no time of its own (headless), where
  /// every pointer dispatches its newest sample.
  pub fn sample(&mut self, frame: Option<(Instant, Duration)>) -> Vec<Sample> {
    let mut out = Vec::new();
    for (&key, h) in self.pointers.iter_mut() {
      // Touch only: mouse and pen take the newest sample (see Resampler).
      let target = frame
        .filter(|_| key.0 == PointerType::Touch)
        .and_then(|(reference, period)| Some((reference.checked_sub(RESAMPLE_LATENCY)?, period)));
      if let Some(position) = h.resample(target) {
        out.push(h.dispatch(key, position));
      }
    }
    out
  }
}

/// Shared handle onto the process's one Resampler. Feeding is a
/// producer-side duty: whoever emits pointer events feeds the histories at
/// emission - the platform loop for real input, synthetic-input producers at
/// their own send sites - through `feed`. The single UI consumer samples per
/// frame signal and clears across engine swaps. Cheap to clone.
#[derive(Clone)]
pub struct SharedResampler(Arc<Mutex<Resampler>>);

impl SharedResampler {
  pub fn new() -> SharedResampler {
    SharedResampler(Arc::new(Mutex::new(Resampler::new())))
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Resampler> {
    self.0.lock().expect("resampler lock poisoned")
  }

  pub fn clear(&self) {
    self.lock().clear()
  }

  pub fn sample(&self, frame: Option<(Instant, Duration)>) -> Vec<Sample> {
    self.lock().sample(frame)
  }

  /// The producer-side duty as one call: pass an event that happened at
  /// `at` through the histories on its way to the UI loop. `send` gets what
  /// travels. A move does not: it is consumed here, and the frame consumer
  /// samples and dispatches it. A down seeds its pointer's history and
  /// travels. An up ends it and travels behind the pointer's last move when
  /// no frame dispatched that yet (see `Resampler::up`). Everything else
  /// passes through. The result is `send`'s, Ok when nothing was sent.
  pub fn feed<E>(&self, event: AlloyEvent, at: Instant, mut send: impl FnMut(AlloyEvent, Instant) -> Result<(), E>) -> Result<(), E> {
    match &event {
      AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, rel, modifiers } => {
        self.lock().push((*pointer_type, *pointer_id), *x, *y, *rel, *modifiers, at);
        return Ok(());
      }
      AlloyEvent::PointerDown { pointer_id, pointer_type, x, y, modifiers, .. } => {
        self.lock().down((*pointer_type, *pointer_id), *x, *y, *modifiers, at);
      }
      AlloyEvent::PointerUp { pointer_id, pointer_type, .. } => {
        let last = self.lock().up((*pointer_type, *pointer_id));
        if let Some(m) = last {
          let flush = AlloyEvent::PointerMove {
            pointer_id: m.pointer_id,
            pointer_type: m.pointer_type,
            x: m.x,
            y: m.y,
            rel: Some((m.dx, m.dy)),
            modifiers: m.modifiers,
          };
          // A sample can sit a hair after the up that ends it (two clocks
          // on Android); the move never follows its up in time.
          send(flush, m.at.min(at))?;
        }
      }
      _ => {}
    }
    send(event, at)
  }
}
