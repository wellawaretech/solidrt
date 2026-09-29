use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::engine::ShutdownHooks;
use crate::logger::default_logger;

// The hooks run from two places, the engine's own end and flux:process
// exit, and a process ending through exit may still unwind through the
// engine: each hook runs once whichever comes first.
#[test]
fn hooks_run_once_however_often_they_are_run() {
  let hooks = ShutdownHooks::new();
  let ran = Arc::new(AtomicUsize::new(0));
  for _ in 0..2 {
    let ran = ran.clone();
    hooks.add(move |_| {
      ran.fetch_add(1, Ordering::SeqCst);
    });
  }
  let logger = default_logger();
  hooks.run(&logger);
  hooks.run(&logger);
  assert_eq!(ran.load(Ordering::SeqCst), 2);
}
