//! A fixed set of worker threads for CPU-bound jobs.
//!
//! The async runtime's blocking pool grows a thread per job in flight, so a
//! caller that starts a hundred heavy jobs at once gets a hundred threads
//! fighting over the cores, each with its own allocator state. A `Workers`
//! bounds that at the source: its threads are started once and take the
//! jobs in the order they were queued, however many are waiting. The caller
//! awaits a job's result and never learns there was a queue.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct Workers {
  queue: Sender<Job>,
}

impl Workers {
  /// Start `threads` workers, each named `name` and its index. They live
  /// as long as the process and sleep while the queue is empty.
  pub fn new(name: &str, threads: usize) -> Self {
    let (queue, jobs) = channel::<Job>();
    let jobs = Arc::new(Mutex::new(jobs));
    for index in 0..threads {
      let jobs = jobs.clone();
      std::thread::Builder::new()
        .name(format!("{name}-{index}"))
        .spawn(move || work(&jobs))
        .unwrap_or_else(|e| panic!("cannot start the worker thread {name}-{index}: {e}"));
    }
    Workers { queue }
  }

  /// Queue `job` and wait for its result. Errs only when the job
  /// panicked, which takes nothing else down: the worker goes on with the
  /// next job.
  pub async fn run<T, F>(&self, job: F) -> Result<T, String>
  where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
  {
    let (done, result) = oneshot::channel();
    let job: Job = Box::new(move || {
      // The receiver is gone when the caller stopped waiting; the result
      // has nobody to go to then.
      let _ = done.send(job());
    });
    self.queue.send(job).map_err(|_| "the worker threads are gone".to_string())?;
    result.await.map_err(|_| "the job panicked".to_string())
  }
}

fn work(jobs: &Mutex<Receiver<Job>>) {
  loop {
    // The lock is held for the wait and released before the job runs, so
    // the other workers take the next jobs meanwhile.
    let job = match jobs.lock() {
      Ok(jobs) => jobs.recv(),
      Err(_) => return,
    };
    match job {
      // A panic in a job drops its result sender, which is how the caller
      // hears of it; the worker itself stays.
      Ok(job) => drop(std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))),
      Err(_) => return,
    }
  }
}
