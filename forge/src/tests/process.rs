use crate::process::{alive, pid};

#[test]
fn alive_reports_own_process_and_not_a_free_pid() {
  assert!(alive(pid()), "the current process is alive");
  // A pid nobody is likely to hold: the highest pids are far past any test
  // runner, and a false positive would need that exact process to exist.
  assert!(!alive(u32::MAX - 1), "a free pid is not alive");
}
