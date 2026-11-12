use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::permission::{request, Permission};

// A platform with no prompt of its own answers yes before `request`
// returns, so a caller can open the device straight away.
#[cfg(not(target_os = "android"))]
#[test]
fn a_platform_without_a_prompt_grants_at_once() {
  let granted = Arc::new(AtomicBool::new(false));
  let answer = granted.clone();
  request(Permission::Microphone, move |ok| answer.store(ok, Ordering::Release)).expect("request is made");
  assert!(granted.load(Ordering::Acquire));
}
