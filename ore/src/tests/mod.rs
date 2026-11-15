mod audio;
mod availability;
mod camera;
mod clipboard;
#[cfg(feature = "window")]
mod dialog;
mod folders;
mod haptics;
mod input;
mod link;
mod locale;
mod permission;
mod platform;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android", target_os = "ios")))]
mod portal;
mod power;
mod share;
#[cfg(feature = "window")]
mod window;

use std::sync::{Mutex, MutexGuard, PoisonError};

// The availability table is process-wide and the tests run in parallel:
// a test that sets a facility's state, or that depends on it, holds this
// for its whole run.
static FACTS: Mutex<()> = Mutex::new(());

pub(crate) fn facts_lock() -> MutexGuard<'static, ()> {
  FACTS.lock().unwrap_or_else(PoisonError::into_inner)
}
