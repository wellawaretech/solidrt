// The cell worker: one thread per owner that turns glyph requests into
// cells off the frame. Rasterizing a glyph is a few hundred microseconds
// of CPU and a distance field more; a label whose glyphs were never seen
// would otherwise stall the frame that first draws it. Jobs go in over a
// channel, done cells come back over another, and the owner drains them at
// its own flush (a frame tick), uploads and re-frames. Dropping the worker
// closes the job channel; the thread finishes the job in hand and exits,
// joined by the drop so an engine teardown leaves no thread behind.
use super::cells::{Cell, CellRequest, Rasterizer};
use super::fonts::FontBytes;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

/// One request: which owner (an atlas handle of the caller's) it is for,
/// the face bytes and what to rasterize. `done` runs on the worker thread
/// right after the result is queued: the owner's way to wake whatever
/// drains the cells (latch a frame request) and to end a hold that counts
/// the job as work in flight.
pub struct CellJob {
  pub owner: u64,
  pub bytes: FontBytes,
  pub request: CellRequest,
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

impl CellWorker {
  /// Start the thread. `name` labels it for a profiler.
  pub fn spawn(name: &str) -> std::io::Result<Self> {
    let (job_tx, job_rx) = channel::<CellJob>();
    let (done_tx, done_rx) = channel::<CellsDone>();
    let thread = std::thread::Builder::new().name(name.to_string()).spawn(move || {
      let mut rasterizer = Rasterizer::default();
      while let Ok(mut job) = job_rx.recv() {
        let bytes = job.bytes.as_ref().as_ref();
        let cells = rasterizer.rasterize(bytes, &job.request).unwrap_or_default();
        let failed = job.request.glyphs.iter().copied().filter(|g| !cells.iter().any(|c| c.glyph == *g)).collect();
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
