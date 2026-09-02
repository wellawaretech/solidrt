// FrameRelease (vsync.rs): the vsync frame-release state machine, tested as
// pure decisions over an injected clock - the invariants the run loop used
// to hold in comments across four mutation sites.

use std::time::{Duration, Instant};

use crate::vsync::{FramePacing, FrameRelease, PacingChange, Release, Wake};

const PERIOD: Duration = Duration::from_millis(16);

fn emits(r: Release) -> bool {
  matches!(r, Release::Emit)
}

fn deferred_arm(r: Release) -> Option<Duration> {
  match r {
    Release::Deferred { arm } => arm,
    Release::Emit => panic!("expected Deferred"),
  }
}

#[test]
fn no_backend_always_emits() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(false, t0);
  assert!(emits(fr.on_present(t0, PERIOD)));
  assert!(fr.idle());
  assert!(fr.wait_deadline().is_none());
}

#[test]
fn swap_paced_emits_directly() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  // Nothing was deferred, so the switch releases nothing (and the caller
  // must not touch the frame-signal clock).
  assert!(matches!(fr.set_pacing(FramePacing::SwapPaced), PacingChange::Changed { released: 0 }));
  assert!(emits(fr.on_present(t0, PERIOD)));
  // Re-sending the same policy is not a change (and must not re-log).
  assert!(matches!(fr.set_pacing(FramePacing::SwapPaced), PacingChange::Unchanged));
}

#[test]
fn first_present_out_of_idle_arms_the_chain() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  let arm = deferred_arm(fr.on_present(t0, PERIOD));
  assert!(arm.is_some(), "chain start must arm a vsync request");
  assert!(!fr.idle());
  // The fallback deadline sits past the next vsync (request + period +
  // delay + slack), so a healthy signal can always beat it.
  let deadline = fr.wait_deadline().expect("a deferred present has a deadline");
  assert!(deadline > t0 + PERIOD);
  // A second present while armed must not send a superseding request.
  assert!(deferred_arm(fr.on_present(t0, PERIOD)).is_none());
}

#[test]
fn signal_releases_all_pending_and_prearms() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  deferred_arm(fr.on_present(t0, PERIOD));
  match fr.on_wake(t0 + PERIOD, PERIOD, true) {
    Wake::Release { emit, timed_out, arm } => {
      assert_eq!(emit, 2);
      assert!(!timed_out);
      assert!(arm.is_some(), "the release pre-arms the next vsync");
    }
    Wake::Idle => panic!("a taken signal with pending presents must release"),
  }
  assert!(fr.idle());
}

#[test]
fn fallback_fires_only_at_the_deadline() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let deadline = fr.wait_deadline().expect("deferred");
  // Before the deadline with no signal: keep waiting.
  assert!(matches!(fr.on_wake(deadline - Duration::from_millis(1), PERIOD, false), Wake::Idle));
  // At the deadline: release with the timeout marked, and a fresh request
  // armed (superseding the late signal, which try_take will discard).
  match fr.on_wake(deadline, PERIOD, false) {
    Wake::Release { emit, timed_out, arm } => {
      assert_eq!(emit, 1);
      assert!(timed_out);
      assert!(arm.is_some());
    }
    Wake::Idle => panic!("the fallback must release at the deadline"),
  }
}

#[test]
fn signal_with_nothing_pending_ends_the_chain() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let t1 = t0 + PERIOD;
  assert!(matches!(fr.on_wake(t1, PERIOD, true), Wake::Release { .. }));
  // Demand stops: the pre-armed request's signal drains with nothing
  // pending (one spare callback) and disarms.
  assert!(matches!(fr.on_wake(t1 + PERIOD, PERIOD, true), Wake::Idle));
  // The next present out of idle starts a fresh chain.
  assert!(deferred_arm(fr.on_present(t1 + 2 * PERIOD, PERIOD)).is_some());
}

#[test]
fn leaving_vsync_locked_releases_deferred_presents() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  match fr.set_pacing(FramePacing::SwapPaced) {
    PacingChange::Changed { released } => assert_eq!(released, 1),
    PacingChange::Unchanged => panic!("switching to SwapPaced is a change"),
  }
  assert!(fr.idle());
  // Back to VsyncLocked: a change, but nothing to release.
  assert!(matches!(fr.set_pacing(FramePacing::VsyncLocked), PacingChange::Changed { released: 0 }));
}

#[test]
fn pacing_excursion_disarms_so_the_return_arms_fresh() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  // Leave VsyncLocked while a request is outstanding, then return before
  // its signal drains: the first present back must arm a fresh request
  // (superseding the old one) rather than trust the stale deadline.
  fr.set_pacing(FramePacing::SwapPaced);
  fr.set_pacing(FramePacing::VsyncLocked);
  assert!(deferred_arm(fr.on_present(t0 + PERIOD, PERIOD)).is_some());
  // The old request's signal, arriving late as a superseded generation,
  // never reaches the machine (VsyncSource::try_take discards it), so no
  // on_wake models it here.
}
