use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::workers::Workers;

const THREADS: usize = 3;
const JOBS: usize = 24;

// However many jobs are queued at once, no more than the pool's threads
// run at a time, and every job's own result comes back to its caller.
#[tokio::test]
async fn runs_no_more_jobs_at_once_than_it_has_threads() {
  let workers = Arc::new(Workers::new("test-workers", THREADS));
  let running = Arc::new(AtomicUsize::new(0));
  let most = Arc::new(AtomicUsize::new(0));
  let jobs: Vec<_> = (0..JOBS)
    .map(|i| {
      let (workers, running, most) = (workers.clone(), running.clone(), most.clone());
      tokio::spawn(async move {
        workers
          .run(move || {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            running.fetch_sub(1, Ordering::SeqCst);
            i * 2
          })
          .await
      })
    })
    .collect();
  for (i, job) in jobs.into_iter().enumerate() {
    assert_eq!(job.await.expect("the task ran"), Ok(i * 2));
  }
  assert_eq!(most.load(Ordering::SeqCst), THREADS, "the pool ran as many jobs at once as it has threads, no more");
}

// A panic reaches the caller of that job with what it said, a literal
// message and a formatted one alike, and the worker takes the next job.
#[tokio::test]
async fn a_panicking_job_fails_alone() {
  let workers = Workers::new("test-workers-panic", 1);
  let failed: Result<(), String> = workers.run(|| panic!("job failure, expected by this test")).await;
  assert_eq!(failed, Err("the job panicked: job failure, expected by this test".to_string()));
  let level = 3;
  let failed: Result<(), String> = workers.run(move || panic!("level {level} failure, expected by this test")).await;
  assert_eq!(failed, Err("the job panicked: level 3 failure, expected by this test".to_string()));
  assert_eq!(workers.run(|| 7).await, Ok(7));
}
