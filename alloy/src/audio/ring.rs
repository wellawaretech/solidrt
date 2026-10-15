// A bounded ring of interleaved f32 frames between one producer thread and
// one consumer (the mix callback), lock-free: two monotonic frame indices,
// released by the side that advances them and acquired by the other, and
// a data buffer each side touches only in the range the indices give it.
//
// A clear is the producer's (a video seek, a sink's `clear`) and must not
// swallow what it pushes right after, so it is a mark, not a command: the
// producer publishes a discard point at its write index with the content
// frame that point stands for, and the consumer jumps to it at its next
// pop. Until then the producer's own view (`consumed`, `queued`) already
// counts the discarded frames as consumed.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub(crate) struct RingShared {
  capacity: usize,
  channels: usize,
  // Written by the producer in [write, write + n), read by the consumer in
  // [read, read + n), never both on the same frame: the indices order it.
  buf: UnsafeCell<Box<[f32]>>,
  write: AtomicU64,
  read: AtomicU64,
  // The discard point and the content frame it stands for, published
  // together under `seq` (a seqlock: odd while being written).
  seq: AtomicU32,
  discard_to: AtomicU64,
  base: AtomicU64,
  // The producer has nothing more to push (the clip ended); cleared by a
  // restart.
  ended: AtomicBool,
  // Restarts so far: a consumer started for a restart waits until it has
  // been applied, so it never plays what the restart discards.
  generation: AtomicU32,
}

// The data cells are shared across threads under the index protocol above.
unsafe impl Sync for RingShared {}
unsafe impl Send for RingShared {}

impl RingShared {
  fn new(capacity: usize, channels: usize) -> RingShared {
    RingShared {
      capacity,
      channels,
      buf: UnsafeCell::new(vec![0.0; capacity * channels].into_boxed_slice()),
      write: AtomicU64::new(0),
      read: AtomicU64::new(0),
      seq: AtomicU32::new(0),
      discard_to: AtomicU64::new(0),
      base: AtomicU64::new(0),
      ended: AtomicBool::new(false),
      generation: AtomicU32::new(0),
    }
  }

  pub(crate) fn generation(&self) -> u32 {
    self.generation.load(Ordering::Acquire)
  }

  /// The discard point and its content frame, read consistently.
  fn discard(&self) -> (u64, u64) {
    loop {
      let before = self.seq.load(Ordering::Acquire);
      let to = self.discard_to.load(Ordering::Acquire);
      let base = self.base.load(Ordering::Acquire);
      let after = self.seq.load(Ordering::Acquire);
      if before == after && before % 2 == 0 {
        return (to, base);
      }
    }
  }

  /// Frames consumed: the read index, or the discard point when that is
  /// ahead of it (a clear the consumer has not applied yet).
  pub(crate) fn consumed(&self) -> u64 {
    self.read.load(Ordering::Acquire).max(self.discard().0)
  }

  /// Frames pushed and not consumed (a pending discard counts as consumed).
  pub(crate) fn queued(&self) -> u64 {
    self.write.load(Ordering::Acquire).saturating_sub(self.consumed())
  }

  /// The content frame at `index` (a frame index of this ring), from the
  /// latest restart's base.
  pub(crate) fn content_at(&self, index: u64) -> u64 {
    let (to, base) = self.discard();
    base.wrapping_add(index.wrapping_sub(to))
  }

  pub(crate) fn ended(&self) -> bool {
    self.ended.load(Ordering::Acquire)
  }

  // Copy `frames` frames in or out at ring index `at`, wrapping once.
  unsafe fn copy_in(&self, at: u64, src: &[f32]) {
    let buf = &mut *self.buf.get();
    let start = (at % self.capacity as u64) as usize * self.channels;
    let first = (buf.len() - start).min(src.len());
    let rest = src.len() - first;
    buf[start..start + first].copy_from_slice(&src[..first]);
    buf[..rest].copy_from_slice(&src[first..]);
  }

  unsafe fn copy_out(&self, at: u64, dst: &mut [f32]) {
    let buf = &*self.buf.get();
    let start = (at % self.capacity as u64) as usize * self.channels;
    let first = (buf.len() - start).min(dst.len());
    let rest = dst.len() - first;
    dst[..first].copy_from_slice(&buf[start..start + first]);
    dst[first..].copy_from_slice(&buf[..rest]);
  }
}

/// The pushing side. One per ring: the index protocol allows one writer.
pub(crate) struct RingProducer(Arc<RingShared>);

/// The popping side, the mix callback's. One per ring at a time.
pub(crate) struct RingConsumer(Arc<RingShared>);

/// A ring of `capacity` frames of `channels` channels, as its two halves.
pub(crate) fn ring(capacity: usize, channels: usize) -> (RingProducer, RingConsumer) {
  let shared = Arc::new(RingShared::new(capacity, channels));
  (RingProducer(Arc::clone(&shared)), RingConsumer(shared))
}

impl RingProducer {
  pub(crate) fn shared(&self) -> &Arc<RingShared> {
    &self.0
  }

  /// Frames that can be pushed now.
  pub(crate) fn free(&self) -> usize {
    (self.0.capacity as u64).saturating_sub(self.0.queued()) as usize
  }

  /// Push whole frames; the frames that fit, returning how many.
  pub(crate) fn push(&mut self, samples: &[f32]) -> usize {
    let frames = (samples.len() / self.0.channels).min(self.free());
    if frames == 0 {
      return 0;
    }
    let write = self.0.write.load(Ordering::Relaxed);
    unsafe { self.0.copy_in(write, &samples[..frames * self.0.channels]) };
    self.0.write.store(write + frames as u64, Ordering::Release);
    frames
  }

  /// Discard everything queued and start counting content from
  /// `base_frame` at the next pushed frame. Clears `ended`.
  pub(crate) fn restart(&mut self, base_frame: u64) {
    let write = self.0.write.load(Ordering::Relaxed);
    self.0.seq.fetch_add(1, Ordering::AcqRel);
    self.0.discard_to.store(write, Ordering::Release);
    self.0.base.store(base_frame, Ordering::Release);
    self.0.seq.fetch_add(1, Ordering::AcqRel);
    self.0.ended.store(false, Ordering::Release);
    self.0.generation.fetch_add(1, Ordering::AcqRel);
  }

  /// Nothing more will be pushed (until a restart).
  pub(crate) fn set_ended(&mut self) {
    self.0.ended.store(true, Ordering::Release);
  }
}

impl RingConsumer {
  /// A consumer over a ring's shared state: for a ring whose previous
  /// consumer has left (a streaming clip played again). One at a time.
  pub(crate) fn over(shared: Arc<RingShared>) -> RingConsumer {
    RingConsumer(shared)
  }

  pub(crate) fn shared(&self) -> &Arc<RingShared> {
    &self.0
  }

  /// Apply a pending discard: jump the read index to it. Returns the
  /// frames skipped.
  pub(crate) fn apply_discard(&mut self) -> u64 {
    let read = self.0.read.load(Ordering::Relaxed);
    let (to, _) = self.0.discard();
    if to > read {
      self.0.read.store(to, Ordering::Release);
      to - read
    } else {
      0
    }
  }

  /// Pop up to the whole frames `out` holds, after any pending discard.
  /// Returns the frames popped; 0 when the ring is empty.
  pub(crate) fn pop(&mut self, out: &mut [f32]) -> usize {
    self.apply_discard();
    let read = self.0.read.load(Ordering::Relaxed);
    let write = self.0.write.load(Ordering::Acquire);
    let frames = ((write - read) as usize).min(out.len() / self.0.channels);
    if frames == 0 {
      return 0;
    }
    unsafe { self.0.copy_out(read, &mut out[..frames * self.0.channels]) };
    self.0.read.store(read + frames as u64, Ordering::Release);
    frames
  }

  /// The consumer's read index (frames).
  pub(crate) fn read_index(&self) -> u64 {
    self.0.read.load(Ordering::Relaxed)
  }
}
