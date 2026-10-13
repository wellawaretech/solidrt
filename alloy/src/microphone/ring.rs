//! A microphone session's one output: a bounded window of samples the
//! producer appends to (SDL's audio thread for a device, the test verb for
//! an installed clip) and every reader drains by a cursor of its own - the
//! session's own reader behind JS `read()`, a module's worker (a speech
//! recognizer, say). Audio cannot be latest-wins like a camera frame, since a
//! consumer needs every sample, so the window keeps `RING_SECONDS` of audio:
//! a reader further behind loses the oldest samples and is told how many,
//! and one that polls at all never gets near the edge. Closing the session
//! wakes every waiter, so a consumer's thread ends with the microphone.

use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Seconds of audio a session keeps for its readers: how far a reader may
/// fall behind before it loses samples. 10 s is 640 KB at 16 kHz mono and
/// 1.9 MB at 48 kHz.
pub const RING_SECONDS: u32 = 10;

/// What a wait on the ring ended with.
pub enum MicWait {
  /// At least the asked-for samples are buffered past the reader's cursor
  /// (the count buffered).
  Samples(usize),
  Timeout,
  /// The session closed; no sample will follow.
  Closed,
}

/// What one read drained.
pub struct MicRead {
  /// Position of the first sample read, in samples since the open.
  pub position: u64,
  /// Samples the reader lost to the window before this read: 0 when it
  /// kept up.
  pub dropped: u64,
  /// Samples appended to the destination.
  pub count: usize,
}

struct RingState {
  /// The window, one `RING_SECONDS` of samples; sample `p` sits at
  /// `p % len`.
  buf: Vec<f32>,
  /// Where a producer writes before the append, kept so a push allocates
  /// nothing in steady state.
  scratch: Vec<f32>,
  /// Samples produced since the open: the next sample's position.
  produced: u64,
  closed: bool,
}

impl RingState {
  /// The oldest position still in the window.
  fn oldest(&self) -> u64 {
    self.produced.saturating_sub(self.buf.len() as u64)
  }

  /// Samples a reader at `cursor` can still read.
  fn readable(&self, cursor: u64) -> usize {
    (self.produced - cursor.max(self.oldest())) as usize
  }

  /// Append samples at the production position. A run longer than the
  /// window keeps its tail, since the head is already lost.
  fn append(&mut self, samples: &[f32]) {
    let cap = self.buf.len();
    let skip = samples.len().saturating_sub(cap);
    let keep = &samples[skip..];
    let at = ((self.produced + skip as u64) % cap as u64) as usize;
    let first = (cap - at).min(keep.len());
    self.buf[at..at + first].copy_from_slice(&keep[..first]);
    self.buf[..keep.len() - first].copy_from_slice(&keep[first..]);
    self.produced += samples.len() as u64;
  }
}

struct RingShared {
  state: Mutex<RingState>,
  changed: Condvar,
  sample_rate: u32,
  /// The open on the SDL tick timeline.
  opened_ns: u64,
}

impl RingShared {
  fn lock(&self) -> MutexGuard<'_, RingState> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }
}

/// The session's end of the ring: the producer pushes, the close closes,
/// readers are made from it. Cloning shares the ring.
#[derive(Clone)]
pub(crate) struct SampleRing(Arc<RingShared>);

impl SampleRing {
  /// A ring for `sample_rate` (non-zero) opened at `opened_ns` on the SDL
  /// tick timeline.
  pub(crate) fn new(sample_rate: u32, opened_ns: u64) -> SampleRing {
    let capacity = sample_rate as usize * RING_SECONDS as usize;
    SampleRing(Arc::new(RingShared {
      state: Mutex::new(RingState { buf: vec![0.0; capacity], scratch: Vec::new(), produced: 0, closed: false }),
      changed: Condvar::new(),
      sample_rate,
      opened_ns,
    }))
  }

  /// Append samples and wake every waiter. A closed ring drops them.
  pub(crate) fn push(&self, samples: &[f32]) {
    let mut state = self.0.lock();
    if state.closed || samples.is_empty() {
      return;
    }
    state.append(samples);
    self.0.changed.notify_all();
  }

  /// Append up to `count` samples a producer writes into the ring's own
  /// scratch under the lock: `fill` gets a slice of `count` and returns how
  /// many it wrote. For a producer that copies out of a source of its own
  /// (SDL's stream) without a buffer of its own.
  pub(crate) fn push_with(&self, count: usize, fill: impl FnOnce(&mut [f32]) -> usize) {
    let mut state = self.0.lock();
    if state.closed || count == 0 {
      return;
    }
    let mut scratch = std::mem::take(&mut state.scratch);
    if scratch.len() < count {
      scratch.resize(count, 0.0);
    }
    let written = fill(&mut scratch[..count]).min(count);
    state.append(&scratch[..written]);
    state.scratch = scratch;
    if written > 0 {
      self.0.changed.notify_all();
    }
  }

  /// Mark the session closed: wake every waiter with `Closed`, refuse
  /// further pushes and reads. The window is dropped: a close is a stop.
  pub(crate) fn close(&self) {
    let mut state = self.0.lock();
    state.closed = true;
    state.buf = Vec::new();
    state.scratch = Vec::new();
    self.0.changed.notify_all();
  }

  /// A reader hearing from now on: its cursor starts at the production
  /// position.
  pub(crate) fn reader(&self) -> MicrophoneTap {
    let cursor = self.0.lock().produced;
    MicrophoneTap { ring: self.0.clone(), cursor }
  }

  /// This clone as a pointer for a C callback's userdata. The clone lives
  /// until `from_raw` takes it back.
  pub(crate) fn into_raw(self) -> *mut c_void {
    Arc::into_raw(self.0) as *mut c_void
  }

  /// The clone behind a pointer from `into_raw`. Dropping the result releases
  /// it; a borrow that must not (a callback) wraps it in `ManuallyDrop`.
  ///
  /// # Safety
  /// `ptr` came from `into_raw` and has not been taken back.
  pub(crate) unsafe fn from_raw(ptr: *mut c_void) -> SampleRing {
    SampleRing(unsafe { Arc::from_raw(ptr as *const RingShared) })
  }
}

/// A reader on a session's ring, usable from any thread; its cursor is its
/// own, so every reader hears every sample it keeps up with.
pub struct MicrophoneTap {
  ring: Arc<RingShared>,
  cursor: u64,
}

impl MicrophoneTap {
  /// Sample rate of the samples read, Hz (mono).
  pub fn sample_rate(&self) -> u32 {
    self.ring.sample_rate
  }

  /// The open on the SDL tick timeline (`CameraFrame::timestamp_ns` is on
  /// it): `position / sample_rate` past this is a sample's time.
  pub fn opened_ns(&self) -> u64 {
    self.ring.opened_ns
  }

  /// Samples buffered past the cursor, without waiting; 0 once closed.
  pub fn available(&self) -> usize {
    let state = self.ring.lock();
    if state.closed {
      return 0;
    }
    state.readable(self.cursor)
  }

  /// Whether the session closed.
  pub fn closed(&self) -> bool {
    self.ring.lock().closed
  }

  /// Block until `min_samples` are buffered past the cursor, the session
  /// closes, or `timeout` passes.
  pub fn wait(&self, min_samples: usize, timeout: Duration) -> MicWait {
    let deadline = Instant::now() + timeout;
    let mut state = self.ring.lock();
    loop {
      if state.closed {
        return MicWait::Closed;
      }
      let readable = state.readable(self.cursor);
      if readable >= min_samples {
        return MicWait::Samples(readable);
      }
      let now = Instant::now();
      if now >= deadline {
        return MicWait::Timeout;
      }
      state = self.ring.changed.wait_timeout(state, deadline - now).unwrap_or_else(PoisonError::into_inner).0;
    }
  }

  /// Drain everything buffered past the cursor into `dst` (appended). A
  /// cursor that fell behind the window is moved to the oldest kept sample
  /// first, and the read reports the samples lost. Err once closed.
  pub fn read_into(&mut self, dst: &mut Vec<f32>) -> Result<MicRead, String> {
    let state = self.ring.lock();
    if state.closed {
      return Err("microphone closed".to_string());
    }
    let oldest = state.oldest();
    let dropped = oldest.saturating_sub(self.cursor);
    let start = self.cursor.max(oldest);
    let count = (state.produced - start) as usize;
    let cap = state.buf.len();
    let at = (start % cap as u64) as usize;
    let first = (cap - at).min(count);
    dst.extend_from_slice(&state.buf[at..at + first]);
    dst.extend_from_slice(&state.buf[..count - first]);
    self.cursor = state.produced;
    Ok(MicRead { position: start, dropped, count })
  }
}
