// The decode thread behind streaming clips: `sol-audio-decode`, lazily
// spawned, keeps every streaming clip's ring filled to a lookahead from
// its decoder, loops by seeking the decoder back to the start (the ring
// stays continuous, so the seam is seamless), restarts on a play or a
// seek, and ends the ring when a clip ends. A decoder that stalls starves
// its own ring and nothing else: the mix never waits on this thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use super::ring::RingProducer;
use super::ClipSource;

/// How far ahead of the mix a streaming clip is decoded, in seconds of its
/// own rate: deep enough to ride out a slow source read, shallow enough
/// that a restart is heard promptly.
pub(crate) const STREAM_LOOKAHEAD_SECONDS: f64 = 0.5;
/// The ring's capacity in seconds: the lookahead plus room for a full
/// decoder read beyond it.
pub(crate) const STREAM_RING_SECONDS: f64 = 1.0;
/// Frames per decoder read.
const DECODE_CHUNK_FRAMES: usize = 2048;
/// How long the thread sleeps between fills when nothing wakes it.
const DECODE_TICK: Duration = Duration::from_millis(50);
/// `restart` unset.
const NO_RESTART: u64 = u64::MAX;

/// What the control thread and the thread share about one streaming clip.
pub(crate) struct StreamControl {
  /// A pending restart at this content frame (NO_RESTART when none).
  restart: AtomicU64,
  pub(crate) looping: AtomicBool,
  /// The clip was unloaded: drop the decoder, end the ring.
  closed: AtomicBool,
}

impl StreamControl {
  fn new() -> StreamControl {
    StreamControl {
      restart: AtomicU64::new(NO_RESTART),
      looping: AtomicBool::new(false),
      closed: AtomicBool::new(false),
    }
  }

  pub(crate) fn restart_at(&self, frame: u64) {
    self.restart.store(frame, Ordering::Release);
  }

  pub(crate) fn close(&self) {
    self.closed.store(true, Ordering::Release);
  }
}

struct Job {
  source: Box<dyn ClipSource>,
  ring: RingProducer,
  control: Arc<StreamControl>,
  channels: usize,
  lookahead: usize,
  chunk: Vec<f32>,
  ended: bool,
}

impl Job {
  /// One pass: honour a restart, then fill to the lookahead.
  fn fill(&mut self) {
    let restart = self.control.restart.swap(NO_RESTART, Ordering::AcqRel);
    if restart != NO_RESTART {
      match self.source.seek(restart) {
        Ok(()) => {
          self.ring.restart(restart);
          self.ended = false;
        }
        Err(e) => log::warn!("[alloy::audio] stream seek: {e}"),
      }
    }
    while !self.ended && self.ring.shared().queued() < self.lookahead as u64 {
      let room = self.ring.free().min(DECODE_CHUNK_FRAMES);
      if room == 0 {
        break;
      }
      let frames = match self.source.read(&mut self.chunk[..room * self.channels]) {
        Ok(frames) => frames,
        Err(e) => {
          log::warn!("[alloy::audio] stream decode: {e}");
          0
        }
      };
      if frames == 0 {
        if self.control.looping.load(Ordering::Acquire) {
          if let Err(e) = self.source.seek(0) {
            log::warn!("[alloy::audio] stream loop: {e}");
            self.end();
          }
          // The ring runs on across the wrap; the position wraps on the
          // clip length in the engine.
          continue;
        }
        self.end();
        break;
      }
      self.ring.push(&self.chunk[..frames * self.channels]);
    }
  }

  fn end(&mut self) {
    self.ended = true;
    self.ring.set_ended();
  }
}

/// The thread and its job list.
pub(crate) struct Decoder {
  jobs: Arc<(Mutex<Vec<Job>>, Condvar)>,
}

impl Decoder {
  pub(crate) fn spawn() -> Decoder {
    let jobs: Arc<(Mutex<Vec<Job>>, Condvar)> = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
    let shared = Arc::clone(&jobs);
    let spawned = thread::Builder::new().name("sol-audio-decode".into()).spawn(move || {
      let (lock, cvar) = &*shared;
      let mut jobs = lock.lock().expect("audio decode lock poisoned");
      loop {
        jobs.retain_mut(|job| {
          if job.control.closed.load(Ordering::Acquire) {
            job.ring.set_ended();
            return false;
          }
          job.fill();
          true
        });
        // Woken by a new job, a restart or a loop change; otherwise the
        // tick keeps the rings topped up.
        jobs = cvar.wait_timeout(jobs, DECODE_TICK).expect("audio decode wait poisoned").0;
      }
    });
    spawned.expect("failed to spawn sol-audio-decode thread");
    Decoder { jobs }
  }

  /// Hand a decoder to the thread, filling `ring` from it at once.
  pub(crate) fn add(&self, source: Box<dyn ClipSource>, ring: RingProducer, channels: u16) -> Arc<StreamControl> {
    let format = source.format();
    let control = Arc::new(StreamControl::new());
    let job = Job {
      source,
      ring,
      control: Arc::clone(&control),
      channels: channels as usize,
      lookahead: (format.sample_rate as f64 * STREAM_LOOKAHEAD_SECONDS) as usize,
      chunk: vec![0.0; DECODE_CHUNK_FRAMES * channels as usize],
      ended: false,
    };
    let (lock, cvar) = &*self.jobs;
    lock.lock().expect("audio decode lock poisoned").push(job);
    cvar.notify_one();
    control
  }

  /// Wake the thread: a restart or loop change wants a fill now.
  pub(crate) fn wake(&self) {
    self.jobs.1.notify_one();
  }
}
