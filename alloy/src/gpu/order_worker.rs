//! The sort worker: the thread that re-sorts a large handed-off key column
//! off the UI thread. A radix sort over a million keys is tens of
//! milliseconds on a phone core, memory-bound and sharing the bus with the
//! draw of the same population, so run in the flush it stalls the frame at
//! every camera turn past the order gate. Here the registry hands the sort
//! a job - the key column it shares with the mirror, the order to read it
//! with, a spare permutation vector and the id buffer's pooled block - and
//! the worker sorts, writes the ids into the block and hands both back; the
//! registry publishes them at its next drain. The order then lags the
//! camera by one sort, which every CPU-sorting viewer in the field accepts,
//! and the frame never waits; the thread runs with a latency worker's
//! placement, so that lag is one sort on a performance core, not on
//! whichever core the scheduler parks a low-utilization thread on (the
//! order lags the camera by the sort's duration, which on a little core at
//! its floor clock was two to four times a big core's). Jobs go in over a
//! channel, results come back
//! over another, and dropping the worker closes the job channel: the thread
//! finishes the job in hand and exits, joined by the drop so an engine
//! teardown leaves no thread behind.
use super::{
  materialize_indices, materialize_indices_through, order_packed, order_permutation, InstanceOrder, OrderScratch,
  PackedKey, RunTable, Selection,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

/// One re-sort for the worker.
pub struct SortJob {
  /// The ordered entry (target, draw) the result returns to.
  pub entry: (u64, u64),
  /// The entry's key buffer: where the ids land, and whose pool the block
  /// belongs to.
  pub buffer: u64,
  /// The registry's submit counter for the entry: a result from an older
  /// job than the one in flight is stale.
  pub generation: u64,
  /// The order as the sort reads it off the key column
  /// (`InstanceOrder::mirror_order`).
  pub order: InstanceOrder,
  /// Bytes per key record (`KeyColumn::mirror_len`).
  pub stride: usize,
  /// A packed key's packing, run table and selection: the column then
  /// holds the packed words in storage order, the job selects runs
  /// before it sorts, and the ids map through the selection.
  pub packed: Option<PackedJob>,
  /// The key column, shared with the mirror.
  pub keys: Arc<Vec<u8>>,
  /// The published prefix of the column in bytes: what sorts.
  pub len: usize,
  /// A vector the permutation is written into (reused across jobs).
  pub perm: Vec<u32>,
  /// A vector the selected record ids are written into (packed jobs;
  /// reused across jobs).
  pub ids: Vec<u32>,
  /// The id buffer's block, buffer-sized: the sorted ids, then the
  /// sentinel to its end.
  pub block: Vec<u8>,
  /// Runs on the worker thread right after the result is queued: the
  /// registry's way to latch a frame request and wake the loop that
  /// drains it.
  pub done: Option<Box<dyn FnOnce() + Send>>,
}

/// A packed job's selection inputs (`SortJob::packed`).
pub struct PackedJob {
  pub key: PackedKey,
  pub table: Arc<RunTable>,
  pub selection: Selection,
}

/// A finished re-sort.
pub struct SortDone {
  pub entry: (u64, u64),
  pub buffer: u64,
  pub generation: u64,
  /// The permutation (perm[i] = the slot drawn i-th: a record, or under a
  /// packed job a selected slot `ids` names).
  pub perm: Vec<u32>,
  /// A packed job's selected record ids in slot order; empty otherwise.
  pub ids: Vec<u32>,
  /// The block with the ids materialized.
  pub block: Vec<u8>,
  /// The sort and the materialization, in milliseconds.
  pub work_ms: f32,
}

/// The sort worker's counters, read by /stats and the settle gate.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrderCounters {
  /// Jobs submitted and not yet drained.
  pub pending: usize,
  /// Jobs the thread has not finished sorting: the part of `pending` no
  /// frame can land yet (the settle gate waits these out on the wall).
  pub running: usize,
  /// Results drained so far.
  pub sorts: u64,
  /// The last drained job's work in milliseconds.
  pub last_ms: f32,
  /// The slowest drained job's work in milliseconds.
  pub max_ms: f32,
}

pub struct OrderWorker {
  jobs: Option<Sender<SortJob>>,
  done: Receiver<SortDone>,
  /// Jobs submitted and not yet sorted; the thread counts one down once
  /// its result is queued, before it latches the frame.
  running: Arc<AtomicUsize>,
  thread: Option<JoinHandle<()>>,
}

impl OrderWorker {
  /// Start the thread. Errs when the OS refuses a thread; the caller then
  /// sorts in place.
  pub fn spawn() -> std::io::Result<Self> {
    let (job_tx, job_rx) = channel::<SortJob>();
    let (done_tx, done_rx) = channel::<SortDone>();
    let running = Arc::new(AtomicUsize::new(0));
    let counting = running.clone();
    let thread = std::thread::Builder::new().name("sol-order".into()).spawn(move || {
      // The sort's duration is the order's lag behind the camera, so the
      // thread takes a latency worker's placement: a performance core,
      // below the frame tier (sdl_utils::latency_worker_thread_priority),
      // re-asserted per job because a cpuset move drops a pin.
      let placement = crate::sdl_utils::latency_worker_thread_priority();
      let mut scratch = OrderScratch::default();
      while let Ok(mut job) = job_rx.recv() {
        placement.reassert();
        let start = Instant::now();
        job.ids.clear();
        match &job.packed {
          Some(packed) => {
            order_packed(&job.order, packed.key, &job.keys[..job.len], &packed.table, &packed.selection, &mut scratch);
            job.ids.extend_from_slice(scratch.ids());
          }
          None => order_permutation(&job.order, job.stride, &job.keys[..job.len], &mut scratch),
        }
        job.perm.clear();
        job.perm.extend_from_slice(scratch.perm());
        match &job.packed {
          Some(_) => materialize_indices_through(&job.perm, &job.ids, &mut job.block),
          None => materialize_indices(&job.perm, &mut job.block),
        }
        let work_ms = start.elapsed().as_secs_f32() * 1000.0;
        let done = job.done.take();
        let result = SortDone {
          entry: job.entry,
          buffer: job.buffer,
          generation: job.generation,
          perm: job.perm,
          ids: job.ids,
          block: job.block,
          work_ms,
        };
        if done_tx.send(result).is_err() {
          break;
        }
        counting.fetch_sub(1, Ordering::Release);
        if let Some(done) = done {
          done();
        }
      }
    })?;
    Ok(Self { jobs: Some(job_tx), done: done_rx, running, thread: Some(thread) })
  }

  /// Queue a job; false once the thread is gone.
  pub fn submit(&self, job: SortJob) -> bool {
    let Some(jobs) = self.jobs.as_ref() else {
      return false;
    };
    self.running.fetch_add(1, Ordering::Relaxed);
    if jobs.send(job).is_err() {
      self.running.fetch_sub(1, Ordering::Relaxed);
      return false;
    }
    true
  }

  /// Jobs submitted and not yet sorted.
  pub fn running(&self) -> usize {
    self.running.load(Ordering::Acquire)
  }

  /// Everything finished so far, oldest first.
  pub fn drain(&self) -> Vec<SortDone> {
    let mut out = Vec::new();
    loop {
      match self.done.try_recv() {
        Ok(done) => out.push(done),
        Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return out,
      }
    }
  }
}

impl Drop for OrderWorker {
  fn drop(&mut self) {
    self.jobs.take();
    if let Some(thread) = self.thread.take() {
      if thread.join().is_err() {
        log::warn!("[order] the sort thread ended by a panic");
      }
    }
  }
}
