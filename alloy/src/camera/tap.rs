//! The camera's one output: a latest-wins slot the camera thread fills and
//! every consumer reads from - the raster thread for the stream texture, the
//! QR decoder, a custom runtime module on a thread of its own. The slot holds
//! one frame; a consumer slower than the camera misses frames instead of
//! queueing them, and `seq` tells it how many. Closing the session marks the
//! slot closed and wakes every waiter, so a consumer's thread ends with the
//! camera.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// One upright camera frame: RGBA8, tightly packed rows top to bottom, the
/// layout `read_texture` and `barcode::scan_rgba` use. What the viewfinder
/// shows: rotated upright, not mirrored, at the open size.
pub struct CameraFrame {
  /// Increases by one per frame the camera produced; a gap is a dropped
  /// frame.
  pub seq: u64,
  /// SDL's capture timestamp, on the SDL tick timeline
  /// (`sdl_utils::ticks_epoch` places it as an `Instant`).
  pub timestamp_ns: u64,
  pub width: u32,
  pub height: u32,
  pub pixels: Vec<u8>,
}

/// What a wait on the tap ended with.
pub enum TapWait {
  Frame(Arc<CameraFrame>),
  Timeout,
  /// The session closed; no frame will follow.
  Closed,
}

/// Buffers kept for reuse: one for the frame the slot holds while the
/// producer fills the next, and one more so a consumer holding a frame
/// across a publish (an inference worker, every time) still costs no
/// allocation.
const SPARE_BUFFERS: usize = 2;

struct TapState {
  latest: Option<Arc<CameraFrame>>,
  /// Buffers of replaced frames no consumer still held, for the producer's
  /// next frames (at most `SPARE_BUFFERS`).
  spare: Vec<Vec<u8>>,
  next_seq: u64,
  closed: bool,
}

struct TapShared {
  state: Mutex<TapState>,
  changed: Condvar,
}

/// A handle on a camera session's frame slot, usable from any thread.
/// Cloning shares the slot.
#[derive(Clone)]
pub struct FrameTap(Arc<TapShared>);

impl FrameTap {
  pub(crate) fn new() -> FrameTap {
    FrameTap(Arc::new(TapShared {
      state: Mutex::new(TapState { latest: None, spare: Vec::new(), next_seq: 1, closed: false }),
      changed: Condvar::new(),
    }))
  }

  fn lock(&self) -> MutexGuard<'_, TapState> {
    self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// The newest frame, without waiting. None before the first frame and
  /// after the close.
  pub fn latest(&self) -> Option<Arc<CameraFrame>> {
    self.lock().latest.clone()
  }

  /// Block until a frame newer than `after` arrives, the session closes, or
  /// `timeout` passes. `after: 0` is any frame.
  pub fn wait_newer(&self, after: u64, timeout: Duration) -> TapWait {
    let deadline = Instant::now() + timeout;
    let mut state = self.lock();
    loop {
      if state.closed {
        return TapWait::Closed;
      }
      if let Some(frame) = &state.latest {
        if frame.seq > after {
          return TapWait::Frame(frame.clone());
        }
      }
      let now = Instant::now();
      if now >= deadline {
        return TapWait::Timeout;
      }
      state = self.0.changed.wait_timeout(state, deadline - now).unwrap_or_else(PoisonError::into_inner).0;
    }
  }

  /// Whether the session closed.
  pub fn closed(&self) -> bool {
    self.lock().closed
  }

  /// A buffer for the producer to fill: a replaced frame's when one came
  /// back unheld, otherwise empty (the producer sizes it).
  pub(crate) fn take_spare(&self) -> Vec<u8> {
    self.lock().spare.pop().unwrap_or_default()
  }

  /// Publish the next frame and wake every waiter. The frame it replaces
  /// becomes a spare when no consumer still holds it. A closed tap drops
  /// the frame.
  pub(crate) fn publish(&self, timestamp_ns: u64, width: u32, height: u32, pixels: Vec<u8>) {
    let mut state = self.lock();
    if state.closed {
      return;
    }
    let seq = state.next_seq;
    state.next_seq += 1;
    let previous = state.latest.replace(Arc::new(CameraFrame { seq, timestamp_ns, width, height, pixels }));
    if let Some(Ok(frame)) = previous.map(Arc::try_unwrap) {
      if state.spare.len() < SPARE_BUFFERS {
        state.spare.push(frame.pixels);
      }
    }
    self.0.changed.notify_all();
  }

  /// Mark the session closed: drop the frame, wake every waiter with
  /// `Closed`, and refuse further publishes.
  pub(crate) fn close(&self) {
    let mut state = self.lock();
    state.closed = true;
    state.latest = None;
    state.spare.clear();
    self.0.changed.notify_all();
  }
}
