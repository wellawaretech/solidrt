// The cell worker: one thread per owner that turns glyph requests into
// cells off the frame. Rasterizing a glyph is a few hundred microseconds
// of CPU and a distance field more; a label whose glyphs were never seen
// would otherwise stall the frame that first draws it. Jobs go in over a
// channel, done cells come back over another, and the owner drains them at
// its own flush (a frame tick), uploads and re-frames. Two priorities: a
// job for glyphs a frame is missing right now runs ahead of a warm-up,
// however many warm-ups are queued. Dropping the worker closes the job
// channel; the thread finishes the job in hand and exits, joined by the
// drop so an engine teardown leaves no thread behind.
use super::cells::{Cell, CellRequest, Rasterizer};
use super::fonts::FontBytes;
use skrifa::outline::GlyphStyles;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

/// How urgent a job is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobPriority {
  /// A frame drew without these glyphs; it completes when they land.
  Needed,
  /// Glyphs nobody asked for yet, made ahead of their first use.
  Warm,
}

/// One request: which owner (an atlas handle of the caller's) it is for,
/// the face bytes and styles and what to rasterize. `done` runs on the worker thread
/// right after the result is queued: the owner's way to wake whatever
/// drains the cells (latch a frame request) and to end a hold that counts
/// the job as work in flight.
pub struct CellJob {
  pub owner: u64,
  pub bytes: FontBytes,
  /// The face's autohinter styles (`Face::styles`), for a hinted request.
  pub styles: GlyphStyles,
  pub request: CellRequest,
  pub priority: JobPriority,
  pub done: Option<Box<dyn FnOnce() + Send>>,
}

/// A finished job: the cells made and the glyphs that could not be.
pub struct CellsDone {
  pub owner: u64,
  pub cells: Vec<Cell>,
  pub failed: Vec<u16>,
}

pub struct CellWorker {
  jobs: Option<Sender<CellJob>>,
  done: Receiver<CellsDone>,
  thread: Option<JoinHandle<()>>,
}

// The worker's view of its inbox: everything received so far, by priority.
#[derive(Default)]
struct Inbox {
  needed: VecDeque<CellJob>,
  warm: VecDeque<CellJob>,
}

impl Inbox {
  fn push(&mut self, job: CellJob) {
    match job.priority {
      JobPriority::Needed => self.needed.push_back(job),
      JobPriority::Warm => self.warm.push_back(job),
    }
  }

  fn pop(&mut self) -> Option<CellJob> {
    self.needed.pop_front().or_else(|| self.warm.pop_front())
  }

  // The next job: whatever arrived while the last one ran goes in first,
  // so a needed job jumps a queued warm-up; blocks when nothing is queued.
  // None once the channel is closed and drained.
  fn next(&mut self, rx: &Receiver<CellJob>) -> Option<CellJob> {
    loop {
      match rx.try_recv() {
        Ok(job) => self.push(job),
        Err(TryRecvError::Empty) => break,
        Err(TryRecvError::Disconnected) => return self.pop(),
      }
    }
    if let Some(job) = self.pop() {
      return Some(job);
    }
    rx.recv().ok()
  }
}

impl CellWorker {
  /// Start the thread. `name` labels it for a profiler.
  pub fn spawn(name: &str) -> std::io::Result<Self> {
    let (job_tx, job_rx) = channel::<CellJob>();
    let (done_tx, done_rx) = channel::<CellsDone>();
    let thread = std::thread::Builder::new().name(name.to_string()).spawn(move || {
      let mut rasterizer = Rasterizer::default();
      let mut inbox = Inbox::default();
      while let Some(mut job) = inbox.next(&job_rx) {
        let cells = rasterizer.rasterize(&job.bytes, &job.styles, &job.request).unwrap_or_default();
        let failed = job.request.glyphs.iter().copied().filter(|g| !cells.iter().any(|c| c.key == *g)).collect();
        let sent = done_tx.send(CellsDone { owner: job.owner, cells, failed }).is_ok();
        if let Some(done) = job.done.take() {
          done();
        }
        if !sent {
          break;
        }
      }
    })?;
    Ok(Self { jobs: Some(job_tx), done: done_rx, thread: Some(thread) })
  }

  /// Queue a job. False once the thread is gone (nothing will come back).
  pub fn submit(&self, job: CellJob) -> bool {
    self.jobs.as_ref().is_some_and(|tx| tx.send(job).is_ok())
  }

  /// Everything finished since the last drain, without waiting.
  pub fn drain(&self) -> Vec<CellsDone> {
    self.done.try_iter().collect()
  }
}

impl Drop for CellWorker {
  fn drop(&mut self) {
    // Closing the job channel ends the loop; the join bounds the exit to
    // the job in hand.
    self.jobs.take();
    if let Some(thread) = self.thread.take() {
      thread.join().ok();
    }
  }
}
